mod analyze;
mod compile;
mod metadata;
mod output;
mod progress;
mod rlib_selection;
mod runner_gen;
mod topology;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::*;
use simplebench_runtime::{
    baseline::{BaselineManager, ComparisonResult},
    config::BenchmarkConfig,
    BenchResult, BenchmarkInfo,
};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Configuration for running benchmarks
struct RunConfig {
    bench_filter: Option<String>,
    samples: Option<usize>,
    warmup_duration: Option<u64>,
    threshold: Option<f64>,
    ci: bool,
    window: Option<usize>,
    confidence: Option<f64>,
    cp_threshold: Option<f64>,
    hazard_rate: Option<f64>,
    parallel: bool,
    jobs: Option<usize>,
    quiet: bool,
}

/// SimpleBench - Simple microbenchmarking for Rust
#[derive(Parser, Debug)]
#[command(name = "cargo-simplebench")]
#[command(bin_name = "cargo simplebench")]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Workspace root directory (default: current directory)
    #[arg(long, global = true)]
    workspace_root: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run benchmarks (default command)
    Run {
        /// Run only benchmarks matching this name (substring match)
        #[arg(long)]
        bench: Option<String>,

        /// Number of timing samples per benchmark
        #[arg(long)]
        samples: Option<usize>,

        /// Warmup duration in seconds (default: 3)
        #[arg(long)]
        warmup_duration: Option<u64>,

        /// Regression threshold percentage (default: 5.0)
        #[arg(long)]
        threshold: Option<f64>,

        /// Enable CI mode: fail on performance regressions
        #[arg(long)]
        ci: bool,

        /// Window size for historical comparison (default: 10)
        #[arg(long)]
        window: Option<usize>,

        /// Statistical confidence level (default: 0.95 = 95%)
        #[arg(long)]
        confidence: Option<f64>,

        /// Change point probability threshold (default: 0.8 = 80%)
        #[arg(long)]
        cp_threshold: Option<f64>,

        /// Bayesian hazard rate (default: 0.1 = change every 10 runs)
        #[arg(long)]
        hazard_rate: Option<f64>,

        /// Run benchmarks in parallel (one per physical core, excluding core 0)
        #[arg(long)]
        parallel: bool,

        /// Number of parallel jobs (cores to use). Implies --parallel.
        #[arg(long, short = 'j')]
        jobs: Option<usize>,

        /// Suppress progress bars
        #[arg(long, short = 'q')]
        quiet: bool,
    },

    /// Clean existing benchmark results
    Clean {},

    /// Analyze benchmark results
    Analyze {
        /// Benchmark name (e.g., "game_math_vector_add" or "crate_name_bench_name")
        benchmark_name: String,

        /// Analyze a specific run by timestamp (e.g., "2025-01-15T10-30-00")
        #[arg(long)]
        run: Option<String>,

        /// Analyze the last N runs
        #[arg(long)]
        last: Option<usize>,
    },
}

fn main() -> Result<()> {
    // Handle cargo invocation: `cargo simplebench` passes "simplebench" as first arg
    let mut args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1] == "simplebench" {
        // Called as `cargo simplebench` - remove the "simplebench" argument
        args.remove(1);
    }

    // Parse arguments
    let cli_args = Args::parse_from(args);

    // Determine workspace root
    let workspace_root = cli_args
        .workspace_root
        .clone()
        .unwrap_or_else(|| env::current_dir().expect("Failed to get current directory"));

    // Handle subcommands
    let run_config = match cli_args.command {
        Some(Commands::Analyze {
            benchmark_name,
            run,
            last,
        }) => {
            return analyze::run_analysis(&workspace_root, &benchmark_name, run, last);
        }
        Some(Commands::Clean {}) => {
            println!("Cleaning .benches directory!");
            return std::fs::remove_dir_all(workspace_root.join(".benches"))
                .map_err(anyhow::Error::msg);
        }
        Some(Commands::Run {
            bench,
            samples,
            warmup_duration,
            threshold,
            ci,
            window,
            confidence,
            cp_threshold,
            hazard_rate,
            parallel,
            jobs,
            quiet,
        }) => {
            // Explicit run command
            RunConfig {
                bench_filter: bench,
                samples,
                warmup_duration,
                threshold,
                ci,
                window,
                confidence,
                cp_threshold,
                hazard_rate,
                parallel: parallel || jobs.is_some(),
                jobs,
                quiet,
            }
        }
        None => {
            // No subcommand - default to running all benchmarks (sequential)
            RunConfig {
                bench_filter: None,
                samples: None,
                warmup_duration: None,
                threshold: None,
                ci: false,
                window: None,
                confidence: None,
                cp_threshold: None,
                hazard_rate: None,
                parallel: false,
                jobs: None,
                quiet: false,
            }
        }
    };

    // Step 1: Analyze workspace
    println!("{}", "Analyzing workspace...".green().bold());
    let workspace_info =
        metadata::analyze_workspace(&workspace_root).context("Failed to analyze workspace")?;

    if workspace_info.benchmark_crates.is_empty() {
        eprintln!("{}", "error: No benchmark crates found!".red().bold());
        eprintln!(
            "{}",
            "       Benchmark crates must depend on simplebench-runtime".dimmed()
        );
        std::process::exit(1);
    }

    println!(
        "     {} {} benchmark crates",
        "Found".dimmed(),
        workspace_info
            .benchmark_crates
            .len()
            .to_string()
            .green()
            .bold()
    );
    for crate_info in &workspace_info.benchmark_crates {
        println!("       {} {}", "•".cyan(), crate_info.name);
    }
    println!();

    // Step 2: Build workspace crates with dev-deps and cfg(test)
    println!(
        "{}",
        "Compiling benchmark crates (release + dev-deps)"
            .green()
            .bold()
    );

    // Use isolated target directory to avoid cache conflicts
    let target_dir = workspace_info.target_directory.join("simplebench");

    // Extract benchmark crate names
    let benchmark_crate_names: Vec<String> = workspace_info
        .benchmark_crates
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let rlibs = rlib_selection::build_and_select_rlibs(
        &workspace_root,
        &benchmark_crate_names,
        &target_dir,
    )
    .context("Failed to build and select rlibs")?;

    println!(
        "     {} {} rlib files",
        "Selected".dimmed(),
        rlibs.len().to_string().green()
    );
    println!();

    // Verify required dependencies are present
    let required_deps = vec!["simplebench_runtime", "inventory"];
    for dep in &required_deps {
        if !rlibs.contains_key(*dep) {
            anyhow::bail!("Required dependency '{}' not found in rlibs", dep);
        }
    }

    // Verify all benchmark crates are present
    for crate_info in &workspace_info.benchmark_crates {
        let crate_name = crate_info.name.replace('-', "_");
        if !rlibs.contains_key(&crate_name) {
            anyhow::bail!("Benchmark crate '{}' not found in rlibs", crate_name);
        }
    }

    // Step 3: Generate runner
    println!("{}", "Generating benchmark runner".green().bold());
    let runner_path = runner_gen::write_runner(
        &workspace_info.target_directory,
        &workspace_info.benchmark_crates,
    )
    .context("Failed to write runner")?;
    println!();

    // Step 4: Compile runner
    println!("{}", "Compiling runner".green().bold());
    let runner_binary = workspace_info.target_directory.join("simplebench_runner");

    let deps_dir = target_dir.join("release").join("deps");

    compile::compile_runner(&runner_path, &runner_binary, &rlibs, &deps_dir)
        .context("Failed to compile runner")?;

    // Step 5: Discover benchmarks via --list
    println!("{}", "Discovering benchmarks".green().bold());
    let benchmarks = discover_benchmarks(&runner_binary, &workspace_root)?;

    // Apply filter if specified
    let benchmarks: Vec<BenchmarkInfo> = if let Some(ref filter) = run_config.bench_filter {
        benchmarks
            .into_iter()
            .filter(|b| b.name.contains(filter))
            .collect()
    } else {
        benchmarks
    };

    if benchmarks.is_empty() {
        eprintln!("{}", "error: No benchmarks found!".red().bold());
        if run_config.bench_filter.is_some() {
            eprintln!(
                "{}",
                "       (filter may have excluded all benchmarks)".dimmed()
            );
        }
        std::process::exit(1);
    }

    println!(
        "     {} {} benchmarks",
        "Found".dimmed(),
        benchmarks.len().to_string().green().bold()
    );
    println!();

    // Load configuration (needed for baseline comparisons)
    let config = BenchmarkConfig::load();

    // Step 6: Run benchmarks (results and comparisons printed inline)
    let (_results, comparisons) = if run_config.parallel {
        run_benchmarks_parallel(
            &runner_binary,
            &workspace_root,
            &benchmarks,
            &run_config,
            &config,
        )?
    } else {
        run_benchmarks_sequential(
            &runner_binary,
            &workspace_root,
            &benchmarks,
            &run_config,
            &config,
        )?
    };

    // Step 7: Print summary
    output::print_summary(&comparisons, &config.comparison);

    // Exit with error if CI mode and regressions detected
    if run_config.ci {
        let regression_count = comparisons.iter().filter(|c| c.is_regression).count();
        if regression_count > 0 {
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Discover benchmarks by running the runner with --list
fn discover_benchmarks(runner: &Path, workspace_root: &Path) -> Result<Vec<BenchmarkInfo>> {
    let output = Command::new(runner)
        .arg("--list")
        .env(
            "SIMPLEBENCH_WORKSPACE_ROOT",
            workspace_root.display().to_string(),
        )
        .output()
        .context("Failed to run runner --list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Runner --list failed: {}", stderr);
    }

    let benchmarks: Vec<BenchmarkInfo> =
        serde_json::from_slice(&output.stdout).context("Failed to parse benchmark list JSON")?;

    Ok(benchmarks)
}

/// Build environment variables for runner execution
fn build_runner_env(workspace_root: &Path, run_config: &RunConfig) -> HashMap<String, String> {
    let mut env = HashMap::new();

    env.insert(
        "SIMPLEBENCH_WORKSPACE_ROOT".to_string(),
        workspace_root.display().to_string(),
    );

    if run_config.ci {
        env.insert("SIMPLEBENCH_CI".to_string(), "1".to_string());
    }

    if let Some(threshold) = run_config.threshold {
        env.insert("SIMPLEBENCH_THRESHOLD".to_string(), threshold.to_string());
    }

    if let Some(samples) = run_config.samples {
        env.insert("SIMPLEBENCH_SAMPLES".to_string(), samples.to_string());
    }

    if let Some(warmup_duration) = run_config.warmup_duration {
        env.insert(
            "SIMPLEBENCH_WARMUP_DURATION".to_string(),
            warmup_duration.to_string(),
        );
    }

    if let Some(window) = run_config.window {
        env.insert("SIMPLEBENCH_WINDOW".to_string(), window.to_string());
    }

    if let Some(confidence) = run_config.confidence {
        env.insert("SIMPLEBENCH_CONFIDENCE".to_string(), confidence.to_string());
    }

    if let Some(cp_threshold) = run_config.cp_threshold {
        env.insert(
            "SIMPLEBENCH_CP_THRESHOLD".to_string(),
            cp_threshold.to_string(),
        );
    }

    if let Some(hazard_rate) = run_config.hazard_rate {
        env.insert(
            "SIMPLEBENCH_HAZARD_RATE".to_string(),
            hazard_rate.to_string(),
        );
    }

    if run_config.quiet {
        env.insert("SIMPLEBENCH_QUIET".to_string(), "1".to_string());
    }

    env
}

/// Run benchmarks sequentially (one at a time on core 1)
fn run_benchmarks_sequential(
    runner: &Path,
    workspace_root: &Path,
    benchmarks: &[BenchmarkInfo],
    run_config: &RunConfig,
    config: &BenchmarkConfig,
) -> Result<(Vec<BenchResult>, Vec<ComparisonResult>)> {
    let cores = vec![1]; // Sequential always uses core 1
    output::print_run_header(benchmarks.len(), 1, false);

    run_benchmarks_with_cores(
        runner,
        workspace_root,
        benchmarks,
        &cores,
        run_config,
        config,
    )
}

/// Run benchmarks in parallel (one per physical core)
fn run_benchmarks_parallel(
    runner: &Path,
    workspace_root: &Path,
    benchmarks: &[BenchmarkInfo],
    run_config: &RunConfig,
    config: &BenchmarkConfig,
) -> Result<(Vec<BenchResult>, Vec<ComparisonResult>)> {
    let mut cores = if let Some(jobs) = run_config.jobs {
        // User specified number of cores
        let available = topology::get_usable_cores();
        available.into_iter().take(jobs).collect()
    } else {
        // Auto-detect physical cores
        topology::get_usable_cores()
    };

    // Ensure we have at least one core
    if cores.is_empty() {
        cores = vec![1];
    }

    output::print_run_header(benchmarks.len(), cores.len(), true);

    run_benchmarks_with_cores(
        runner,
        workspace_root,
        benchmarks,
        &cores,
        run_config,
        config,
    )
}

/// Message types sent from benchmark runner threads
enum RunnerMessage {
    /// Progress update from stderr
    Progress(progress::ProgressMessage),
    /// Benchmark completed with result
    Complete {
        name: String,
        core: usize,
        result: Box<Result<BenchResult, String>>,
        stderr_lines: Vec<String>,
    },
}

/// Run benchmarks using specified cores, spawning one runner per benchmark
/// Returns both results and comparisons (printed inline as each benchmark completes)
fn run_benchmarks_with_cores(
    runner: &Path,
    workspace_root: &Path,
    benchmarks: &[BenchmarkInfo],
    cores: &[usize],
    run_config: &RunConfig,
    config: &BenchmarkConfig,
) -> Result<(Vec<BenchResult>, Vec<ComparisonResult>)> {
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc;

    let base_env = build_runner_env(workspace_root, run_config);
    let mut all_results = Vec::new();
    let mut all_comparisons = Vec::new();

    // Initialize baseline manager and progress display
    let baseline_manager = BaselineManager::new().ok();
    let mut progress_display = progress::BenchmarkProgress::new(run_config.quiet);

    // Process benchmarks in batches (batch size = number of cores)
    for batch in benchmarks.chunks(cores.len()) {
        let (tx, rx) = mpsc::channel::<RunnerMessage>();
        let mut pending = batch.len();

        // Spawn all runners in this batch simultaneously
        for (i, bench) in batch.iter().enumerate() {
            let core = cores[i];
            let bench_name = bench.name.clone();
            let tx = tx.clone();

            let mut child = Command::new(runner)
                .env("SIMPLEBENCH_SINGLE_BENCH", "1")
                .env("SIMPLEBENCH_BENCH_FILTER", &bench.name)
                .env("SIMPLEBENCH_PIN_CORE", core.to_string())
                .envs(&base_env)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Failed to spawn runner");

            let stderr = child.stderr.take().unwrap();
            let stdout = child.stdout.take().unwrap();

            // Thread to read stdout (runs concurrently to avoid pipe deadlock)
            let stdout_handle = std::thread::spawn(move || {
                let mut stdout_content = String::new();
                let _ =
                    std::io::Read::read_to_string(&mut BufReader::new(stdout), &mut stdout_content);
                stdout_content
            });

            // Thread to stream stderr and parse progress
            std::thread::spawn(move || {
                let mut stderr_lines = Vec::new();
                let stderr_reader = BufReader::new(stderr);

                // Stream stderr lines for progress updates
                for line in stderr_reader.lines().map_while(Result::ok) {
                    if let Ok(wrapper) = serde_json::from_str::<progress::ProgressWrapper>(&line) {
                        let _ = tx.send(RunnerMessage::Progress(wrapper.progress));
                    } else {
                        // Non-progress line (errors, warnings)
                        stderr_lines.push(line);
                    }
                }

                // Wait for stdout thread to complete
                let stdout_content = stdout_handle.join().unwrap_or_default();

                // Wait for child to complete
                let status = child.wait();

                let result = match status {
                    Ok(s) if s.success() => serde_json::from_str::<BenchResult>(&stdout_content)
                        .map_err(|e| {
                            format!("Failed to parse result: {}\nstdout: {}", e, stdout_content)
                        }),
                    Ok(s) => Err(format!("Benchmark failed with status: {}", s)),
                    Err(e) => Err(format!("Failed to wait for benchmark: {}", e)),
                };

                let _ = tx.send(RunnerMessage::Complete {
                    name: bench_name,
                    core,
                    result: Box::new(result),
                    stderr_lines,
                });
            });
        }
        drop(tx); // Close sender so rx.iter() will terminate

        // Process messages as they arrive
        while pending > 0 {
            match rx.recv() {
                Ok(RunnerMessage::Progress(msg)) => {
                    progress_display.update(&msg);
                }
                Ok(RunnerMessage::Complete {
                    name,
                    core,
                    result,
                    stderr_lines,
                }) => {
                    // Clear progress bar before printing result
                    progress_display.finish();

                    match *result {
                        Ok(bench_result) => {
                            // Print benchmark result
                            output::print_benchmark_result(&bench_result, core);

                            // Process baseline comparison immediately
                            let comparison = process_single_result_baseline(
                                &bench_result,
                                &baseline_manager,
                                config,
                            );
                            all_comparisons.push(comparison);

                            println!(); // Blank line after each benchmark + comparison
                            all_results.push(bench_result);
                        }
                        Err(e) => {
                            eprintln!("{} Failed benchmark {}: {}", "ERROR".red().bold(), name, e);
                            // Print any non-progress stderr lines
                            for line in stderr_lines {
                                eprintln!("  {}", line);
                            }
                        }
                    }
                    pending -= 1;
                }
                Err(_) => break, // Channel closed
            }
        }
    }

    Ok((all_results, all_comparisons))
}

/// Process a single benchmark result against baselines and print comparison
fn process_single_result_baseline(
    result: &BenchResult,
    baseline_manager: &Option<BaselineManager>,
    config: &BenchmarkConfig,
) -> ComparisonResult {
    let crate_name = result.module.split("::").next().unwrap_or("unknown");

    if let Some(ref bm) = baseline_manager {
        // Load recent baselines for window-based comparison
        if let Ok(historical) =
            bm.load_recent_baselines(crate_name, &result.name, config.comparison.window_size)
        {
            if !historical.is_empty() {
                // Use CPD-based comparison
                let comp_result = simplebench_runtime::baseline::detect_regression_with_cpd(
                    result,
                    &historical,
                    config.comparison.threshold,
                    config.comparison.confidence_level,
                    config.comparison.cp_threshold,
                    config.comparison.hazard_rate,
                );

                // Print comparison immediately after benchmark result
                if let Some(ref comparison) = comp_result.comparison {
                    output::print_comparison(comparison, &result.name, comp_result.is_regression);
                }

                // Save baseline
                if let Err(e) = bm.save_baseline(crate_name, result, comp_result.is_regression) {
                    eprintln!(
                        "Warning: Failed to save baseline for {}: {}",
                        result.name, e
                    );
                }

                return comp_result;
            }
        }

        // First run - no baseline
        output::print_new_baseline(&result.name);

        if let Err(e) = bm.save_baseline(crate_name, result, false) {
            eprintln!(
                "Warning: Failed to save baseline for {}: {}",
                result.name, e
            );
        }
    }

    ComparisonResult {
        benchmark_name: result.name.clone(),
        comparison: None,
        is_regression: false,
    }
}
