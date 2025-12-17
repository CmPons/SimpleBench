# IQR-Based Outlier Filtering Implementation Plan

**Date**: 2025-12-10
**Status**: Planning Phase
**Goal**: Further reduce variance by excluding outliers using IQR method

---

## Executive Summary

This plan implements statistical outlier removal using the Interquartile Range (IQR) method to exclude extreme measurements from percentile and mean calculations. Current implementation achieves 0-3% variance, and outlier removal may reduce this further and improve stability.

**Key Changes:**
- Create centralized IQR filtering function in simplebench-runtime
- Apply outlier filtering to `calculate_percentiles()` and `calculate_statistics()`
- Make filtering configurable (default: enabled)
- Update analyze command to show filtered vs unfiltered comparisons
- Consolidate duplicate IQR code from analyze.rs

---

## Background

### Current State

**Percentile Calculation** (`simplebench-runtime/src/lib.rs:97-117`):
- Uses ALL samples to calculate mean, p50, p90, p99
- No outlier detection or removal
- Achieved 0-3% variance with Phase 1 (high sample counts + fixed iterations)

**Statistics Calculation** (`simplebench-runtime/src/lib.rs:120-182`):
- Used by analyze command
- Calculates comprehensive statistics from raw samples
- No outlier filtering applied

**Analyze Command** (`cargo-simplebench/src/analyze.rs:159-239`):
- Has IQR calculation for DISPLAY purposes only (lines 163-174)
- Shows outlier counts but doesn't use them to filter data
- Duplicate IQR code that should be consolidated

### Problem Statement

System noise (OS scheduling, CPU frequency changes, memory page faults) can create measurement spikes that:
1. Inflate mean values (especially with smaller sample sizes)
2. Shift percentiles upward
3. Increase variance between runs
4. Cause false positive regressions

**Example outlier scenario:**
```
99 samples: 5.0μs, 5.1μs, 5.0μs, ... (typical)
1 outlier: 25.0μs (OS scheduler preemption)

Without filtering:
  mean = 5.2μs (inflated by 4%)

With IQR filtering:
  mean = 5.05μs (accurate)
```

### Why IQR Method?

Research ([PMC6647454](https://pmc.ncbi.nlm.nih.gov/articles/PMC6647454/)) shows:
- **IQR (Tukey's fence)**: 25% breakdown point, robust until 20% outliers
- **Better than Z-score**: Z-score fails with any outliers (0% breakdown point)
- **Used by Criterion.rs**: Industry-standard Rust benchmarking tool

**IQR Formula:**
```
Q1 = 25th percentile
Q3 = 75th percentile
IQR = Q3 - Q1

Lower fence = Q1 - 1.5 × IQR
Upper fence = Q3 + 1.5 × IQR

Outliers: samples < lower_fence OR samples > upper_fence
```

---

## Architecture Design

### 1. Core Outlier Filtering Module

**New file**: `simplebench-runtime/src/outliers.rs`

```rust
use std::time::Duration;

/// Configuration for outlier detection
#[derive(Debug, Clone, Copy)]
pub struct OutlierConfig {
    /// Enable outlier filtering (default: true)
    pub enabled: bool,

    /// IQR multiplier for fence calculation (default: 1.5)
    /// - 1.5 = standard Tukey's fence (recommended)
    /// - 3.0 = more conservative (fewer outliers removed)
    pub iqr_multiplier: f64,
}

impl Default for OutlierConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            iqr_multiplier: 1.5,
        }
    }
}

/// Result of outlier detection with detailed statistics
#[derive(Debug, Clone)]
pub struct OutlierAnalysis {
    /// Original sample count
    pub total_samples: usize,

    /// Number of outliers detected
    pub outlier_count: usize,

    /// Percentage of samples that are outliers
    pub outlier_percentage: f64,

    /// Lower fence threshold (nanoseconds)
    pub lower_fence: u128,

    /// Upper fence threshold (nanoseconds)
    pub upper_fence: u128,

    /// Q1 (25th percentile)
    pub q1: u128,

    /// Q3 (75th percentile)
    pub q3: u128,

    /// Interquartile range
    pub iqr: u128,

    /// Indices of outlier samples in original array
    pub outlier_indices: Vec<usize>,
}

/// Filter outliers from Duration samples using IQR method
///
/// Returns (filtered_samples, outlier_analysis)
pub fn filter_outliers_duration(
    samples: &[Duration],
    config: OutlierConfig,
) -> (Vec<Duration>, OutlierAnalysis) {
    // Convert to nanoseconds for calculation
    let samples_ns: Vec<u128> = samples.iter().map(|d| d.as_nanos()).collect();

    let (filtered_ns, analysis) = filter_outliers_u128(&samples_ns, config);

    // Convert back to Duration
    let filtered_durations: Vec<Duration> = filtered_ns
        .into_iter()
        .map(|ns| Duration::from_nanos(ns as u64))
        .collect();

    (filtered_durations, analysis)
}

/// Filter outliers from u128 samples (nanoseconds) using IQR method
///
/// Returns (filtered_samples, outlier_analysis)
pub fn filter_outliers_u128(
    samples: &[u128],
    config: OutlierConfig,
) -> (Vec<u128>, OutlierAnalysis) {
    if !config.enabled || samples.len() < 4 {
        // Need at least 4 samples for quartile calculation
        return (
            samples.to_vec(),
            OutlierAnalysis {
                total_samples: samples.len(),
                outlier_count: 0,
                outlier_percentage: 0.0,
                lower_fence: 0,
                upper_fence: u128::MAX,
                q1: 0,
                q3: 0,
                iqr: 0,
                outlier_indices: vec![],
            },
        );
    }

    // Calculate quartiles
    let mut sorted = samples.to_vec();
    sorted.sort();

    let q1_idx = (sorted.len() * 25) / 100;
    let q3_idx = (sorted.len() * 75) / 100;

    let q1 = sorted[q1_idx.min(sorted.len() - 1)];
    let q3 = sorted[q3_idx.min(sorted.len() - 1)];
    let iqr = q3.saturating_sub(q1);

    // Calculate fences
    let fence_distance = ((iqr as f64) * config.iqr_multiplier) as u128;
    let lower_fence = q1.saturating_sub(fence_distance);
    let upper_fence = q3.saturating_add(fence_distance);

    // Filter outliers and track indices
    let mut filtered = Vec::with_capacity(samples.len());
    let mut outlier_indices = Vec::new();

    for (idx, &sample) in samples.iter().enumerate() {
        if sample >= lower_fence && sample <= upper_fence {
            filtered.push(sample);
        } else {
            outlier_indices.push(idx);
        }
    }

    let outlier_count = outlier_indices.len();
    let outlier_percentage = (outlier_count as f64 / samples.len() as f64) * 100.0;

    let analysis = OutlierAnalysis {
        total_samples: samples.len(),
        outlier_count,
        outlier_percentage,
        lower_fence,
        upper_fence,
        q1,
        q3,
        iqr,
        outlier_indices,
    };

    (filtered, analysis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_outliers() {
        let samples = vec![100, 101, 102, 103, 104, 105];
        let config = OutlierConfig::default();

        let (filtered, analysis) = filter_outliers_u128(&samples, config);

        assert_eq!(filtered.len(), 6);
        assert_eq!(analysis.outlier_count, 0);
        assert_eq!(analysis.outlier_percentage, 0.0);
    }

    #[test]
    fn test_with_outliers() {
        // 99 normal samples + 1 extreme outlier
        let mut samples: Vec<u128> = (100..199).collect();
        samples.push(500); // Extreme outlier

        let config = OutlierConfig::default();
        let (filtered, analysis) = filter_outliers_u128(&samples, config);

        // Outlier should be removed
        assert!(filtered.len() < samples.len());
        assert_eq!(analysis.outlier_count, 1);
        assert_eq!(analysis.outlier_indices, vec![99]);
    }

    #[test]
    fn test_disabled_filtering() {
        let samples = vec![100, 101, 102, 500]; // 500 is clear outlier
        let config = OutlierConfig {
            enabled: false,
            iqr_multiplier: 1.5,
        };

        let (filtered, analysis) = filter_outliers_u128(&samples, config);

        // All samples retained when disabled
        assert_eq!(filtered.len(), 4);
        assert_eq!(analysis.outlier_count, 0);
    }

    #[test]
    fn test_conservative_multiplier() {
        let samples = vec![100, 101, 102, 103, 104, 150]; // 150 is moderate outlier

        // Standard (1.5x) - should detect outlier
        let config_standard = OutlierConfig {
            enabled: true,
            iqr_multiplier: 1.5,
        };
        let (_, analysis_standard) = filter_outliers_u128(&samples, config_standard);

        // Conservative (3.0x) - may not detect moderate outlier
        let config_conservative = OutlierConfig {
            enabled: true,
            iqr_multiplier: 3.0,
        };
        let (_, analysis_conservative) = filter_outliers_u128(&samples, config_conservative);

        // Standard should detect more outliers
        assert!(analysis_standard.outlier_count >= analysis_conservative.outlier_count);
    }

    #[test]
    fn test_duration_filtering() {
        let samples = vec![
            Duration::from_nanos(100),
            Duration::from_nanos(101),
            Duration::from_nanos(102),
            Duration::from_nanos(500), // Outlier
        ];

        let config = OutlierConfig::default();
        let (filtered, analysis) = filter_outliers_duration(&samples, config);

        assert!(filtered.len() < samples.len());
        assert!(analysis.outlier_count > 0);
    }
}
```

### 2. Update Configuration

**File**: `simplebench-runtime/src/config.rs`

Add outlier configuration to MeasurementConfig:

```rust
use crate::outliers::OutlierConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasurementConfig {
    /// Number of timing samples to collect per benchmark
    #[serde(default = "default_samples")]
    pub samples: usize,

    /// Number of iterations per sample
    #[serde(default = "default_iterations")]
    pub iterations: usize,

    /// Number of warmup iterations before measurement
    #[serde(default = "default_warmup_iterations")]
    pub warmup_iterations: usize,

    /// Outlier filtering configuration
    #[serde(default)]
    pub outlier_filter: OutlierConfig,
}
```

Add environment variable support:

```rust
// In apply_env_overrides():
if let Ok(filter_enabled) = std::env::var("SIMPLEBENCH_FILTER_OUTLIERS") {
    self.measurement.outlier_filter.enabled = filter_enabled == "1" || filter_enabled.to_lowercase() == "true";
}

if let Ok(multiplier) = std::env::var("SIMPLEBENCH_IQR_MULTIPLIER") {
    if let Ok(val) = multiplier.parse() {
        self.measurement.outlier_filter.iqr_multiplier = val;
    }
}
```

Add CLI arguments in `cargo-simplebench/src/main.rs`:

```rust
/// Disable outlier filtering
#[arg(long, global = true)]
no_outlier_filter: bool,

/// IQR multiplier for outlier detection (default: 1.5)
#[arg(long, global = true)]
iqr_multiplier: Option<f64>,
```

### 3. Update Percentile Calculation

**File**: `simplebench-runtime/src/lib.rs`

Update `calculate_percentiles` to use outlier filtering:

```rust
pub fn calculate_percentiles(timings: &[Duration]) -> Percentiles {
    calculate_percentiles_with_config(timings, OutlierConfig::default())
}

pub fn calculate_percentiles_with_config(
    timings: &[Duration],
    outlier_config: OutlierConfig,
) -> Percentiles {
    use crate::outliers::filter_outliers_duration;

    // Apply outlier filtering
    let (filtered_timings, _analysis) = filter_outliers_duration(timings, outlier_config);

    // Use filtered data for calculation
    let samples = if filtered_timings.is_empty() {
        // Fallback to original if all samples filtered (shouldn't happen)
        timings
    } else {
        &filtered_timings
    };

    let mut sorted_timings = samples.to_vec();
    sorted_timings.sort();

    let len = sorted_timings.len();
    let p50_idx = (len * 50) / 100;
    let p90_idx = (len * 90) / 100;
    let p99_idx = (len * 99) / 100;

    // Calculate mean from filtered data
    let sum_nanos: u128 = samples.iter().map(|d| d.as_nanos()).sum();
    let mean_nanos = sum_nanos / (len as u128);
    let mean = Duration::from_nanos(mean_nanos as u64);

    Percentiles {
        p50: sorted_timings[p50_idx.min(len - 1)],
        p90: sorted_timings[p90_idx.min(len - 1)],
        p99: sorted_timings[p99_idx.min(len - 1)],
        mean,
    }
}
```

### 4. Update Statistics Calculation

**File**: `simplebench-runtime/src/lib.rs`

Update `calculate_statistics` to use outlier filtering:

```rust
pub fn calculate_statistics(samples: &[u128]) -> Statistics {
    calculate_statistics_with_config(samples, OutlierConfig::default())
}

pub fn calculate_statistics_with_config(
    samples: &[u128],
    outlier_config: OutlierConfig,
) -> Statistics {
    use crate::outliers::filter_outliers_u128;

    if samples.is_empty() {
        return Statistics {
            mean: 0,
            median: 0,
            p90: 0,
            p99: 0,
            std_dev: 0.0,
            variance: 0.0,
            min: 0,
            max: 0,
            sample_count: 0,
        };
    }

    // Apply outlier filtering
    let (filtered_samples, _analysis) = filter_outliers_u128(samples, outlier_config);

    // Use filtered data for calculation
    let data = if filtered_samples.is_empty() {
        samples
    } else {
        &filtered_samples
    };

    let sample_count = data.len();

    // Sort for percentile calculations
    let mut sorted = data.to_vec();
    sorted.sort();

    // Calculate percentiles
    let p50_idx = (sample_count * 50) / 100;
    let p90_idx = (sample_count * 90) / 100;
    let p99_idx = (sample_count * 99) / 100;

    let median = sorted[p50_idx.min(sample_count - 1)];
    let p90 = sorted[p90_idx.min(sample_count - 1)];
    let p99 = sorted[p99_idx.min(sample_count - 1)];

    // Calculate mean from filtered data
    let sum: u128 = data.iter().sum();
    let mean = sum / (sample_count as u128);

    // Calculate variance and standard deviation
    let mean_f64 = mean as f64;
    let variance: f64 = data
        .iter()
        .map(|&s| {
            let diff = s as f64 - mean_f64;
            diff * diff
        })
        .sum::<f64>()
        / (sample_count as f64);

    let std_dev = variance.sqrt();

    // Min and max from filtered data
    let min = *sorted.first().unwrap();
    let max = *sorted.last().unwrap();

    Statistics {
        mean,
        median,
        p90,
        p99,
        std_dev,
        variance,
        min,
        max,
        sample_count,
    }
}
```

### 5. Update BenchResult to Include Outlier Info

**File**: `simplebench-runtime/src/lib.rs`

Add outlier analysis to BenchResult:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub name: String,
    pub module: String,
    pub iterations: usize,
    pub samples: usize,
    pub percentiles: Percentiles,
    pub all_timings: Vec<Duration>,

    /// Optional outlier analysis (if filtering was enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outlier_analysis: Option<OutlierAnalysis>,
}
```

Update measurement to include outlier analysis:

```rust
pub fn measure_function_impl<F>(
    name: String,
    module: String,
    func: F,
    iterations: usize,
    samples: usize,
    outlier_config: OutlierConfig,
) -> BenchResult
where
    F: Fn(),
{
    let mut all_timings = Vec::with_capacity(samples);

    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iterations {
            func();
        }
        let elapsed = start.elapsed();
        all_timings.push(elapsed);
    }

    // Calculate percentiles with outlier filtering
    let (filtered_timings, outlier_analysis) = filter_outliers_duration(&all_timings, outlier_config);
    let percentiles = calculate_percentiles_with_config(&all_timings, outlier_config);

    BenchResult {
        name,
        module,
        iterations,
        samples,
        percentiles,
        all_timings,
        outlier_analysis: if outlier_config.enabled {
            Some(outlier_analysis)
        } else {
            None
        },
    }
}
```

### 6. Update Analyze Command

**File**: `cargo-simplebench/src/analyze.rs`

Consolidate IQR code and add filtered/unfiltered comparison:

```rust
use simplebench_runtime::outliers::{filter_outliers_u128, OutlierConfig};

/// Print outlier analysis with filtered vs unfiltered comparison
fn print_outlier_analysis_enhanced(samples: &[u128], stats: &Statistics) {
    println!("{}", "Outlier Analysis".cyan().bold());
    println!("{}", "─".repeat(50).dimmed());

    // Use shared IQR filtering function
    let config = OutlierConfig::default();
    let (filtered_samples, analysis) = filter_outliers_u128(samples, config);

    println!("  {}", "IQR Method (1.5× threshold):".yellow());
    println!("    {}  {}", "Q1 (25th):".dimmed(), format_ns(analysis.q1));
    println!("    {}  {}", "Q3 (75th):".dimmed(), format_ns(analysis.q3));
    println!("    {}  {}", "IQR:".dimmed(), format_ns(analysis.iqr));
    println!("    {}  {}", "Lower fence:".dimmed(), format_ns(analysis.lower_fence));
    println!("    {}  {}", "Upper fence:".dimmed(), format_ns(analysis.upper_fence));
    println!("    {}  {} ({:.1}%)",
        "Outliers:".dimmed(),
        analysis.outlier_count,
        analysis.outlier_percentage
    );
    println!();

    // Show impact of filtering
    if analysis.outlier_count > 0 {
        println!("  {}", "Impact of Outlier Filtering:".green().bold());

        // Calculate unfiltered stats
        let unfiltered_stats = simplebench_runtime::calculate_statistics_with_config(
            samples,
            OutlierConfig { enabled: false, ..Default::default() }
        );

        // Calculate filtered stats
        let filtered_stats = simplebench_runtime::calculate_statistics_with_config(
            samples,
            config
        );

        // Show comparison
        let mean_diff_pct = ((filtered_stats.mean as f64 - unfiltered_stats.mean as f64)
            / unfiltered_stats.mean as f64) * 100.0;
        let p90_diff_pct = ((filtered_stats.p90 as f64 - unfiltered_stats.p90 as f64)
            / unfiltered_stats.p90 as f64) * 100.0;

        println!("    {}  {} → {} ({:+.1}%)",
            "Mean:".dimmed(),
            format_ns(unfiltered_stats.mean),
            format_ns(filtered_stats.mean).green(),
            mean_diff_pct
        );
        println!("    {}  {} → {} ({:+.1}%)",
            "p90:".dimmed(),
            format_ns(unfiltered_stats.p90),
            format_ns(filtered_stats.p90).green(),
            p90_diff_pct
        );
        println!("    {}  {:.1}% → {:.1}%",
            "Variance:".dimmed(),
            (unfiltered_stats.std_dev / unfiltered_stats.mean as f64) * 100.0,
            (filtered_stats.std_dev / filtered_stats.mean as f64) * 100.0,
        );
    }

    // Print flagged samples
    if !analysis.outlier_indices.is_empty() {
        println!();
        println!("  {}", "Flagged samples (IQR):".red());
        let median = stats.median;
        for idx in analysis.outlier_indices.iter().take(5) {
            let sample = samples[*idx];
            let diff_pct = if median > 0 {
                ((sample as f64 - median as f64) / median as f64) * 100.0
            } else {
                0.0
            };
            println!("    #{}: {} ({:+.1}%)",
                idx,
                format_ns(sample),
                diff_pct
            );
        }
        if analysis.outlier_indices.len() > 5 {
            println!("    {} more outliers...", analysis.outlier_indices.len() - 5);
        }
    }

    println!("{}", "─".repeat(50).dimmed());
}
```

### 7. Update Output Formatting

**File**: `simplebench-runtime/src/output.rs`

Add outlier info to benchmark output:

```rust
pub fn print_benchmark_result_line(result: &BenchResult) {
    let outlier_info = if let Some(ref analysis) = result.outlier_analysis {
        if analysis.outlier_count > 0 {
            format!(" [{} outliers filtered]", analysis.outlier_count).dimmed().to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    println!("{}", format_benchmark_result(result) + &outlier_info);
}
```

---

## Implementation Tasks

### Phase 1: Core Infrastructure (Day 1)

**Task 1.1: Create Outlier Module**
- [ ] Create `simplebench-runtime/src/outliers.rs`
- [ ] Implement `OutlierConfig` struct
- [ ] Implement `OutlierAnalysis` struct
- [ ] Implement `filter_outliers_u128()` function
- [ ] Implement `filter_outliers_duration()` function
- [ ] Write comprehensive unit tests
- [ ] Add module to `lib.rs`: `pub mod outliers;`

**Task 1.2: Update Configuration**
- [ ] Add `outlier_filter: OutlierConfig` to `MeasurementConfig`
- [ ] Add environment variable support in `apply_env_overrides()`
- [ ] Update unit tests for configuration
- [ ] Add CLI arguments to `cargo-simplebench/src/main.rs`
- [ ] Pass CLI args through env vars to runner

### Phase 2: Apply Filtering (Day 1-2)

**Task 2.1: Update Percentile Calculation**
- [ ] Create `calculate_percentiles_with_config()` in `lib.rs`
- [ ] Update existing `calculate_percentiles()` to use filtering
- [ ] Update all call sites to pass outlier config
- [ ] Add unit tests for filtered vs unfiltered

**Task 2.2: Update Statistics Calculation**
- [ ] Create `calculate_statistics_with_config()` in `lib.rs`
- [ ] Update existing `calculate_statistics()` to use filtering
- [ ] Add unit tests

**Task 2.3: Update BenchResult**
- [ ] Add `outlier_analysis: Option<OutlierAnalysis>` field
- [ ] Update `measure_function_impl()` to populate outlier analysis
- [ ] Update all BenchResult construction sites
- [ ] Ensure backward compatibility with serde skip

### Phase 3: Consolidation & Enhancement (Day 2)

**Task 3.1: Consolidate Analyze Code**
- [ ] Remove duplicate IQR calculation from `analyze.rs:163-174`
- [ ] Use `filter_outliers_u128()` from outliers module
- [ ] Update `print_outlier_analysis()` to use shared code

**Task 3.2: Enhance Analyze Command**
- [ ] Add filtered vs unfiltered comparison
- [ ] Show impact metrics (mean change, variance reduction)
- [ ] Update formatting for clarity

**Task 3.3: Update Output Formatting**
- [ ] Add outlier count to benchmark result line
- [ ] Keep it subtle (dimmed) - not main focus
- [ ] Update tests

### Phase 4: Testing & Validation (Day 2-3)

**Task 4.1: Unit Tests**
- [ ] Run all unit tests: `cargo test`
- [ ] Verify no regressions
- [ ] Add integration tests for outlier filtering

**Task 4.2: Integration Testing**
- [ ] Delete test-workspace/.benches/
- [ ] Run benchmarks: `cd test-workspace && cargo simplebench`
- [ ] Verify outlier filtering works
- [ ] Check output formatting

**Task 4.3: Variance Validation**
- [ ] Delete test-workspace/.benches/
- [ ] Run 5 consecutive benchmarks
- [ ] Compare variance with vs without filtering:
  ```bash
  # With filtering (default)
  for i in 1 2 3 4 5; do
      echo "=== Run $i ==="
      cargo simplebench | tee run${i}_filtered.txt
  done

  # Without filtering
  for i in 1 2 3 4 5; do
      echo "=== Run $i ==="
      cargo simplebench --no-outlier-filter | tee run${i}_unfiltered.txt
  done
  ```
- [ ] Verify filtered results have ≤ unfiltered variance
- [ ] Use analyze command to compare

**Task 4.4: Analyze Command Testing**
- [ ] Test analyze on benchmark with outliers
- [ ] Verify filtered vs unfiltered comparison displays correctly
- [ ] Check that IQR consolidation worked

---

## Configuration Examples

### CLI Usage

```bash
# Default (filtering enabled)
cargo simplebench

# Disable filtering
cargo simplebench --no-outlier-filter

# Custom IQR multiplier (more conservative)
cargo simplebench --iqr-multiplier 3.0

# Via environment variable
SIMPLEBENCH_FILTER_OUTLIERS=0 cargo simplebench
SIMPLEBENCH_IQR_MULTIPLIER=2.0 cargo simplebench
```

### Config File (simplebench.toml)

```toml
[measurement]
samples = 100_000
iterations = 5
warmup_iterations = 50

[measurement.outlier_filter]
enabled = true
iqr_multiplier = 1.5

[comparison]
threshold = 5.0
```

---

## Expected Outcomes

### Variance Improvement

**Current (no filtering)**: 0-3% variance across all benchmarks

**Expected (with filtering)**:
- Similar or slightly better variance (0-2%)
- More consistent results across runs
- Reduced impact of system noise
- Lower false positive rate in CI

**Trade-offs**:
- Minimal performance impact (filtering is fast)
- Slightly fewer samples used for calculation
- More complex code (mitigated by good modularization)

### Output Examples

**Before (no filtering)**:
```
BENCH game_math::bench_vec3_add [100000 samples × 5 iters]
      mean: 5.12μs, p50: 5.08μs, p90: 5.23μs, p99: 5.45μs
      STABLE ↗ 0.8% (mean: 5.08μs -> 5.12μs)
```

**After (with filtering)**:
```
BENCH game_math::bench_vec3_add [100000 samples × 5 iters] [237 outliers filtered]
      mean: 5.06μs, p50: 5.05μs, p90: 5.18μs, p99: 5.35μs
      STABLE ↗ 0.2% (mean: 5.05μs -> 5.06μs)
```

**Analyze Command Output**:
```
Outlier Analysis
──────────────────────────────────────────────────
  IQR Method (1.5× threshold):
    Q1 (25th):  4.95μs
    Q3 (75th):  5.15μs
    IQR:  200ns
    Lower fence:  4.65μs
    Upper fence:  5.45μs
    Outliers:  237 (0.2%)

  Impact of Outlier Filtering:
    Mean:  5.12μs → 5.06μs (-1.2%)
    p90:  5.23μs → 5.18μs (-1.0%)
    Variance:  2.3% → 1.8%

  Flagged samples (IQR):
    #4523: 8.34μs (+64.6%)
    #8912: 7.82μs (+54.4%)
    #12445: 6.73μs (+32.8%)
    ...
    234 more outliers...
──────────────────────────────────────────────────
```

---

## Documentation Updates

### Files to Update

**CLAUDE.md**:
- Add outlier filtering to "Measurement Strategy" section
- Document IQR method and configuration options
- Update examples

**README.md** (if exists):
- Add outlier filtering feature
- Show configuration examples
- Explain trade-offs

**variance-reduction/critical_notes.md**:
- Add note about outlier filtering
- Update testing procedures
- Document when to enable/disable

---

## Testing Checklist

- [ ] Unit tests pass for outliers module
- [ ] Unit tests pass for updated calculate_percentiles
- [ ] Unit tests pass for updated calculate_statistics
- [ ] Integration tests pass
- [ ] test-workspace benchmarks run successfully
- [ ] Outlier filtering can be disabled
- [ ] Custom IQR multiplier works
- [ ] Analyze command shows filtered vs unfiltered
- [ ] Variance is maintained or improved
- [ ] Baseline storage compatibility preserved
- [ ] Output formatting looks good
- [ ] Documentation updated

---

## Risk Assessment

### Low Risk
- ✅ Core IQR algorithm is well-tested in literature
- ✅ Consolidation removes duplicate code
- ✅ Configuration allows disabling if issues arise
- ✅ Unit tests will catch logic errors

### Medium Risk
- ⚠️ May filter too many "valid" samples with default 1.5× multiplier
  - **Mitigation**: Make multiplier configurable, test with 1.5×, 2.0×, 3.0×
- ⚠️ Baseline compatibility if serialization changes
  - **Mitigation**: Use `#[serde(skip_serializing_if = "Option::is_none")]` for new fields

### Mitigations
- Make filtering optional (can be disabled)
- Extensive testing before committing
- Use analyze command to validate improvements
- Keep multiplier configurable for tuning

---

## Success Criteria

**Must Have**:
- [ ] Outlier filtering implementation is correct and tested
- [ ] Can be enabled/disabled via CLI and config
- [ ] Consolidates duplicate IQR code from analyze.rs
- [ ] All existing tests pass
- [ ] Variance is maintained or improved

**Nice to Have**:
- [ ] Variance improved by 10-30% over non-filtered
- [ ] Analyze command provides clear insights
- [ ] Documentation is comprehensive
- [ ] Output formatting is polished

---

## Timeline

**Day 1**: Tasks 1.1, 1.2, 2.1, 2.2 (Core infrastructure + application)
**Day 2**: Tasks 2.3, 3.1, 3.2, 3.3 (Integration + consolidation)
**Day 3**: Tasks 4.1, 4.2, 4.3, 4.4 (Testing + validation)

**Total Estimate**: 2-3 days

---

## Follow-up Work

After implementation, consider:

1. **Adaptive IQR multiplier**: Automatically adjust based on outlier rate
2. **Multiple filtering methods**: Add MAD-based filtering as alternative
3. **Outlier visualization**: Generate plots showing outlier distribution
4. **Historical outlier tracking**: Track outlier rate over time
5. **CI integration**: Fail if outlier rate exceeds threshold

---

**Document Status**: Ready for Review
**Next Step**: Review with user, gather feedback, begin implementation
