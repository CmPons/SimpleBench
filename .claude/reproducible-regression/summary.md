# First-Run Regression - Quick Summary

## The Problem
`bench_vec3_normalize` consistently runs ~6% **faster** on first run, then **slows down** and stabilizes:
- Run 1: 1.30ms (baseline)
- Run 2+: 1.38-1.41ms (6-7% slower, stable)

## What I Found

### ✅ Confirmed
1. **100% reproducible** - happens every time
2. **Affects FP-heavy benchmarks** - vec3_normalize (6.7%), entity_update_loop (3.2%)
3. **First run has higher CV** - First: 1.1-4.4%, Second+: 0.8-0.9%
4. **Second run is MORE consistent but SLOWER** - not just noise

### ❌ CPU Frequency Scaling is NOT the Main Cause
- With powersave: 6.9% regression
- With performance governor: 5.9% regression (only 1% improvement)
- **Conclusion**: Governor helps slightly but doesn't solve it

### 🤔 Unexplained: ~5% Performance Gap
**Frequency data:**
- Run 1: Avg 4591 MHz (never below 2998 MHz)
- Run 2: Avg 4549 MHz (dropped to 400 MHz between benchmarks)

Even with performance governor locked to high frequency, the regression persists.

## Leading Hypotheses

1. **Thermal throttling** - CPU heats up after first run
2. **AMD CPPC learning** - Power management adapts after first workload
3. **Power state equilibrium** - System settles into lower-performance P-state
4. **FPU thermal state** - Floating-point units warm up and throttle

## Impact
- **First baseline is ~6% optimistic** for FP benchmarks
- Creates false regression on second run
- Undermines confidence in results

## Recommended Next Steps

1. **Test thermal hypothesis**: Monitor CPU temperature during runs
2. **Test warmup hypothesis**: Try 10,000 warmup iterations instead of 50
3. **Test isolation hypothesis**: Run only vec3_normalize alone
4. **Research AMD CPPC/EPP**: Investigate Energy Performance Preference tuning

## Workaround Options

1. **Always discard first run** (doubles runtime)
2. **Use median of 3 baseline runs** (3x initial cost, more robust)
3. **Add aggressive warmup phase** (may stabilize state before benchmarks)
4. **Require performance governor** (helps 1%, not a fix)

## Files
- `investigation.md` - Full detailed analysis with data, references, and hypotheses
- `summary.md` - This quick reference
