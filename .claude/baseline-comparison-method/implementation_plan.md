# Bayesian Online Change Point Detection - Implementation Plan

**Date**: 2025-12-11
**Context**: Implementing statistical window + full BOCPD for SimpleBench regression detection
**Goal**: Replace single-run comparison with historical window analysis to eliminate false positives

---

## Problem Summary

Current SimpleBench behavior:
- Compares `current_mean` vs `last_run_mean` only
- 14+ historical runs stored but unused
- Single outlier (1.20ms) becomes baseline
- Next normal run (1.30ms) falsely flagged as 8% regression

**Solution**: Use rolling window of historical runs with statistical significance testing

---

## Implementation Approach

We'll implement both Phase 1 (statistical window) and Phase 2 (full BOCPD) together:

1. **Statistical Window**: Mean + stddev + confidence intervals (solves outlier problem)
2. **Bayesian Change Point Detection**: Detects gradual regressions and distribution shifts
3. **Combined Decision Logic**: Require statistical significance + practical significance + change point probability

---

## Architecture Changes

### New Module: `simplebench-runtime/src/statistics.rs`

Core statistical functions used by both approaches:

```rust
pub mod statistics {
    /// Calculate mean of a slice
    pub fn mean(values: &[f64]) -> f64;

    /// Calculate standard deviation
    pub fn standard_deviation(values: &[f64]) -> f64;

    /// Calculate variance
    pub fn variance(values: &[f64]) -> f64;

    /// Calculate confidence interval bounds
    pub fn confidence_interval(
        mean: f64,
        stddev: f64,
        confidence_level: f64, // e.g., 0.95 for 95%
    ) -> (f64, f64); // (lower_bound, upper_bound)

    /// Calculate z-score (how many stddevs away from mean)
    pub fn z_score(value: f64, mean: f64, stddev: f64) -> f64;
}
```

### New Module: `simplebench-runtime/src/changepoint.rs`

Bayesian Online Change Point Detection implementation:

```rust
pub mod changepoint {
    /// Bayesian Online CPD core algorithm
    pub struct BayesianCPD {
        hazard_rate: f64,           // Prior probability of change point
        run_length_probs: Vec<f64>, // Posterior distribution over run lengths
    }

    impl BayesianCPD {
        pub fn new(hazard_rate: f64) -> Self;

        /// Update with new observation, returns change point probability
        pub fn update(&mut self, value: f64, historical: &[f64]) -> f64;

        /// Student's t-distribution likelihood
        fn student_t_likelihood(
            &self,
            value: f64,
            historical: &[f64],
        ) -> f64;

        /// Geometric hazard function (prior)
        fn hazard_function(&self, run_length: usize) -> f64;
    }

    /// Simplified API: calculate change point probability for a new value
    pub fn bayesian_change_point_probability(
        new_value: f64,
        historical: &[f64],
        hazard_rate: f64,
    ) -> f64;
}
```

### Modified: `simplebench-runtime/src/baseline.rs`

#### 1. New function: `load_recent_baselines()`

Replace single baseline loading with window loading:

```rust
impl BaselineManager {
    /// Load last N baseline runs for a benchmark
    pub fn load_recent_baselines(
        &self,
        crate_name: &str,
        benchmark_name: &str,
        count: usize,
    ) -> Result<Vec<BaselineData>, std::io::Error> {
        let history_dir = self.history_dir(crate_name, benchmark_name);

        // List all run timestamps
        let mut runs: Vec<String> = std::fs::read_dir(history_dir)?
            .filter_map(|entry| {
                entry.ok().and_then(|e| {
                    e.file_name().to_str()
                        .and_then(|s| s.strip_suffix(".json"))
                        .map(|s| s.to_string())
                })
            })
            .collect();

        // Sort chronologically
        runs.sort();

        // Take last N runs
        let recent = runs.iter().rev().take(count);

        // Load baseline data for each run
        let mut baselines = Vec::new();
        for timestamp in recent {
            let path = history_dir.join(format!("{}.json", timestamp));
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(baseline) = serde_json::from_str::<BaselineData>(&data) {
                    baselines.push(baseline);
                }
            }
        }

        Ok(baselines)
    }
}
```

#### 2. New function: `detect_regression_with_cpd()`

Combined statistical window + BOCPD logic:

```rust
use crate::statistics::*;
use crate::changepoint::*;

pub fn detect_regression_with_cpd(
    current: &BenchResult,
    historical: &[BaselineData],
    threshold: f64,
    confidence_level: f64,   // e.g., 0.95
    cp_threshold: f64,       // e.g., 0.8 (80% probability)
    hazard_rate: f64,        // e.g., 0.1 (expect change every 10 runs)
) -> ComparisonResult {
    if historical.is_empty() {
        return ComparisonResult {
            benchmark_name: current.name.clone(),
            comparison: None,
            is_regression: false,
        };
    }

    // Extract means from historical runs
    let historical_means: Vec<f64> = historical.iter()
        .map(|b| b.statistics.mean)
        .collect();

    let current_mean = current.percentiles.mean.as_nanos() as f64;

    // --- Statistical Window Analysis ---
    let hist_mean = mean(&historical_means);
    let hist_stddev = standard_deviation(&historical_means);

    // Z-score: how many standard deviations away?
    let z_score = z_score(current_mean, hist_mean, hist_stddev);

    // Confidence interval (one-tailed for regressions)
    let z_critical = if confidence_level == 0.95 {
        1.645  // 95% one-tailed
    } else if confidence_level == 0.99 {
        2.326  // 99% one-tailed
    } else {
        1.96   // Default two-tailed 95%
    };

    let upper_bound = hist_mean + (z_critical * hist_stddev);
    let statistically_significant = current_mean > upper_bound;

    // --- Bayesian Change Point Detection ---
    let change_probability = bayesian_change_point_probability(
        current_mean,
        &historical_means,
        hazard_rate,
    );

    // --- Practical Significance ---
    let percentage_change = ((current_mean - hist_mean) / hist_mean) * 100.0;
    let practically_significant = percentage_change > threshold;

    // --- Combined Decision ---
    // Fail if ALL three conditions met:
    // 1. Statistical significance (outside confidence interval)
    // 2. Practical significance (exceeds threshold percentage)
    // 3. High change point probability (likely a real distribution shift)
    let is_regression =
        statistically_significant &&
        practically_significant &&
        change_probability > cp_threshold;

    ComparisonResult {
        benchmark_name: current.name.clone(),
        comparison: Some(Comparison {
            current_mean,
            baseline_mean: hist_mean,
            percentage_change,
            baseline_count: historical.len(),
            z_score: Some(z_score),
            confidence_interval: Some((hist_mean - z_critical * hist_stddev, upper_bound)),
            change_probability: Some(change_probability),
        }),
        is_regression,
    }
}
```

#### 3. Update `Comparison` struct

Add new fields for statistical analysis:

```rust
pub struct Comparison {
    pub current_mean: f64,
    pub baseline_mean: f64,
    pub percentage_change: f64,
    pub baseline_count: usize,

    // NEW: Statistical window fields
    pub z_score: Option<f64>,
    pub confidence_interval: Option<(f64, f64)>,  // (lower, upper)

    // NEW: Change point detection fields
    pub change_probability: Option<f64>,
}
```

#### 4. Update `process_with_baselines()`

Replace single baseline loading with window loading:

```rust
impl BaselineManager {
    pub fn process_with_baselines(
        &self,
        results: &[BenchResult],
        config: &BenchmarkConfig,
    ) -> Vec<ComparisonResult> {
        results
            .iter()
            .map(|result| {
                // Load window of historical runs (default: 10)
                let window_size = config.window_size.unwrap_or(10);

                match self.load_recent_baselines(
                    &result.crate_name,
                    &result.name,
                    window_size,
                ) {
                    Ok(historical) if !historical.is_empty() => {
                        // Use new CPD comparison
                        detect_regression_with_cpd(
                            result,
                            &historical,
                            config.threshold,
                            config.confidence_level.unwrap_or(0.95),
                            config.cp_threshold.unwrap_or(0.8),
                            config.hazard_rate.unwrap_or(0.1),
                        )
                    }
                    _ => {
                        // No baseline available
                        ComparisonResult {
                            benchmark_name: result.name.clone(),
                            comparison: None,
                            is_regression: false,
                        }
                    }
                }
            })
            .collect()
    }
}
```

### Modified: `simplebench-runtime/src/config.rs`

Add new configuration parameters:

```rust
pub struct BenchmarkConfig {
    // ... existing fields ...

    // NEW: Statistical window size (default: 10)
    pub window_size: Option<usize>,

    // NEW: Confidence level for statistical tests (default: 0.95)
    pub confidence_level: Option<f64>,

    // NEW: Change point probability threshold (default: 0.8)
    pub cp_threshold: Option<f64>,

    // NEW: Bayesian hazard rate (default: 0.1)
    pub hazard_rate: Option<f64>,
}

impl BenchmarkConfig {
    pub fn apply_env_overrides(&mut self) {
        // ... existing overrides ...

        // NEW: Window size
        if let Ok(val) = env::var("SIMPLEBENCH_WINDOW") {
            if let Ok(size) = val.parse::<usize>() {
                self.window_size = Some(size);
            }
        }

        // NEW: Confidence level
        if let Ok(val) = env::var("SIMPLEBENCH_CONFIDENCE") {
            if let Ok(conf) = val.parse::<f64>() {
                self.confidence_level = Some(conf);
            }
        }

        // NEW: Change point threshold
        if let Ok(val) = env::var("SIMPLEBENCH_CP_THRESHOLD") {
            if let Ok(thresh) = val.parse::<f64>() {
                self.cp_threshold = Some(thresh);
            }
        }

        // NEW: Hazard rate
        if let Ok(val) = env::var("SIMPLEBENCH_HAZARD_RATE") {
            if let Ok(rate) = val.parse::<f64>() {
                self.hazard_rate = Some(rate);
            }
        }
    }
}
```

### Modified: `cargo-simplebench/src/main.rs`

Add CLI flags for new parameters:

```rust
#[derive(clap::Parser)]
struct Cli {
    // ... existing fields ...

    /// Window size for historical comparison (default: 10)
    #[arg(long, default_value = "10")]
    window: usize,

    /// Statistical confidence level (default: 0.95 = 95%)
    #[arg(long, default_value = "0.95")]
    confidence: f64,

    /// Change point probability threshold (default: 0.8 = 80%)
    #[arg(long, default_value = "0.8")]
    cp_threshold: f64,

    /// Bayesian hazard rate (default: 0.1 = change every 10 runs)
    #[arg(long, default_value = "0.1")]
    hazard_rate: f64,
}

fn main() {
    // ... parse args ...

    // Set environment variables for runner
    env::set_var("SIMPLEBENCH_WINDOW", args.window.to_string());
    env::set_var("SIMPLEBENCH_CONFIDENCE", args.confidence.to_string());
    env::set_var("SIMPLEBENCH_CP_THRESHOLD", args.cp_threshold.to_string());
    env::set_var("SIMPLEBENCH_HAZARD_RATE", args.hazard_rate.to_string());

    // ... rest of main ...
}
```

### Modified: Output Formatting

Update comparison output to show new statistics:

```rust
// In simplebench-runtime/src/lib.rs or wherever comparison results are printed

fn print_comparison(result: &ComparisonResult) {
    if let Some(cmp) = &result.comparison {
        println!("  Baseline: {:.2?} (n={})",
            Duration::from_nanos(cmp.baseline_mean as u64),
            cmp.baseline_count);

        println!("  Change: {:.2}%", cmp.percentage_change);

        if let Some(z) = cmp.z_score {
            println!("  Z-score: {:.2}", z);
        }

        if let Some(cp_prob) = cmp.change_probability {
            println!("  Change point probability: {:.2}%", cp_prob * 100.0);
        }

        if result.is_regression {
            println!("  ❌ REGRESSION DETECTED");
        } else {
            println!("  ✓ OK");
        }
    }
}
```

---

## CI Mode Integration

### Existing Behavior

SimpleBench already implements `--ci` mode via:
- `cargo-simplebench/src/main.rs:64` - CLI flag `--ci: bool`
- `simplebench-runtime/src/baseline.rs:408` - `check_regressions_and_exit()` function
- Generated runner calls `check_regressions_and_exit()` when `config.comparison.ci_mode` is true

**Current flow**:
1. Runner executes all benchmarks
2. Comparisons performed via `process_with_baselines()`
3. If `ci_mode == true`, calls `check_regressions_and_exit(comparisons, config)`
4. If any `comparison.is_regression == true`, prints error and exits with code 1

### No Changes Required

The new `detect_regression_with_cpd()` function sets `ComparisonResult.is_regression` based on the combined decision logic (statistical + practical + change point). This boolean flows through to the existing CI mode check.

**Integration is automatic**:
- `detect_regression_with_cpd()` returns `ComparisonResult` with `is_regression: bool`
- `check_regressions_and_exit()` checks `comparisons.iter().any(|c| c.is_regression)`
- Exit code 1 if any regression found

### Testing CI Mode

The implementation must verify:
1. **No false positives**: Outlier scenario → exit code 0 in CI mode
2. **Detect real regressions**: Acute regression → exit code 1 in CI mode
3. **Error message clarity**: Exit message should mention statistical criteria used

**Test script** (see Real-World Validation section):
```bash
# After establishing 10-run baseline

# Test 1: Normal run in CI mode (should pass)
../target/release/cargo-simplebench --ci
echo "Exit code: $?"  # Expected: 0

# Test 2: Inject regression, run in CI mode (should fail)
# (edit code to add work)
../target/release/cargo-simplebench --ci
echo "Exit code: $?"  # Expected: 1
```

---

## Bayesian Online Change Point Detection Algorithm

### Core Implementation

Based on Adams & MacKay (2007) paper, simplified for Gaussian data:

```rust
// simplebench-runtime/src/changepoint.rs

pub struct BayesianCPD {
    hazard_rate: f64,
    run_length_probs: Vec<f64>,
}

impl BayesianCPD {
    pub fn new(hazard_rate: f64) -> Self {
        Self {
            hazard_rate,
            run_length_probs: vec![1.0], // Start with run length 0
        }
    }

    pub fn update(&mut self, value: f64, historical: &[f64]) -> f64 {
        if historical.is_empty() {
            return 0.0;
        }

        let n = self.run_length_probs.len();
        let mut new_probs = vec![0.0; n + 1];

        // Hazard function (geometric prior)
        let hazard = self.hazard_function(n);

        // Predictive probability for each run length
        for (r, &prob) in self.run_length_probs.iter().enumerate() {
            let likelihood = self.student_t_likelihood(
                value,
                &historical[historical.len().saturating_sub(r + 1)..],
            );

            // Growth probability (no change point)
            new_probs[r + 1] += prob * (1.0 - hazard) * likelihood;

            // Change point probability
            new_probs[0] += prob * hazard * likelihood;
        }

        // Normalize
        let sum: f64 = new_probs.iter().sum();
        if sum > 0.0 {
            for p in &mut new_probs {
                *p /= sum;
            }
        }

        self.run_length_probs = new_probs;

        // Return probability of change point (run length = 0)
        self.run_length_probs[0]
    }

    fn student_t_likelihood(&self, value: f64, historical: &[f64]) -> f64 {
        if historical.is_empty() {
            return 0.5;
        }

        let n = historical.len() as f64;
        let mean = historical.iter().sum::<f64>() / n;
        let variance = historical.iter()
            .map(|&x| (x - mean).powi(2))
            .sum::<f64>() / n;

        if variance < 1e-10 {
            // Very low variance, use normal approximation
            let stddev = variance.sqrt();
            let z = ((value - mean) / stddev).abs();
            return (-0.5 * z * z).exp();
        }

        // Student's t-distribution with n-1 degrees of freedom
        let df = n - 1.0;
        let t = (value - mean) / variance.sqrt();
        let t_squared = t * t;

        // Simplified Student's t PDF (good enough for our purposes)
        let coef = ((1.0 + t_squared / df).powf(-(df + 1.0) / 2.0));
        coef
    }

    fn hazard_function(&self, run_length: usize) -> f64 {
        // Geometric distribution: P(change at time t) = hazard_rate
        self.hazard_rate
    }
}

/// Simplified API: calculate change point probability
pub fn bayesian_change_point_probability(
    new_value: f64,
    historical: &[f64],
    hazard_rate: f64,
) -> f64 {
    let mut cpd = BayesianCPD::new(hazard_rate);
    cpd.update(new_value, historical)
}
```

---

## Implementation Steps

### Step 1: Create `statistics.rs` module

- [ ] Create `simplebench-runtime/src/statistics.rs`
- [ ] Implement `mean()` function
- [ ] Implement `variance()` function
- [ ] Implement `standard_deviation()` function
- [ ] Implement `z_score()` function
- [ ] Implement `confidence_interval()` function
- [ ] Add unit tests for all functions
- [ ] Add module to `simplebench-runtime/src/lib.rs`

### Step 2: Create `changepoint.rs` module

- [ ] Create `simplebench-runtime/src/changepoint.rs`
- [ ] Implement `BayesianCPD` struct
- [ ] Implement `new()` constructor
- [ ] Implement `update()` method
- [ ] Implement `student_t_likelihood()` helper
- [ ] Implement `hazard_function()` helper
- [ ] Implement `bayesian_change_point_probability()` convenience function
- [ ] Add unit tests with known change points
- [ ] Add module to `simplebench-runtime/src/lib.rs`

### Step 3: Update `baseline.rs` data structures

- [ ] Add `z_score: Option<f64>` to `Comparison` struct
- [ ] Add `confidence_interval: Option<(f64, f64)>` to `Comparison` struct
- [ ] Add `change_probability: Option<f64>` to `Comparison` struct
- [ ] Update serialization/deserialization

### Step 4: Implement `load_recent_baselines()`

- [ ] Add method to `BaselineManager`
- [ ] List all run timestamps in history directory
- [ ] Sort chronologically
- [ ] Take last N runs (parameter)
- [ ] Load each baseline's JSON data
- [ ] Return `Vec<BaselineData>`
- [ ] Add unit test with mock filesystem

### Step 5: Implement `detect_regression_with_cpd()`

- [ ] Create function in `baseline.rs`
- [ ] Extract historical means from baselines
- [ ] Calculate statistical window metrics (mean, stddev, z-score)
- [ ] Calculate confidence interval bounds
- [ ] Run Bayesian change point detection
- [ ] Calculate percentage change
- [ ] Combine three conditions for regression decision
- [ ] Return `ComparisonResult` with all metadata
- [ ] Add unit tests covering all scenarios

### Step 6: Update `process_with_baselines()`

- [ ] Replace `load_baseline()` with `load_recent_baselines()`
- [ ] Pass window size from config
- [ ] Replace old comparison logic with `detect_regression_with_cpd()`
- [ ] Pass all config parameters (confidence, cp_threshold, hazard_rate)
- [ ] Handle empty historical data case

### Step 7: Update `config.rs`

- [ ] Add `window_size: Option<usize>` field
- [ ] Add `confidence_level: Option<f64>` field
- [ ] Add `cp_threshold: Option<f64>` field
- [ ] Add `hazard_rate: Option<f64>` field
- [ ] Implement env var overrides for all new fields
- [ ] Add defaults in relevant constructors

### Step 8: Update CLI in `cargo-simplebench`

- [ ] Add `--window` flag (default: 10)
- [ ] Add `--confidence` flag (default: 0.95)
- [ ] Add `--cp-threshold` flag (default: 0.8)
- [ ] Add `--hazard-rate` flag (default: 0.1)
- [ ] Set environment variables before runner execution
- [ ] Update help text

### Step 9: Update output formatting

- [ ] Display baseline count (n=N)
- [ ] Display z-score if available
- [ ] Display confidence interval if available
- [ ] Display change point probability if available
- [ ] Format output clearly and concisely

### Step 10: Testing & Validation

- [ ] Delete existing baselines: `cd test-workspace && ../target/release/cargo-simplebench clean`
- [ ] Run 10 normal benchmarks to establish baseline distribution
- [ ] Inject artificial outlier (modify code to be temporarily faster)
- [ ] Verify outlier doesn't become baseline
- [ ] Restore normal code, verify next run passes
- [ ] Inject acute regression (5x slower code)
- [ ] Verify acute regression detected
- [ ] Test with minimal historical data (1-2 runs)
- [ ] Test with no historical data (first run)
- [ ] **Test CI mode**: Run `cargo simplebench --ci` after establishing baseline, verify exit code 0
- [ ] **Test CI mode with regression**: Inject regression, run `--ci`, verify exit code 1
- [ ] **Test CI mode returns to normal**: Restore code, run `--ci`, verify exit code 0

---

## Expected Behavior After Implementation

### Scenario 1: Outlier Handling (The Original Problem)

**Historical runs**: [1.30, 1.31, 1.29, 1.30, 1.30, 1.31, 1.29, 1.30, 1.31, 1.30] ms

**Run 11 (outlier)**: 1.20ms
- Mean of historical: 1.30ms, stddev: 0.007ms
- Z-score: (1.20 - 1.30) / 0.007 = **-14.3** (far outside CI)
- Change probability: **~0.95** (high, but it's FASTER)
- Result: **PASS** (not a regression, just faster)

**Run 12**: 1.30ms (back to normal)
- Historical now: [1.31, 1.29, 1.30, 1.30, 1.31, 1.29, 1.30, 1.31, 1.30, 1.20] ms
- Mean: 1.295ms, stddev: 0.03ms
- Z-score: (1.30 - 1.295) / 0.03 = **0.17** (well within CI)
- Change probability: **~0.05** (not a change point)
- Result: **PASS** ✓

### Scenario 2: Acute Regression Detection

**Historical runs**: [1.30, 1.31, 1.29, 1.30, 1.30, 1.31, 1.29, 1.30, 1.31, 1.30] ms

**Run 11 (regression)**: 1.50ms (15% slower)
- Mean: 1.30ms, stddev: 0.007ms
- Z-score: (1.50 - 1.30) / 0.007 = **28.6** (far outside CI)
- Percentage change: **+15%** (exceeds 5% threshold)
- Change probability: **~0.99** (very likely change point)
- All 3 conditions met: **FAIL** ❌

### Scenario 3: False Positive Prevention

**Historical runs**: [1.30, 1.31, 1.29, 1.30, 1.30, 1.31, 1.29, 1.30, 1.31, 1.30] ms

**Run 11**: 1.38ms (6% slower, but within natural variance)
- Mean: 1.30ms, stddev: 0.007ms
- Z-score: (1.38 - 1.30) / 0.007 = **11.4** (outside CI)
- Percentage change: **+6%** (exceeds 5% threshold)
- Change probability: **~0.85** (high)
- All 3 conditions met: **FAIL** ❌

**Note**: If this is too sensitive, user can increase `--confidence 0.99` or `--cp-threshold 0.9`

---

## Configuration Recommendations

### Default Configuration (Conservative)

```bash
cargo simplebench \
  --window 10 \
  --confidence 0.95 \
  --cp-threshold 0.8 \
  --hazard-rate 0.1 \
  --threshold 5.0
```

This configuration:
- Uses last 10 runs for comparison
- Requires 95% statistical confidence
- Requires 80% change point probability
- Expects change every 10 runs on average
- 5% practical significance threshold

### Strict Configuration (Reduce False Positives)

```bash
cargo simplebench \
  --window 20 \
  --confidence 0.99 \
  --cp-threshold 0.9 \
  --hazard-rate 0.05 \
  --threshold 8.0
```

This configuration:
- Uses last 20 runs (more historical context)
- Requires 99% statistical confidence
- Requires 90% change point probability
- Expects change every 20 runs
- 8% practical significance threshold

### Sensitive Configuration (Catch Small Regressions)

```bash
cargo simplebench \
  --window 10 \
  --confidence 0.90 \
  --cp-threshold 0.7 \
  --hazard-rate 0.2 \
  --threshold 3.0
```

This configuration:
- Uses last 10 runs
- Requires 90% statistical confidence
- Requires 70% change point probability
- Expects change every 5 runs
- 3% practical significance threshold

---

## Testing Strategy

### Unit Tests

1. **statistics.rs**:
   - Test `mean()` with known values
   - Test `standard_deviation()` matches known results
   - Test `z_score()` calculation
   - Test `confidence_interval()` bounds

2. **changepoint.rs**:
   - Test with synthetic data containing known change points
   - Test with stable data (no change points)
   - Test with gradual drift
   - Test with outliers

3. **baseline.rs**:
   - Test `load_recent_baselines()` with mock filesystem
   - Test `detect_regression_with_cpd()` with various scenarios
   - Test edge cases (empty history, single run, etc.)

### Integration Tests

1. **End-to-end validation**:
   - Clean baselines
   - Run 10 stable benchmarks
   - Inject outlier, verify it passes
   - Run normal benchmark, verify it passes
   - Inject regression, verify it fails
   - Test gradual regression over 5 runs

2. **Configuration testing**:
   - Test each CLI flag
   - Test environment variable overrides
   - Test config file + env var precedence

### Real-World Validation

Use existing `test-workspace` benchmarks:

```bash
cd test-workspace

# Clean slate
../target/release/cargo-simplebench clean

# Establish baseline (run 10 times)
for i in 1 2 3 4 5 6 7 8 9 10; do
    echo "=== Baseline Run $i ==="
    ../target/release/cargo-simplebench
done

# Test outlier handling: temporarily make code faster
# Edit game-math/src/lib.rs, reduce work in bench_vec3_normalize
# Run once, should pass
../target/release/cargo-simplebench
echo "Exit code: $?"  # Should be 0

# Restore original code, run again, should still pass
git restore game-math/src/lib.rs
../target/release/cargo-simplebench
echo "Exit code: $?"  # Should be 0

# Test regression detection: make code slower
# Edit game-math/src/lib.rs, add extra work
../target/release/cargo-simplebench --ci
echo "Exit code: $?"  # Should be 1 (failure)

# Test CI mode without regression: restore code
git restore game-math/src/lib.rs
../target/release/cargo-simplebench --ci
echo "Exit code: $?"  # Should be 0 (success)
```

---

## Success Criteria

1. **Outlier false positives eliminated**: 1.20ms outlier followed by 1.30ms normal run → PASS
2. **Acute regressions detected**: 1.30ms → 1.50ms (15% slower) → FAIL
3. **Statistical confidence**: All decisions backed by >95% confidence
4. **Historical data utilized**: 10+ runs used for comparison, not just last 1
5. **Configurable**: Users can tune sensitivity via CLI flags
6. **Clear output**: Z-scores and change probabilities displayed
7. **No breaking changes**: Existing baselines still work (graceful degradation to single-run comparison if history unavailable)
8. **CI mode works correctly**:
   - Exit code 0 when no regressions detected
   - Exit code 1 when any regression detected (using new CPD logic)
   - Error message shows threshold used in decision

---

## Files to Create

- `simplebench-runtime/src/statistics.rs` (~100 lines)
- `simplebench-runtime/src/changepoint.rs` (~200 lines)

## Files to Modify

- `simplebench-runtime/src/lib.rs` (add module declarations)
- `simplebench-runtime/src/baseline.rs` (~150 lines changes)
- `simplebench-runtime/src/config.rs` (~50 lines changes)
- `cargo-simplebench/src/main.rs` (~30 lines changes)

## Estimated Implementation Time

- **Core statistics module**: 1 hour
- **Bayesian CPD module**: 2-3 hours
- **Baseline integration**: 2 hours
- **Configuration updates**: 1 hour
- **CLI updates**: 30 minutes
- **Testing & validation**: 2-3 hours

**Total**: ~8-10 hours of focused development

---

## References

- Research findings: `.claude/baseline-comparison-method/research_findings.md`
- Adams & MacKay (2007): "Bayesian Online Changepoint Detection"
- Bencher.dev Change Point Detection: https://bencher.dev/docs/explanation/thresholds/
- Turing Institute CPD Benchmark: https://github.com/alan-turing-institute/TCPDBench
