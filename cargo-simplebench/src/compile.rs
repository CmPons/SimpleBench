use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Compile the runner using rustc with manual linking
///
/// This function invokes rustc directly with --extern flags for each dependency
pub fn compile_runner(
    runner_source: &Path,
    output_binary: &Path,
    rlibs: &HashMap<String, PathBuf>,
    deps_dir: &Path,
) -> Result<()> {
    let mut cmd = Command::new("rustc");

    // Basic compilation flags
    cmd.arg(runner_source)
        .arg("--edition")
        .arg("2021")
        .arg("-o")
        .arg(output_binary);

    // Set optimization level (can be 0, runner optimization doesn't matter)
    cmd.arg("-C").arg("opt-level=0");

    // Add dependency search path
    cmd.arg("-L").arg(format!("dependency={}", deps_dir.display()));

    // Add --extern flags for each rlib
    for (crate_name, rlib_path) in rlibs {
        cmd.arg("--extern")
            .arg(format!("{}={}", crate_name, rlib_path.display()));
    }

    eprintln!("Compiling runner with rustc...");
    eprintln!("  Source: {}", runner_source.display());
    eprintln!("  Output: {}", output_binary.display());
    eprintln!("  Linked crates: {}", rlibs.len());

    let output = cmd
        .output()
        .context("Failed to execute rustc")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        eprintln!("rustc stdout: {}", stdout);
        eprintln!("rustc stderr: {}", stderr);
        anyhow::bail!("rustc compilation failed");
    }

    Ok(())
}
