# Reproducible Regression Fix - Implementation Plan

**Date**: 2025-12-11
**Purpose**: Implement observability, control, and time-based warmup to address SimpleBench's first-run regression

---

## Project Philosophy

**SimpleBench is proudly Linux-first**:
- Linux: Full feature support with CPU frequency, governor, thermal monitoring
- macOS/Windows: Core functionality works, but advanced features gracefully degrade
- All platform-specific code must fail gracefully with informative warnings

---

## Phase 1: CPU Observability Infrastructure

### Goal
Implement comprehensive CPU monitoring to understand what's happening during benchmarks.

### Components

#### 1.1 Linux CPU Monitoring Module (`simplebench-runtime/src/cpu_monitor.rs`)

**Responsibilities**:
- Read CPU frequency from `/sys/devices/system/cpu/cpu{N}/cpufreq/scaling_cur_freq`
- Read CPU governor from `/sys/devices/system/cpu/cpu{N}/cpufreq/scaling_governor`
- Read CPU frequency range (min/max) from `/sys/devices/system/cpu/cpu{N}/cpufreq/cpuinfo_{min,max}_freq`
- Read thermal zones from `/sys/class/thermal/thermal_zone{N}/temp`
- Discover available thermal zones (iterate 0-20, check which exist)

**API Design**:
```rust
pub struct CpuMonitor {
    cpu_core: usize,
    thermal_zone: Option<usize>,
}

impl CpuMonitor {
    /// Create monitor for specific CPU core
    pub fn new(cpu_core: usize) -> Self;

    /// Read current frequency (returns kHz, or None if unavailable)
    pub fn read_frequency(&self) -> Option<u64>;

    /// Read current governor (returns governor name, or None if unavailable)
    pub fn read_governor(&self) -> Option<String>;

    /// Read frequency range (min, max in kHz)
    pub fn read_frequency_range(&self) -> Option<(u64, u64)>;

    /// Read current temperature (returns millidegrees C, or None)
    pub fn read_temperature(&self) -> Option<i32>;

    /// Find available thermal zones
    pub fn discover_thermal_zones() -> Vec<usize>;
}

pub struct CpuSnapshot {
    pub timestamp: Instant,
    pub frequency_khz: Option<u64>,
    pub temperature_millic: Option<i32>,
}

impl CpuSnapshot {
    pub fn frequency_mhz(&self) -> Option<f64>;
    pub fn temperature_celsius(&self) -> Option<f64>;
}
```

**Platform Behavior**:
- **Linux**: Full implementation reading from `/sys/`
- **macOS/Windows**: All methods return `None`, print warning once per run

#### 1.2 Pre-Benchmark Environment Check

Add `verify_benchmark_environment()` function called before any benchmarks run.

**Output Format**:
```
Verifying benchmark environment...
  Platform: Linux (full monitoring support)
  CPU 0 governor: powersave
    ⚠ WARNING: Not using 'performance' governor
    Consider: sudo cpupower frequency-set -g performance
  CPU 0 frequency range: 400 MHz - 5000 MHz
  CPU 0 current frequency: 1200 MHz
  Found 2 thermal zone(s)
    Zone 0: 45°C
    Zone 1: 42°C
Environment check complete.
```

**Non-Linux Output**:
```
Verifying benchmark environment...
  Platform: macOS (limited monitoring support)
    ℹ CPU frequency/thermal monitoring not available on this platform
Environment check complete.
```

**Integration Point**: Call from `run_and_stream_benchmarks()` before running any benchmarks.

#### 1.3 Per-Sample CPU Monitoring

Modify measurement loop to capture CPU state with each timing sample.

**Current Flow**:
```rust
for _ in 0..samples {
    // warmup
    // measure timing
    times.push(elapsed);
}
```

**New Flow**:
```rust
let monitor = CpuMonitor::new(cpu_core);

for _ in 0..samples {
    // warmup
    // measure timing
    let snapshot = CpuSnapshot {
        timestamp: Instant::now(),
        frequency_khz: monitor.read_frequency(),
        temperature_millic: monitor.read_temperature(),
    };

    samples.push(BenchmarkSample {
        duration: elapsed,
        cpu_snapshot: snapshot,
    });
}
```

**Data Structure Changes**:

Add to `BenchmarkResult`:
```rust
pub struct BenchmarkResult {
    // ... existing fields ...
    pub cpu_samples: Vec<CpuSnapshot>,  // NEW: CPU state for each timing sample
}
```

**Storage**: CPU samples saved to baseline JSON files alongside timing data.

---

## Phase 2: Enhanced Analysis with CPU Context

### Goal
Update the `analyze` command to detect and report thermal/frequency anomalies.

### Components

#### 2.1 CPU Analysis Module (`simplebench-runtime/src/cpu_analysis.rs`)

**Responsibilities**:
- Detect thermal throttling (temperature increases >20°C during run, or exceeds threshold)
- Detect "cold start" (initial temperature <50°C, or initial frequency <50% of max)
- Detect frequency variance (frequency changes >10% during run)
- Calculate frequency statistics (min/max/mean/stddev)
- Calculate temperature statistics (min/max/mean/stddev)

**API Design**:
```rust
pub struct CpuAnalysis {
    pub frequency_stats: Option<FrequencyStats>,
    pub temperature_stats: Option<TemperatureStats>,
    pub warnings: Vec<CpuWarning>,
}

pub struct FrequencyStats {
    pub min_mhz: f64,
    pub max_mhz: f64,
    pub mean_mhz: f64,
    pub stddev_mhz: f64,
    pub variance_percent: f64,  // (max - min) / mean * 100
}

pub struct TemperatureStats {
    pub min_celsius: f64,
    pub max_celsius: f64,
    pub mean_celsius: f64,
    pub increase_celsius: f64,  // max - min
}

pub enum CpuWarning {
    ColdStart { initial_temp_celsius: f64 },
    ThermalThrottling { temp_increase_celsius: f64, max_temp_celsius: f64 },
    FrequencyVariance { variance_percent: f64 },
    LowFrequency { mean_mhz: f64, max_available_mhz: f64, percent_of_max: f64 },
}

impl CpuAnalysis {
    pub fn from_snapshots(snapshots: &[CpuSnapshot], max_freq_khz: Option<u64>) -> Self;
}
```

#### 2.2 Update `cargo simplebench analyze` Command

**Current Output**:
```
Analyzing benchmark: game_math_vector_add (last 5 runs)

Run 1 (2025-12-10 14:23:45): 1.30ms (baseline)
Run 2 (2025-12-10 14:24:12): 1.38ms (+6.2% ⚠)
Run 3 (2025-12-10 14:24:45): 1.39ms (+6.9% ⚠)
...
```

**Enhanced Output**:
```
Analyzing benchmark: game_math_vector_add (last 5 runs)

Run 1 (2025-12-10 14:23:45): 1.30ms (baseline)
  CPU: 4200-4500 MHz (mean: 4350 MHz, variance: 7%)
  Temp: 45-52°C (increase: +7°C)
  ⚠ Cold start detected (initial: 45°C)

Run 2 (2025-12-10 14:24:12): 1.38ms (+6.2% ⚠)
  CPU: 4400-4600 MHz (mean: 4500 MHz, variance: 4%)
  Temp: 58-65°C (increase: +7°C)

Run 3 (2025-12-10 14:24:45): 1.39ms (+6.9% ⚠)
  CPU: 4350-4550 MHz (mean: 4450 MHz, variance: 4%)
  Temp: 60-68°C (increase: +8°C)

Performance Summary:
  Baseline: 1.30ms (cold start - may be optimistic)
  Recent mean: 1.39ms (+6.5% from baseline)
  Recommendation: Baseline established on cold system. Consider re-establishing
                  baseline after implementing system warmup.
```

**Integration**:
- Modify `cargo-simplebench/src/commands/analyze.rs` (create if doesn't exist)
- Load historical data including CPU samples
- Call `CpuAnalysis::from_snapshots()` for each run
- Format output with CPU context

#### 2.3 Update Live Benchmark Output

**Current Output**:
```
Running benchmark: game_math::vector_add... 1.35ms (mean) [p50: 1.30ms, p90: 1.40ms, p99: 1.50ms]
  vs baseline: +3.8%
```

**Enhanced Output** (Linux only, when CPU data available):
```
Running benchmark: game_math::vector_add... 1.35ms (mean) [p50: 1.30ms, p90: 1.40ms, p99: 1.50ms]
  CPU: 4200-4600 MHz (mean: 4400 MHz), Temp: 55-62°C (+7°C)
  vs baseline: +3.8%
  ⚠ Frequency variance detected (9% variance)
```

**Integration**: Modify `run_and_stream_benchmarks()` output logic.

---

## Phase 3: Time-Based Warmup

### Goal
Replace iteration-based warmup with time-based warmup (matching Criterion's approach).

### Components

#### 3.1 Remove Iteration-Based Warmup

**Current Code** (`simplebench-runtime/src/lib.rs`):
```rust
pub fn measure_with_warmup<F>(
    bench_fn: F,
    samples: usize,
    iterations: usize,
    warmup_iterations: usize,  // ← REMOVE THIS
) -> Vec<Duration>
```

**Changes**:
1. Remove `warmup_iterations` parameter
2. Remove `SIMPLEBENCH_WARMUP_ITERATIONS` env var
3. Remove from `BenchmarkConfig` struct

#### 3.2 Implement Time-Based Warmup

**New Configuration**:
```rust
pub struct BenchmarkConfig {
    // ... existing fields ...
    pub warmup_duration_secs: u64,  // NEW: default 3 seconds (matching Criterion)
}
```

**Environment Variable**:
```bash
SIMPLEBENCH_WARMUP_DURATION=5  # seconds
```

**Algorithm** (Criterion-style exponential doubling):
```rust
fn warmup_benchmark<F>(bench_fn: &F, warmup_duration: Duration, iterations: usize)
where
    F: Fn()
{
    let start = Instant::now();
    let mut total_iterations = 0;
    let mut batch_size = 1;

    while start.elapsed() < warmup_duration {
        // Run benchmark function batch_size times
        for _ in 0..batch_size {
            for _ in 0..iterations {
                bench_fn();
            }
        }

        total_iterations += batch_size * iterations;
        batch_size *= 2;  // Exponential doubling
    }

    eprintln!("  Warmup: {}ms ({} iterations)",
              start.elapsed().as_millis(),
              total_iterations);
}
```

**Integration**:
```rust
pub fn measure_with_warmup<F>(
    bench_fn: F,
    samples: usize,
    iterations: usize,
    warmup_duration: Duration,  // NEW PARAMETER
    cpu_monitor: Option<&CpuMonitor>,  // NEW PARAMETER
) -> BenchmarkResult
where
    F: Fn(),
{
    // Perform warmup
    warmup_benchmark(&bench_fn, warmup_duration, iterations);

    // Measure samples
    let mut sample_data = Vec::with_capacity(samples);

    for _ in 0..samples {
        // ... existing measurement code with CPU monitoring ...
    }

    // ... rest of measurement logic ...
}
```

**Default Value**: 3 seconds (matching Criterion's default)

**Breaking Change**: Yes - removes `warmup_iterations` config parameter. Document in commit message.

---

## Phase 4: `cargo simplebench run` Command

### Goal
Allow users to run specific benchmarks or all benchmarks on-demand.

### Components

#### 4.1 CLI Argument Parsing

**Current Command Structure**:
```bash
cargo simplebench              # Run all benchmarks (current default)
cargo simplebench clean        # Clean baselines
cargo simplebench analyze ...  # Analyze historical data
```

**New Command Structure**:
```bash
cargo simplebench              # Run all benchmarks (unchanged)
cargo simplebench run          # Run all benchmarks (explicit)
cargo simplebench run --bench <name>   # Run specific benchmark
cargo simplebench clean        # Clean baselines (unchanged)
cargo simplebench analyze ...  # Analyze historical data (unchanged)
```

**Implementation** (`cargo-simplebench/src/main.rs`):

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cargo-simplebench")]
#[command(bin_name = "cargo")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run benchmarks (default if no command specified)
    Run {
        /// Run only benchmarks matching this name (substring match)
        #[arg(long)]
        bench: Option<String>,

        /// Number of samples per benchmark
        #[arg(long)]
        samples: Option<usize>,

        /// Number of iterations per sample
        #[arg(long)]
        iterations: Option<usize>,

        /// Warmup duration in seconds
        #[arg(long)]
        warmup_duration: Option<u64>,

        /// Regression threshold percentage
        #[arg(long)]
        threshold: Option<f64>,

        /// CI mode: fail on any regression
        #[arg(long)]
        ci: bool,
    },

    /// Clean all baselines
    Clean,

    /// Analyze historical benchmark data
    Analyze {
        /// Benchmark name to analyze
        benchmark: String,

        /// Number of recent runs to show
        #[arg(long, default_value = "10")]
        last: usize,
    },
}
```

**Backward Compatibility**:
- `cargo simplebench` with no args → defaults to `Run { bench: None, ... }`
- Existing `--samples`, `--iterations`, etc. continue to work

#### 4.2 Benchmark Filtering

**Filtering Logic** (in runner.rs generation or execution):

```rust
fn should_run_benchmark(bench_name: &str, filter: Option<&str>) -> bool {
    match filter {
        None => true,  // Run all benchmarks
        Some(pattern) => bench_name.contains(pattern),  // Substring match
    }
}
```

**Implementation Options**:

**Option A: Filter at Runtime** (Recommended)
- Pass filter via environment variable: `SIMPLEBENCH_BENCH_FILTER`
- Runner binary checks filter before executing each benchmark
- Simpler, no recompilation needed

**Option B: Filter at Compile Time**
- Generate runner.rs with only filtered benchmarks
- More complex, requires recompilation for each filter change

**Recommendation**: Option A (runtime filtering)

**Environment Variable**:
```rust
// In cargo-simplebench/src/main.rs
if let Some(bench_filter) = run.bench {
    std::env::set_var("SIMPLEBENCH_BENCH_FILTER", bench_filter);
}
```

**In runner.rs template** (`cargo-simplebench/src/runner_gen.rs`):
```rust
fn main() {
    let bench_filter = std::env::var("SIMPLEBENCH_BENCH_FILTER").ok();

    // ... pre-benchmark environment check ...

    for bench in inventory::iter::<SimpleBench> {
        // Apply filter
        if let Some(ref filter) = bench_filter {
            if !bench.name.contains(filter) {
                continue;  // Skip this benchmark
            }
        }

        // ... run benchmark ...
    }
}
```

#### 4.3 Output for Filtered Runs

**Example Output**:
```bash
$ cargo simplebench run --bench vector_add

Verifying benchmark environment...
  Platform: Linux (full monitoring support)
  CPU 0 governor: performance
  CPU 0 frequency range: 400 MHz - 5000 MHz
  Found 2 thermal zone(s)
Environment check complete.

Running 2 benchmarks (filter: "vector_add"):

Running benchmark: game_math::vector_add...
  Warmup: 3000ms (2,450,000 iterations)
  1.35ms (mean) [p50: 1.30ms, p90: 1.40ms, p99: 1.50ms]
  CPU: 4400-4600 MHz (mean: 4500 MHz), Temp: 55-62°C (+7°C)
  vs baseline: +3.8%

Running benchmark: game_entities::vector_add_entities...
  Warmup: 3000ms (1,850,000 iterations)
  2.10ms (mean) [p50: 2.05ms, p90: 2.15ms, p99: 2.25ms]
  CPU: 4450-4650 MHz (mean: 4550 MHz), Temp: 58-65°C (+7°C)
  vs baseline: +1.2%

Summary:
  2 benchmarks run (2 matched filter: "vector_add")
  0 regressions detected
```

---

## Implementation Order

### Stage 1: CPU Monitoring Foundation (Linux-only)
**Estimated Effort**: 2-3 hours

1. Create `simplebench-runtime/src/cpu_monitor.rs`
2. Implement `CpuMonitor` struct with Linux sysfs reading
3. Implement `CpuSnapshot` struct
4. Add unit tests (read actual system values on Linux, verify None on other platforms)
5. Add pre-benchmark environment check to `run_and_stream_benchmarks()`

**Testing**:
```bash
cd test-workspace
cargo simplebench  # Should show environment check output
```

### Stage 2: Per-Sample CPU Monitoring
**Estimated Effort**: 2-3 hours

1. Modify `BenchmarkResult` to include `cpu_samples: Vec<CpuSnapshot>`
2. Update measurement loop to capture CPU state per sample
3. Update baseline storage format to include CPU data
4. Update live benchmark output to show CPU stats (min/max/mean frequency and temp)

**Breaking Change**: Baseline format changes (old baselines still loadable, but won't have CPU data)

**Testing**:
```bash
cd test-workspace
cargo simplebench clean  # Clear old baselines
cargo simplebench        # Run with CPU monitoring
# Verify output shows CPU frequency and temperature ranges
```

### Stage 3: Enhanced Analysis
**Estimated Effort**: 2-3 hours

1. Create `simplebench-runtime/src/cpu_analysis.rs`
2. Implement `CpuAnalysis::from_snapshots()`
3. Implement warning detection (cold start, throttling, variance)
4. Create `cargo-simplebench/src/commands/analyze.rs` (if doesn't exist)
5. Update `analyze` command to show CPU context and warnings

**Testing**:
```bash
cd test-workspace
cargo simplebench clean
cargo simplebench   # Run 1 (cold start)
cargo simplebench   # Run 2 (warm start)
cargo simplebench   # Run 3
cargo simplebench analyze vector_add --last 3
# Should show cold start warning on Run 1, thermal differences across runs
```

### Stage 4: Time-Based Warmup
**Estimated Effort**: 1-2 hours

1. Remove `warmup_iterations` from `BenchmarkConfig`
2. Add `warmup_duration_secs` to `BenchmarkConfig` (default: 3)
3. Implement `warmup_benchmark()` with exponential doubling
4. Update `measure_with_warmup()` to use time-based warmup
5. Update environment variable handling (`SIMPLEBENCH_WARMUP_DURATION`)
6. Remove old `SIMPLEBENCH_WARMUP_ITERATIONS` env var

**Breaking Change**: Yes - removes `warmup_iterations` parameter

**Testing**:
```bash
cd test-workspace
cargo simplebench clean
cargo simplebench --warmup-duration 5  # 5 second warmup
# Verify output shows "Warmup: 5000ms (N iterations)" for each benchmark
```

### Stage 5: `run` Command with Filtering
**Estimated Effort**: 2-3 hours

1. Refactor CLI parsing to use `clap` subcommands
2. Add `Commands::Run` with `--bench` option
3. Implement `SIMPLEBENCH_BENCH_FILTER` env var
4. Update runner.rs template to respect filter
5. Update output to show "N benchmarks run (M matched filter)"
6. Ensure backward compatibility (`cargo simplebench` with no args works)

**Testing**:
```bash
cd test-workspace
cargo simplebench run --bench vector_add  # Should run only 2 benchmarks
cargo simplebench run                      # Should run all benchmarks
cargo simplebench                          # Should run all benchmarks (backward compat)
```

---

## Success Criteria

### Stage 1-2: CPU Monitoring
- [ ] Pre-benchmark check shows CPU governor, frequency range, thermal zones
- [ ] Live benchmark output shows CPU frequency and temperature ranges per benchmark
- [ ] CPU data saved to baseline JSON files
- [ ] Graceful degradation on non-Linux platforms (warnings, not errors)

### Stage 3: Analysis
- [ ] `cargo simplebench analyze` shows CPU context for each run
- [ ] Cold start detected and labeled in analysis output
- [ ] Thermal throttling detected and labeled (>20°C increase or >85°C)
- [ ] Frequency variance detected and labeled (>10% variance)

### Stage 4: Time-Based Warmup
- [ ] Warmup runs for configured duration (default 3 seconds)
- [ ] Warmup uses exponential doubling (1, 2, 4, 8, ... iterations)
- [ ] Warmup reports total duration and iteration count
- [ ] `--warmup-duration` CLI flag works
- [ ] `SIMPLEBENCH_WARMUP_DURATION` env var works

### Stage 5: Run Command
- [ ] `cargo simplebench run` works (runs all benchmarks)
- [ ] `cargo simplebench run --bench <name>` filters to matching benchmarks
- [ ] Filter uses substring matching
- [ ] Output shows "N benchmarks run (M matched filter)"
- [ ] Backward compatibility: `cargo simplebench` still works as before

---

## Documentation Updates

After implementation, update:

1. **CLAUDE.md**:
   - Document new CPU monitoring features
   - Update warmup strategy (time-based, not iteration-based)
   - Document `cargo simplebench run --bench` command
   - Add "Linux-first" philosophy statement

2. **README.md** (if exists):
   - Update usage examples
   - Document new `run` command
   - Document CPU monitoring output

3. **Git Commit Messages**:
   - Stage 1: "feat(monitoring): add Linux CPU frequency and thermal monitoring"
   - Stage 2: "feat(monitoring): capture CPU state per benchmark sample"
   - Stage 3: "feat(analysis): detect cold start, throttling, and frequency variance"
   - Stage 4: "feat(warmup): implement time-based warmup (breaking change)"
   - Stage 5: "feat(cli): add 'run' command with benchmark filtering"

---

## Platform Support Matrix

| Feature                     | Linux | macOS | Windows |
|-----------------------------|-------|-------|---------|
| Core benchmarking           | ✅    | ✅    | ✅      |
| CPU affinity                | ✅    | ❌    | ❌      |
| CPU frequency monitoring    | ✅    | ❌    | ❌      |
| CPU governor reading        | ✅    | ❌    | ❌      |
| Thermal monitoring          | ✅    | ❌    | ❌      |
| Pre-benchmark env check     | ✅    | ⚠️    | ⚠️      |
| Time-based warmup           | ✅    | ✅    | ✅      |
| Benchmark filtering         | ✅    | ✅    | ✅      |

**Legend**:
- ✅ Full support
- ⚠️ Limited support (runs but shows warnings about unavailable features)
- ❌ Not supported

---

## Risk Assessment

### Low Risk
- CPU monitoring infrastructure (Linux-only, graceful degradation)
- Analysis enhancements (backward compatible, optional)
- `run` command (backward compatible, additive)

### Medium Risk
- Time-based warmup (breaking change, but simple migration)
- Per-sample CPU monitoring (changes baseline format, but backward compatible loading)

### Mitigation
- Comprehensive testing on clean baselines
- Clear documentation of breaking changes
- Graceful handling of old baseline formats

---

## Future Enhancements (Out of Scope)

Not included in this implementation plan, but worth considering later:

1. **System-wide warmup** (10-30 second CPU-intensive phase before first benchmark)
2. **Thermal throttling detection and retry** (Android Microbenchmark approach)
3. **CPU frequency locking** (requires root, use `cpupower` or direct sysfs writes)
4. **macOS support** (use `sysctl` for limited CPU info)
5. **Windows support** (use WMI for CPU info)
6. **Configurable thermal thresholds** (per-user or per-machine)
7. **Automatic baseline re-establishment** (detect cold start, offer to re-run)

---

## References

- `.claude/reproducible-regression/criterion_warmup_research.md` - Comprehensive research on Criterion, Google Benchmark, and Android Microbenchmark warmup strategies
- `.claude/reproducible-regression/investigation.md` - Initial investigation of the 6% regression
- Criterion.rs default warmup: 3 seconds per benchmark
- Android Microbenchmark: Active thermal management with pause/retry
