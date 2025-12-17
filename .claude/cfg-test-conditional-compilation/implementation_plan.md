# Plan: Benchmark Conditional Compilation via `cargo rustc` + Isolated Target Directory

## Status: ✅ IMPLEMENTED

## Problem
Users want to conditionally compile benchmark-only code using idiomatic `#[cfg(test)]` that doesn't appear in production builds.

### Previous Attempts & Why They Failed

1. **RUSTFLAGS="--cfg test"** - Applies to ALL crates including external dependencies. External crates have `#[cfg(test)]` code expecting their dev-dependencies (like `rand`), causing compilation failures.

2. **cargo test --release** - Cargo only puts `--cfg test` on test *binaries*, not on linkable rlibs. We need rlibs to link into our runner.

3. **Custom profile** - Built-in `bench` profile shares `target/release/` directory, causing cache conflicts.

4. **Dev-dependencies** - Cargo only makes dev-dependencies available for test targets, not library builds. `cargo rustc -p <crate>` builds the lib target which doesn't have access to dev-deps.

## Solution: `cargo rustc` with Per-Crate Flags + Regular Dependencies

**Key Discovery**: `cargo rustc -p <crate> -- --cfg test` passes the flag ONLY to that specific crate, not to its dependencies!

Combined with `--target-dir` for isolation, this gives us exactly what we need.

### How It Works

When `cargo simplebench` runs:
1. For each benchmark crate: `cargo rustc -p <crate> --release --target-dir target/simplebench -- --cfg test`
2. Dependencies build normally (no cfg(test)) - they compile successfully
3. Workspace crates build with `--cfg test` - enables `#[cfg(test)]` code
4. Isolated target directory - no cache conflicts with normal builds
5. Produces linkable rlibs with cfg(test) enabled

### User Experience

```toml
# In benchmark crate's Cargo.toml
[dependencies]
simplebench-runtime = { path = "..." }
simplebench-macros = { path = "..." }
```

```rust
// Production code - always compiled
pub struct MyStruct { ... }

// Benchmarks - only compiled when running cargo simplebench
#[cfg(test)]
mod benchmarks {
    use super::*;
    use simplebench_macros::bench;

    #[bench]
    fn my_benchmark() {
        // ...
    }
}
```

Then just: `cargo simplebench`

### Key Benefits
- **Idiomatic Rust**: Uses standard `#[cfg(test)]` pattern
- **Zero production overhead**: All benchmark code excluded from normal `cargo build`
- **No cache conflicts**: Isolated `target/simplebench/` directory
- **No clean needed**: Separate cache = consistent builds
- **opt-level=3**: Release profile for accurate measurements

### Important Note
simplebench must be a **regular dependency** (not dev-dependency) because Cargo only makes dev-dependencies available for test targets, not library builds. However, all benchmark CODE is still excluded from production builds via `#[cfg(test)]`.

## Implementation (Completed)

### 1. rlib_selection.rs
- Replaced `select_rlibs()` with `build_and_select_rlibs()`
- Per-crate `cargo rustc -p <crate> --release --target-dir <dir> -- --cfg test`
- Uses file size heuristic for rlib selection (smaller = opt-level=3)

### 2. main.rs
- Changed target directory to `target/simplebench`
- Extracts benchmark crate names from workspace info
- Passes crate names to build function

### 3. test-workspace lib.rs files
- Wrapped benchmark code in `#[cfg(test)] mod benchmarks { ... }`
- Benchmarks use `use super::*` to access production types

### 4. CLAUDE.md
- Documented `#[cfg(test)]` pattern for benchmark helpers
- Explained why dev-dependencies don't work

## Verification

```bash
# Production build - no benchmark code
cargo build --release
nm -C target/release/libgame_math.rlib | grep bench  # No results

# SimpleBench build - benchmarks included
cargo simplebench  # Runs all 8 benchmarks
```
