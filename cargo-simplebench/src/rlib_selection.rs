use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct RlibInfo {
    pub path: PathBuf,
    pub opt_level: String,
}

/// Select the correct rlib files for linking the runner
///
/// Uses cargo build --message-format=json to identify which rlibs have opt-level=3
/// Falls back to file size heuristic if JSON parsing fails
pub fn select_rlibs(workspace_root: &Path, profile: &str) -> Result<HashMap<String, PathBuf>> {
    // Try JSON parsing approach first (preferred)
    match select_rlibs_json(workspace_root, profile) {
        Ok(rlibs) => {
            eprintln!("Selected rlibs using JSON parsing (opt-level=3)");
            Ok(rlibs)
        }
        Err(e) => {
            eprintln!("JSON parsing failed ({}), falling back to file size heuristic", e);
            // Fallback to file size heuristic
            let target_dir = workspace_root.join("target").join(profile).join("deps");
            select_rlibs_by_size(&target_dir)
        }
    }
}

#[derive(Debug, Deserialize)]
struct CargoMessage {
    reason: String,
    #[serde(default)]
    target: Option<Target>,
    #[serde(default)]
    profile: Option<Profile>,
    #[serde(default)]
    filenames: Vec<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct Target {
    name: String,
}

#[derive(Debug, Deserialize)]
struct Profile {
    opt_level: String,
}

/// Primary approach: Parse cargo build JSON output to select opt-level=3 rlibs
fn select_rlibs_json(workspace_root: &Path, profile: &str) -> Result<HashMap<String, PathBuf>> {
    // Build workspace with JSON output
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--message-format=json")
        .current_dir(workspace_root);

    if profile == "release" {
        cmd.arg("--release");
    }

    let output = cmd
        .output()
        .context("Failed to execute cargo build")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Cargo build failed: {}", stderr);
    }

    // Parse JSON output line by line
    let stdout = String::from_utf8(output.stdout)
        .context("Cargo output is not valid UTF-8")?;

    let mut rlib_versions: HashMap<String, Vec<RlibInfo>> = HashMap::new();

    for line in stdout.lines() {
        let msg: CargoMessage = match serde_json::from_str(line) {
            Ok(m) => m,
            Err(_) => continue, // Skip non-JSON lines
        };

        if msg.reason != "compiler-artifact" {
            continue;
        }

        let target = match msg.target {
            Some(t) => t,
            None => continue,
        };

        let profile = match msg.profile {
            Some(p) => p,
            None => continue,
        };

        // Find .rlib file in filenames
        if let Some(rlib_path) = msg.filenames.iter().find(|f| {
            f.extension().and_then(|e| e.to_str()) == Some("rlib")
        }) {
            let crate_name = target.name.replace('-', "_");

            rlib_versions
                .entry(crate_name)
                .or_default()
                .push(RlibInfo {
                    path: rlib_path.clone(),
                    opt_level: profile.opt_level,
                });
        }
    }

    // Select opt-level=3 versions (or highest available)
    let mut selected_rlibs = HashMap::new();

    for (crate_name, versions) in rlib_versions {
        let selected = if versions.len() == 1 {
            // Only one version exists
            &versions[0]
        } else {
            // Multiple versions - select opt-level=3 (target build)
            versions
                .iter()
                .find(|v| v.opt_level == "3")
                .or_else(|| {
                    // Fallback: highest opt-level
                    versions
                        .iter()
                        .max_by_key(|v| v.opt_level.parse::<u8>().unwrap_or(0))
                })
                .context("No rlib versions found")?
        };

        selected_rlibs.insert(crate_name, selected.path.clone());
    }

    Ok(selected_rlibs)
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
