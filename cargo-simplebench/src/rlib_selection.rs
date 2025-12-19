use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Build workspace crates with dev-dependencies and cfg(test) enabled
///
/// Uses a hybrid approach:
/// 1. `cargo test -p <crate> --release --no-run` to build ALL dependencies including dev-deps
/// 2. Parse JSON output to collect rlib paths (opt_level=3 for runtime, opt_level=0 for proc-macros)
/// 3. Manually invoke rustc with all --extern flags to produce rlibs with --cfg test
///
/// This enables idiomatic `[dev-dependencies]` usage for simplebench-* crates.
///
/// IMPORTANT: Tracks the simplebench_runtime version from the first benchmark crate and ensures
/// all subsequent crates use the same version. This is critical because `inventory` uses static
/// globals with compilation-specific hashes - all crates must link against the exact same version.
pub fn build_and_select_rlibs(
    workspace_root: &Path,
    benchmark_crates: &[String],
    target_dir: &Path,
) -> Result<HashMap<String, PathBuf>> {
    let mut all_rlibs: HashMap<String, PathBuf> = HashMap::new();
    // Track canonical versions of critical dependencies to ensure consistency across all benchmark crates.
    // The inventory crate uses static REGISTRY globals with compilation-specific hashes, so all
    // crates must link against the exact same versions of simplebench_runtime and inventory.
    let mut canonical_critical_deps: HashMap<String, PathBuf> = HashMap::new();

    for crate_name in benchmark_crates {
        println!("     Building {} with dev-dependencies", crate_name);

        // Step 1: Build dev-deps via cargo test --no-run
        let output = Command::new("cargo")
            .args([
                "test",
                "-p",
                crate_name,
                "--release",
                "--no-run",
                "--message-format=json",
                "--target-dir",
            ])
            .arg(target_dir)
            .current_dir(workspace_root)
            .output()
            .context(format!(
                "Failed to execute cargo test --no-run for {}",
                crate_name
            ))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to build dev-deps for {}: {}", crate_name, stderr);
        }

        // Step 2: Parse JSON to collect rlib paths
        let (crate_rlibs, crate_critical_deps) = parse_cargo_json(&output.stdout, crate_name)?;

        // Track/verify consistent critical dependency versions across all benchmark crates
        for (dep_name, dep_path) in &crate_critical_deps {
            match canonical_critical_deps.get(dep_name) {
                None => {
                    // First crate sets the canonical version
                    canonical_critical_deps.insert(dep_name.clone(), dep_path.clone());
                }
                Some(canonical) if canonical != dep_path => {
                    // Different version detected - warn but use canonical for consistency
                    eprintln!(
                        "     Warning: Different {} versions detected for {}",
                        dep_name, crate_name
                    );
                    eprintln!("       Expected: {}", canonical.display());
                    eprintln!("       Found:    {}", dep_path.display());
                    eprintln!("       Using canonical version for consistency");
                }
                _ => {} // Same version, all good
            }
        }

        // Step 3: Get source path and manifest dir for this crate
        let (src_path, manifest_dir) = get_crate_paths(workspace_root, crate_name)?;

        // Step 4: Manually invoke rustc to produce rlib with --cfg test
        // CRITICAL: Use canonical versions of critical deps for this crate's compilation
        // to ensure all benchmark crates link against the same inventory/simplebench_runtime
        let mut rlibs_for_compile = crate_rlibs.clone();
        for (dep_name, dep_path) in &canonical_critical_deps {
            rlibs_for_compile.insert(dep_name.clone(), dep_path.clone());
        }
        let extern_args = build_extern_args(&rlibs_for_compile);
        let normalized_name = crate_name.replace('-', "_");

        let out_dir = target_dir.join("release").join("deps");

        let mut cmd = Command::new("rustc");
        cmd.args(["--edition", "2021"])
            .arg(&src_path)
            .args(["--crate-name", &normalized_name])
            .args(["--crate-type", "rlib"])
            .args(["-C", "opt-level=3"])
            .arg("--cfg")
            .arg("test")
            .arg("-L")
            .arg(format!("dependency={}", out_dir.display()))
            .args(&extern_args)
            .arg("--out-dir")
            .arg(&out_dir)
            .env("CARGO_MANIFEST_DIR", &manifest_dir)
            .current_dir(workspace_root);

        let rustc_output = cmd
            .output()
            .context(format!("Failed to execute rustc for {}", crate_name))?;

        if !rustc_output.status.success() {
            let stderr = String::from_utf8_lossy(&rustc_output.stderr);
            anyhow::bail!("rustc failed for {}: {}", crate_name, stderr);
        }

        // Find the rlib we just created
        let rlib_path = find_crate_rlib(&out_dir, &normalized_name)?;

        // Merge crate_rlibs into all_rlibs, but DON'T overwrite:
        // - previously manually-built benchmark crates (they have --cfg test enabled)
        // - canonical critical deps (simplebench_runtime, inventory)
        for (name, path) in crate_rlibs {
            // Skip if this is a benchmark crate we've already manually built
            let is_already_built_benchmark = benchmark_crates
                .iter()
                .any(|bc| bc.replace('-', "_") == name && all_rlibs.contains_key(&name));

            // Skip if this is a critical dep (we'll add canonical versions at the end)
            let is_critical_dep = CRITICAL_DEPS.contains(&name.as_str());

            if !is_already_built_benchmark && !is_critical_dep {
                all_rlibs.insert(name, path);
            }
        }

        // Insert our manually-built benchmark crate rlib
        all_rlibs.insert(normalized_name, rlib_path);
    }

    // Ensure all canonical critical dependencies are in the final map
    // This overrides any versions that may have been inserted from later crate builds
    for (dep_name, dep_path) in &canonical_critical_deps {
        all_rlibs.insert(dep_name.clone(), dep_path.clone());
    }

    Ok(all_rlibs)
}

/// Cargo JSON artifact structure
#[derive(Deserialize, Debug)]
struct CargoArtifact {
    reason: String,
    target: CargoTarget,
    profile: CargoProfile,
    filenames: Vec<PathBuf>,
}

#[derive(Deserialize, Debug)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct CargoProfile {
    opt_level: serde_json::Value, // Can be string "0"/"3" or number
}

impl CargoProfile {
    fn opt_level_int(&self) -> u32 {
        match &self.opt_level {
            serde_json::Value::String(s) => s.parse().unwrap_or(0),
            serde_json::Value::Number(n) => n.as_u64().unwrap_or(0) as u32,
            _ => 0,
        }
    }
}

/// Critical dependencies that must be consistent across all benchmark crates.
/// The inventory crate uses static REGISTRY globals with compilation-specific hashes,
/// so all crates must link against the exact same versions of these dependencies.
const CRITICAL_DEPS: &[&str] = &["simplebench_runtime", "inventory"];

/// Parse cargo --message-format=json output to collect rlib/so paths
///
/// Returns a tuple of (all_rlibs, critical_deps) where:
/// - all_rlibs: HashMap of crate_name -> rlib_path for all dependencies
/// - critical_deps: HashMap of critical dep names -> paths for version tracking
fn parse_cargo_json(
    stdout: &[u8],
    exclude_crate: &str,
) -> Result<(HashMap<String, PathBuf>, HashMap<String, PathBuf>)> {
    let mut rlibs: HashMap<String, PathBuf> = HashMap::new();
    let mut critical_deps: HashMap<String, PathBuf> = HashMap::new();
    let exclude_normalized = exclude_crate.replace('-', "_");

    for line in stdout.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        // Try to parse as artifact
        let artifact: CargoArtifact = match serde_json::from_str(&line) {
            Ok(a) => a,
            Err(_) => continue, // Skip non-artifact lines
        };

        if artifact.reason != "compiler-artifact" {
            continue;
        }

        // Skip custom-build targets
        if artifact.target.kind.iter().any(|k| k == "custom-build") {
            continue;
        }

        // Skip the crate we're building (we'll build it ourselves)
        let target_name = artifact.target.name.replace('-', "_");
        if target_name == exclude_normalized {
            continue;
        }

        // Check if this is a lib/rlib (opt_level=3) or proc-macro (opt_level=0)
        let is_runtime_lib = artifact.profile.opt_level_int() == 3
            && artifact
                .target
                .kind
                .iter()
                .any(|k| k == "lib" || k == "rlib");

        let is_proc_macro = artifact.target.kind.iter().any(|k| k == "proc-macro");

        if !is_runtime_lib && !is_proc_macro {
            continue;
        }

        // Find rlib or so file
        for filename in &artifact.filenames {
            let ext = filename.extension().and_then(|e| e.to_str());
            if ext == Some("rlib") || ext == Some("so") {
                // Track critical dependencies separately for version consistency
                if CRITICAL_DEPS.contains(&target_name.as_str()) {
                    critical_deps.insert(target_name.clone(), filename.clone());
                }
                rlibs.insert(target_name.clone(), filename.clone());
                break;
            }
        }
    }

    Ok((rlibs, critical_deps))
}

/// Build --extern arguments from collected rlibs
fn build_extern_args(rlibs: &HashMap<String, PathBuf>) -> Vec<String> {
    rlibs
        .iter()
        .flat_map(|(name, path)| {
            vec![
                "--extern".to_string(),
                format!("{}={}", name, path.display()),
            ]
        })
        .collect()
}

/// Get source path and manifest directory for a crate from cargo metadata
fn get_crate_paths(workspace_root: &Path, crate_name: &str) -> Result<(PathBuf, PathBuf)> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version=1", "--no-deps"])
        .current_dir(workspace_root)
        .output()
        .context("Failed to execute cargo metadata")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo metadata failed: {}", stderr);
    }

    #[derive(Deserialize)]
    struct Metadata {
        packages: Vec<Package>,
    }

    #[derive(Deserialize)]
    struct Package {
        name: String,
        manifest_path: PathBuf,
        targets: Vec<Target>,
    }

    #[derive(Deserialize)]
    struct Target {
        kind: Vec<String>,
        src_path: PathBuf,
    }

    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).context("Failed to parse cargo metadata")?;

    for package in &metadata.packages {
        if package.name == crate_name {
            for target in &package.targets {
                if target.kind.iter().any(|k| k == "lib" || k == "rlib") {
                    // manifest_path is the Cargo.toml path, we need its parent directory
                    let manifest_dir = package
                        .manifest_path
                        .parent()
                        .ok_or_else(|| {
                            anyhow::anyhow!("Could not get parent dir of manifest path")
                        })?
                        .to_path_buf();
                    return Ok((target.src_path.clone(), manifest_dir));
                }
            }
        }
    }

    anyhow::bail!("Could not find lib source path for crate: {}", crate_name)
}

/// Find the rlib file for a crate in deps directory
/// Looks for both lib<crate>.rlib (manual rustc) and lib<crate>-<hash>.rlib (cargo)
fn find_crate_rlib(deps_dir: &Path, crate_name: &str) -> Result<PathBuf> {
    // First check for exact match (manual rustc output has no hash)
    let exact_path = deps_dir.join(format!("lib{}.rlib", crate_name));
    if exact_path.exists() {
        return Ok(exact_path);
    }

    // Fall back to looking for hash-suffixed rlibs
    let prefix = format!("lib{}-", crate_name);

    for entry in
        fs::read_dir(deps_dir).context(format!("Failed to read deps directory: {:?}", deps_dir))?
    {
        let entry = entry?;
        let path = entry.path();

        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if filename.starts_with(&prefix) && filename.ends_with(".rlib") {
                return Ok(path);
            }
        }
    }

    anyhow::bail!("Could not find rlib for crate: {}", crate_name)
}
