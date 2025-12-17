# Parallel Benchmark Execution Research

## Executive Summary

SimpleBench can be parallelized using **existing infrastructure** with minimal changes:

1. Add `SIMPLEBENCH_PIN_CORE` env var - runner reads this to set CPU affinity
2. Add `SIMPLEBENCH_PARALLEL_WORKER` env var - runner outputs only benchmark results (no headers/summaries)
3. Move header/summary output to `cargo-simplebench` orchestrator
4. `cargo-simplebench --parallel` spawns up to 7 runner processes
5. Each runner gets `SIMPLEBENCH_BENCH_FILTER` + `SIMPLEBENCH_PIN_CORE` + `SIMPLEBENCH_PARALLEL_WORKER`
6. Results stream to user as each benchmark completes (same format as today)

**Default behavior**: Sequential (current stable behavior)
**Parallel mode**: Opt-in via `--parallel` flag (experimental)
**Expected speedup**: ~7x on 8-core system

## Current Architecture

### How SimpleBench Works Today

```
cargo-simplebench:
  1. Build workspace
  2. Generate & compile runner
  3. Spawn single runner process
  4. Runner prints everything: headers, results, summaries
```

### Current Output (from runner)

```
Set affinity to core 0

Running benchmarks with 1000 samples × 1000 iterations

game_math::vector_add
     warmup    5.2s (1.2M iterations)
     mean      1.234 µs ± 0.5%
     p50       1.230 µs
     p90       1.280 µs
     p99       1.350 µs
     vs baseline: +0.8% (no regression)

game_math::matrix_multiply
     ...

Summary: 8 benchmarks, 0 regressions
```

## The Output Problem

In parallel mode, we spawn 7 runners each with a different `SIMPLEBENCH_BENCH_FILTER`. But `run_and_stream_benchmarks()` assumes it's the only process and prints:

1. **Line 293-300**: `"Set affinity to core 0"`
2. **Line 331-338**: `"Running benchmarks with 1000 samples × 1000 iterations"`
3. **Line 340-354**: `"Filtering to 1 (1 benchmark matched filter: "vector_add")"`
4. **Line 432-444**: Summary footer with filter stats

**If we naively spawn 7 runners:**

```
Set affinity to core 0
Running benchmarks with 1000 samples × 1000 iterations
Filtering to 1 (1 benchmark matched filter: "vector_add")

Set affinity to core 0
Running benchmarks with 1000 samples × 1000 iterations
Filtering to 1 (1 benchmark matched filter: "matrix_multiply")

Set affinity to core 0
Running benchmarks with 1000 samples × 1000 iterations
Filtering to 1 (1 benchmark matched filter: "dot_product")

... (7 times, interleaved unpredictably)
```

**Problems:**
- User didn't request any filter - confusing messages
- "Set affinity to core 0" repeated 7 times (and wrong - each is different)
- Headers duplicated and interleaved
- 7 summary footers

## Solution: Move Headers to Orchestrator

Instead of suppressing useful output, **move it to the orchestrator**. The runner in parallel mode outputs only the benchmark results (which we want to stream immediately). The orchestrator prints headers once, then streams results as they arrive.

### Output Responsibility Split

| Output | Sequential Mode | Parallel Mode |
|--------|-----------------|---------------|
| "Set affinity to core N" | Runner | Orchestrator (once, lists all cores) |
| "Running benchmarks with..." | Runner | Orchestrator (once) |
| Filter message | Runner | **Never** (internal detail) |
| Benchmark results | Runner | Runner (streamed, unchanged format) |
| Summary | Runner | Orchestrator (aggregated) |

### Parallel Mode Output (User Experience)

```
Analyzing workspace...
     Found 3 benchmark crates (8 benchmarks total)

Compiling workspace (bench profile)
Generating benchmark runner
Compiling runner

Running 8 benchmarks in parallel (experimental)
     Using cores: 1, 2, 3, 4, 5, 6, 7
     1000 samples × 1000 iterations

game_math::vector_add                      [core 1]
     warmup    5.1s (1.2M iterations)
     mean      1.234 µs ± 0.5%
     p50       1.230 µs
     p90       1.280 µs
     p99       1.350 µs
     vs baseline: +0.8% (no regression)

game_math::dot_product                     [core 3]
     warmup    5.0s (1.1M iterations)
     mean      0.891 µs ± 0.4%
     p50       0.889 µs
     p90       0.920 µs
     p99       0.980 µs
     vs baseline: +0.3% (no regression)

game_math::matrix_multiply                 [core 2]
     warmup    5.2s (980K iterations)
     mean      5.670 µs ± 0.6%
     ...

... (results stream as benchmarks complete, order may vary)

Summary: 8 benchmarks, 0 regressions, 2 improvements
```

**Key points:**
- Header printed once by orchestrator
- Core assignments shown upfront
- No filter messages (user didn't request filter)
- Benchmark results **unchanged format** - just `[core N]` annotation added
- Results stream immediately as each runner completes
- Single aggregated summary at end

### Runner Changes

When `SIMPLEBENCH_PARALLEL_WORKER` is set:

```rust
pub fn run_and_stream_benchmarks(config: &BenchmarkConfig) -> Vec<BenchResult> {
    let is_parallel_worker = std::env::var("SIMPLEBENCH_PARALLEL_WORKER").is_ok();
    let pin_core = std::env::var("SIMPLEBENCH_PIN_CORE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    // Set affinity (always do this)
    match affinity::set_thread_affinity([pin_core]) {
        Ok(_) => {
            // Only print in sequential mode
            if !is_parallel_worker {
                println!("{} {}\n", "Set affinity to core".green().bold(),
                         pin_core.to_string().cyan().bold());
            }
        }
        Err(e) => eprintln!("Failed to set core affinity {e:?}"),
    };

    // Only print header in sequential mode
    if !is_parallel_worker {
        println!(
            "{} {} {} {} {}",
            "Running benchmarks with".green().bold(),
            config.measurement.samples,
            "samples ×".green().bold(),
            config.measurement.iterations,
            "iterations".green().bold()
        );

        // Only show filter message if user explicitly set it AND not parallel
        if let Some(ref filter) = bench_filter {
            println!("Filtering to {} ...", filtered_count);
        }
    }

    // ... run benchmarks ...

    // ALWAYS print benchmark results (this is the streaming output we want!)
    print_benchmark_result_line(&result);
    print_comparison_line(...);

    // Only print summary in sequential mode
    if !is_parallel_worker && !comparisons.is_empty() {
        print_streaming_summary(&comparisons, &config.comparison);
    }

    results
}
```

**What stays the same:**
- `print_benchmark_result_line(&result)` - always called, streams results
- `print_comparison_line(...)` - always called, shows baseline comparison
- Benchmark output format unchanged

**What's conditional:**
- Header ("Set affinity...", "Running benchmarks with...")
- Filter message (never shown in parallel mode)
- Summary footer

### Orchestrator Changes

```rust
fn run_parallel(runner: &Path, benchmarks: &[String], cores: &[usize], config: &RunConfig) -> Result<()> {
    // Print header ONCE
    println!("{}", "Running benchmarks in parallel (experimental)".green().bold());
    println!("     Using cores: {}", cores.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(", "));
    println!("     {} samples × {} iterations\n",
             config.samples.unwrap_or(1000),
             config.iterations.unwrap_or(1000));

    let mut all_results = Vec::new();

    // Spawn in waves
    for batch in benchmarks.chunks(cores.len()) {
        let mut children: Vec<(String, usize, Child)> = Vec::new();

        for (i, bench) in batch.iter().enumerate() {
            let core = cores[i];
            let child = Command::new(runner)
                .env("SIMPLEBENCH_BENCH_FILTER", bench)
                .env("SIMPLEBENCH_PIN_CORE", core.to_string())
                .env("SIMPLEBENCH_PARALLEL_WORKER", "1")
                .stdout(Stdio::piped())
                .spawn()?;
            children.push((bench.clone(), core, child));
        }

        // Stream results as they complete
        for (bench, core, mut child) in children {
            let output = child.wait_with_output()?;
            // Output already includes benchmark results - just print with core annotation
            // Could parse and re-format, or just pass through
            print_with_core_annotation(&output.stdout, core);
        }
    }

    // Print aggregated summary
    print_summary(&all_results);

    Ok(())
}
```

## System Topology

### Your Machine (8 physical cores, hyperthreading enabled)

```
Physical Core 0: CPU 0, CPU 8   <- Reserved for OS
Physical Core 1: CPU 1, CPU 9   <- Use CPU 1
Physical Core 2: CPU 2, CPU 10  <- Use CPU 2
Physical Core 3: CPU 3, CPU 11  <- Use CPU 3
Physical Core 4: CPU 4, CPU 12  <- Use CPU 4
Physical Core 5: CPU 5, CPU 13  <- Use CPU 5
Physical Core 6: CPU 6, CPU 14  <- Use CPU 6
Physical Core 7: CPU 7, CPU 15  <- Use CPU 7

Usable cores for benchmarks: [1, 2, 3, 4, 5, 6, 7]
```

### Core Selection Rules

1. **Reserve core 0** - OS kernel processes default here
2. **Use only one sibling per physical core** - avoid HT interference
3. **Read topology from sysfs**: `/sys/devices/system/cpu/cpuN/topology/thread_siblings_list`

### Topology Detection

```rust
// cargo-simplebench/src/topology.rs

pub fn get_usable_cores() -> Vec<usize> {
    let total_cpus = affinity::get_core_num();
    let mut physical_cores = Vec::new();
    let mut seen_physical = HashSet::new();

    for cpu in 0..total_cpus {
        let siblings = read_thread_siblings(cpu);
        let physical_id = *siblings.iter().min().unwrap();

        // Skip core 0's physical core
        if physical_id == 0 {
            continue;
        }

        // Take first sibling of each physical core
        if !seen_physical.contains(&physical_id) {
            seen_physical.insert(physical_id);
            physical_cores.push(physical_id);
        }
    }

    physical_cores  // [1, 2, 3, 4, 5, 6, 7] on your system
}

fn read_thread_siblings(cpu: usize) -> Vec<usize> {
    let path = format!("/sys/devices/system/cpu/cpu{}/topology/thread_siblings_list", cpu);
    std::fs::read_to_string(&path)
        .map(|s| parse_cpu_list(&s))
        .unwrap_or_else(|_| vec![cpu])
}
```

## CLI Interface

```bash
# Default: sequential execution (current stable behavior)
cargo simplebench

# Experimental parallel mode (opt-in)
cargo simplebench --parallel

# Limit parallelism (implies --parallel)
cargo simplebench --jobs 4
```

**Rationale for sequential default:**
- Parallel execution is experimental and needs validation
- Sequential mode is battle-tested and produces reliable results
- Users should explicitly opt-in to experimental features

## Performance Projections

| Benchmarks | Sequential | Parallel (7 cores) | Speedup |
|------------|------------|-------------------|---------|
| 7          | ~42s       | ~6s               | 7.0x    |
| 8          | ~48s       | ~12s (2 waves)    | 4.0x    |
| 14         | ~84s       | ~12s              | 7.0x    |
| 21         | ~126s      | ~18s              | 7.0x    |

## Implementation Summary

### Phase 1: Add Environment Variables to Runner

In `simplebench-runtime/src/lib.rs`:
- Read `SIMPLEBENCH_PIN_CORE` for CPU affinity (default: 0)
- Read `SIMPLEBENCH_PARALLEL_WORKER` to skip headers/summaries
- Keep benchmark result output unchanged (always stream)

### Phase 2: Add Topology Detection

In `cargo-simplebench/src/topology.rs`:
- Read sysfs to detect physical cores
- Exclude core 0
- Return usable core list

### Phase 3: Add --parallel Flag

In `cargo-simplebench/src/main.rs`:
- Add `--parallel` CLI flag
- Add `--jobs N` for explicit parallelism
- Print header once, spawn runners, stream results, print summary

### Phase 4: Benchmark Discovery

Need to know benchmark names before spawning. Options:
- Run runner with `--list` flag to enumerate benchmarks
- Parse inventory at orchestrator level
- Run once to discover, then parallel

## Assumptions

1. **Idle system**: No significant background load
2. **Single socket**: NUMA effects not considered
3. **No thermal throttling**: CPU can sustain parallel load
4. **Independent benchmarks**: No shared mutable state

## References

- [Low Latency Tuning Guide](https://rigtorp.se/low-latency-guide/) - Erik Rigtorp
- [CPU Core Pinning Best Practices](https://manuel.bernhardt.io/posts/2023-11-16-core-pinning/) - Manuel Bernhardt
- [Linux CPU Topology sysfs](https://www.kernel.org/doc/html/latest/admin-guide/cputopology.html)
- [affinity crate](https://docs.rs/affinity/0.1.2/affinity/)
