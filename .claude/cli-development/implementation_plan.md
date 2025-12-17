# SimpleBench Implementation Plan

## Project Goal
Create a simple microbenchmarking tool for a personal Rust game engine that provides clear performance regression detection without the complexity and noise of existing solutions.

## Core Requirements (Simplified)
1. **Simple API**: `#[mbench]` attribute on functions
2. **Clean command**: `cargo simplebench` 
3. **Clear output**: Pass/fail with percentage changes
4. **Regression detection**: Fail on significant slowdowns
5. **Minimal setup**: Works with existing Cargo.toml

## Implementation Strategy

### Phase 1: Core Runtime (Week 1)
**Goal**: Get basic timing and measurement working

**Components**:
- `simplebench-runtime` crate
- Basic timing loop with batch measurement (100 samples × 100 iterations)
- Simple percentile calculation (p50, p90, p99)
- JSON output format for results

**Files to create**:
- `simplebench-runtime/Cargo.toml`
- `simplebench-runtime/src/lib.rs` - timing logic, percentiles
- `simplebench-runtime/src/measurement.rs` - core measurement functions
- `simplebench-runtime/src/output.rs` - JSON formatting

**Key functions**:
```rust
pub fn measure_function<F>(func: F, iterations: usize, samples: usize) -> BenchResult
pub fn calculate_percentiles(timings: &[Duration]) -> Percentiles
pub fn compare_with_baseline(current: &BenchResult, baseline: &BenchResult) -> Comparison
```

### Phase 2: Macro System (Week 2)  
**Goal**: Get `#[mbench]` attribute working with inventory registration

**Components**:
- `simplebench-macros` proc macro crate
- Inventory-based registration system
- Simple attribute parsing (no complex config initially)

**Files to create**:
- `simplebench-macros/Cargo.toml`
- `simplebench-macros/src/lib.rs` - `#[mbench]` macro implementation

**Macro output**:
```rust
#[mbench]
fn my_benchmark() { /* ... */ }

// Expands to:
fn my_benchmark() { /* ... */ }
inventory::submit! {
    SimpleBench {
        name: "my_benchmark",
        module: module_path!(),
        func: my_benchmark,
    }
}
```

### Phase 3: Manual Testing and rlib Selection Validation ✅ COMPLETED
**Goal**: Prove the unified runner approach works before building CLI automation

**Status**: Successfully validated manual rustc approach with 3-crate test workspace containing 8 benchmarks.

**Test workspace created**:
- `test-workspace/` - 3-crate game engine workspace
- `game-math/` - Vector/matrix operations (3 benchmarks)
- `game-entities/` - Entity management (3 benchmarks)
- `game-physics/` - Collision detection (2 benchmarks)

**Critical Discovery: Cargo Dual-Build Issue**

Cargo builds dependencies **twice** when compiling workspace crates:
1. **Host build** (opt-level=0): For proc-macros and build scripts
2. **Target build** (opt-level=3): For runtime dependencies when using `--release`

**Problem**: When benchmark crates are built with `cargo build --release`:
- Benchmark crates compiled with opt-level=3
- They depend on opt-level=3 versions of `simplebench-runtime` and `inventory`
- If runner links opt-level=0 versions → **version mismatch** → inventory finds 0 benchmarks

**Solution Validated**:
✅ Runner optimization level is **irrelevant**
✅ Must select **opt-level=3 rlibs** for dependencies (target build, not host build)
✅ File size heuristic works: opt-level=3 produces **smaller** rlibs (select smallest)
✅ Better approach: Parse `cargo build --message-format=json` output

**Manual validation process**:
```bash
# Build workspace
cd test-workspace
cargo build --release

# Find correct rlibs (opt-level=3 versions)
ls -lh target/release/deps/libsimplebench_runtime-*.rlib
# Select the SMALLEST rlib for each dependency

# Manual rustc invocation
rustc runner.rs --edition 2021 -C opt-level=0 \
  --extern simplebench_runtime=target/release/deps/libsimplebench_runtime-<hash>.rlib \
  --extern inventory=target/release/deps/libinventory-<hash>.rlib \
  --extern game_math=target/release/deps/libgame_math-<hash>.rlib \
  --extern game_entities=target/release/deps/libgame_entities-<hash>.rlib \
  --extern game_physics=target/release/deps/libgame_physics-<hash>.rlib \
  -L dependency=target/release/deps

# Run and verify
./runner  # Output: Found 8 benchmarks ✅
```

**Key Findings**:
1. ✅ Inventory successfully collects benchmarks across multiple linked crates
2. ✅ Manual rustc linking works when correct rlib versions selected
3. ✅ Runner binary optimization level doesn't affect benchmark discovery
4. ✅ File size heuristic (select smallest rlib) reliably selects opt-level=3 versions
5. ✅ JSON parsing approach is more robust for production use

### Phase 4: CLI Tool (Week 4)
**Goal**: Automate the manual process from Phase 3

**Components**:
- `cargo-simplebench` binary crate
- Cargo metadata parsing to find workspace crates
- **Intelligent rlib selection** using JSON parsing or file size heuristic
- runner.rs generation with proper extern declarations
- rustc invocation with correct rlib linking

**Files to create**:
- `cargo-simplebench/Cargo.toml`
- `cargo-simplebench/src/main.rs` - CLI entry point
- `cargo-simplebench/src/metadata.rs` - cargo metadata parsing
- `cargo-simplebench/src/rlib_selection.rs` - **NEW: rlib discovery with JSON parsing**
- `cargo-simplebench/src/runner_gen.rs` - runner.rs code generation
- `cargo-simplebench/src/compile.rs` - rustc invocation

**Critical Implementation: rlib Selection Algorithm**

**Primary Approach: JSON Parsing** (most robust)
```rust
// src/rlib_selection.rs
use serde_json::Value;
use std::collections::HashMap;
use std::process::Command;

pub struct RlibInfo {
    pub path: PathBuf,
    pub opt_level: String,
    pub crate_name: String,
}

pub fn select_rlibs_for_workspace(
    workspace_root: &Path,
    profile: &str,  // "release" or "dev"
) -> Result<HashMap<String, PathBuf>> {
    // Step 1: Build workspace with JSON output
    let output = Command::new("cargo")
        .arg("build")
        .arg(if profile == "release" { "--release" } else { "" })
        .arg("--message-format=json")
        .current_dir(workspace_root)
        .output()?;

    // Step 2: Parse JSON lines
    let mut rlib_versions: HashMap<String, Vec<RlibInfo>> = HashMap::new();

    for line in String::from_utf8(output.stdout)?.lines() {
        let json: Value = serde_json::from_str(line)?;

        if json["reason"] != "compiler-artifact" {
            continue;
        }

        let crate_name = json["target"]["name"].as_str().unwrap();
        let opt_level = json["profile"]["opt_level"].as_str().unwrap();
        let filenames = json["filenames"].as_array().unwrap();

        // Find .rlib file
        if let Some(rlib_path) = filenames.iter()
            .find(|f| f.as_str().unwrap().ends_with(".rlib"))
        {
            rlib_versions.entry(crate_name.to_string())
                .or_default()
                .push(RlibInfo {
                    path: PathBuf::from(rlib_path.as_str().unwrap()),
                    opt_level: opt_level.to_string(),
                    crate_name: crate_name.to_string(),
                });
        }
    }

    // Step 3: Select opt-level=3 versions (or highest available)
    let mut selected_rlibs = HashMap::new();

    for (crate_name, versions) in rlib_versions {
        let selected = if versions.len() == 1 {
            // Only one version exists
            &versions[0]
        } else {
            // Multiple versions - select opt-level=3 (target build)
            versions.iter()
                .find(|v| v.opt_level == "3")
                .or_else(|| {
                    // Fallback: highest opt-level
                    versions.iter().max_by_key(|v| v.opt_level.parse::<u8>().unwrap_or(0))
                })
                .expect("No rlib versions found")
        };

        selected_rlibs.insert(crate_name, selected.path.clone());
    }

    Ok(selected_rlibs)
}
```

**Fallback Approach: File Size Heuristic** (simpler, still reliable)
```rust
pub fn select_rlibs_by_size(deps_dir: &Path) -> Result<HashMap<String, PathBuf>> {
    let mut rlib_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();

    // Group rlibs by crate name (before the hash)
    for entry in fs::read_dir(deps_dir)? {
        let path = entry?.path();
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if filename.ends_with(".rlib") {
                let crate_name = filename
                    .strip_prefix("lib").unwrap()
                    .split('-').next().unwrap();
                rlib_groups.entry(crate_name.to_string())
                    .or_default()
                    .push(path);
            }
        }
    }

    // Select the SMALLEST rlib for each crate (opt-level=3 is smaller)
    let mut selected = HashMap::new();
    for (crate_name, rlibs) in rlib_groups {
        let smallest = rlibs.iter()
            .min_by_key(|path| fs::metadata(path).unwrap().len())
            .unwrap();
        selected.insert(crate_name, smallest.clone());
    }

    Ok(selected)
}
```

**CLI Workflow**:
```bash
# User runs:
cargo simplebench

# CLI executes:
1. Parse workspace with `cargo metadata`
2. Build all crates: `cargo build --release --message-format=json`
3. Parse JSON output → select opt-level=3 rlibs
4. Identify benchmark crates (those depending on simplebench-runtime)
5. Generate runner.rs with extern declarations
6. Compile runner: rustc runner.rs --extern crate1=path/to/rlib1 ...
7. Execute runner binary
8. Display results and compare with baselines
```

**Runner Generation Template**:
```rust
// Generated by cargo-simplebench
extern crate simplebench_runtime;
extern crate inventory;
extern crate game_math;      // Benchmark crate 1
extern crate game_entities;  // Benchmark crate 2
extern crate game_physics;   // Benchmark crate 3

fn main() {
    let benchmarks = inventory::iter::<simplebench_runtime::BenchmarkDef>();

    println!("Found {} benchmarks", benchmarks.count());

    for bench in benchmarks {
        let result = simplebench_runtime::run_benchmark(bench);
        println!("{}: {:?}", bench.name, result);
    }
}
```

**Why JSON Parsing is Better**:
1. **Direct identification**: Know exactly which rlib has which opt-level
2. **No ambiguity**: File size is indirect and could fail in edge cases
3. **Explicit metadata**: Full profile info (opt-level, debug-assertions, etc.)
4. **Future-proof**: Works even if Cargo's optimization strategy changes
5. **Self-documenting**: Algorithm clearly expresses intent (select opt-level=3)

### Phase 5: Historical Tracking (Week 5)
**Goal**: Add baseline storage and regression detection

**Components**:
- `.benches/` directory structure
- JSON file storage per benchmark
- Simple regression comparison (current vs last run)
- Machine identification (MAC address)

**Storage format**:
```
.benches/
  <hostname>/
    <crate_name>_<benchmark_name>.json
```

**Regression logic**:
- Enabled when passing the --ci flag. Otherwise simply output results.
- Compare p90 timing against last successful run
- Default threshold: 5% regression = failure. Configurable with simple flag to cargo-simplebench.
- First run always passes (establishes baseline)

## Simplified Decisions

### What to Skip Initially
- Complex configuration (global defaults only)
- Per-benchmark config overrides  
- Statistical outlier detection
- HTML reports and graphs
- Parallel benchmark execution
- CI-specific features

### Game Engine Focus
- Expect realistic workloads (thousands of entities, full frame updates)
- Timer overhead negligible for game engine benchmarks
- Serial execution fine for personal use
- Simple pass/fail more valuable than detailed statistics

### Architecture Simplifications
- Single machine (your development setup)
- Single user (no multi-developer baseline conflicts)
- Simple storage (JSON files, no database)
- Fixed measurement parameters (100 samples × 100 iterations)
- Fixed percentile reporting (p50, p90, p99)

## Success Criteria

**Phase 1**: ✅ Can measure a function 10,000 times and get reliable percentiles
**Phase 2**: ✅ Can write `#[mbench] fn test() {}` and have it auto-register
**Phase 3**: ✅ Can manually link game engine crates into unified runner (VALIDATED)
**Phase 4**: ✅ `cargo simplebench` runs all benchmarks automatically
**Phase 5**: Performance regressions fail the command with clear output

**Phase 3 Success Metrics Achieved**:
- ✅ 3-crate test workspace created with realistic game engine benchmarks
- ✅ Manual rustc compilation successfully links all crates
- ✅ Inventory collects all 8 benchmarks from across 3 crates
- ✅ Identified and solved Cargo dual-build rlib selection issue
- ✅ Validated both file size heuristic and JSON parsing approaches
- ✅ Runner optimization level confirmed irrelevant to benchmark discovery

## Expected Timeline
- **5 weeks total** for personal use MVP
- **Phase 1-2**: Core functionality (2 weeks)
- **Phase 3**: Proof of concept (1 week)  
- **Phase 4**: Automation (1 week)
- **Phase 5**: Polish (1 week)

## Risk Mitigation
- **rlib linking issues**: ✅ RESOLVED - Phase 3 validated manual rustc approach works
- **Cargo dual-build complexity**: ✅ UNDERSTOOD - JSON parsing or file size heuristic solves it
- **Inventory registration**: Well-established pattern, low risk
- **Cargo metadata changes**: Stable API, widely used
- **Timer precision**: Game engine workloads large enough to measure accurately
- **JSON parsing complexity**: Fallback to file size heuristic if needed

## Technical Insights from Phase 3

### The Cargo Dual-Build Problem
Cargo builds dependencies twice when using proc-macros:
- **Host build** (opt-level=0): For proc-macros running during compilation
- **Target build** (opt-level=3 with --release): For runtime execution

Both builds produce rlibs with **different hashes** in the same deps directory.

### Why Runner Optimization Doesn't Matter
The runner binary's own optimization level is irrelevant. What matters:
- Benchmark crates depend on **specific rlib versions** (identified by hash)
- Inventory registration happens **at compile time** of benchmark crates
- If runner links **different versions** → inventory can't find registrations
- Solution: Runner must link **exact same rlib versions** benchmark crates used

### File Size as Proxy for Optimization Level
- `opt-level=0`: Larger code (no inlining, more debug info, unoptimized)
- `opt-level=3`: Smaller code (aggressive inlining, dead code elimination)
- Selecting **smallest rlib** reliably picks the opt-level=3 version
- Works in practice but less explicit than JSON parsing

### Why JSON Parsing is Preferred
`cargo build --message-format=json` outputs one JSON object per compiled artifact:
```json
{
  "reason": "compiler-artifact",
  "target": { "name": "simplebench_runtime" },
  "profile": { "opt_level": "3" },
  "filenames": ["/path/to/libsimplebench_runtime-hash.rlib"]
}
```

This makes rlib selection **explicit and unambiguous**.

## Next Steps: Phase 4 Implementation

### Ready to Build
Phase 3 research provides **everything needed** to implement Phase 4 CLI automation:

1. **rlib selection algorithm** - detailed pseudocode provided
2. **Manual validation** - proven working with 8-benchmark test workspace
3. **Technical understanding** - dual-build issue fully understood
4. **Two implementation strategies** - JSON parsing (preferred) + file size fallback

### Implementation Order for Phase 4
```
1. cargo-simplebench/src/metadata.rs
   - Parse `cargo metadata` to find workspace members
   - Identify crates depending on simplebench-runtime

2. cargo-simplebench/src/rlib_selection.rs
   - Implement JSON parsing approach (primary)
   - Implement file size fallback (secondary)
   - Add tests using test-workspace/

3. cargo-simplebench/src/runner_gen.rs
   - Template for runner.rs generation
   - Generate extern declarations for all benchmark crates

4. cargo-simplebench/src/compile.rs
   - Execute rustc with correct --extern flags
   - Use selected rlib paths from rlib_selection

5. cargo-simplebench/src/main.rs
   - Orchestrate: metadata → build → select → generate → compile → run
   - Display benchmark results
```

### Test Using Existing Workspace
The `test-workspace/` from Phase 3 provides immediate validation:
```bash
cd test-workspace
cargo simplebench  # Should find and run all 8 benchmarks
```

Expected output:
```
Building workspace...
Found 3 benchmark crates: game-math, game-entities, game-physics
Selected 5 rlibs (opt-level=3)
Generating runner...
Compiling runner...
Running benchmarks...

Found 8 benchmarks:
  game_math::vector_add - 1.2ns
  game_math::matrix_multiply - 45.3ns
  game_math::quaternion_rotate - 8.7ns
  game_entities::spawn_entity - 123.4ns
  game_entities::update_entities - 2.1µs
  game_entities::despawn_entity - 89.2ns
  game_physics::aabb_collision - 34.5ns
  game_physics::sweep_test - 156.7ns

All benchmarks completed successfully.
```

## Future Enhancements (Post-MVP)
- Parallel execution
- Configuration file support
- Per-benchmark thresholds
- Trend analysis across multiple runs
- Integration with game engine profiling
- Automated benchmark discovery in CI
- Support for criterion-style comparison output
- Flamegraph integration for regression analysis
