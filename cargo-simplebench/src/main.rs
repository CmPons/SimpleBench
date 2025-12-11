mod analyze;
mod compile;
mod metadata;
mod rlib_selection;
mod runner_gen;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::*;
use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Configuration for running benchmarks
struct RunConfig {
    bench_filter: Option<String>,
    samples: Option<usize>,
    iterations: Option<usize>,
    warmup_duration: Option<u64>,
    threshold: Option<f64>,
    ci: bool,
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

        /// Number of iterations per sample
        #[arg(long)]
        iterations: Option<usize>,

        /// Warmup duration in seconds (default: 3)
        #[arg(long)]
        warmup_duration: Option<u64>,

        /// Regression threshold percentage (default: 5.0)
        #[arg(long)]
        threshold: Option<f64>,

        /// Enable CI mode: fail on performance regressions
        #[arg(long)]
        ci: bool,
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
            iterations,
            warmup_duration,
            threshold,
            ci,
        }) => {
            // Explicit run command
            RunConfig {
                bench_filter: bench,
                samples,
                iterations,
                warmup_duration,
                threshold,
                ci,
            }
        }
        None => {
            // No subcommand - default to running all benchmarks
            RunConfig {
                bench_filter: None,
                samples: None,
                iterations: None,
                warmup_duration: None,
                threshold: None,
                ci: false,
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

    // Step 2: Build workspace and select rlibs
    println!("{}", "Compiling workspace (release profile)".green().bold());
    let profile = "release";
    let rlibs =
        rlib_selection::select_rlibs(&workspace_root, profile).context("Failed to select rlibs")?;

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
    let deps_dir = workspace_info.target_directory.join(profile).join("deps");

    compile::compile_runner(&runner_path, &runner_binary, &rlibs, &deps_dir)
        .context("Failed to compile runner")?;

    // Step 5: Run benchmarks
    println!();

    let mut cmd = Command::new(&runner_binary);
    cmd.env("CLICOLOR_FORCE", "1");

    // Pass workspace root for baseline storage
    cmd.env(
        "SIMPLEBENCH_WORKSPACE_ROOT",
        workspace_root.display().to_string(),
    );

    // Pass CLI overrides as environment variables
    if run_config.ci {
        cmd.env("SIMPLEBENCH_CI", "1");
    }

    if let Some(threshold) = run_config.threshold {
        cmd.env("SIMPLEBENCH_THRESHOLD", threshold.to_string());
    }

    if let Some(samples) = run_config.samples {
        cmd.env("SIMPLEBENCH_SAMPLES", samples.to_string());
    }

    if let Some(iterations) = run_config.iterations {
        cmd.env("SIMPLEBENCH_ITERATIONS", iterations.to_string());
    }

    if let Some(warmup_duration) = run_config.warmup_duration {
        cmd.env("SIMPLEBENCH_WARMUP_DURATION", warmup_duration.to_string());
    }

    // Pass benchmark filter to runner
    if let Some(filter) = run_config.bench_filter {
        cmd.env("SIMPLEBENCH_BENCH_FILTER", filter);
    }

    let status = cmd.status().context("Failed to execute runner")?;

    if !status.success() {
        std::process::exit(1);
    }

    Ok(())
}
