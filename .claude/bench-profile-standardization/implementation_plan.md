# Bench Profile Standardization - Implementation Plan

## Overview

This plan outlines the migration from SimpleBench's current custom approach to Rust's standard benchmarking conventions:

1. **Use `--bench` profile instead of `--release`**: Enables users to leverage `#[cfg(bench)]` to exclude benchmark-specific code from production binaries
2. **Rename `#[mbench]` to `#[bench]`**: Aligns with Rust's standard naming convention for benchmark attributes

## Motivation

### Problem 1: Benchmark Code in Production Binaries

Currently, SimpleBench uses the `--release` profile, which means:
- Benchmark functions and dependencies are compiled into production code
- No way for users to conditionally exclude benchmark-specific code
- Increased binary size and potential security surface

**Solution**: Use the built-in `bench` profile, which activates `#[cfg(bench)]`, allowing users to write:

```rust
#[cfg(bench)]
use some_testing_dependency;

#[cfg(bench)]
pub fn expensive_validation() { ... }
```

### Problem 2: Non-Standard Naming

The `#[mbench]` attribute doesn't align with Rust ecosystem conventions:
- Standard library uses `#[bench]` (though unstable)
- Other benchmark frameworks (Criterion, Divan) use `#[bench]` or similar
- Our name feels arbitrary and less discoverable

**Solution**: Rename to `#[bench]` for familiarity and ecosystem consistency.

## Implementation Strategy

This migration requires coordinated changes across all three crates and the test workspace. We'll use a breaking-change approach since there's only one user (the developer).

### Phase 1: Update Macro Crate

**File**: `simplebench-macros/src/lib.rs`

Changes:
1. Rename `pub fn mbench` → `pub fn bench`
2. Update all internal documentation/comments
3. Update wrapper function naming: `__simplebench_wrapper_*` → `__bench_wrapper_*` (optional, internal only)
4. Update tests in `tests/integration_test.rs`

**Impact**: Breaking change - all existing `#[mbench]` usages will fail to compile

### Phase 2: Update Runtime Crate

**File**: `simplebench-runtime/src/lib.rs`

Changes:
1. Update re-export comment from `mbench` to `bench`
2. No code changes required (runtime is attribute-agnostic)

**Impact**: Minimal - mostly documentation

### Phase 3: Update CLI Tool for Bench Profile

**Files**:
- `cargo-simplebench/src/main.rs`
- `cargo-simplebench/src/rlib_selection.rs`

**Changes in `main.rs`**:

1. Line 209-210: Change build output message
   ```rust
   // OLD:
   println!("{}", "Compiling workspace (release profile)".green().bold());
   let profile = "release";

   // NEW:
   println!("{}", "Compiling workspace (bench profile)".green().bold());
   let profile = "bench";
   ```

2. Line 249: Update deps directory path
   ```rust
   // Target directory already correctly uses variable `profile`
   let deps_dir = workspace_info.target_directory.join(profile).join("deps");
   ```

**Changes in `rlib_selection.rs`**:

1. Function signature already accepts `profile: &str` parameter - no change needed

2. Line 58-61: Update cargo build command
   ```rust
   // OLD:
   if profile == "release" {
       cmd.arg("--release");
   }

   // NEW:
   if profile == "bench" {
       cmd.arg("--profile").arg("bench");
   }
   ```

3. Update opt-level selection logic (lines 114-133):
   - The `bench` profile has `opt-level = 3` by default in Cargo.toml
   - Keep existing logic that selects `opt_level == "3"` rlibs
   - This should work unchanged, but add comment explaining bench profile expectations

**Impact**: Changes how workspace is built, but maintains rlib selection logic

### Phase 4: Verify Bench Profile Configuration

**File**: Root `Cargo.toml` (workspace)

Add explicit bench profile configuration to document expectations:

```toml
[profile.bench]
opt-level = 3
debug = false
debug-assertions = false
overflow-checks = false
lto = false
panic = 'unwind'
incremental = false
codegen-units = 16
```

**Note**: These are Cargo's defaults for `bench` profile. We're making them explicit for documentation purposes.

**Impact**: Documentation only - no behavior change

### Phase 5: Update Test Workspace

**Files**:
- `test-workspace/game-math/src/lib.rs`
- `test-workspace/game-entities/src/lib.rs`
- `test-workspace/game-physics/src/lib.rs`

Changes:
1. Replace all `#[mbench]` with `#[bench]`
2. Update imports: `use simplebench_macros::mbench;` → `use simplebench_macros::bench;`

**Optional**: Add example `#[cfg(bench)]` usage to demonstrate the benefit:

```rust
#[cfg(bench)]
use some_helper_only_for_benchmarks;

#[cfg(bench)]
pub fn setup_benchmark_data() -> Vec<Vec3> {
    // This code won't be in production builds
    vec![Vec3::new(1.0, 2.0, 3.0); 1000]
}

#[bench]
fn bench_vec3_normalize() {
    let vectors = setup_benchmark_data();
    for v in &vectors {
        let _normalized = v.normalize();
    }
}
```

**Impact**: Breaking change - requires updating all benchmark functions

### Phase 6: Update Documentation

**Files**:
- `README.md` (root)
- `CLAUDE.md`
- `.claude/cli-development/*.md` (historical docs)

Changes:
1. Replace all references to `#[mbench]` with `#[bench]`
2. Replace "release profile" with "bench profile" in build descriptions
3. Add section explaining `#[cfg(bench)]` capability
4. Update all code examples
5. Update "Common Commands" to reflect bench profile

**Impact**: Documentation only

## Migration Checklist

### Pre-Migration
- [ ] Create this implementation plan
- [ ] Review all affected files
- [ ] Ensure test workspace has clean baseline state

### Execution Order
1. [ ] Phase 1: Update macro crate (rename attribute)
2. [ ] Phase 2: Update runtime crate (documentation)
3. [ ] Phase 3: Update CLI tool (bench profile support)
4. [ ] Phase 4: Verify bench profile config in Cargo.toml
5. [ ] Phase 5: Update test workspace (use new attribute)
6. [ ] Phase 6: Update all documentation

### Validation
- [ ] Run `cargo build --release` in all crates - should succeed
- [ ] Run `cargo test` in all crates - should succeed
- [ ] Run `cargo simplebench` in test workspace - should discover all benchmarks
- [ ] Verify baselines are created in correct location
- [ ] Test `#[cfg(bench)]` conditional compilation:
  - Add dummy `#[cfg(bench)]` function to test workspace
  - Build with `--profile bench` - should compile
  - Build with `--release` - should NOT include the function
- [ ] Run full benchmark suite to ensure no regressions
- [ ] Verify variance is still <5%

### Post-Migration
- [ ] Clean old baselines: `cd test-workspace && ../target/release/cargo-simplebench clean`
- [ ] Generate fresh baselines
- [ ] Update git history with detailed commit message

## Technical Details

### Bench Profile vs Release Profile

| Aspect | Release Profile | Bench Profile |
|--------|----------------|---------------|
| Opt Level | 3 | 3 |
| Debug Assertions | off | off |
| `#[cfg(bench)]` | ❌ Not active | ✅ Active |
| Use Case | Production binaries | Benchmarking |
| LTO | Optional (default off) | off |
| Incremental | Optional | off |

**Key Difference**: The `#[cfg(bench)]` flag is the primary motivation. Both profiles have equivalent optimization levels.

### Rlib Selection Impact

Current rlib selection logic in `rlib_selection.rs`:
1. Parses `cargo build --message-format=json` output
2. Identifies all rlibs with their `opt_level` values
3. Selects rlibs with `opt_level == "3"`

**After migration**:
- `cargo build --profile bench` will still produce `opt_level == "3"` rlibs
- Selection logic remains unchanged
- Target directory changes from `target/release/deps` → `target/bench/deps`

**Critical**: The profile variable is already parameterized throughout the CLI tool, so this change is localized to the build invocation.

### Attribute Naming Collision

**Concern**: Rust's standard library has an unstable `#[bench]` attribute for nightly benchmarks.

**Resolution**:
- Our `#[bench]` is a proc macro from `simplebench-macros`
- Users explicitly import: `use simplebench_macros::bench;`
- No collision with std `#[bench]` because:
  1. Std's `#[bench]` is feature-gated behind `#![feature(test)]`
  2. Our macro is explicitly imported by path
  3. Proc macros take precedence over attributes in the same scope

**Example**:
```rust
// This works - our proc macro
use simplebench_macros::bench;

#[bench]  // Uses simplebench_macros::bench
fn my_benchmark() { }

// This would conflict only if:
// #![feature(test)]
// extern crate test;
// And we didn't import simplebench_macros::bench
```

Since SimpleBench targets stable Rust, this is not a concern.

## Risks and Mitigations

### Risk 1: Bench Profile Build Failures
**Risk**: Unknown differences between release and bench profiles cause build failures

**Mitigation**:
- Bench profile uses same opt-level as release (3)
- Test build with `cargo build --profile bench` before full migration
- Both profiles are well-tested in Rust ecosystem

### Risk 2: Variance Changes
**Risk**: Different profile might affect measurement variance

**Mitigation**:
- Optimization level is identical (opt-level=3)
- Variance is controlled by our sampling strategy, not profile
- Validation step includes variance testing
- Baseline cleanup ensures fresh comparison

### Risk 3: User Confusion
**Risk**: Users might not understand when to use `#[cfg(bench)]`

**Mitigation**:
- Add clear documentation with examples
- `#[cfg(bench)]` is optional - benchmarks work without it
- Document common use cases (test data generation, dev dependencies)

### Risk 4: Rlib Selection Changes
**Risk**: Bench profile might produce different rlib characteristics

**Mitigation**:
- Rlib selection logic is opt-level based, not profile based
- opt-level=3 is consistent across both profiles
- JSON parsing is profile-agnostic
- Fallback (file size) also works regardless of profile name

## Success Criteria

Migration is successful when:

1. ✅ All benchmarks compile and run with `#[bench]` attribute
2. ✅ `cargo simplebench` uses `--profile bench` for builds
3. ✅ Rlib selection correctly identifies bench profile artifacts
4. ✅ Variance remains <5% on repeated runs
5. ✅ `#[cfg(bench)]` conditional compilation works (tested with dummy code)
6. ✅ All tests pass (`cargo test` in all crates)
7. ✅ Documentation accurately reflects new naming and profile
8. ✅ No performance regressions detected in CI mode

## Future Enhancements

After this migration, users can:

1. **Exclude benchmark code from production**:
   ```rust
   #[cfg(bench)]
   pub mod bench_utils {
       // Only compiled for benchmarks
   }
   ```

2. **Use bench-specific dependencies**:
   ```toml
   [dev-dependencies]
   some-test-helper = "1.0"  # Available in bench profile
   ```

3. **Conditional feature flags**:
   ```rust
   #[cfg(bench)]
   use expensive_validation::validate_all;
   ```

This standardization aligns SimpleBench with Rust ecosystem conventions while providing tangible benefits for production binary size and security.
