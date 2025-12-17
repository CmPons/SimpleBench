# Better Heuristic for rlib Selection

## Date
2025-12-08

## Problem
Phase 3 investigation concluded that we should select the SMALLER rlib when multiple versions exist. However, file size is an indirect heuristic. Can we do better?

## User Hypothesis
**Confirmed**: The optimization level of the runner binary doesn't matter. What matters is selecting the correct dependency rlib versions.

## Experiments Conducted

### Test 1: opt-level=0 runner + opt-level=3 rlibs
```bash
rustc runner.rs --edition 2021 -C opt-level=0 \
  --extern simplebench_runtime=.../libsimplebench_runtime-eb85215ec7f57785.rlib \
  ...
```
**Result**: ✅ Found 8 benchmarks

### Test 2: opt-level=3 runner + opt-level=0 rlibs
```bash
rustc runner.rs --edition 2021 -C opt-level=3 \
  --extern simplebench_runtime=.../libsimplebench_runtime-dbc52cd4074dfbb2.rlib \
  ...
```
**Result**: ❌ Found 0 benchmarks

## Conclusion
**The runner's optimization level is irrelevant.** The issue is purely about linking against the correct dependency rlib versions.

## Root Cause (Refined Understanding)
When benchmark crates are compiled with `cargo build --release`:
1. They are built as rlibs with `opt-level=3`
2. They depend on the `opt-level=3` versions of `simplebench-runtime` and `inventory`
3. If the runner links against the `opt-level=0` versions, there's a **version mismatch**
4. Inventory can't find benchmarks because they were registered with a different rlib version

The optimization level is just a **symptom** of the real issue: Cargo builds dependencies twice (host vs target), and we must use the **target build** versions.

## Better Solution: cargo build --message-format=json

Instead of using file size as a heuristic, we can parse Cargo's JSON output to identify which rlib corresponds to which build:

```bash
cargo build --message-format=json --release 2>&1 | \
  jq -r 'select(.reason == "compiler-artifact") |
         select(.target.name == "inventory" or .target.name == "simplebench_runtime") |
         "\(.target.name) | opt_level=\(.profile.opt_level) | \(.filenames[0])"'
```

**Output:**
```
inventory | opt_level=0 | .../libinventory-5ad607dd8dd35a24.rlib
inventory | opt_level=3 | .../libinventory-b7dfb06ec571761c.rlib
simplebench_runtime | opt_level=0 | .../libsimplebench_runtime-dbc52cd4074dfbb2.rlib
simplebench_runtime | opt_level=3 | .../libsimplebench_runtime-eb85215ec7f57785.rlib
```

### Algorithm for Phase 4 CLI

```python
def select_correct_rlibs(workspace_crates):
    # Run cargo build with JSON output
    cargo_output = run("cargo build --message-format=json --release")

    rlib_map = {}

    for line in cargo_output:
        if line["reason"] != "compiler-artifact":
            continue

        crate_name = line["target"]["name"]
        opt_level = line["profile"]["opt_level"]
        rlib_path = line["filenames"][0]  # .rlib file

        # Store all versions
        if crate_name not in rlib_map:
            rlib_map[crate_name] = []
        rlib_map[crate_name].append({
            "path": rlib_path,
            "opt_level": opt_level,
            "profile": line["profile"]
        })

    # Select the opt-level=3 version for each crate
    selected_rlibs = {}
    for crate_name, versions in rlib_map.items():
        if len(versions) == 1:
            selected_rlibs[crate_name] = versions[0]["path"]
        else:
            # Multiple versions exist - select opt-level=3
            opt3_versions = [v for v in versions if v["opt_level"] == "3"]
            if opt3_versions:
                selected_rlibs[crate_name] = opt3_versions[0]["path"]
            else:
                # Fallback: select highest opt-level
                selected_rlibs[crate_name] = max(versions, key=lambda v: v["opt_level"])["path"]

    return selected_rlibs
```

## Why This Is Better Than File Size

1. **Direct identification**: We know exactly which rlib has which opt-level
2. **No ambiguity**: File size is an indirect heuristic that could fail in edge cases
3. **Explicit profile info**: The JSON includes the full profile (opt-level, debug-assertions, etc.)
4. **Future-proof**: Works even if Cargo's optimization strategy changes
5. **Self-documenting**: The algorithm clearly expresses intent (select opt-level=3)

## Alternative: cargo metadata

Another approach is to use `cargo metadata` to understand the dependency graph, but this doesn't tell us which **artifacts** were built. The `--message-format=json` output is more direct because it shows actual compilation results.

## Recommendations for Phase 4

**Primary approach**: Use `cargo build --message-format=json --release` and select rlibs with `opt_level=3` (or the highest opt-level if multiple exist).

**Fallback**: If JSON parsing proves complex, file size heuristic (select smallest) still works as documented in Phase 3.

**Alternative**: Generate temporary Cargo project and let Cargo handle everything (most robust, but less educational).

## File Size Still Works (But Why)

The file size heuristic works because:
- `opt-level=0` produces larger code (no optimizations, more debug info)
- `opt-level=3` produces smaller code (inlining, dead code elimination)

However, this is implementation-dependent and could theoretically break if:
- Cargo changes optimization defaults
- Different profiles produce similar-sized artifacts
- Target-specific optimizations behave differently

Using JSON output is more reliable.
