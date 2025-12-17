# Progress Feedback Implementation Plan

Based on research in `research.md`. Implementation adds real-time progress bars during benchmark execution.

## Overview

**Goal**: Provide visual feedback during long-running benchmarks via stderr JSON protocol + indicatif progress bars.

**Key Constraint**: stdout reserved for JSON results → progress uses stderr.

---

## Phase 1: Runtime Progress Emission

Add progress message infrastructure to `simplebench-runtime`.

### 1.1 Create Progress Module

**File**: `simplebench-runtime/src/progress.rs`

```rust
use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "phase")]
pub enum ProgressPhase {
    #[serde(rename = "warmup")]
    Warmup { elapsed_ms: u64, target_ms: u64 },
    #[serde(rename = "samples")]
    Samples { current: u32, total: u32 },
    #[serde(rename = "complete")]
    Complete,
}

#[derive(Serialize)]
pub struct ProgressMessage<'a> {
    pub bench: &'a str,
    #[serde(flatten)]
    pub phase: ProgressPhase,
}

/// Emit progress JSON to stderr (non-blocking, fire-and-forget)
pub fn emit_progress(msg: &ProgressMessage) {
    // Wrap in {"progress": ...} envelope
    if let Ok(json) = serde_json::to_string(&serde_json::json!({"progress": msg})) {
        eprintln!("{}", json);
    }
}
```

### 1.2 Modify Warmup Function

**File**: `simplebench-runtime/src/measurement.rs`

Update `warmup_benchmark()` signature and implementation:

```rust
pub fn warmup_benchmark<F>(func: &F, duration: Duration, bench_name: &str) -> u32
where
    F: Fn(),
{
    let target_ms = duration.as_millis() as u64;
    let mut last_report = Instant::now();

    // ... existing warmup loop ...

    // Inside loop, after each batch:
    if last_report.elapsed() >= Duration::from_millis(100) {
        emit_progress(&ProgressMessage {
            bench: bench_name,
            phase: ProgressPhase::Warmup {
                elapsed_ms: start.elapsed().as_millis() as u64,
                target_ms,
            },
        });
        last_report = Instant::now();
    }
}
```

### 1.3 Modify Measurement Function

**File**: `simplebench-runtime/src/measurement.rs`

Update `measure_function_impl()` to emit sample progress:

```rust
pub fn measure_function_impl<F>(
    func: &F,
    iterations: u32,
    samples: u32,
    bench_name: &str,  // NEW
) -> Vec<Duration>
where
    F: Fn(),
{
    let report_interval = (samples / 100).max(1);

    for sample_idx in 0..samples {
        // Progress check BEFORE timing
        if sample_idx % report_interval == 0 {
            emit_progress(&ProgressMessage {
                bench: bench_name,
                phase: ProgressPhase::Samples {
                    current: sample_idx,
                    total: samples,
                },
            });
        }

        // Existing timing code unchanged
        let start = Instant::now();
        for _ in 0..iterations {
            func();
        }
        results.push(start.elapsed());
    }

    // Final progress
    emit_progress(&ProgressMessage {
        bench: bench_name,
        phase: ProgressPhase::Complete,
    });

    results
}
```

### 1.4 Update Call Chain

**File**: `simplebench-runtime/src/measurement.rs`

Update `measure_with_warmup()` to pass benchmark name through:

```rust
pub fn measure_with_warmup<F>(
    func: &F,
    config: &BenchmarkConfig,
    bench_name: &str,  // NEW
) -> Vec<Duration>
```

### 1.5 Wire Up in lib.rs

**File**: `simplebench-runtime/src/lib.rs`

- Add `pub mod progress;`
- Update `run_and_stream_benchmarks()` to pass benchmark name to measurement functions

### 1.6 Testing

- Run existing benchmarks, verify JSON progress appears on stderr
- Verify stdout JSON result unchanged
- Verify progress doesn't affect timing accuracy

---

## Phase 2: CLI Progress Display

Add indicatif progress bars to `cargo-simplebench`.

### 2.1 Add Dependencies

**File**: `cargo-simplebench/Cargo.toml`

```toml
[dependencies]
indicatif = "0.17"
```

### 2.2 Create Progress Module

**File**: `cargo-simplebench/src/progress.rs`

```rust
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;

pub struct BenchmarkProgress {
    multi: MultiProgress,
    bars: HashMap<String, ProgressBar>,
    is_tty: bool,
}

impl BenchmarkProgress {
    pub fn new() -> Self {
        let is_tty = std::io::stderr().is_terminal();
        Self {
            multi: MultiProgress::new(),
            bars: HashMap::new(),
            is_tty,
        }
    }

    pub fn update(&mut self, bench: &str, phase: &ProgressPhase) {
        if !self.is_tty { return; }

        let bar = self.bars.entry(bench.to_string())
            .or_insert_with(|| self.create_bar(bench));

        match phase {
            ProgressPhase::Warmup { elapsed_ms, target_ms } => {
                bar.set_message(format!(
                    "Warming up... {:.1}s / {:.1}s",
                    *elapsed_ms as f64 / 1000.0,
                    *target_ms as f64 / 1000.0
                ));
            }
            ProgressPhase::Samples { current, total } => {
                bar.set_length(*total as u64);
                bar.set_position(*current as u64);
            }
            ProgressPhase::Complete => {
                bar.finish_with_message("✓");
            }
        }
    }

    fn create_bar(&self, bench: &str) -> ProgressBar {
        let bar = self.multi.add(ProgressBar::new(100));
        bar.set_style(ProgressStyle::default_bar()
            .template("{prefix:20} [{bar:20}] {pos:>4}/{len:4} {msg}")
            .unwrap());
        bar.set_prefix(bench.to_string());
        bar
    }
}
```

### 2.3 Add Quiet Flag

**File**: `cargo-simplebench/src/main.rs`

Add to CLI args:
```rust
#[arg(short, long, help = "Suppress progress bars")]
quiet: bool,
```

Pass to runner via environment:
```rust
if args.quiet {
    cmd.env("SIMPLEBENCH_QUIET", "1");
}
```

### 2.4 Streaming Stderr Reader

**File**: `cargo-simplebench/src/main.rs`

Replace `wait_with_output()` with streaming approach:

```rust
use std::io::{BufRead, BufReader};
use std::thread;

fn run_benchmark_with_progress(cmd: &mut Command, progress: &mut BenchmarkProgress) -> Result<String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stderr = child.stderr.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    // Spawn thread to read stderr progress
    let progress_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().flatten() {
            if let Ok(msg) = serde_json::from_str::<ProgressWrapper>(&line) {
                // Send to main thread for display update
            }
        }
    });

    // Read stdout for JSON result
    let mut stdout_content = String::new();
    BufReader::new(stdout).read_to_string(&mut stdout_content)?;

    child.wait()?;
    progress_handle.join().unwrap();

    Ok(stdout_content)
}
```

### 2.5 Progress Message Parsing

**File**: `cargo-simplebench/src/progress.rs`

```rust
#[derive(Deserialize)]
pub struct ProgressWrapper {
    pub progress: ProgressMessage,
}

#[derive(Deserialize)]
pub struct ProgressMessage {
    pub bench: String,
    #[serde(flatten)]
    pub phase: ProgressPhase,
}

#[derive(Deserialize)]
#[serde(tag = "phase")]
pub enum ProgressPhase {
    #[serde(rename = "warmup")]
    Warmup { elapsed_ms: u64, target_ms: u64 },
    #[serde(rename = "samples")]
    Samples { current: u32, total: u32 },
    #[serde(rename = "complete")]
    Complete,
}
```

### 2.6 Testing

- Run `cargo simplebench` in TTY, verify progress bars appear
- Run with `--quiet`, verify no progress
- Pipe output, verify clean JSON (no progress noise)
- Test parallel mode (if implemented)

---

## Phase 3: Polish & Edge Cases

### 3.1 Parallel Mode (Optional Enhancement)

If parallel benchmark execution is ever added:
- Use `MultiProgress` to show up to 4 bars
- Sort by progress percentage
- Remove completed bars, add new ones

### 3.2 ETA Calculation

Add estimated time remaining to sample progress:

```rust
// Track start time per benchmark
let elapsed = start.elapsed();
let rate = current as f64 / elapsed.as_secs_f64();
let remaining = (total - current) as f64 / rate;
```

### 3.3 Error Handling

- If stderr contains non-JSON lines (errors), display them
- Don't crash on malformed progress JSON

---

## File Changes Summary

| File | Changes |
|------|---------|
| `simplebench-runtime/src/progress.rs` | NEW - Progress types and emit function |
| `simplebench-runtime/src/measurement.rs` | Add progress emission to warmup and measurement |
| `simplebench-runtime/src/lib.rs` | Add progress module, pass bench names |
| `cargo-simplebench/Cargo.toml` | Add indicatif dependency |
| `cargo-simplebench/src/progress.rs` | NEW - Progress bar management |
| `cargo-simplebench/src/main.rs` | Streaming stderr, --quiet flag |

---

## Testing Checklist

- [ ] Progress JSON emitted correctly on stderr
- [ ] stdout JSON result unchanged
- [ ] Progress bars render in TTY
- [ ] No progress in non-TTY (CI)
- [ ] `--quiet` suppresses progress
- [ ] Timing accuracy unaffected (compare variance)
- [ ] Ctrl+C cleanup works
- [ ] Malformed progress doesn't crash

---

## Implementation Order

1. **Phase 1.1-1.5**: Runtime progress emission (can test independently)
2. **Phase 2.1-2.2**: Add indicatif, create progress module
3. **Phase 2.3-2.4**: Wire up streaming and display
4. **Phase 2.5-2.6**: Test end-to-end
5. **Phase 3**: Polish based on usage

Estimated effort: ~200 lines runtime, ~150 lines CLI
