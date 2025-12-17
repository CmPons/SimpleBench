# Hanging Analysis: cargo-simplebench Runner Execution

## Executive Summary

Analysis of potential hang scenarios in `cargo-simplebench`. **One critical hang scenario identified** in the runner execution phase where child processes can block indefinitely with no timeout mechanism.

## Architecture Overview

The benchmark execution flow:

```
1. Discovery: runner --list → JSON benchmark names
2. Filter: --bench flag applies substring filter in orchestrator
3. Execute: For each benchmark, spawn runner with SIMPLEBENCH_BENCH_FILTER=<exact_name>
4. Wait: Threads wait on child.wait_with_output()
5. Collect: mpsc channel collects results as processes complete
```

## Critical Hang Scenario: Runner Process Deadlock

**Location**: `cargo-simplebench/src/main.rs:518-594` (`run_benchmarks_with_cores`)

### The Problem

When executing benchmarks, the orchestrator:

1. Spawns child processes for each benchmark (line 525-536)
2. Creates threads that call `child.wait_with_output()` (line 546-549)
3. Uses an `mpsc::channel` to collect results (line 541-542)
4. The main thread iterates over the channel receiver (line 554)

**If a child process hangs (e.g., benchmark function has infinite loop):**

```rust
// Line 544-550
for (name, core, child) in children {
    let tx = tx.clone();  // Clone sender into thread
    std::thread::spawn(move || {
        let output = child.wait_with_output();  // BLOCKS FOREVER if child hangs
        let _ = tx.send((name, core, output));  // Never reached
    });  // tx clone is never dropped
}
drop(tx);  // Original sender dropped, but clones still exist in threads!

// Line 554 - HANGS FOREVER
for (name, core, output_result) in rx {  // Waits for all senders to drop
    // ...
}
```

The channel iteration (`for ... in rx`) blocks until ALL senders are dropped. Since hanging threads keep their `tx` clones alive, the main thread waits forever.

### No Timeout Mechanism

There is **no timeout** anywhere in the execution phase:
- `wait_with_output()` blocks indefinitely
- No `try_wait()` polling with timeout
- No process kill after N seconds

### Reproduction Scenario

A benchmark with an infinite loop would trigger this:

```rust
#[bench]
fn infinite_loop_bench() {
    loop {
        std::hint::spin_loop();
    }
}
```

## --bench Flag Analysis

**Question**: Could the `--bench` flag cause hanging due to filtering mismatches?

**Answer**: No, but there's an inconsistency worth noting.

### Filter Matching Comparison

| Phase | Location | Match Type |
|-------|----------|------------|
| Discovery | `list_benchmarks_json()` | All benchmarks (no filter) |
| Orchestrator filter | main.rs:303-310 | **Substring** (`contains`) |
| Single bench mode | lib.rs:203-204 | **Exact** (`==`) |
| Streaming mode | lib.rs:385-390 | **Substring** (`contains`) |

**Why it doesn't cause hangs**: The orchestrator passes the **exact benchmark name** (not the filter pattern) to `SIMPLEBENCH_BENCH_FILTER`:

```rust
// main.rs:527
.env("SIMPLEBENCH_BENCH_FILTER", &bench.name)  // Exact name, not filter pattern
```

So even though the runner uses exact matching, it receives the exact name and succeeds.

### Edge Case: Non-matching Filter

If `--bench nonexistent` is used:
1. Discovery finds all benchmarks
2. Filter excludes all (`contains("nonexistent")` fails)
3. `benchmarks.is_empty()` check at main.rs:312 catches this
4. Exits with error message (no hang)

## Other Analyzed Components (No Hang Risk)

### Build Phase (`rlib_selection.rs`)

Uses `Command::new().output()` which blocks, but:
- `cargo test --no-run` is well-behaved
- `cargo metadata` completes quickly
- `rustc` compilation will fail or succeed, not hang

### Measurement Loop (`measurement.rs`)

Bounded iterations:
- Warmup: Time-bounded (`while start.elapsed() < warmup_duration`)
- Samples: Count-bounded (`for _ in 0..samples`)

Only hangs if user's benchmark function itself hangs (infinite loop).

## Recommendations

### Option 1: Add Process Timeout

Add a timeout to child process waiting:

```rust
use std::time::{Duration, Instant};

// Instead of wait_with_output(), poll with timeout
let start = Instant::now();
let timeout = Duration::from_secs(300); // 5 minute timeout per benchmark

loop {
    match child.try_wait() {
        Ok(Some(status)) => {
            // Process finished
            let output = child.wait_with_output().unwrap();
            break handle_output(output);
        }
        Ok(None) => {
            // Still running
            if start.elapsed() > timeout {
                child.kill().ok();
                break handle_timeout(name);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(e) => break handle_error(e),
    }
}
```

### Option 2: Add --timeout CLI Flag

```rust
#[arg(long, default_value = "300")]
timeout_secs: u64,
```

### Option 3: Watchdog Thread

Spawn a watchdog thread that kills children after timeout:

```rust
let watchdog = std::thread::spawn(move || {
    std::thread::sleep(timeout);
    for child in &mut children {
        child.kill().ok();
    }
});
```

## Summary Table

| Scenario | Risk Level | Hang Possible? | Notes |
|----------|------------|----------------|-------|
| Runner child process hangs | **CRITICAL** | Yes | No timeout, blocks forever |
| --bench filter mismatch | None | No | Handled gracefully |
| Build phase timeout | Low | Unlikely | cargo/rustc are well-behaved |
| Benchmark infinite loop | User error | Yes | User's fault, but still hangs system |

## Files Analyzed

- `cargo-simplebench/src/main.rs` - Main execution flow, process spawning
- `cargo-simplebench/src/compile.rs` - Runner compilation
- `cargo-simplebench/src/runner_gen.rs` - Runner template generation
- `cargo-simplebench/src/rlib_selection.rs` - Build phase
- `simplebench-runtime/src/lib.rs` - Runner runtime, single bench execution
- `simplebench-runtime/src/measurement.rs` - Timing loops
