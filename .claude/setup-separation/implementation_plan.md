# Setup Separation Implementation Plan

## Problem Summary

SimpleBench measures the entire benchmark function body on every iteration. With 1000 samples × 1000 iterations, setup code runs **1,000,000+ times** instead of once.

## Solution: rstest-Style Setup Attribute

Benchmarks with setup specify it in the attribute. The macro detects the pattern from the function signature:

```rust
// Simple benchmark - no setup, no params → measure whole function
#[bench]
fn simple_benchmark() {
    quick_operation();
}

// Benchmark with setup - setup attr + params → setup once, pass &data
#[bench(setup = create_data)]
fn benchmark_with_setup(data: &Data) {
    operation(data);  // Only this runs per iteration
}

// Inline setup closure
#[bench(setup = || random_vectors(1000))]
fn benchmark_inline(data: &Vec<Vec3>) {
    for v in data { v.normalize(); }
}
```

## Detection Logic

| `setup` attr | Function params | Behavior |
|--------------|-----------------|----------|
| No | None | Measure entire function (backward compatible) |
| Yes | Has `&T` param | Setup once, pass `&T` to each iteration |
| No | Has params | **Compile error** |
| Yes | None | **Compile error** |

## Architecture

### SimpleBench Struct Change

```rust
// Before
pub struct SimpleBench {
    pub name: &'static str,
    pub module: &'static str,
    pub func: fn(),
}

// After
pub struct SimpleBench {
    pub name: &'static str,
    pub module: &'static str,
    pub run: fn(&BenchmarkConfig) -> BenchResult,
}
```

Config goes in, result comes out. No thread-locals needed.

### Runtime Functions

```rust
// For simple benchmarks (no setup)
pub fn measure_simple<F>(
    config: &BenchmarkConfig,
    name: &str,
    module: &str,
    func: F,
) -> BenchResult
where
    F: FnMut();

// For benchmarks with setup
pub fn measure_with_setup<T, S, B>(
    config: &BenchmarkConfig,
    name: &str,
    module: &str,
    setup: S,
    bench: B,
) -> BenchResult
where
    S: FnOnce() -> T,
    B: FnMut(&T);
```

### Macro Expansion

**Simple benchmark (no setup):**
```rust
// User writes:
#[bench]
fn simple_benchmark() {
    quick_operation();
}

// Macro generates:
fn simple_benchmark() {
    quick_operation();
}

fn __simplebench_run_simple_benchmark(config: &BenchmarkConfig) -> BenchResult {
    simplebench_runtime::measure_simple(
        config,
        "simple_benchmark",
        module_path!(),
        || simple_benchmark(),
    )
}

inventory::submit! {
    SimpleBench {
        name: "simple_benchmark",
        module: module_path!(),
        run: __simplebench_run_simple_benchmark,
    }
}
```

**Benchmark with setup:**
```rust
// User writes:
#[bench(setup = create_data)]
fn benchmark_with_setup(data: &Data) {
    operation(data);
}

// Macro generates:
fn benchmark_with_setup(data: &Data) {
    operation(data);
}

fn __simplebench_run_benchmark_with_setup(config: &BenchmarkConfig) -> BenchResult {
    simplebench_runtime::measure_with_setup(
        config,
        "benchmark_with_setup",
        module_path!(),
        || create_data(),
        |data| benchmark_with_setup(data),
    )
}

inventory::submit! {
    SimpleBench {
        name: "benchmark_with_setup",
        module: module_path!(),
        run: __simplebench_run_benchmark_with_setup,
    }
}
```

---

## Implementation Phases

### Phase 1: Runtime Changes

#### 1.1 Update SimpleBench Struct

**File**: `simplebench-runtime/src/lib.rs`

```rust
pub struct SimpleBench {
    pub name: &'static str,
    pub module: &'static str,
    pub run: fn(&BenchmarkConfig) -> BenchResult,
}
```

#### 1.2 Add measure_simple Function

**File**: `simplebench-runtime/src/measurement.rs`

```rust
pub fn measure_simple<F>(
    config: &BenchmarkConfig,
    name: &str,
    module: &str,
    mut func: F,
) -> BenchResult
where
    F: FnMut(),
{
    // Warmup
    let (warmup_ms, warmup_iters) = warmup_closure(
        &mut func,
        Duration::from_secs(config.measurement.warmup_duration_secs),
        config.measurement.iterations,
    );

    // Measurement
    let timings = measure_closure(
        &mut func,
        config.measurement.iterations,
        config.measurement.samples,
    );

    BenchResult {
        name: name.to_string(),
        module: module.to_string(),
        iterations: config.measurement.iterations,
        samples: config.measurement.samples,
        percentiles: calculate_percentiles(&timings),
        all_timings: timings,
        cpu_samples: vec![],
        warmup_ms: Some(warmup_ms),
        warmup_iterations: Some(warmup_iters),
    }
}
```

#### 1.3 Add measure_with_setup Function

**File**: `simplebench-runtime/src/measurement.rs`

```rust
pub fn measure_with_setup<T, S, B>(
    config: &BenchmarkConfig,
    name: &str,
    module: &str,
    setup: S,
    mut bench: B,
) -> BenchResult
where
    S: FnOnce() -> T,
    B: FnMut(&T),
{
    // Run setup ONCE
    let data = setup();

    // Create closure that borrows data
    let mut func = || bench(&data);

    // Warmup
    let (warmup_ms, warmup_iters) = warmup_closure(
        &mut func,
        Duration::from_secs(config.measurement.warmup_duration_secs),
        config.measurement.iterations,
    );

    // Measurement
    let timings = measure_closure(
        &mut func,
        config.measurement.iterations,
        config.measurement.samples,
    );

    BenchResult {
        name: name.to_string(),
        module: module.to_string(),
        iterations: config.measurement.iterations,
        samples: config.measurement.samples,
        percentiles: calculate_percentiles(&timings),
        all_timings: timings,
        cpu_samples: vec![],
        warmup_ms: Some(warmup_ms),
        warmup_iterations: Some(warmup_iters),
    }
}
```

#### 1.4 Add Helper Functions

```rust
fn warmup_closure<F>(func: &mut F, duration: Duration, iterations: usize) -> (u128, u64)
where
    F: FnMut(),
{
    let start = Instant::now();
    let mut total_iterations = 0u64;
    let mut batch_size = 1u64;

    while start.elapsed() < duration {
        for _ in 0..batch_size {
            for _ in 0..iterations {
                func();
            }
        }
        total_iterations += batch_size * (iterations as u64);
        batch_size *= 2;
    }

    (start.elapsed().as_millis(), total_iterations)
}

fn measure_closure<F>(func: &mut F, iterations: usize, samples: usize) -> Vec<Duration>
where
    F: FnMut(),
{
    let mut timings = Vec::with_capacity(samples);

    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iterations {
            func();
        }
        timings.push(start.elapsed());
    }

    timings
}
```

#### 1.5 Update run_and_stream_benchmarks

**File**: `simplebench-runtime/src/lib.rs`

```rust
// Change from:
let result = measure_with_warmup(
    bench.name.to_string(),
    bench.module.to_string(),
    bench.func,
    config.measurement.iterations,
    config.measurement.samples,
    config.measurement.warmup_duration_secs,
);

// To:
let result = (bench.run)(config);
```

#### 1.6 Update run_single_benchmark_json

Same pattern - call `(bench.run)(config)` instead of `measure_with_warmup`.

---

### Phase 2: Macro Changes

#### 2.1 Parse Setup Attribute

**File**: `simplebench-macros/src/lib.rs`

```rust
#[proc_macro_attribute]
pub fn bench(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let input_fn = parse_macro_input!(input as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    let has_params = !input_fn.sig.inputs.is_empty();

    // Parse setup attribute
    let mut setup_expr: Option<Expr> = None;
    for arg in args {
        if let Meta::NameValue(nv) = arg {
            if nv.path.is_ident("setup") {
                setup_expr = Some(nv.value);
            }
        }
    }

    // Validate combinations
    match (setup_expr.is_some(), has_params) {
        (false, true) => {
            return syn::Error::new_spanned(
                &input_fn.sig,
                "benchmark with parameters requires #[bench(setup = ...)]"
            ).to_compile_error().into();
        }
        (true, false) => {
            return syn::Error::new_spanned(
                &input_fn.sig,
                "#[bench(setup = ...)] requires function to accept &T parameter"
            ).to_compile_error().into();
        }
        _ => {}
    }

    if let Some(setup) = setup_expr {
        generate_with_setup(fn_name, &fn_name_str, &input_fn, setup)
    } else {
        generate_simple(fn_name, &fn_name_str, &input_fn)
    }
}
```

#### 2.2 Generate Simple Benchmark

```rust
fn generate_simple(
    fn_name: &Ident,
    fn_name_str: &str,
    input_fn: &ItemFn,
) -> TokenStream {
    let run_fn_name = format_ident!("__simplebench_run_{}", fn_name);
    let module_path = quote! { module_path!() };

    quote! {
        #input_fn

        fn #run_fn_name(
            config: &::simplebench_runtime::BenchmarkConfig
        ) -> ::simplebench_runtime::BenchResult {
            ::simplebench_runtime::measure_simple(
                config,
                #fn_name_str,
                #module_path,
                || #fn_name(),
            )
        }

        ::simplebench_runtime::inventory::submit! {
            ::simplebench_runtime::SimpleBench {
                name: #fn_name_str,
                module: #module_path,
                run: #run_fn_name,
            }
        }
    }.into()
}
```

#### 2.3 Generate Benchmark With Setup

```rust
fn generate_with_setup(
    fn_name: &Ident,
    fn_name_str: &str,
    input_fn: &ItemFn,
    setup_expr: Expr,
) -> TokenStream {
    let run_fn_name = format_ident!("__simplebench_run_{}", fn_name);
    let module_path = quote! { module_path!() };

    quote! {
        #input_fn

        fn #run_fn_name(
            config: &::simplebench_runtime::BenchmarkConfig
        ) -> ::simplebench_runtime::BenchResult {
            ::simplebench_runtime::measure_with_setup(
                config,
                #fn_name_str,
                #module_path,
                || #setup_expr,
                |data| #fn_name(data),
            )
        }

        ::simplebench_runtime::inventory::submit! {
            ::simplebench_runtime::SimpleBench {
                name: #fn_name_str,
                module: #module_path,
                run: #run_fn_name,
            }
        }
    }.into()
}
```

---

### Phase 3: Update Test Workspace

#### 3.1 game-math/src/lib.rs

```rust
#[cfg(test)]
mod benchmarks {
    use super::*;
    use super::tests::random_vectors;
    use simplebench_macros::bench;

    #[bench(setup = || random_vectors(1000))]
    fn bench_vec3_normalize(vectors: &Vec<Vec3>) {
        for v in vectors {
            let _normalized = v.normalize();
        }
    }

    #[bench(setup = || random_vectors(100))]
    fn bench_vec3_cross_product(vectors: &Vec<Vec3>) {
        for i in 0..vectors.len() - 1 {
            let _result = vectors[i].cross(&vectors[i + 1]);
        }
    }

    #[bench(setup = || random_vectors(500))]
    fn bench_matrix_transform_batch(vectors: &Vec<Vec3>) {
        for v in vectors {
            let _transformed = Vec3::new(
                v.x * 0.866 - v.y * 0.5,
                v.x * 0.5 + v.y * 0.866,
                v.z
            );
        }
    }
}
```

#### 3.2 Update Other Crates

Apply same pattern to `game-entities` and `game-physics`.

---

## File Changes Summary

| File | Change |
|------|--------|
| `simplebench-runtime/src/lib.rs` | Update `SimpleBench` struct, update execution |
| `simplebench-runtime/src/measurement.rs` | Add `measure_simple`, `measure_with_setup`, helpers |
| `simplebench-macros/src/lib.rs` | Parse `setup` attr, generate appropriate wrapper |
| `test-workspace/*/src/lib.rs` | Update benchmarks to use setup pattern |

---

## Testing Checklist

- [ ] Simple benchmarks (no setup) still work
- [ ] Setup benchmarks run setup exactly once
- [ ] Compile error if setup attr without params
- [ ] Compile error if params without setup attr
- [ ] Inline closures work: `#[bench(setup = || expr)]`
- [ ] Function references work: `#[bench(setup = my_setup_fn)]`
- [ ] Timing accuracy unchanged
- [ ] Warmup works correctly with both patterns

---

## Migration Guide

```markdown
## Migrating Benchmarks with Setup

If your benchmark has setup code that shouldn't run every iteration,
add the `setup` attribute:

Before (setup runs 1M+ times - SLOW):
```rust
#[bench]
fn benchmark() {
    let data = expensive_setup();  // Runs every iteration!
    operation(&data);
}
```

After (setup runs once):
```rust
#[bench(setup = || expensive_setup())]
fn benchmark(data: &Data) {
    operation(data);  // Only this runs per iteration
}
```

Or with a named setup function:
```rust
fn make_test_data() -> Data {
    expensive_setup()
}

#[bench(setup = make_test_data)]
fn benchmark(data: &Data) {
    operation(data);
}
```

Simple benchmarks without setup remain unchanged:
```rust
#[bench]
fn simple_benchmark() {
    quick_operation();  // Whole function measured
}
```
```
