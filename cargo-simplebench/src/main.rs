mod metadata;
mod rlib_selection;
mod runner_gen;
mod compile;

use anyhow::{Context, Result};
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<()> {
    // Handle cargo invocation: `cargo simplebench` passes "simplebench" as first arg
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1] == "simplebench" {
        // Called as `cargo simplebench` - skip the "simplebench" argument
    }

    // Determine workspace root (current directory)
    let workspace_root = env::current_dir()
        .context("Failed to get current directory")?;

    println!("SimpleBench - Workspace Benchmark Runner");
    println!("=========================================\n");

    // Step 1: Analyze workspace
    println!("Step 1: Analyzing workspace...");
    let workspace_info = metadata::analyze_workspace(&workspace_root)
        .context("Failed to analyze workspace")?;

    if workspace_info.benchmark_crates.is_empty() {
        eprintln!("ERROR: No benchmark crates found!");
        eprintln!("Benchmark crates must depend on simplebench-runtime");
        std::process::exit(1);
    }

    println!("  Found {} benchmark crates:", workspace_info.benchmark_crates.len());
    for crate_info in &workspace_info.benchmark_crates {
        println!("    - {}", crate_info.name);
    }
    println!();

    // Step 2: Build workspace and select rlibs
    println!("Step 2: Building workspace with --release...");
    let profile = "release";
    let rlibs = rlib_selection::select_rlibs(&workspace_root, profile)
        .context("Failed to select rlibs")?;

    println!("  Selected {} rlib files (opt-level=3)", rlibs.len());
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
    println!("Step 3: Generating runner...");
    let runner_path = runner_gen::write_runner(
        &workspace_info.target_directory,
        &workspace_info.benchmark_crates,
    )
    .context("Failed to write runner")?;
    println!("  Generated: {}", runner_path.display());
    println!();

    // Step 4: Compile runner
    println!("Step 4: Compiling runner...");
    let runner_binary = workspace_info.target_directory.join("simplebench_runner");
    let deps_dir = workspace_info.target_directory.join(profile).join("deps");

    compile::compile_runner(&runner_path, &runner_binary, &rlibs, &deps_dir)
        .context("Failed to compile runner")?;
    println!("  Compiled: {}", runner_binary.display());
    println!();

    // Step 5: Run benchmarks
    println!("Step 5: Running benchmarks...");
    println!("═══════════════════════════════════════\n");

    let output = Command::new(&runner_binary)
        .output()
        .context("Failed to execute runner")?;

    // Print stdout and stderr
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));

    if !output.status.success() {
        anyhow::bail!("Runner execution failed");
    }

    println!("\n═══════════════════════════════════════");
    println!("SimpleBench completed successfully!");

    Ok(())
}
