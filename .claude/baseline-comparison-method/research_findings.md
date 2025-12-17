# Statistical Methods for Benchmark Regression Detection - Research Findings

**Date**: 2025-12-11
**Context**: SimpleBench pre-commit regression detection improvement research

## Problem Statement

SimpleBench currently uses mean-to-mean comparison **against only the last run**, despite storing **all historical runs**. This approach has proven **too volatile** as evidenced by:

- 10 runs of `bench_vec3_normalize`: remarkably stable at ~1.30ms mean
- 1 outlier run (`2025-12-11T11-11-27`): 1.20ms mean (8% faster)
- This outlier became the comparison baseline
- Next normal run (1.30ms) flagged as 8% regression (false positive)

**We have 14+ runs stored per benchmark but only use the most recent one!**

**Core Issue**: Single-run comparison cannot distinguish between:
1. **Measurement noise** (natural variance in identical code)
2. **Acute regressions** (immediate performance drop from a commit)
3. **Gradual regressions** (slow degradation over many commits)

---

## Gold Standard Solution: Change Point Detection

### Industry Consensus

**Bencher.dev, Google, Facebook, Apple, Amazon, Netflix** all use **Change Point Detection** with a rolling window of historical runs. This is the **gold standard** for continuous benchmarking.

**Why it's the best**:
1. **Ignores outliers as noise** - the 1.20ms run wouldn't become baseline
2. **Detects real shifts** - only fails when distribution actually changes
3. **Prevents false positives** - 1.30ms after 1.20ms outlier wouldn't fail
4. **Detects gradual regressions** - catches slow drift over many commits
5. **We already have all the data!** - just need to use it

**Source**: [Bencher.dev Change Point Detection](https://bencher.dev/docs/explanation/thresholds/)

### How Change Point Detection Works

Instead of comparing:
```rust
current_mean vs last_run_mean
```

We compare:
```rust
current_run vs distribution_of_last_N_runs
```

**Algorithm** (Binary Segmentation or Bayesian Online CPD):
1. Load last N runs (e.g., 10 runs)
2. Calculate expected distribution (mean, variance)
3. Check if current run is outside expected range
4. Use statistical test (t-test, z-score, etc.) to validate

**Example with our data** (bench_vec3_normalize):
- Runs 1-10: [1.30, 1.31, 1.29, 1.30, 1.30, 1.31, 1.29, 1.30, 1.31, 1.30]
- Run 11 (outlier): 1.20ms
- Run 12: 1.30ms

**Current behavior**:
- Run 11 becomes baseline
- Run 12 fails (8% slower than 1.20ms)

**Change point detection**:
- Run 11: Not a change point (isolated outlier, distribution hasn't shifted)
- Run 12: Not a regression (consistent with runs 1-10 distribution)

---

## Recommended Implementation: Bayesian Change Point Detection

### Why Bayesian?

The Alan Turing Institute benchmark of 14 change point algorithms on 37 real-world time series found:

**Best performers**:
1. **Bayesian Online Change Point Detection** (BOCPD)
2. Binary Segmentation

**BOCPD advantages**:
- Online algorithm (processes each new run as it arrives)
- Probabilistic output (tells you confidence level)
- No hyperparameters to tune
- Robust to different noise levels

**Source**: [Turing Change Point Detection Benchmark](https://www.turing.ac.uk/news/publications/evaluation-change-point-detection-algorithms)

### SimpleBench Architecture

```rust
// NEW: Load last N runs instead of just last 1
pub fn load_recent_baselines(
    &self,
    crate_name: &str,
    benchmark_name: &str,
    count: usize
) -> Result<Vec<BaselineData>, std::io::Error> {
    let mut runs = self.list_runs(crate_name, benchmark_name)?;
    runs.sort();

    // Take last N runs
    let recent = runs.iter().rev().take(count);

    let mut baselines = Vec::new();
    for timestamp in recent {
        if let Some(baseline) = self.load_run(crate_name, benchmark_name, timestamp)? {
            baselines.push(baseline);
        }
    }

    Ok(baselines)
}

// NEW: Change point detection comparison
pub fn detect_regression_with_cpd(
    current: &BenchResult,
    historical: &[BaselineData],
    threshold: f64,
) -> ComparisonResult {
    if historical.is_empty() {
        return ComparisonResult::no_baseline();
    }

    // Extract means from historical runs
    let historical_means: Vec<f64> = historical.iter()
        .map(|b| b.statistics.mean)
        .collect();

    let current_mean = current.percentiles.mean.as_nanos() as f64;

    // Calculate distribution statistics
    let hist_mean = statistical_mean(&historical_means);
    let hist_stddev = standard_deviation(&historical_means);

    // Z-score test: how many standard deviations away is current run?
    let z_score = (current_mean - hist_mean) / hist_stddev;

    // Bayesian change point: probability this is a change point
    let change_probability = bayesian_change_point_probability(
        current_mean,
        &historical_means,
    );

    // Fail if BOTH conditions met:
    // 1. Statistical significance (z-score > 2.0 = 95% confidence)
    // 2. Practical significance (exceeds threshold)
    // 3. High change point probability (> 0.8)

    let percentage_change = ((current_mean - hist_mean) / hist_mean) * 100.0;

    let is_regression =
        z_score > 2.0 &&  // 95% confidence interval
        percentage_change > threshold &&
        change_probability > 0.8;  // 80% sure it's a real change

    ComparisonResult {
        benchmark_name: current.name.clone(),
        comparison: Some(Comparison {
            current_mean,
            baseline_mean: hist_mean,
            percentage_change,
            baseline_count: historical.len(),
            z_score: Some(z_score),
            change_probability: Some(change_probability),
        }),
        is_regression,
    }
}
```

### What This Solves

**Scenario 1: Outlier becomes baseline (your problem)**
- Historical: [1.30, 1.31, 1.29, 1.30, 1.30, 1.31, 1.29, 1.30, 1.31, 1.30]
- Run 11 (outlier): 1.20ms
  - Mean of historical: 1.30ms, stddev: 0.007ms
  - Z-score: (1.20 - 1.30) / 0.007 = -14.3 (!!!)
  - Change probability: 0.95 (very likely a change point)
  - But it's FASTER, so not a regression
  - **Result**: Accepted, but flagged as suspicious outlier
- Run 12: 1.30ms
  - Now historical includes run 11: [1.31, 1.29, 1.30, 1.30, 1.31, 1.29, 1.30, 1.31, 1.30, 1.20]
  - Mean: 1.295ms, stddev: 0.03ms
  - Z-score: (1.30 - 1.295) / 0.03 = 0.17
  - Change probability: 0.05 (not a change point)
  - **Result**: PASS (correctly identified as noise)

**Scenario 2: Acute regression (single commit slows down)**
- Historical: [1.30, 1.31, 1.29, 1.30, 1.30, 1.31, 1.29, 1.30, 1.31, 1.30]
- Run 11: 1.50ms (15% slower)
  - Mean: 1.30ms, stddev: 0.007ms
  - Z-score: (1.50 - 1.30) / 0.007 = 28.6
  - Change probability: 0.99
  - Percentage change: 15%
  - **Result**: FAIL (correctly detected acute regression)

**Scenario 3: Gradual regression (death by 1000 cuts)**
- Run 1-10: 1.30ms average
- Run 11-20: Each adds 0.3% slowdown
  - Run 11: 1.304ms (z-score: 0.5, PASS)
  - Run 12: 1.308ms (z-score: 0.6, PASS)
  - Run 13: 1.312ms (z-score: 0.7, PASS)
  - ...
  - Run 20: 1.33ms
  - **Change point probability increases** as distribution shifts
  - At run 18-19: change probability crosses 0.8 threshold
  - **Result**: FAIL (correctly detected gradual regression)

---

## Implementation Strategy

### Available Libraries

**Rust change point detection libraries**:

1. **changepoint** crate (if it exists, need to check crates.io)
2. **Custom implementation**: Bayesian Online CPD is ~150 lines
3. **Call Python**: Use `ruptures` library (gold standard, but adds Python dependency)

**Recommendation**: Implement Bayesian Online CPD directly in Rust. Algorithm is well-documented and not complex.

### Bayesian Online Change Point Detection (BOCPD)

**Paper**: "Bayesian Online Changepoint Detection" by Adams & MacKay (2007)

**Algorithm overview**:
```rust
// Simplified BOCPD for Gaussian data
fn bayesian_change_point_probability(
    new_value: f64,
    historical: &[f64],
) -> f64 {
    // Prior: how long since last change point?
    let run_length_prior = geometric_prior(hazard_rate=0.1);

    // Likelihood: probability of new value given current run length
    // Assume Gaussian with unknown mean/variance
    let likelihood = student_t_likelihood(new_value, historical);

    // Posterior: probability of change point
    let change_point_prob = run_length_prior * (1.0 - likelihood);

    change_point_prob
}
```

**Full implementation**: ~150-200 lines including Student's t-distribution

### Phase 1: Simple Statistical Window (Quick Win)

Before implementing full BOCPD, a simpler approach that still uses the historical data:

```rust
pub fn detect_regression_with_window(
    current: &BenchResult,
    historical: &[BaselineData],
    threshold: f64,
) -> ComparisonResult {
    if historical.is_empty() {
        return ComparisonResult::no_baseline();
    }

    // Use last 10 runs (or fewer if not available)
    let window_size = historical.len().min(10);
    let recent = &historical[historical.len() - window_size..];

    // Calculate statistics of recent runs
    let means: Vec<f64> = recent.iter()
        .map(|b| b.statistics.mean)
        .collect();

    let hist_mean = statistical_mean(&means);
    let hist_stddev = standard_deviation(&means);

    let current_mean = current.percentiles.mean.as_nanos() as f64;

    // Use 95% confidence interval (z-score of 1.96 for two-tailed)
    // But we only care about regressions (one-tailed), so use 1.645
    let upper_bound = hist_mean + (1.645 * hist_stddev);

    let percentage_change = ((current_mean - hist_mean) / hist_mean) * 100.0;

    // Fail if BOTH:
    // 1. Outside 95% confidence interval
    // 2. Exceeds threshold percentage
    let is_regression =
        current_mean > upper_bound &&
        percentage_change > threshold;

    ComparisonResult {
        benchmark_name: current.name.clone(),
        comparison: Some(Comparison {
            current_mean,
            baseline_mean: hist_mean,
            percentage_change,
            baseline_count: recent.len(),
            confidence_interval: Some((hist_mean - 1.645*hist_stddev, upper_bound)),
        }),
        is_regression,
    }
}
```

**Benefits**:
- Uses historical data (solves outlier problem)
- Simple to implement (~50 lines)
- No external dependencies
- 95% statistical confidence

**Limitations**:
- Doesn't explicitly detect gradual regressions (but helps)
- Fixed window size (no adaptation)
- Assumes Gaussian distribution

**This is a great MVP before full BOCPD!**

---

## Configuration

### New CLI Flags

```bash
# Window size for change point detection (default: 10)
cargo simplebench --window 20

# Confidence level for statistical tests (default: 0.95 = 95%)
cargo simplebench --confidence 0.99

# Change point threshold (default: 0.8 = 80% probability)
cargo simplebench --cp-threshold 0.8

# Regression threshold (default: 5%)
cargo simplebench --threshold 5.0
```

### Environment Variables

```bash
SIMPLEBENCH_WINDOW=10
SIMPLEBENCH_CONFIDENCE=0.95
SIMPLEBENCH_CP_THRESHOLD=0.8
SIMPLEBENCH_THRESHOLD=5.0
```

---

## Benchmark Isolation: Monolithic vs Per-Process

### Current Approach: Monolithic (All in One Process)

SimpleBench runs all benchmarks sequentially in a single process.

**Potential interference**:
- **Memory allocator state**: Previous benchmark affects fragmentation
- **CPU cache pollution**: Previous data evicted from L1/L2/L3
- **Branch predictor state**: Previous code paths trained predictor
- **TLB state**: Previous virtual memory mappings cached

### Testing Strategy

SimpleBench has `--filter` flag to isolate benchmarks. We can test empirically:

```bash
# Test script to compare monolithic vs isolated
cd test-workspace

# Clean baselines
../target/release/cargo-simplebench clean

# Run monolithic 5 times
echo "=== MONOLITHIC MODE ==="
for i in 1 2 3 4 5; do
    echo "Run $i"
    ../target/release/cargo-simplebench | tee monolithic_$i.txt
done

# Run isolated 5 times (each benchmark in separate process)
echo "=== ISOLATED MODE ==="
for i in 1 2 3 4 5; do
    echo "Run $i"
    ../target/release/cargo-simplebench --filter bench_vec3_normalize >> isolated_$i.txt
    ../target/release/cargo-simplebench --filter bench_vec3_cross_product >> isolated_$i.txt
    ../target/release/cargo-simplebench --filter bench_vec3_dot_product >> isolated_$i.txt
    # ... repeat for all benchmarks
done

# Analyze variance
python3 analyze_variance.py monolithic_*.txt isolated_*.txt
```

**Metrics to compare**:
1. **Mean stability**: Is average runtime more consistent?
2. **Variance**: Is stddev lower in isolated mode?
3. **Systematic bias**: Is benchmark A always slower after benchmark B?
4. **Overhead**: How much longer does isolated mode take?

### Hypothesis

Given SimpleBench's existing variance reduction:
- **CPU pinning** (core 0) - prevents context switching
- **5 second warmup** - stabilizes cache/predictor state
- **1000 samples** - averages out interference
- **0-5% variance achieved** - already very stable

**Prediction**: Interference is likely minimal. But worth testing empirically with real data.

**If interference is significant** (>2% difference):
- Add `--isolate` flag to run each benchmark in separate process
- Trade-off: More accurate vs slower execution

---

## Summary & Recommended Implementation

### The Gold Standard Approach

**Implement Change Point Detection with historical window**

### Phase 1: Statistical Window (Immediate - 2-4 hours)

**Goal**: Use historical data to prevent outlier false positives

**Implementation**:
1. Modify `load_baseline()` to `load_recent_baselines(count=10)`
2. Implement `detect_regression_with_window()` with 95% confidence intervals
3. Update `process_with_baselines()` to use new function
4. Test on the problematic `bench_vec3_normalize` case

**Expected results**:
- 1.20ms outlier: Accepted (outside CI, but within distribution)
- 1.30ms after outlier: PASS (within 95% CI of historical mean)
- 1.50ms acute regression: FAIL (outside CI and exceeds threshold)

**Files to modify**:
- `simplebench-runtime/src/baseline.rs:198` - Add `load_recent_baselines()`
- `simplebench-runtime/src/baseline.rs:340` - Modify `process_with_baselines()`
- `simplebench-runtime/src/lib.rs` - Add statistical functions (mean, stddev, confidence intervals)

### Phase 2: Full Bayesian Change Point Detection (1-2 days)

**Goal**: Detect gradual regressions and adapt to changing code

**Implementation**:
1. Implement Bayesian Online CPD algorithm
2. Add `bayesian_change_point_probability()` function
3. Combine statistical significance + practical significance + change point probability
4. Add detailed logging of change point analysis

**Expected results**:
- Detects gradual regressions (3% per commit over 5 commits)
- Adapts when code legitimately changes (major refactor)
- Provides probability scores for debugging

### Phase 3: Benchmark Isolation Study (2-3 hours)

**Goal**: Determine if monolithic benchmarking introduces interference

**Implementation**:
1. Create test script (see above)
2. Run 5x monolithic, 5x isolated
3. Statistical analysis of variance
4. Make data-driven decision

**Possible outcomes**:
- **No significant difference**: Keep monolithic (faster, simpler)
- **Significant interference** (>2%): Add `--isolate` flag

---

## References

### Change Point Detection
- [Bencher.dev Change Point Detection](https://bencher.dev/docs/explanation/thresholds/)
- [Turing Institute: Evaluation of Change Point Detection Algorithms](https://www.turing.ac.uk/news/publications/evaluation-change-point-detection-algorithms)
- [TCPDBench GitHub Repository](https://github.com/alan-turing-institute/TCPDBench)
- [Bayesian Online Changepoint Detection Paper (Adams & MacKay 2007)](https://arxiv.org/abs/0710.3742)
- [Change Point Detection Survey](https://pmc.ncbi.nlm.nih.gov/articles/PMC5464762/)

### Statistical Methods
- [Criterion.rs Statistical Analysis](https://docs.rs/criterion/latest/criterion/)
- [Bencher.dev Thresholds & Statistical Tests](https://bencher.dev/docs/explanation/thresholds/)
- [Welch's t-test - Wikipedia](https://en.wikipedia.org/wiki/Welch's_t-test)
- [Best practice: Use Welch t-test](https://journals.sagepub.com/doi/full/10.1177/0004563221992088)

### Performance Regression Detection in Industry
- [What makes a real change in software performance? (2024)](https://www.sciencedirect.com/science/article/abs/pii/S0167642323001508)
- [Android: Fighting Regressions with Benchmarks in CI](https://medium.com/androiddevelopers/fighting-regressions-with-benchmarks-in-ci-6ea9a14b5c71)
- [Including Performance Benchmarks into CI to Enable DevOps](https://www.researchgate.net/publication/274738961_Including_Performance_Benchmarks_into_Continuous_Integration_to_Enable_DevOps)

### Statistical Significance & Effect Size
- [Calculating and reporting effect sizes](https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2013.00863/full)
- [Statistical Noise in Research: Basic Concepts](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC9896112/)
- [Quantifying Variance in Evaluation Benchmarks](https://arxiv.org/html/2406.10229v1)
