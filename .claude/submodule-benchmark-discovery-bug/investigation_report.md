# SimpleBench Submodule Benchmark Discovery Bug

## Issue Summary

SimpleBench fails to discover benchmarks defined in crates that are part of a git submodule within a Cargo workspace. The bug was observed in the 2DBoomer project where the `common` crate (in the `engine/` submodule) has 5 benchmarks that were not being discovered, while the `physics` crate (also in the submodule) had its 5 benchmarks running correctly.

## Environment

- SimpleBench version: 2.1.0
- Project: 2DBoomer (game engine with git submodule)
- Affected crate: `engine/source/libs/common` (5 benchmarks in `src/world.rs`)
- Working crate: `engine/source/libs/physics` (5 benchmarks in `src/lib.rs`)

## Root Cause Analysis

### The Bug Is NOT Submodule-Related

Despite initial assumptions, the bug has **nothing to do with git submodules**. Both `common` and `physics` crates are in the same submodule (`engine/`) but exhibited different behavior.

### Actual Root Cause: simplebench_runtime Version Mismatch

The `inventory` crate uses static global variables (REGISTRY) to collect registered items. Each compilation of `simplebench_runtime` produces a REGISTRY with a **unique hash suffix** based on compilation metadata:

```
REGISTRY17h5800060c34476887E  // Version A
REGISTRY17hc85ab9a488c94979E  // Version B
```

When benchmark crates are compiled against one version of `simplebench_runtime`, they register their benchmarks in that version's REGISTRY. When the runner is compiled against a **different** version, it iterates over an **empty different REGISTRY**.

### Evidence

1. **Multiple simplebench_runtime versions exist** in `target/simplebench/release/deps/`:
   ```
   libsimplebench_runtime-193d68c913b5df66.rlib
   libsimplebench_runtime-1fba70eb7a085256.rlib  # CORRECT - has hc85ab9a488c94979E
   libsimplebench_runtime-53134442f4c1e92a.rlib  # WRONG - has h5800060c34476887E
   libsimplebench_runtime-5ae5df41abbe9173.rlib
   libsimplebench_runtime-9e4257ef415db984.rlib
   libsimplebench_runtime-c86b5358ea9ab17d.rlib
   ```

2. **Benchmark crates reference one REGISTRY**:
   ```
   $ nm target/simplebench/release/deps/libcommon.rlib | grep REGISTRY
   U _ZN...SimpleBench...REGISTRY17hc85ab9a488c94979E

   $ nm target/simplebench/release/deps/libphysics.rlib | grep REGISTRY
   U _ZN...SimpleBench...REGISTRY17hc85ab9a488c94979E
   ```

3. **Wrong simplebench_runtime was selected for runner**:
   ```
   $ nm libsimplebench_runtime-53134442f4c1e92a.rlib | grep REGISTRY
   B _ZN...SimpleBench...REGISTRY17h5800060c34476887E  # Different hash!
   ```

4. **Correct simplebench_runtime fixes the issue**:
   ```
   $ nm libsimplebench_runtime-1fba70eb7a085256.rlib | grep REGISTRY
   B _ZN...SimpleBench...REGISTRY17hc85ab9a488c94979E  # Matching hash!
   ```

### Why Physics Worked But Common Didn't

Upon closer inspection, **both crates were affected equally**. The initial observation that "only physics benchmarks are found" was misleading - the 5 benchmarks found were actually from a **different run** where the correct simplebench_runtime happened to be selected. In subsequent testing, **zero benchmarks** were discovered when the wrong version was linked.

## Technical Details

### How Cargo Creates Multiple Versions

When running `cargo test -p <crate> --release --no-run`, Cargo may build dependencies multiple times with different metadata:

1. **Host builds** (for proc-macros and build scripts) - opt-level=0
2. **Target builds** (for runtime code) - opt-level=3
3. **Different feature combinations** can also create separate versions

Each build produces an rlib with a unique hash in its filename.

### How SimpleBench's rlib_selection.rs Fails

The current `parse_cargo_json()` function in `rlib_selection.rs`:

1. Builds dev-deps via `cargo test -p <crate> --release --no-run --message-format=json`
2. Parses JSON output to collect rlib paths
3. Selects rlibs based on opt_level (3 for runtime, 0 for proc-macros)

The problem: when multiple opt-level=3 versions of `simplebench_runtime` exist, the selection is **non-deterministic** or based on criteria (like file size) that don't guarantee consistency with what the benchmark crates were compiled against.

## Fix Required

The fix must ensure that **the exact same simplebench_runtime rlib** is used for:
1. Compiling the benchmark crates (common, physics, etc.)
2. Compiling the runner

### Proposed Solution

When collecting rlibs for a benchmark crate, track which `simplebench_runtime` version that crate was compiled against (from its JSON output). Then use that **same version** when compiling the runner.

Implementation options:

1. **Parse dependencies from each crate's build output**: Each crate's JSON output shows which rlibs it was linked against. Extract the simplebench_runtime path from there.

2. **Build all benchmark crates first, then use the last-seen simplebench_runtime**: Since all benchmark crates should use the same version (they're in the same workspace), the last one seen should be correct.

3. **Clear/isolate the target directory before building**: Ensure only one simplebench_runtime version exists by using a clean target directory.

## Reproduction Steps

1. Clone a workspace with multiple benchmark crates (e.g., 2DBoomer)
2. Run `cargo simplebench` multiple times
3. Observe that benchmark discovery is inconsistent - sometimes finding all benchmarks, sometimes finding none

## Files Modified

- `cargo-simplebench/src/rlib_selection.rs`:
  1. Added `CRITICAL_DEPS` constant to track `simplebench_runtime` and `inventory`
  2. Modified `parse_cargo_json()` to return critical dep paths separately
  3. Modified `build_and_select_rlibs()` to:
     - Track canonical versions of critical deps from first crate
     - Use canonical versions when compiling each benchmark crate
     - Preserve manually-built benchmark crates (with `--cfg test`) from being overwritten

## Root Causes (Two Issues Found)

### Issue 1: Critical Dependency Version Mismatch
When multiple versions of `simplebench_runtime` and `inventory` exist in target/deps, the runner could link against different versions than benchmark crates.

**Fix**: Track canonical versions from first crate, enforce for all subsequent crates and runner.

### Issue 2: Benchmark Crate Overwriting
When physics depends on common, physics's cargo build includes `libcommon-<hash>.rlib` (without `--cfg test`). The `all_rlibs.extend(crate_rlibs)` was overwriting the manually-built `libcommon.rlib` (with `--cfg test` and benchmarks).

**Fix**: When extending rlibs, skip:
- Previously manually-built benchmark crates
- Critical dependencies (handled separately)

## Verification

After fixing, running this test should always find all 10 benchmarks:

```bash
cd /home/chrisp/Documents/2DBoomer
cargo simplebench run
# Should show: Found 10 benchmarks (5 common + 5 physics)
```

**Verified**: Fix successfully discovers all 10 benchmarks.

## Key Insight

This bug demonstrates a fundamental challenge with Rust's compilation model: when using static registration patterns like `inventory::submit!`, **all participating crates must be linked against the exact same version of the collector crate**, or the registrations will be silently lost.

Additionally, when benchmark crates depend on each other, care must be taken to preserve the manually-compiled rlibs (with `--cfg test`) rather than using cargo's cached versions (without benchmarks).
