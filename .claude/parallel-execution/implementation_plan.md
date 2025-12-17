# Plan: Parallel Benchmark Execution

## Overview

Refactor SimpleBench so the runner is a pure JSON-outputting executor, with all formatting handled by the orchestrator. Sequential mode becomes "parallel with 1 core" - same code path.

## Design Decisions

1. **Core 0 always reserved** - even sequential mode uses core 1 (not core 0)
2. **One runner per benchmark** - spawn N runners, each runs exactly one benchmark
3. **Discovery via --list** - runner supports `--list` flag to output benchmark names as JSON

## Design Principle

**Runner**: Pure executor. Takes single benchmark name + core, outputs JSON result to stdout.
**Orchestrator**: Owns all user-facing output. Spawns runner(s), parses JSON, pretty-prints.

**Key insight**: Sequential and parallel are identical - both spawn one runner per benchmark. The only difference is how many cores are used:
- Sequential: `cores = [1]` → run benchmarks one at a time on core 1
- Parallel: `cores = [1,2,3,4,5,6,7]` → run up to 7 benchmarks simultaneously

No `is_parallel_worker` conditionals. No difference in execution logic whatsoever.

## Key Changes

### 1. Runner Changes (`simplebench-runtime`)

**Current**: `run_and_stream_benchmarks()` prints headers, results, summaries directly.

**New**: Runner is a pure JSON executor with two modes:

```rust
// List mode: output benchmark names
pub fn list_benchmarks_json() {
    let names: Vec<_> = inventory::iter::<SimpleBench>()
        .map(|b| json!({"name": b.name, "module": b.module}))
        .collect();
    println!("{}", serde_json::to_string(&names).unwrap());
}

// Run mode: execute single benchmark, output JSON result
pub fn run_single_benchmark_json(bench_name: &str, config: &BenchmarkConfig) {
    let pin_core = env::var("SIMPLEBENCH_PIN_CORE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);  // Default to core 1, not 0

    affinity::set_thread_affinity([pin_core]).ok();

    for bench in inventory::iter::<SimpleBench>() {
        if bench.name == bench_name {
            let result = measure_with_warmup(bench, &config);
            println!("{}", serde_json::to_string(&result).unwrap());
            return;
        }
    }
}
```

### 2. Runner Generation Changes (`cargo-simplebench/src/runner_gen.rs`)

**New**: Generated runner supports `--list` and single benchmark execution.

```rust
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config = BenchmarkConfig::load();

    if args.contains(&"--list".to_string()) {
        simplebench_runtime::list_benchmarks_json();
    } else if let Ok(bench_name) = std::env::var("SIMPLEBENCH_BENCH_FILTER") {
        simplebench_runtime::run_single_benchmark_json(&bench_name, &config);
    }
}
```

### 3. Orchestrator Changes (`cargo-simplebench/src/main.rs`)

**New flow:**
1. Print header ("Analyzing workspace...")
2. Build & compile runner
3. Run `runner --list` to discover benchmarks → parse JSON
4. Print "Running N benchmarks on M core(s)..."
5. Spawn one runner per benchmark (up to M in parallel)
6. **Poll for completion, print results as they arrive**
7. Print summary

**Sequential = parallel with 1 core:**
```rust
struct RunningBench {
    name: String,
    core: usize,
    child: Child,
}

fn run_benchmarks(runner: &Path, benchmarks: &[String], parallel: bool) -> Result<Vec<BenchResult>> {
    let cores = if parallel {
        topology::get_usable_cores()  // [1, 2, 3, 4, 5, 6, 7]
    } else {
        vec![1]  // Single core 1
    };

    let mut all_results = Vec::new();

    // Process benchmarks in batches (batch size = number of cores)
    for batch in benchmarks.chunks(cores.len()) {
        // Spawn all runners in this batch
        let mut running: Vec<RunningBench> = batch.iter().enumerate()
            .map(|(i, bench_name)| {
                let core = cores[i];
                let child = Command::new(runner)
                    .env("SIMPLEBENCH_BENCH_FILTER", bench_name)
                    .env("SIMPLEBENCH_PIN_CORE", core.to_string())
                    .stdout(Stdio::piped())
                    .spawn()
                    .expect("Failed to spawn runner");
                RunningBench { name: bench_name.clone(), core, child }
            })
            .collect();

        // Poll until all complete, collecting results as they finish
        while !running.is_empty() {
            std::thread::sleep(Duration::from_millis(50));

            running = running.into_iter()
                .filter_map(|mut bench| {
                    match bench.child.try_wait() {
                        Ok(Some(_)) => {
                            // Completed - collect result
                            if let Ok(output) = bench.child.wait_with_output() {
                                if let Ok(result) = serde_json::from_slice::<BenchResult>(&output.stdout) {
                                    print_benchmark_result(&result, bench.core);
                                    all_results.push(result);
                                }
                            }
                            None  // Remove from running
                        }
                        Ok(None) => Some(bench),  // Still running
                        Err(_) => None,           // Error, remove
                    }
                })
                .collect();
        }
    }

    Ok(all_results)
}
```

### 4. New Module: `cargo-simplebench/src/topology.rs`

```rust
pub fn get_usable_cores() -> Vec<usize> {
    // Read /sys/devices/system/cpu/cpuN/topology/thread_siblings_list
    // Return one CPU per physical core, excluding core 0
    // Fallback: vec![1] if detection fails
}
```

### 5. New/Move: `cargo-simplebench/src/output.rs`

Move pretty-printing from `simplebench-runtime/src/output.rs`:
```rust
pub fn print_benchmark_result(result: &BenchResult, core: usize);
pub fn print_comparison(comparison: &Comparison, is_regression: bool);
pub fn print_summary(comparisons: &[ComparisonResult], config: &ComparisonConfig);
```

## Files to Modify

1. **`simplebench-runtime/src/lib.rs`**
   - Add `list_benchmarks_json()` function
   - Add `run_single_benchmark_json()` function
   - Keep existing functions for backwards compatibility initially

2. **`cargo-simplebench/src/runner_gen.rs`**
   - Generate runner with `--list` support and single-benchmark execution

3. **`cargo-simplebench/src/main.rs`**
   - Add `--parallel` flag (default: false/sequential)
   - Add `--jobs N` flag
   - Run `--list` to discover benchmarks
   - Implement runner spawning (one per benchmark)
   - **Poll with `try_wait()` + `filter_map()` for streaming output**
   - Parse JSON results
   - Call output functions for pretty-printing

4. **`cargo-simplebench/src/topology.rs`** (new)
   - `get_usable_cores() -> Vec<usize>`
   - Read sysfs, exclude core 0, one per physical core

5. **`cargo-simplebench/src/output.rs`** (new or move from runtime)
   - Pretty-printing functions for results/comparisons/summary

## CLI Changes

```bash
cargo simplebench              # Sequential (core 1 only)
cargo simplebench --parallel   # Parallel (cores 1-7 auto-detected)
cargo simplebench --jobs 4     # Parallel with 4 cores
```

## Execution Flow

```
1. cargo-simplebench starts
2. Analyze workspace, build, compile runner
3. Run `runner --list` → get benchmark names as JSON
4. Print header: "Running N benchmarks on M core(s)..."
5. Spawn runners in batches (batch size = number of cores):
   - Sequential: batch of 1, core 1
   - Parallel: batch of 7, cores 1-7
6. Poll with try_wait(), print results as each runner completes
7. Each runner: FILTER="bench_name" PIN_CORE=N → JSON result
8. After all batches complete, print summary
```

## Benefits

- No `is_parallel_worker` conditionals anywhere
- Runner is pure: single benchmark name → JSON result
- Same code path for sequential/parallel (just different batch size)
- All formatting in orchestrator
- **True streaming output** - results print as benchmarks finish
- Easy to test runner in isolation
- Clean separation of concerns
