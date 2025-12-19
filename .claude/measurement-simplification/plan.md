# Measurement Simplification & Owning Setup Plan

## Overview

Two related changes to SimpleBench:
1. **Remove iterations × samples** - Measure every execution individually (each sample = 1 function call)
2. **Add owning setup** - New setup variant where benchmark can take either `&T` or `T` (setup runs before every sample)

## Current State

### Measurement Model (iterations × samples)
```
warmup: run benchmark in exponential batches for N seconds
measurement:
  for sample in 0..samples:
    start = now()
    for _ in 0..iterations:   # <-- This loop goes away
      benchmark_fn()
    timing[sample] = elapsed()
```

Currently each sample timing = duration of `iterations` executions batched together. This obscures per-call variance.

### Setup Model (reference-based, runs once)
```rust
// Current: setup runs once, benchmark borrows data
#[bench(setup = create_data)]
fn benchmark(data: &Data) { ... }  // Gets &T

// Generated:
let data = setup();           // Once
let mut func = || bench(&data);  // Closure borrows
// warmup + measurement use func
```

**Limitation**: Cannot test operations that consume/mutate data (e.g., `Vec::sort` on same data, `HashMap::insert` of same key).

## Proposed Changes

### Change 1: Single-Call Samples

Each sample = exactly 1 benchmark function call. Remove the inner iteration loop entirely.

**Before:**
```rust
for sample in 0..samples {
    let start = Instant::now();
    for _ in 0..iterations {
        func();
    }
    all_timings.push(start.elapsed());
}
```

**After:**
```rust
for sample in 0..samples {
    let start = Instant::now();
    func();
    all_timings.push(start.elapsed());
}
```

#### Impact

| Component | Change |
|-----------|--------|
| `MeasurementConfig` | Remove `iterations` field |
| `BenchResult` | Remove `iterations` field |
| `measure_closure()` | Remove inner iteration loop |
| `warmup_closure()` | Remove inner iteration loop |
| `warmup_benchmark()` | Remove inner iteration loop |
| `measure_function_impl()` | Remove (legacy, can delete) |
| `validate_measurement_params()` | Remove iterations validation |
| `config.rs` | Remove `SIMPLEBENCH_ITERATIONS` env var handling |
| CLI (`cargo-simplebench`) | Remove `--iterations` flag |
| Output formatting | Remove iterations display |
| Baseline storage | Remove iterations from stored data (migrations?) |

#### Warmup Changes

With single-call samples, warmup becomes:
```rust
while elapsed < warmup_duration {
    for _ in 0..batch_size {
        func();  // Single call per "iteration"
    }
    batch_size *= 2;
}
```

The exponential batching still makes sense for warmup efficiency.

### Change 2: Per-Sample Setup (`setup_each`)

Add a second setup mode where setup runs before every sample. The benchmark can take either `&T` (borrow) or `T` (ownership) based on user preference.

**New syntax:**
```rust
// Borrowing - setup runs each time, benchmark borrows
#[bench(setup_each = create_data)]
fn benchmark(data: &Data) { ... }  // Gets &T

// Owning - setup runs each time, benchmark takes ownership
#[bench(setup_each = create_data)]
fn benchmark(data: Data) { ... }  // Gets T
```

**Semantics:**
- `setup = ...` (existing): Setup runs once, benchmark gets `&T`
- `setup_each = ...` (new): Setup runs before each sample
  - If benchmark takes `&T`: setup runs, benchmark borrows, data dropped after
  - If benchmark takes `T`: setup runs, benchmark consumes data

#### Use Cases

```rust
// Owning: Sorting a fresh vector each time (consumes via mut)
#[bench(setup_each = || vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3])]
fn bench_sort(mut data: Vec<i32>) {
    data.sort();
}

// Owning: Consuming an iterator
#[bench(setup_each = || (0..1000).collect::<Vec<_>>())]
fn bench_consume_iter(data: Vec<i32>) {
    let _sum: i32 = data.into_iter().sum();
}

// Borrowing: Fresh random data each sample, but only reading
#[bench(setup_each = || random_vectors(1000))]
fn bench_normalize(vectors: &Vec<Vec3>) {
    for v in vectors {
        let _ = v.normalize();
    }
}
```

#### Implementation

##### Macro Changes (`simplebench-macros/src/lib.rs`)

Add parsing for `setup_each` attribute:
```rust
Some("setup_each") => {
    setup_each_expr = Some(nv.value);
}
```

Add validation:
- `setup_each` requires function parameter
- Cannot use both `setup` and `setup_each`

Detect parameter type to choose runtime function:
```rust
fn is_reference_param(input_fn: &ItemFn) -> bool {
    // Check if first parameter type starts with &
    if let Some(first_param) = input_fn.sig.inputs.first() {
        if let syn::FnArg::Typed(pat_type) = first_param {
            if let syn::Type::Reference(_) = &*pat_type.ty {
                return true;
            }
        }
    }
    false
}
```

Generate wrapper based on parameter type:
```rust
fn generate_with_setup_each(
    fn_name: &syn::Ident,
    fn_name_str: &str,
    input_fn: &ItemFn,
    setup_expr: Expr,
) -> TokenStream {
    let run_fn_name = format_ident!("__simplebench_run_{}", fn_name);
    let is_ref = is_reference_param(input_fn);

    let measure_call = if is_ref {
        // Benchmark takes &T - use borrowing version
        quote! {
            ::simplebench_runtime::measure_with_setup_each_ref(
                config,
                #fn_name_str,
                module_path!(),
                || (#setup_expr)(),
                |data| #fn_name(data),  // Receives &T
            )
        }
    } else {
        // Benchmark takes T - use owning version
        quote! {
            ::simplebench_runtime::measure_with_setup_each(
                config,
                #fn_name_str,
                module_path!(),
                || (#setup_expr)(),
                |data| #fn_name(data),  // Receives T
            )
        }
    };

    quote! {
        #input_fn

        fn #run_fn_name(
            config: &::simplebench_runtime::config::BenchmarkConfig
        ) -> ::simplebench_runtime::BenchResult {
            #measure_call
        }

        ::simplebench_runtime::inventory::submit! {
            ::simplebench_runtime::SimpleBench {
                name: #fn_name_str,
                module: module_path!(),
                run: #run_fn_name,
            }
        }
    }
}
```

##### Runtime Changes (`simplebench-runtime/src/measurement.rs`)

Two new measurement functions:

**1. Owning version (`measure_with_setup_each`):**
```rust
pub fn measure_with_setup_each<T, S, B>(
    config: &BenchmarkConfig,
    name: &str,
    module: &str,
    mut setup: S,
    mut bench: B,
) -> BenchResult
where
    S: FnMut() -> T,
    B: FnMut(T),  // Takes ownership
{
    // Warmup: run setup + bench together
    let (warmup_ms, warmup_iters) = warmup_with_setup(&mut setup, &mut bench, ...);

    // Measurement
    let mut all_timings = Vec::with_capacity(config.measurement.samples);

    for _ in 0..config.measurement.samples {
        let data = setup();  // Fresh data each time

        let start = Instant::now();
        bench(data);  // Consumes data
        all_timings.push(start.elapsed());
    }

    // Return BenchResult
}
```

**2. Borrowing version (`measure_with_setup_each_ref`):**
```rust
pub fn measure_with_setup_each_ref<T, S, B>(
    config: &BenchmarkConfig,
    name: &str,
    module: &str,
    mut setup: S,
    mut bench: B,
) -> BenchResult
where
    S: FnMut() -> T,
    B: FnMut(&T),  // Takes reference
{
    // Warmup
    let (warmup_ms, warmup_iters) = warmup_with_setup_ref(&mut setup, &mut bench, ...);

    // Measurement
    let mut all_timings = Vec::with_capacity(config.measurement.samples);

    for _ in 0..config.measurement.samples {
        let data = setup();  // Fresh data each time

        let start = Instant::now();
        bench(&data);  // Borrows data
        all_timings.push(start.elapsed());

        drop(data);  // Explicit drop (happens anyway)
    }

    // Return BenchResult
}
```

#### Warmup Consideration

For `setup_each`, warmup runs setup + bench together in exponential batches:
```rust
fn warmup_with_setup<T, S, B>(
    setup: &mut S,
    bench: &mut B,
    duration: Duration,
    bench_name: &str,
) -> (u128, u64)
where
    S: FnMut() -> T,
    B: FnMut(T),
{
    let start = Instant::now();
    let mut total = 0u64;
    let mut batch_size = 1u64;

    while start.elapsed() < duration {
        for _ in 0..batch_size {
            let data = setup();
            bench(data);
            total += 1;
        }
        batch_size *= 2;
    }

    (start.elapsed().as_millis(), total)
}
```

This warms up both setup and benchmark together, which is realistic.

## File Changes Summary

### simplebench-runtime/src/config.rs
- Remove `iterations: usize` from `MeasurementConfig`
- Remove `default_iterations()` function
- Remove `SIMPLEBENCH_ITERATIONS` env var handling in `apply_env_overrides()`
- Update `Default` impl for `MeasurementConfig`
- Update tests that reference iterations

### simplebench-runtime/src/measurement.rs
- Remove iteration loops from:
  - `warmup_benchmark()` - remove inner `for _ in 0..iterations` loop
  - `warmup_closure()` - remove inner `for _ in 0..iterations` loop
  - `measure_closure()` - remove inner `for _ in 0..iterations` loop
- Remove `iterations` parameter from function signatures
- Add `measure_with_setup_each()` - owning version (takes `T`)
- Add `measure_with_setup_each_ref()` - borrowing version (takes `&T`)
- Add `warmup_with_setup()` and `warmup_with_setup_ref()` helpers
- Delete `measure_function_impl()` (legacy, unused with new architecture)
- Delete `measure_with_warmup()` (legacy wrapper)
- Update `validate_measurement_params()` - remove iterations validation

### simplebench-runtime/src/lib.rs
- Remove `iterations: usize` from `BenchResult` struct
- Remove `iterations` from `BenchResult::default()`
- Update any display/serialization that references `result.iterations`
- Add re-exports for new measurement functions

### simplebench-macros/src/lib.rs
- Add `setup_each_expr: Option<Expr>` parsing in attribute handling
- Add `is_reference_param()` helper to detect `&T` vs `T` parameter
- Add `generate_with_setup_each()` code generation function
- Update validation:
  - `setup_each` requires function parameter
  - Cannot use both `setup` and `setup_each`
- Remove unused `_iterations` and `_samples` parsing (config-only, not macro attrs)

### cargo-simplebench/src/main.rs
- Remove `iterations: Option<usize>` from `RunConfig` struct
- Remove `--iterations` / `-i` CLI argument from clap definition
- Remove `SIMPLEBENCH_ITERATIONS` env var setting in `build_env()`
- Update help text/documentation

### cargo-simplebench/src/output.rs
- Remove iterations from result display formatting
- Update JSON output format (if iterations was included)

### Test Updates
- `simplebench-runtime/src/config.rs`: Update `test_default_config`, `test_env_overrides`, etc.
- `simplebench-runtime/src/measurement.rs`: Update `test_measure_function_basic`, `test_validate_measurement_params`
- `test-workspace/`: Update any benchmarks if needed
- Add new tests for `setup_each` with both `&T` and `T` variants

## Migration Notes

### Baseline Compatibility

Existing baselines store `iterations` field. Options:
1. **Breaking change**: New baselines incompatible with old
2. **Default fallback**: Treat missing `iterations` as 1 when loading old baselines
3. **Version field**: Add version to baseline format

Recommendation: Option 1 (breaking change) - baselines are ephemeral and should be regenerated anyway.

### User Migration

- Users with `--iterations N` in scripts will get an error. Document in changelog.
- Users with `SIMPLEBENCH_ITERATIONS` env var set will have it ignored (no error, just unused).
- Users with `iterations` in `simplebench.toml` will get a deserialization warning/error.

### Config File Migration

Old `simplebench.toml`:
```toml
[measurement]
samples = 1000
iterations = 1000  # <-- Remove this
warmup_duration_secs = 3
```

New `simplebench.toml`:
```toml
[measurement]
samples = 1000
warmup_duration_secs = 3
```

## Implementation Order

1. **Runtime: Remove iterations from config**
   - `config.rs`: Remove `iterations` field, default fn, env var handling
   - This will cause compile errors that guide remaining changes

2. **Runtime: Remove iterations from measurement**
   - `measurement.rs`: Remove iteration loops from warmup/measure functions
   - Remove `iterations` parameter from function signatures
   - Delete legacy functions (`measure_function_impl`, `measure_with_warmup`)

3. **Runtime: Remove iterations from BenchResult**
   - `lib.rs`: Remove `iterations` field from `BenchResult`
   - Update any code referencing `result.iterations`

4. **Runtime: Add setup_each measurement functions**
   - `measurement.rs`: Add `measure_with_setup_each()` and `measure_with_setup_each_ref()`
   - Add warmup helpers for setup_each

5. **Macro: Add setup_each support**
   - Parse `setup_each` attribute
   - Add `is_reference_param()` helper
   - Add `generate_with_setup_each()` code generation
   - Update validation logic

6. **CLI: Remove --iterations flag**
   - `main.rs`: Remove from `RunConfig`, clap args, env building

7. **Output: Update display**
   - `output.rs`: Remove iterations from result formatting

8. **Tests: Update and add**
   - Fix all broken tests
   - Add tests for `setup_each` with `&T` and `T`

9. **Documentation**
   - Update README with new `setup_each` syntax
   - Update docstrings

## Testing Strategy

1. **Unit tests**: Verify measurement functions work without iterations
2. **Integration tests**: Run existing benchmarks in test-workspace, verify they produce results
3. **New tests**: Add benchmarks using `setup_each`:
   - `setup_each` with `&T` (borrowing, fresh data each sample)
   - `setup_each` with `T` (owning, consuming operations like sort)
4. **Compile tests**: Verify error messages for invalid attribute combinations:
   - `setup` + `setup_each` together → error
   - `setup_each` without parameter → error
   - `setup` with non-reference parameter → error (existing)
5. **Manual verification**: Run `cargo simplebench` on test-workspace and verify output format
