mod compile;
mod metadata;
mod rlib_selection;
mod runner_gen;

use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use std::env;
use std::path::PathBuf;
use std::process::Command;

/// SimpleBench - Simple microbenchmarking for Rust
#[derive(Parser, Debug)]
#[command(name = "cargo-simplebench")]
#[command(bin_name = "cargo simplebench")]
#[command(version, about, long_about = None)]
struct Args {
    /// Enable CI mode: fail on performance regressions
    #[arg(long)]
    ci: bool,

    /// Regression threshold percentage (default: 5.0)
    #[arg(long)]
    threshold: Option<f64>,

    /// Number of timing samples per benchmark (default: 200)
    #[arg(long)]
    samples: Option<usize>,

    /// Number of iterations per sample (default: auto-scale)
    #[arg(long)]
    iterations: Option<usize>,

    /// Number of warmup iterations (default: 50)
    #[arg(long)]
    warmup_iterations: Option<usize>,

    /// Target sample duration in milliseconds for auto-scaling (default: 10)
    #[arg(long)]
    target_duration_ms: Option<u64>,

    /// Workspace root directory (default: current directory)
    #[arg(long)]
    workspace_root: Option<PathBuf>,
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
        .unwrap_or_else(|| env::current_dir().expect("Failed to get current directory"));

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
    if cli_args.ci {
        cmd.env("SIMPLEBENCH_CI", "1");
    }

    if let Some(threshold) = cli_args.threshold {
        cmd.env("SIMPLEBENCH_THRESHOLD", threshold.to_string());
    }

    if let Some(samples) = cli_args.samples {
        cmd.env("SIMPLEBENCH_SAMPLES", samples.to_string());
    }

    if let Some(iterations) = cli_args.iterations {
        cmd.env("SIMPLEBENCH_ITERATIONS", iterations.to_string());
    }

    if let Some(warmup) = cli_args.warmup_iterations {
        cmd.env("SIMPLEBENCH_WARMUP_ITERATIONS", warmup.to_string());
    }

    if let Some(duration) = cli_args.target_duration_ms {
        cmd.env("SIMPLEBENCH_TARGET_DURATION_MS", duration.to_string());
    }

    let status = cmd.status().context("Failed to execute runner")?;

    if !status.success() {
        std::process::exit(1);
    }

    Ok(())
}
