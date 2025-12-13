use anyhow::{Context, Result};
use cargo_metadata::{DependencyKind, MetadataCommand, Package};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    pub target_directory: PathBuf,
    pub benchmark_crates: Vec<BenchmarkCrate>,
}

#[derive(Debug, Clone)]
pub struct BenchmarkCrate {
    pub name: String,
}

/// Parse workspace metadata and identify benchmark crates
pub fn analyze_workspace(workspace_root: &Path) -> Result<WorkspaceInfo> {
    let metadata = MetadataCommand::new()
        .current_dir(workspace_root)
        .exec()
        .context("Failed to execute cargo metadata")?;

    let target_directory = metadata.target_directory.clone().into_std_path_buf();

    // Find all workspace member packages
    let workspace_member_ids: HashSet<_> = metadata.workspace_members.iter().collect();

    let mut benchmark_crates = Vec::new();

    for package in &metadata.packages {
        // Only consider workspace members
        if !workspace_member_ids.contains(&package.id) {
            continue;
        }

        // Check if this package depends on simplebench-runtime and has a lib target
        if depends_on_simplebench_runtime(package) && has_lib_target(package) {
            benchmark_crates.push(BenchmarkCrate {
                name: package.name.clone(),
            });
        }
    }

    Ok(WorkspaceInfo {
        target_directory,
        benchmark_crates,
    })
}

/// Check if a package depends on simplebench-runtime (as regular or dev dependency)
fn depends_on_simplebench_runtime(package: &Package) -> bool {
    package.dependencies.iter().any(|dep| {
        let name_matches = dep.name == "simplebench-runtime" || dep.name == "simplebench_runtime";
        // Accept both regular dependencies and dev-dependencies
        let kind_ok = matches!(dep.kind, DependencyKind::Normal | DependencyKind::Development);
        name_matches && kind_ok
    })
}

/// Check if a package has a library target (rlib/lib)
fn has_lib_target(package: &Package) -> bool {
    package.targets.iter().any(|target| {
        target.kind.iter().any(|kind| {
            kind == "lib" || kind == "rlib"
        })
    })
}
