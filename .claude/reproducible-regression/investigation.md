# Reproducible First-Run vs Second-Run Regression Investigation

**Date**: 2025-12-10
**Issue**: `game_math::bench_vec3_normalize` consistently shows ~6% regression on second run after baseline is established

## Problem Summary

When running `cargo simplebench` repeatedly:
- **First run** (baseline establishment): 1.30ms, CV: 1.1-1.3%
- **Second run**: 1.38-1.39ms (6-7% regression), CV: 0.8-0.9%
- **Subsequent runs**: Stable at 1.38-1.41ms

The pattern is highly reproducible and occurs **every time** baselines are cleared and benchmarks are re-run.

## Key Observations

### 1. Pattern Specificity
- **Affects FP-heavy benchmarks most**: `bench_vec3_normalize` (6.7% regression) and `bench_entity_update_loop` (3.2% regression)
- **Integer-heavy benchmarks remain stable**: Most other benchmarks show <1% variation
- **Not continuous degradation**: Performance stabilizes after second run (doesn't keep getting worse)

### 2. Statistical Characteristics
- **First run has HIGHER variance**: CV: 1.1-4.4%
- **Second run has LOWER variance**: CV: 0.8-0.9%
- This indicates the second run is more **consistent but systematically slower**, not just noisier

### 3. CPU Frequency Monitoring Results

#### With Powersave Governor (Default)
**First Run:**
- Result: 1.30ms
- CPU Freq: Min 2998 MHz, Max 4649 MHz, **Avg 4591 MHz**
- CPU never dropped below 2998 MHz during benchmark

**Second Run:**
- Result: 1.39ms (6.9% regression)
- CPU Freq: **Min 400 MHz**, Max 4649 MHz, **Avg 4549 MHz**
- CPU idled to 400 MHz between benchmarks, suggesting frequency scaling transitions

#### With Performance Governor (Tested)
**First Run:**
- Result: 1.31ms, CV: 4.4%

**Second Run:**
- Result: 1.39ms (5.9% regression), CV: 0.8%

**Third Run:**
- Result: 1.41ms (stable)

**Fourth Run:**
- Result: 1.40ms (stable)

### 4. Critical Finding: Performance Governor Does NOT Eliminate Regression

The regression **still occurs** with the performance governor (5.9% vs 6.9% with powersave).

This means **CPU frequency scaling is NOT the primary cause**, or at least not the only cause.

## System Information

- **CPU Scaling Driver**: `amd-pstate-epp`
- **Default Governor**: `powersave`
- **CPU Frequency Range**: 400 MHz - 4673 MHz
- **Platform**: NixOS (Linux 6.12.58)
- **SimpleBench Config**: 1000 samples × 1000 iterations, 50 warmup iterations

## Benchmark Code Analysis

The `bench_vec3_normalize` benchmark:
```rust
#[mbench]
fn bench_vec3_normalize() {
    let mut vectors = Vec::new();
    for i in 0..1000 {
        vectors.push(Vec3::new(i as f32, i as f32 * 2.0, i as f32 * 3.0));
    }

    for v in &vectors {
        let _normalized = v.normalize();
    }
}
```

The `normalize()` function performs:
- `sqrt()` calculation
- 3 floating-point divisions

This is heavy floating-point math, which may be more sensitive to certain system state changes.

## Research Findings

From industry sources:

1. **Google Benchmark Library** recommends using the "performance" governor to reduce variance ([Reducing Variance](https://google.github.io/benchmark/reducing_variance.html))

2. **Phoronix Intel i9-11900K Testing** showed powersave vs performance yielded:
   - 9% performance increase overall
   - 20-30% improvements in some cases
   - CPU power consumption increased by 37%
   - Peak frequency: 5.07 GHz (powersave) vs 5.3 GHz (performance)
   ([Source](https://www.phoronix.com/review/intel-11900k-pstate))

3. **Modern AMD CPUs** (amd-pstate-epp): Both powersave and performance governors provide dynamic scaling, but use different Energy Performance Preference (EPP) hints
   ([Kernel Documentation](https://www.kernel.org/doc/Documentation/cpu-freq/governors.txt))

## Hypotheses for Root Cause

### ✅ Confirmed Contributing Factor: CPU Frequency Scaling
- Powersave governor causes CPU to idle to 400 MHz between benchmarks
- First run benefits from higher minimum frequency (2998 MHz)
- **However**: This only explains ~1% of the 6% regression (performance governor still shows 5.9% regression)

### 🔍 Likely Additional Factors

1. **Thermal Effects**
   - First run on "cold" CPU may have higher thermal headroom
   - Subsequent runs may throttle due to heat buildup
   - AMD CPUs use thermal-aware frequency scaling

2. **Cache/Memory State**
   - First run might benefit from different cache state
   - Memory allocator state could differ between runs
   - However, warmup iterations should mitigate this

3. **CPU Power Management State Transitions**
   - C-states and P-states may settle into different equilibrium after first run
   - First run might trigger more aggressive boost behavior
   - AMD CPPC (Collaborative Processor Performance Control) may learn workload patterns

4. **Floating-Point Unit (FPU) State**
   - First run FPU might be in different power/thermal state
   - Denormal number handling could differ
   - SIMD unit warm-up effects

5. **Branch Predictor/Cache Pollution**
   - Running the full benchmark suite might pollute branch prediction or caches
   - Subsequent runs start with "polluted" state

## Reproduction Steps

```bash
# Clean baselines
rm -rf test-workspace/.benches/

# Run 1 - establishes baseline
cargo simplebench
# bench_vec3_normalize: ~1.30ms

# Run 2 - will regress
cargo simplebench
# bench_vec3_normalize: ~1.38ms (6-7% regression)

# Run 3+ - stable
cargo simplebench
# bench_vec3_normalize: ~1.38-1.41ms
```

## Impact Assessment

**Severity**: Medium-High
- Makes baseline comparison unreliable for FP-heavy benchmarks
- First baseline is consistently ~6% faster than "true" performance
- Creates false regression warnings on second run
- Affects user confidence in benchmark results

**Scope**:
- Primarily affects floating-point heavy benchmarks
- Integer/memory benchmarks mostly unaffected
- Reproducible across governor settings (powersave and performance)

## Recommendations for Further Investigation

1. **Test with different warmup strategies**
   - Increase warmup iterations significantly (currently 50)
   - Add system-wide warmup before any benchmarks run
   - Test if running a "throwaway" benchmark first helps

2. **Monitor thermal state**
   - Track CPU temperature during runs
   - Check if thermal throttling is occurring
   - Use `sensors` or `/sys/class/thermal/` monitoring

3. **Test with isolated single benchmark**
   - Run only `bench_vec3_normalize` in isolation
   - See if regression still occurs without other benchmarks

4. **Investigate AMD CPPC behavior**
   - Research AMD's Collaborative Processor Performance Control
   - Check Energy Performance Preference (EPP) settings
   - Test with EPP values: performance (0), balance_performance (128), balance_power (192), power (255)

5. **Profile actual CPU frequency during benchmark execution**
   - Not just overall average, but per-sample frequency
   - Correlate slow samples with frequency dips
   - Use `perf` to track CPU frequency events

6. **Test on different hardware**
   - Verify if this is AMD-specific or general
   - Test on Intel with intel_pstate driver
   - Test on desktop vs laptop (different thermal characteristics)

## Potential Solutions

### Option 1: Always Discard First Run
- Modify SimpleBench to always run benchmarks twice
- Only use second+ runs for baseline/comparison
- **Pros**: Simple, works around the issue
- **Cons**: Doubles benchmark runtime, doesn't fix root cause

### Option 2: Implement Aggressive System Warmup
- Add system-wide warmup phase before any benchmarks
- Run CPU-intensive workload to stabilize frequency/thermal state
- **Pros**: May eliminate the first-run anomaly
- **Cons**: Adds overhead, may not fully solve the issue

### Option 3: Require Performance Governor
- Document that SimpleBench requires performance governor
- Add detection and warning if powersave is active
- Provide NixOS configuration example
- **Pros**: Best practice for benchmarking
- **Cons**: Reduces ~1% of variance but doesn't eliminate the 5.9% core issue

### Option 4: Statistical Approach - Multiple Baseline Runs
- Establish baseline from median of 3-5 runs
- Detect and flag first-run anomalies
- **Pros**: More robust baseline
- **Cons**: 3-5x longer initial baseline establishment

### Option 5: Root Cause Investigation and Fix
- Deeply investigate thermal/power management interaction
- Potentially file bug report with AMD/kernel developers if hardware/driver issue
- **Pros**: Proper fix
- **Cons**: Time-consuming, may not be actionable

## NixOS Configuration Note

For users wanting to test with performance governor, add to NixOS configuration:

```nix
# configuration.nix
powerManagement.cpuFreqGovernor = "performance";
```

Then rebuild: `sudo nixos-rebuild switch`

However, note that this only reduces the regression from 6.9% to 5.9%, so it's not a complete solution.

## Next Steps

1. Test with significantly increased warmup iterations (1000+)
2. Monitor CPU temperature during runs
3. Profile per-sample CPU frequency correlation
4. Test running only single benchmark in isolation
5. Research AMD CPPC/EPP settings specific to amd-pstate-epp driver

## Conclusion

This is a **real, reproducible, systematic performance difference** between first run and subsequent runs, primarily affecting floating-point heavy workloads. CPU frequency scaling contributes (~1%) but is **not the primary cause** (5%+ remains unexplained). Further investigation into thermal management, power state transitions, and AMD-specific CPU features is needed to fully understand and resolve this issue.
