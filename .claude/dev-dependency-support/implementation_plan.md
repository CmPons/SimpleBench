# Plan: True Dev-Dependency Support for SimpleBench

## Status: VALIDATED - Ready for Implementation

## Goal
Enable simplebench-runtime and simplebench-macros to be declared as `[dev-dependencies]`, matching idiomatic Rust patterns where test/benchmark code uses dev-deps.

## Problem
Cargo only resolves dev-dependencies when building test targets (`--test` flag), but `--test` produces a **binary**, not an rlib. SimpleBench needs rlibs to link into its unified runner.

## Solution: Hybrid Cargo + Manual Rustc

Use cargo to build dependencies, then manually invoke rustc to produce rlibs with `--cfg test`.

### Validated Workflow

```
1. cargo test -p <crate> --release --no-run --message-format=json
   └── Builds ALL dependencies including dev-deps
   └── Outputs JSON with artifact paths

2. Parse JSON → collect rlib paths
   └── opt_level=3 for runtime libs
   └── opt_level=0 for proc-macros

3. rustc --crate-type rlib --cfg test --extern dep1=path1 --extern dep2=path2 ...
   └── Produces rlib WITH benchmark code
   └── Dev-deps (like rand) are available

4. Link rlibs into runner as before
```

### Proof of Concept Results

```bash
# Cargo test builds rand and 42 other deps
cargo test -p game-math --release --no-run

# Manual rustc with ALL externs produces working rlib
rustc --crate-type rlib --cfg test -L deps --extern rand=... --extern simplebench_*=...
# Result: 49KB rlib with benchmark symbols

# Runner finds all benchmarks (including ones using rand)
./test_runner
# Found 3 benchmarks:
#   - game_math::benchmarks::bench_matrix_transform_batch
#   - game_math::benchmarks::bench_vec3_cross_product
#   - game_math::benchmarks::bench_vec3_normalize
```

## Implementation Changes

### 1. rlib_selection.rs - Major Rewrite

Replace current `build_and_select_rlibs()` with new workflow:

```rust
pub fn build_and_select_rlibs(
    workspace_root: &Path,
    benchmark_crates: &[String],
    target_dir: &Path,
) -> Result<HashMap<String, PathBuf>> {

    // Step 1: Build dev-deps via cargo test
    for crate_name in benchmark_crates {
        let output = Command::new("cargo")
            .args(["test", "-p", crate_name, "--release", "--no-run",
                   "--message-format=json", "--target-dir"])
            .arg(target_dir)
            .current_dir(workspace_root)
            .output()?;

        // Parse JSON to collect ALL rlib paths
        let all_rlibs = parse_cargo_json(&output.stdout)?;

        // Step 2: Manual rustc for benchmark crate
        let extern_args = build_extern_args(&all_rlibs);
        let src_path = get_crate_src_path(workspace_root, crate_name)?;

        Command::new("rustc")
            .args(["--edition", "2021"])
            .arg(&src_path)
            .args(["--crate-name", &crate_name.replace('-', "_")])
            .args(["--crate-type", "rlib"])
            .args(["-C", "opt-level=3"])
            .arg("--cfg").arg("test")
            .arg("-L").arg(format!("dependency={}/release/deps", target_dir.display()))
            .args(extern_args)
            .args(["--out-dir", &format!("{}/release/deps", target_dir.display())])
            .output()?;
    }

    // Return paths to our manually-built rlibs
    select_benchmark_rlibs(target_dir, benchmark_crates)
}

fn parse_cargo_json(stdout: &[u8]) -> Result<HashMap<String, PathBuf>> {
    // Parse JSON lines
    // For each compiler-artifact:
    //   - If opt_level=3 and kind=lib/rlib → use this path
    //   - If kind=proc-macro → use this path (always opt_level=0)
    // Return map of crate_name → rlib_path
}

fn build_extern_args(rlibs: &HashMap<String, PathBuf>) -> Vec<String> {
    rlibs.iter()
        .flat_map(|(name, path)| vec!["--extern".to_string(), format!("{}={}", name, path.display())])
        .collect()
}
```

### 2. metadata.rs - Get Source Paths

Add function to get crate source paths from cargo metadata:

```rust
pub fn get_crate_src_path(workspace_root: &Path, crate_name: &str) -> Result<PathBuf> {
    // Parse cargo metadata to find the crate's lib.rs path
    // Return PathBuf to src/lib.rs (or custom path from Cargo.toml)
}
```

### 3. test-workspace Cargo.toml Updates

Change all benchmark crates to use dev-dependencies:

```toml
# game-math/Cargo.toml
[dev-dependencies]
simplebench-runtime = { path = "../../simplebench-runtime" }
simplebench-macros = { path = "../../simplebench-macros" }
# Users can add other dev-deps like rand
```

### 4. Update CLAUDE.md

Document the new dev-dependency support and how it works.

## Key Implementation Details

### JSON Parsing Logic

```rust
#[derive(Deserialize)]
struct CargoArtifact {
    reason: String,
    target: Target,
    profile: Profile,
    filenames: Vec<PathBuf>,
}

#[derive(Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
}

#[derive(Deserialize)]
struct Profile {
    opt_level: String,  // "0" or "3"
}

fn is_runtime_lib(artifact: &CargoArtifact) -> bool {
    artifact.profile.opt_level == "3" &&
    artifact.target.kind.iter().any(|k| k == "lib" || k == "rlib")
}

fn is_proc_macro(artifact: &CargoArtifact) -> bool {
    artifact.target.kind.iter().any(|k| k == "proc-macro")
}
```

### Extern Flag Generation

Must handle crate name normalization (hyphens → underscores):

```rust
fn normalize_crate_name(name: &str) -> String {
    name.replace('-', "_")
}

// --extern simplebench_runtime=/path/to/libsimplebench_runtime-xxx.rlib
```

### Edge Cases

1. **Multiple rlib versions**: Cargo may emit multiple artifacts for same crate (host vs target). Select opt_level=3 for runtime, any for proc-macro.

2. **Crate name vs package name**: Use `target.name` from JSON (already normalized).

3. **Source path discovery**: Parse cargo metadata for `targets[].src_path` where `kind=["lib"]`.

## Benefits

1. **Idiomatic Rust**: `[dev-dependencies]` is the standard pattern
2. **Shared test helpers**: Test modules using dev-deps work in benchmarks
3. **Zero production overhead**: Benchmark deps completely absent from production builds
4. **Full compatibility**: Works with any dev-dependency users might have

## Testing Plan

1. Update game-math to use rand in test helper
2. Benchmarks call test helper (transitive rand usage)
3. Verify `cargo build --release` produces clean rlib (no rand/simplebench symbols)
4. Verify `cargo simplebench` finds and runs all benchmarks

## Files to Modify

- `cargo-simplebench/src/rlib_selection.rs` - major rewrite
- `cargo-simplebench/src/metadata.rs` - add src path lookup
- `test-workspace/game-math/Cargo.toml` - switch to dev-deps
- `test-workspace/game-entities/Cargo.toml` - switch to dev-deps
- `test-workspace/game-physics/Cargo.toml` - switch to dev-deps
- `test-workspace/game-math/src/lib.rs` - already has rand test helper
- `CLAUDE.md` - update documentation
