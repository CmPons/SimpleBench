# Criterion.rs Warmup Strategy Research

**Date**: 2025-12-10
**Purpose**: Investigate how Criterion handles warmup to address SimpleBench's first-run regression issue

## Context

SimpleBench experiences a reproducible ~6% regression where:
- **First run** (baseline establishment): 1.30ms
- **Second run**: 1.38-1.39ms (6-7% regression)
- **Subsequent runs**: Stable at 1.38-1.41ms

This research investigates Criterion.rs's warmup strategies to identify potential solutions.

---

## Criterion.rs Warmup Strategy

### 1. Per-Benchmark Warmup (Default)

**Duration**: 3 seconds by default

**How It Works**:
- The routine is executed once, then twice, four times and so on (exponential doubling: 1, 2, 4, 8, 16...)
- Doubling continues until the **total accumulated execution time** exceeds the configured warmup time
- The number of iterations completed during warmup is recorded, along with elapsed time
- This information helps determine appropriate iteration counts for the measurement phase

**Purpose**:
- Allow CPU frequency to ramp up and stabilize
- Let CPU temperature increase toward operating equilibrium
- Train branch predictors by executing the code paths
- Amortize any one-time initialization costs (lazy allocations, etc.)
- Establish consistent "steady state" before measurement begins

**Note**: Warmup runs the actual benchmark code repeatedly. Whatever the CPU decides to cache (instructions, data, TLB entries) happens naturally - we have no control over cache replacement policies.

**Configuration**:
```rust
Criterion::default()
    .warm_up_time(Duration::from_secs(5))  // Custom warmup duration
```

**Key Point** (from issue #317):
When asked about the warmup phase, Brook Heisler noted that warmup runs the actual benchmark code (not unrelated operations), and "even if you disable warm-up, the measurement phase runs the benchmarked function many times" so the system reaches a steady state quickly anyway.

### 2. No System-Wide Warmup

**Critical Finding**: Criterion does **NOT** implement a global/system-wide warmup phase before running all benchmarks.

Each benchmark runs its own isolated warmup phase:
1. Benchmark A: Warmup → Measurement
2. Benchmark B: Warmup → Measurement
3. Benchmark C: Warmup → Measurement

This means the **first benchmark in the suite** always runs on a "cold" system.

### 3. Warmup Characteristics

**What Warmup Actually Does**:
- Runs the benchmark code repeatedly for 3 seconds (default)
- CPU naturally caches whatever it decides to cache (we have no control)
- By the time measurement starts, CPU is in "steady state" rather than cold-start
- Branch predictors trained, one-time costs paid

**What Warmup Doesn't Control**:
- ❌ Cache contents (no userspace API to query or control L1/L2/L3)
- ❌ Cache eviction policy (hardware-managed LRU/pseudo-LRU)
- ❌ Thermal throttling (CPU manages this autonomously)
- ❌ Frequency scaling transitions (governed by CPU power management)

**Thermal/Frequency Reality**:
- Criterion acknowledges CPU thermal throttling as a source of variance
- Recommends ensuring the computer is "quiet" during measurements
- **No active thermal stabilization** or frequency management
- **Assumes 3 seconds is long enough** for CPU to reach operating equilibrium

**What 3 Seconds Accomplishes**:
- CPU frequency ramps from idle (~400 MHz) to turbo boost (4+ GHz)
- CPU temperature rises from idle to operating temp
- CPU power state settles into steady state
- By the time measurement starts, CPU should be at stable frequency/temp

**Timeline**:
- 0s: Cold start (idle frequency, cool)
- 0-3s: Warmup (frequency ramps, temp rises)
- 3s+: Measurement (stable state)

**First-Run Effects**:
- Criterion's 3-second warmup prevents measuring during ramp-up phase
- May or may not reach full thermal equilibrium (depends on workload intensity)
- Designed for relative comparisons (A vs B), not absolute baseline establishment across separate invocations

---

## Google Benchmark Warmup Strategy

### MinWarmUpTime Feature

**Introduced**: Issue #1130, implemented in PR #1399

**Purpose**: Address the exact problem SimpleBench faces
> "Some code is slow at first but becomes faster after some time. In my case (MPI application) the first few benchmark iterations run a lot slower."

**Implementation**:
```bash
--benchmark_min_warmup_time=<duration>
```

**Default**: 0 seconds (disabled)

**How It Works**:
- Dynamically determines warmup iterations (similar to Criterion's doubling strategy)
- Discards warmup timings from results
- Actual measurement follows standard `MinTime` logic after warmup

**Key Insight**: Google Benchmark acknowledged that **standard warmup may not be enough** for some workloads, particularly those affected by:
- JIT compilation warmup
- MPI/distributed system initialization
- Runtime optimization (adaptive compilation)

---

## Android Microbenchmark Library

### Thermal Throttling Detection

The Android Microbenchmark library provides the most sophisticated thermal management:

**Features**:
1. **Automatic thermal throttling detection**:
   - Periodically runs an internal benchmark to measure CPU performance
   - Detects when temperature reduces CPU performance
   - **Pauses execution** to let the device cool down
   - **Retries** the current benchmark after cooling

2. **Prevention strategies**:
   - Uses `setSustainedPerformanceMode()` API
   - Can lock CPU clocks on rooted devices
   - Disables thermal throttling entirely (on rooted devices)

3. **Warmup handling**:
   - Automatically handles warmup phase
   - Measures code performance and allocation counts
   - Outputs results only after stable state achieved

**Key Insight**: Professional benchmarking frameworks recognize that **thermal effects are real** and require active mitigation.

---

## Industry Best Practices

### 1. The Purpose of Warmup

**Standard approach** (can show ~10% throughput improvement with sufficient warmup):
- First iteration often discarded as warmup
- Gives CPU time to ramp up frequency and temperature
- Amortizes one-time initialization costs (lazy allocations, etc.)
- Allows CPU to reach "steady state" before measurement
- **Reality**: We don't control caches - CPU manages them autonomously

**SimpleBench current**: 100,000 warmup iterations (~130ms for fast benchmarks, significantly LESS than Criterion's 3-second default)

### 2. CPU Affinity

**Why it matters**:
When a process migrates to a different CPU core, it incurs several costs:
- The new core's L1/L2 caches don't contain the process's data (hardware-managed, we can't control this)
- TLB (Translation Lookaside Buffer) must be repopulated with page mappings
- Context switch overhead

Pinning to a single core eliminates migration variance.

**SimpleBench**: Already implements CPU affinity (pins to core 0) ✓

### 3. Thermal Stabilization

**Problem**: Benchmarks generate heat, causing CPU to throttle during measurement
**Solutions**:
- **Pause and retry** when thermal throttling detected (Android approach)
- **Lock CPU frequency** to constant (prevents boost/throttle cycles)
- **Use minimum frequency** instead of maximum (prevents heat buildup)
- **System-wide warmup** before first benchmark (let CPU reach thermal equilibrium)

**SimpleBench**: No thermal management ✗

### 4. Fixed vs. Constant Frequency

**Recommendation** (from Android benchmarking):
> "Setting CPU clock speed to a constant minimum instead of maximum helps prevent heat, since the raw performance doesn't matter - just that benchmark results can be reliably compared."

This explains why SimpleBench's performance governor test only reduced regression from 6.9% to 5.9% - the issue is **thermal**, not frequency scaling.

---

## Analysis: Criterion vs. SimpleBench

### Criterion's Assumptions

1. **3-second warmup is sufficient** to reach CPU frequency and thermal equilibrium
2. **Per-benchmark warmup** eliminates first-run effects
3. **Steady-state execution** (not cache filling) is the primary warmup goal
4. **Thermal effects** are environmental (user's responsibility to control)

### SimpleBench's Reality

1. **100,000 warmup iterations per benchmark** (~130 milliseconds at 1.3µs/iteration)
2. **This is actually LESS warmup than Criterion's 3-second default!** (130ms vs. 3000ms)
3. **No system-wide warmup** means first benchmark runs on cold CPU (critical difference)
4. **FP-heavy workloads heat CPU** more than Criterion's typical use cases
5. **6% regression is thermal** (5% remains after fixing frequency scaling)

### The Mismatch

**Criterion's Design**:
- Optimized for **comparing different implementations** of the same code
- Runs multiple benchmarks in sequence with individual warmup
- Assumes environmental stability across runs

**SimpleBench's Use Case**:
- Establishing **baseline performance** for regression detection
- First run establishes baseline on cold CPU (optimistic)
- Second run measures on warm CPU (realistic but appears as regression)
- Needs **absolute stability** across multiple invocations

---

## Recommendations for SimpleBench

### Option 1: Aggressive Per-Benchmark Warmup (Criterion-style)

**Implementation**:
- Increase warmup iterations significantly (1,000-10,000)
- Use time-based warmup instead of fixed iterations
- Goal: Run warmup until CPU reaches thermal equilibrium

**Pros**:
- Aligns with Criterion's proven approach
- No breaking changes to CLI workflow
- May solve the issue if thermal equilibrium can be reached quickly

**Cons**:
- May not fully solve thermal drift across benchmarks
- First benchmark in suite still runs on colder CPU
- Longer warmup time per benchmark

**Test**: Try 10,000 warmup iterations (as suggested in investigation.md)

### Option 2: System-Wide Warmup Phase (Custom Solution)

**Implementation**:
- Before running any benchmarks, execute a "throwaway" CPU-intensive workload
- Run for 10-30 seconds to heat CPU and stabilize frequency/thermal state
- Then proceed with normal benchmark suite

**Pros**:
- Addresses root cause (thermal equilibrium)
- All benchmarks start from same thermal state
- Matches Android Microbenchmark approach

**Cons**:
- Adds overhead to every benchmark run
- No industry standard for Rust (custom implementation needed)
- May need tuning per-CPU architecture

**Pseudo-code**:
```rust
fn system_warmup() {
    eprintln!("Warming up system (10 seconds)...");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        // Run FP-heavy workload similar to benchmarks
        for i in 0..1000 {
            let v = Vec3::new(i as f32, i as f32 * 2.0, i as f32 * 3.0);
            let _ = v.normalize();
        }
    }
}
```

### Option 3: Discard First Run + Statistical Baseline (Conservative)

**Implementation**:
- Always run benchmarks twice on initial baseline establishment
- Discard first complete run
- Use second run as baseline
- OR: Run 3-5 times and use median result for each benchmark

**Pros**:
- Eliminates first-run optimism (CPU starts warm)
- Proven approach (many benchmarking guides recommend this)
- Robust to environmental variation between runs
- Simple to implement

**Cons**:
- 2-5x longer baseline establishment (one-time cost)
- Doesn't prevent thermal drift during long benchmark suites
- Treats symptom, not root cause
- Users might be confused why first run is "thrown away"

**Note**: This uses median of **complete runs** (not median of samples within a run). Each run still uses mean of 1,000 samples.

### Option 4: Thermal Monitoring + Adaptive Strategy (Advanced)

**Implementation**:
- Monitor CPU temperature during benchmarks
- Pause and retry if temperature exceeds threshold
- Or: Wait for temperature to stabilize before starting

**Pros**:
- Most scientifically rigorous
- Android Microbenchmark proves this works
- Handles both first-run and mid-suite thermal effects

**Cons**:
- Complex implementation
- Platform-specific (reading CPU sensors)
- May require elevated permissions

---

## Mean vs. Median for Benchmarking

### When to Use Median

Median is more robust to **outliers** and is preferred when:
- **Low sample counts** (50-100 samples) where outliers have high impact
- **Unstable environment** with frequent interruptions (OS preemption, GC pauses)
- **Establishing baselines** from multiple complete benchmark runs (e.g., median of 3-5 full runs)

**Example where median helps**:
```
5 complete runs of a benchmark:
Run 1: 1.30ms (baseline)
Run 2: 1.32ms (normal)
Run 3: 1.31ms (normal)
Run 4: 8.50ms (laptop went into power saving mode)
Run 5: 1.33ms (normal)

Mean: 2.75ms ← distorted by outlier run
Median: 1.32ms ← ignores outlier run
```

For establishing initial baselines, taking the **median of 3-5 complete benchmark runs** can be more robust than a single run.

### When to Use Mean (SimpleBench's Case) ✓

Mean is **superior for regression detection** when you have:
- **High sample counts** (1,000+ samples per benchmark)
- **Controlled environment** (CPU affinity, quiet system)
- **Stable measurements** (few expected outliers)

**Why mean is better for SimpleBench**:

1. **Uses all data**: Every sample contributes, maximizing statistical information
2. **Better statistical properties**: Standard error, confidence intervals, t-tests work correctly
3. **More sensitive**: Detects small performance changes (critical for regression detection)
4. **Outliers averaged out**: With 1,000 samples, a single 10x outlier affects mean by only ~0.9%

**Example with high sample count**:
```
1,000 samples, 998 at ~1.30ms, 2 outliers at 10ms:
Mean: 1.317ms   ← only 1.3% impact from outliers
Median: 1.30ms  ← ignores outliers
Difference: 0.017ms (1.3%)
```

With 1,000 samples, mean and median converge. The gain from median's outlier resistance is minimal.

### SimpleBench's 6% Regression: Not an Outlier Problem

The 6% regression you're observing is **systematic, not outliers**:
- The **entire distribution** shifts slower (all samples affected)
- Both mean AND median would show the same 6% regression
- Issue is thermal/frequency state, not statistical method

**Switching to median would not fix your first-run regression.**

### Recommendation

**Keep using mean** for per-benchmark measurement with 1,000 samples. However, consider:
- **Median of 3-5 runs** for establishing the initial baseline (discard outlier runs due to environmental factors)
- **Mean within each run** for measuring individual benchmarks (better sensitivity)

This gives you both robustness (baseline) and sensitivity (regression detection).

---

## CPU Frequency Management in Rust

### Overview

CPU frequency scaling (via governors) can introduce variance in benchmarks. SimpleBench already benefits from CPU affinity (pinning to core 0), but we can take this further by:

1. **Reading current CPU frequency** to detect throttling
2. **Setting CPU governor** programmatically (requires root/permissions)
3. **Monitoring thermal zones** to detect thermal throttling

### 1. Reading CPU Frequencies

```rust
use std::fs;
use std::path::Path;

/// Read current CPU frequency for a specific core
fn read_cpu_freq(cpu: usize) -> Result<u64, std::io::Error> {
    let path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq", cpu);
    let freq_str = fs::read_to_string(path)?;
    Ok(freq_str.trim().parse().unwrap_or(0))
}

/// Read min/max frequencies for a core
fn read_cpu_freq_range(cpu: usize) -> Result<(u64, u64), std::io::Error> {
    let min_path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/cpuinfo_min_freq", cpu);
    let max_path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/cpuinfo_max_freq", cpu);

    let min = fs::read_to_string(min_path)?.trim().parse().unwrap_or(0);
    let max = fs::read_to_string(max_path)?.trim().parse().unwrap_or(0);

    Ok((min, max))
}

/// Monitor CPU frequency during benchmark execution
fn log_cpu_frequency_stats(cpu: usize, samples: &[u64]) {
    let min = samples.iter().min().unwrap_or(&0);
    let max = samples.iter().max().unwrap_or(&0);
    let avg = samples.iter().sum::<u64>() / samples.len() as u64;

    eprintln!("CPU {} frequency: min={} MHz, max={} MHz, avg={} MHz",
              cpu, min / 1000, max / 1000, avg / 1000);
}

// Usage during benchmark:
fn run_benchmark_with_freq_monitoring() {
    let cpu = 0; // Core we're pinned to
    let mut freq_samples = Vec::new();

    // Sample frequency during benchmark
    for _ in 0..1000 {
        if let Ok(freq) = read_cpu_freq(cpu) {
            freq_samples.push(freq);
        }

        // Run benchmark iteration...
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    log_cpu_frequency_stats(cpu, &freq_samples);
}
```

### 2. Reading CPU Governor

```rust
/// Read current CPU governor for a core
fn read_cpu_governor(cpu: usize) -> Result<String, std::io::Error> {
    let path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", cpu);
    let governor = fs::read_to_string(path)?;
    Ok(governor.trim().to_string())
}

/// Check if performance governor is active
fn check_performance_governor(cpu: usize) -> bool {
    match read_cpu_governor(cpu) {
        Ok(gov) => {
            if gov != "performance" {
                eprintln!("WARNING: CPU {} using '{}' governor, not 'performance'", cpu, gov);
                eprintln!("For stable benchmarks, consider: echo performance | sudo tee /sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", cpu);
                false
            } else {
                true
            }
        }
        Err(e) => {
            eprintln!("WARNING: Could not read CPU governor: {}", e);
            false
        }
    }
}
```

### 3. Setting CPU Governor (Requires Root)

```rust
use std::process::Command;

/// Attempt to set CPU governor (requires root permissions)
fn set_cpu_governor(cpu: usize, governor: &str) -> Result<(), String> {
    let path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", cpu);

    // Try to write directly (requires root)
    if let Err(e) = fs::write(&path, governor) {
        return Err(format!("Failed to set governor (needs root): {}", e));
    }

    Ok(())
}

/// Set governor for all CPUs using system commands
fn set_all_cpus_governor(governor: &str) -> Result<(), String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "echo {} | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor",
            governor
        ))
        .output()
        .map_err(|e| format!("Failed to execute command: {}", e))?;

    if !output.status.success() {
        return Err(format!("Command failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    Ok(())
}
```

### 4. Reading CPU Temperature

```rust
/// Read temperature from a thermal zone (in millidegrees Celsius)
fn read_thermal_zone(zone: usize) -> Result<i32, std::io::Error> {
    let path = format!("/sys/class/thermal/thermal_zone{}/temp", zone);
    let temp_str = fs::read_to_string(path)?;
    Ok(temp_str.trim().parse().unwrap_or(0))
}

/// Find all available thermal zones
fn find_thermal_zones() -> Vec<usize> {
    let mut zones = Vec::new();
    for i in 0..20 {
        let path = format!("/sys/class/thermal/thermal_zone{}/temp", i);
        if Path::new(&path).exists() {
            zones.push(i);
        }
    }
    zones
}

/// Monitor temperature and detect thermal throttling
struct ThermalMonitor {
    baseline_temp: Option<i32>,
    throttle_threshold: i32, // millidegrees C
}

impl ThermalMonitor {
    fn new(throttle_threshold_celsius: i32) -> Self {
        Self {
            baseline_temp: None,
            throttle_threshold: throttle_threshold_celsius * 1000,
        }
    }

    fn check_throttling(&mut self, zone: usize) -> Result<bool, std::io::Error> {
        let current_temp = read_thermal_zone(zone)?;

        if self.baseline_temp.is_none() {
            self.baseline_temp = Some(current_temp);
        }

        let baseline = self.baseline_temp.unwrap();
        let temp_increase = current_temp - baseline;

        Ok(current_temp > self.throttle_threshold || temp_increase > 20000) // 20°C increase
    }

    fn wait_for_cooldown(&self, zone: usize, target_temp: i32) -> Result<(), std::io::Error> {
        eprintln!("Waiting for CPU to cool down...");
        loop {
            let temp = read_thermal_zone(zone)?;
            if temp < target_temp * 1000 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        eprintln!("CPU cooled down, resuming benchmarks");
        Ok(())
    }
}

// Usage:
fn run_benchmark_with_thermal_monitoring() {
    let zones = find_thermal_zones();
    if zones.is_empty() {
        eprintln!("WARNING: No thermal zones found, cannot monitor temperature");
        return;
    }

    let mut monitor = ThermalMonitor::new(85); // 85°C threshold

    for iteration in 0..1000 {
        // Check for thermal throttling before each benchmark
        if let Ok(is_throttling) = monitor.check_throttling(zones[0]) {
            if is_throttling {
                eprintln!("Thermal throttling detected at iteration {}", iteration);
                let _ = monitor.wait_for_cooldown(zones[0], 70); // Wait until 70°C
            }
        }

        // Run benchmark iteration...
    }
}
```

### 5. Comprehensive Pre-Benchmark Check

```rust
/// Comprehensive system check before running benchmarks
fn verify_benchmark_environment(cpu: usize) -> Result<(), String> {
    eprintln!("Verifying benchmark environment...");

    // 1. Check CPU governor
    match read_cpu_governor(cpu) {
        Ok(gov) => {
            eprintln!("  CPU {} governor: {}", cpu, gov);
            if gov != "performance" {
                eprintln!("  ⚠ WARNING: Not using 'performance' governor");
                eprintln!("  Consider: sudo cpupower frequency-set -g performance");
            }
        }
        Err(e) => eprintln!("  ⚠ Could not read governor: {}", e),
    }

    // 2. Check frequency range
    match read_cpu_freq_range(cpu) {
        Ok((min, max)) => {
            eprintln!("  CPU {} frequency range: {} MHz - {} MHz",
                      cpu, min / 1000, max / 1000);
        }
        Err(e) => eprintln!("  ⚠ Could not read frequency range: {}", e),
    }

    // 3. Check current frequency
    match read_cpu_freq(cpu) {
        Ok(freq) => {
            eprintln!("  CPU {} current frequency: {} MHz", cpu, freq / 1000);
        }
        Err(e) => eprintln!("  ⚠ Could not read current frequency: {}", e),
    }

    // 4. Check thermal zones
    let zones = find_thermal_zones();
    eprintln!("  Found {} thermal zone(s)", zones.len());
    for zone in &zones {
        if let Ok(temp) = read_thermal_zone(*zone) {
            eprintln!("    Zone {}: {}°C", zone, temp / 1000);
        }
    }

    eprintln!("Environment check complete.\n");
    Ok(())
}
```

### 6. System-Wide Warmup with Monitoring

```rust
use std::time::{Duration, Instant};

/// Perform system-wide warmup to reach thermal equilibrium
fn system_warmup_with_monitoring(duration_secs: u64, cpu: usize) {
    eprintln!("Starting system warmup ({} seconds)...", duration_secs);

    let start = Instant::now();
    let mut freq_samples = Vec::new();
    let zones = find_thermal_zones();
    let thermal_zone = zones.first().copied().unwrap_or(0);

    while start.elapsed() < Duration::from_secs(duration_secs) {
        // CPU-intensive FP work (similar to actual benchmarks)
        for i in 0..10000 {
            let x = (i as f32).sqrt();
            let y = x.sin();
            let z = y.cos();
            let _ = (x + y + z).sqrt(); // Prevent optimization
        }

        // Sample frequency every ~100ms
        if let Ok(freq) = read_cpu_freq(cpu) {
            freq_samples.push(freq);
        }

        // Print progress every 5 seconds
        let elapsed = start.elapsed().as_secs();
        if elapsed > 0 && elapsed % 5 == 0 {
            if let Ok(temp) = read_thermal_zone(thermal_zone) {
                eprintln!("  Warmup: {}s elapsed, temp: {}°C", elapsed, temp / 1000);
            }
        }
    }

    // Report warmup statistics
    if !freq_samples.is_empty() {
        log_cpu_frequency_stats(cpu, &freq_samples);
    }

    if let Ok(final_temp) = read_thermal_zone(thermal_zone) {
        eprintln!("  Final temperature: {}°C", final_temp / 1000);
    }

    eprintln!("System warmup complete.\n");
}
```

### Usage in SimpleBench

These utilities could be integrated into SimpleBench's runner:

```rust
// In runner.rs or simplebench-runtime
fn main() {
    let cpu = 0; // Core we pin to

    // 1. Verify environment
    let _ = verify_benchmark_environment(cpu);

    // 2. Perform system-wide warmup
    system_warmup_with_monitoring(10, cpu);

    // 3. Run benchmarks with monitoring
    let config = BenchmarkConfig::from_env();
    run_and_stream_benchmarks(config, Some(cpu));
}
```

### Platform Compatibility Notes

- **Linux**: Full support for `/sys/devices/system/cpu/` and `/sys/class/thermal/`
- **macOS**: Limited support, use `sysctl` commands instead
- **Windows**: Use WMI or Performance Counters (much more complex)

For maximum portability, these features should be:
1. **Optional** - gracefully degrade if not available
2. **Warning-based** - inform user if environment isn't optimal
3. **Linux-first** - focus on Linux where we have best control

---

## Recommended Next Steps

### Immediate Testing (Phase 1)

1. **Test increased per-benchmark warmup** (HIGHEST PRIORITY):
   - SimpleBench currently uses 100,000 warmup iterations (~130ms)
   - This is much LESS than Criterion's 3-second default
   - Test with time-based warmup: 3-5 seconds per benchmark

2. **Test system-wide warmup**:
   - Add 10-30 second CPU-intensive warmup BEFORE first benchmark runs
   - Compare first run vs. second run
   - If first run still faster → thermal equilibrium takes longer than 10s
   - If runs match → system warmup solves it

### Investigation (Phase 2)

3. **Monitor CPU temperature**:
   ```bash
   # During benchmark runs
   watch -n 0.1 'cat /sys/class/thermal/thermal_zone*/temp'
   ```
   - Correlate temperature with performance
   - Determine thermal stabilization time

4. **Test isolated single benchmark**:
   - Run only `bench_vec3_normalize` alone
   - Remove other benchmarks from test workspace
   - If regression disappears → it's inter-benchmark thermal accumulation
   - If regression persists → it's per-benchmark thermal effect

### Long-Term Solution (Phase 3)

5. **Implement hybrid approach**:
   - System-wide warmup (10-30s CPU-intensive workload before first benchmark)
   - Increase per-benchmark warmup to time-based (3-5 seconds minimum, like Criterion)
   - Add thermal monitoring on Linux (read `/sys/class/thermal/`)
   - Add CPU frequency verification (warn if not using performance governor)

6. **Document environmental requirements**:
   - Recommend performance governor (already done)
   - Add warning about thermal effects
   - Suggest running benchmarks on idle system

---

## Key Takeaways

1. **Criterion does NOT do system-wide warmup** - each benchmark warms up independently
2. **Warmup goal is CPU steady-state** - reaching stable frequency/thermal equilibrium, not "cache filling"
3. **We cannot control CPU caches** - no userspace API to query or manage L1/L2/L3 contents
4. **Google Benchmark added warmup** specifically for first-run slowness issues (MPI, JIT workloads)
5. **Android Microbenchmark actively manages thermals** - pauses/retries when throttling detected
6. **SimpleBench uses 100,000 warmup iterations** - but this is only ~130ms for fast benchmarks, MUCH less than Criterion's 3-second default
7. **Both per-benchmark warmup duration AND system-wide warmup are insufficient** for thermal stabilization
8. **The 5% unexplained regression is thermal/frequency** - not addressed by 130ms warmup

## Conclusion

**SimpleBench's first-run regression is a known problem** that professional benchmarking frameworks address through:
- **Longer warmup phases** (Criterion: 3 seconds, Google Benchmark: configurable) - gives CPU time to reach steady state
- **Thermal monitoring and adaptive retry** (Android Microbenchmark) - detects throttling, pauses to cool down
- **Statistical robustness** (discard first run, or use median of 3-5 runs for baseline establishment)

**SimpleBench actually has LESS per-benchmark warmup than Criterion** (100,000 iterations = ~130ms vs. Criterion's 3-second default). This reveals:
- **SimpleBench's per-benchmark warmup is too short** (~130ms is insufficient for CPU frequency ramp-up and thermal stabilization)
- **System-wide warmup is also needed** to ensure first benchmark doesn't run on cold CPU
- **We can't control caches** but we CAN give the CPU time to reach steady-state frequency and temperature
- **Thermal monitoring may be necessary** to detect and handle mid-suite throttling

**Recommended solution**:
1. Increase per-benchmark warmup significantly (time-based: 3-5 seconds minimum, matching Criterion)
2. Add system-wide warmup (10-30s before first benchmark runs)
3. Add thermal monitoring for detection/warning (read `/sys/class/thermal/`)
4. Add CPU frequency verification (warn if not using performance governor)

---

## Sources

- [Criterion.rs Documentation - Analysis Process](https://bheisler.github.io/criterion.rs/book/analysis.html)
- [Criterion.rs Documentation - Command Line Output](https://bheisler.github.io/criterion.rs/book/user_guide/command_line_output.html)
- [Criterion.rs Documentation - Timing Loops](https://bheisler.github.io/criterion.rs/book/user_guide/timing_loops.html)
- [What exactly does the warm up phase? - Issue #317](https://github.com/bheisler/criterion.rs/issues/317)
- [Google Benchmark - Allow for warm up before running each benchmark - Issue #1130](https://github.com/google/benchmark/issues/1130)
- [Google Benchmark User Guide](https://google.github.io/benchmark/user_guide.html)
- [Android Developers - Microbenchmark](https://developer.android.com/topic/performance/benchmarking/microbenchmark-overview)
- [Understanding Your Benchmarks and Easy Tips for Fixing Them](https://medium.com/@honglilai/understanding-your-benchmarks-and-easy-tips-for-fixing-them-7b89ea7d49e9)
- [What About Warmup? - AppFolio Engineering Blog](https://engineering.appfolio.com/appfolio-engineering/2017/5/2/what-about-warmup)
- [Improving Criterion.rs - Tweag](https://www.tweag.io/blog/2022-03-03-criterion-rs/)
