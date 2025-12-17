# Progress Feedback Research

## Problem Statement

When running benchmarks with long execution times, users have no visibility into progress. With default settings (1000 samples × 1000 iterations + 3s warmup), a single benchmark can take several seconds to complete with zero feedback, making it unclear whether simplebench is:
1. Still running normally
2. Hung on a particular benchmark
3. Making progress through samples

## Current Architecture

### Measurement Flow

```
measure_with_warmup() (measurement.rs:38-60)
├── warmup_benchmark() - Time-based loop (~3s)
│   └── while elapsed < warmup_duration:   <-- PROGRESS POINT 1 (every 100ms)
│       └── for _ in 0..iterations: func()
│       └── batch_size *= 2  (exponential doubling)
└── measure_function_impl() - Main measurement
    └── for sample_idx in 0..samples:      <-- PROGRESS POINT 2 (every 1%)
        └── for _ in 0..iterations: func()
```

### Communication Channel

```
cargo-simplebench (orchestrator)
    ↓
    spawns runner binary with:
      - stdout: Stdio::piped() → JSON result
      - stderr: Stdio::piped() → errors only (currently)
      - env vars for config (SIMPLEBENCH_*)
    ↓
    waits for child.wait_with_output()
    ↓
    parses JSON from stdout
```

**Key constraint**: stdout is reserved for the JSON result. Progress must use stderr.

## Progress Points

### 1. Warmup Phase
- Location: `warmup_benchmark()` (measurement.rs:14)
- Duration: ~3 seconds (configurable)
- Progress metric: elapsed time vs target duration
- Display: "Warming up... 1.5s / 3.0s"

### 2. Sample Collection (Primary)
- Location: `measure_function_impl()` (measurement.rs:79)
- Loop: fixed `for sample_idx in 0..samples`
- Progress metric: sample_idx / samples
- Display: Progress bar with percentage and ETA

## Implementation Design

### Progress Message Protocol (stderr JSON)

Runner emits JSON progress messages to stderr. Each message includes the benchmark name so the orchestrator can correlate progress with the correct progress bar:

```json
{"progress":{"bench":"vector_add","phase":"warmup","elapsed_ms":0,"target_ms":3000}}
{"progress":{"bench":"vector_add","phase":"warmup","elapsed_ms":1500,"target_ms":3000}}
{"progress":{"bench":"vector_add","phase":"warmup","elapsed_ms":3000,"target_ms":3000}}
{"progress":{"bench":"vector_add","phase":"samples","current":0,"total":1000}}
{"progress":{"bench":"vector_add","phase":"samples","current":500,"total":1000}}
{"progress":{"bench":"vector_add","phase":"samples","current":1000,"total":1000}}
{"progress":{"bench":"vector_add","phase":"complete"}}
```

Result still goes to stdout as before (unchanged).

### Orchestrator Progress Bar

Use `indicatif` crate with `MultiProgress` for polished progress bars:

**Sequential mode** (single bar):
```
vector_add  [████████░░░░░░░░░░░░]  42% (420/1000 samples)  ETA: 2.3s
```

**Parallel mode** (up to 4 bars, sorted by progress):
```
matrix_mul    [████████████████████]  100% (1000/1000) ✓
vector_add    [████████████░░░░░░░░]  62% (620/1000)  ETA: 1.1s
collision     [████████░░░░░░░░░░░░]  41% (410/1000)  ETA: 2.3s
entity_spawn  Warming up... 2.1s / 3.0s
```

**Full lifecycle** (what you'd see for one benchmark):
```
vector_add  Warming up... 0.0s / 3.0s
vector_add  Warming up... 1.5s / 3.0s
vector_add  Warming up... 3.0s / 3.0s
vector_add  [░░░░░░░░░░░░░░░░░░░░]   0% (0/1000)
vector_add  [██████████░░░░░░░░░░]  50% (500/1000)  ETA: 1.2s
vector_add  [████████████████████] 100% (1000/1000) ✓
```

### Progress Update Frequency

To avoid overhead and noise:
- **Warmup**: Update every 100ms (time-based)
- **Samples**: Update every `max(1, samples / 100)` samples (~100 updates max)

This gives smooth progress without impacting measurement timing.

## Implementation Locations

### simplebench-runtime changes

1. **New module**: `src/progress.rs`
   - `ProgressMessage` struct with `bench`, `phase`, and phase-specific data
   - `emit_progress(msg)` - writes JSON line to stderr

2. **measurement.rs** modifications:
   - `warmup_benchmark()`: Accept benchmark name, emit warmup progress every 100ms
   - `measure_function_impl()`: Accept benchmark name, emit sample progress at intervals
   - `measure_with_warmup()`: Pass benchmark name through to both functions

3. **lib.rs**:
   - Add `pub mod progress;`
   - Re-export progress types

### cargo-simplebench changes

1. **Cargo.toml**: Add `indicatif` dependency

2. **main.rs** modifications:
   - Add `--quiet` / `-q` flag to `RunConfig` and CLI args
   - Change from `wait_with_output()` to streaming stderr read
   - Spawn thread to read stderr line-by-line
   - Parse progress JSON, correlate by benchmark name
   - Update `indicatif::MultiProgress` bars (max 4, sorted by progress)
   - Collect stdout for final JSON result
   - Skip progress display if `--quiet` or non-TTY

## Impact on Timing Accuracy

Progress reporting is placed **between samples**, not inside the iteration loop:

```rust
for sample_idx in 0..samples {
    // Progress check here (outside timing)
    if should_report_progress(sample_idx) {
        emit_progress(...);
    }

    // Timing starts here
    let start = Instant::now();
    for _ in 0..iterations {
        func();  // No progress here!
    }
    let elapsed = start.elapsed();
    // Timing ends here
}
```

The `emit_progress()` call is a simple `eprintln!` of pre-formatted JSON - negligible overhead compared to sample timing.

## Testing Plan

1. **Visual verification**: Run on test-workspace, confirm progress bar looks good
2. **Timing accuracy**: Compare variance with/without progress (should be identical)
3. **Non-TTY behavior**: Ensure works in CI (progress can be disabled or simplified)
4. **Cancellation**: Ctrl+C should still work cleanly

## Design Decisions

### Parallel Mode Display

Display up to **4 progress bars** simultaneously using `indicatif::MultiProgress`. When more than 4 benchmarks are running in parallel:
- Show the 4 benchmarks with the **most progress** (highest percentage)
- As benchmarks complete, their bars disappear and others take their place
- Completed benchmarks print their result line above the progress bars

This keeps the display manageable while showing the most relevant information.

### TTY Detection

Progress output is **disabled when stderr is not a TTY**:
- CI environments get clean output without progress spam
- Piped output remains parseable
- Use `std::io::stderr().is_terminal()` (Rust 1.70+) or `atty` crate

When disabled, the runner still emits progress JSON (no code change), but the orchestrator simply ignores it.

### Quiet Flag

Add `--quiet` / `-q` flag to `cargo simplebench run`:
- Suppresses progress bars entirely
- Only shows final results and summary
- Useful for scripting or when progress is distracting

```bash
cargo simplebench run --quiet
cargo simplebench run -q
```
