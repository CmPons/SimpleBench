use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;


/// Build workspace crates with cfg(test) enabled and select rlibs for linking
///
/// Uses `cargo rustc -p <crate> -- --cfg test` to pass the flag only to workspace crates,
/// not to external dependencies. This enables idiomatic `#[cfg(test)]` conditional compilation.
///
/// Uses an isolated target directory (target/simplebench) to avoid cache conflicts.
pub fn build_and_select_rlibs(
    workspace_root: &Path,
    benchmark_crates: &[String],
    target_dir: &Path,
) -> Result<HashMap<String, PathBuf>> {
    // Build each benchmark crate with --cfg test (dependencies build normally)
    for crate_name in benchmark_crates {
        println!("     Building {} with cfg(test)", crate_name);

        let output = Command::new("cargo")
            .arg("rustc")
            .arg("-p")
            .arg(crate_name)
            .arg("--release")
            .arg("--target-dir")
            .arg(target_dir)
            .arg("--")
            .arg("--cfg")
            .arg("test")
            .current_dir(workspace_root)
            .output()
            .context(format!("Failed to execute cargo rustc for {}", crate_name))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to build {}: {}", crate_name, stderr);
        }
    }

    // Now select rlibs from the isolated target directory
    let deps_dir = target_dir.join("release").join("deps");
    select_rlibs_by_size(&deps_dir)
}

/// Fallback approach: Select rlibs by file size (smaller = opt-level=3)
fn select_rlibs_by_size(deps_dir: &Path) -> Result<HashMap<String, PathBuf>> {
    let mut rlib_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();

    // Group rlibs by crate name (before the hash)
    for entry in fs::read_dir(deps_dir)
        .context(format!("Failed to read deps directory: {:?}", deps_dir))?
    {
        let entry = entry?;
        let path = entry.path();

        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if filename.ends_with(".rlib") {
                // Extract crate name: libcrate_name-hash.rlib -> crate_name
                if let Some(name_part) = filename.strip_prefix("lib") {
                    if let Some(crate_name) = name_part.split('-').next() {
                        rlib_groups
                            .entry(crate_name.to_string())
                            .or_default()
                            .push(path);
                    }
                }
            }
        }
    }

    // Select the SMALLEST rlib for each crate (opt-level=3 is smaller)
    let mut selected = HashMap::new();
    for (crate_name, rlibs) in rlib_groups {
        let smallest = rlibs
            .iter()
            .min_by_key(|path| fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX))
            .context(format!("No rlibs found for {}", crate_name))?;

        selected.insert(crate_name, smallest.clone());
    }

    Ok(selected)
}
