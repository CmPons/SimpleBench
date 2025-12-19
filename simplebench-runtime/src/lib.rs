//! SimpleBench Runtime - Core library for the SimpleBench microbenchmarking framework.
//!
//! This crate provides the runtime components for SimpleBench:
//! - Benchmark registration via the [`SimpleBench`] struct and `inventory` crate
//! - Timing and measurement with warmup phases
//! - Statistical analysis of benchmark results
//! - Baseline storage and regression detection
//!
//! # Usage
//!
//! This crate is typically used alongside `simplebench-macros` which provides the
//! `#[bench]` attribute for easy benchmark registration:
//!
//! ```rust,ignore
//! use simplebench_macros::bench;
//!
//! // Simple benchmark - measures single function calls
//! #[bench]
//! fn my_benchmark() {
//!     // code to benchmark
//! }
//!
//! // Setup runs once, benchmark receives reference
//! #[bench(setup = create_data)]
//! fn benchmark_with_setup(data: &Data) {
//!     process(data);
//! }
//!
//! // Setup runs before each sample - for mutations/consumption
//! #[bench(setup_each = || vec![3, 1, 4, 1, 5])]
//! fn bench_sort(mut data: Vec<i32>) {
//!     data.sort();
//! }
//! ```
//!
//! The `cargo simplebench` CLI tool handles compilation and execution of benchmarks.

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod baseline;
pub mod changepoint;
pub mod config;
pub mod cpu_analysis;
pub mod cpu_monitor;
pub mod measurement;
pub mod output;
pub mod progress;
pub mod statistics;

pub use baseline::*;
pub use changepoint::*;
pub use config::*;
pub use cpu_analysis::*;
pub use cpu_monitor::*;
pub use measurement::*;
pub use output::*;
pub use progress::*;
pub use statistics::*;

// Re-export inventory for use by the macro
pub use inventory;

/// Percentile statistics for a benchmark run.
///
/// Contains the 50th, 90th, and 99th percentile timings along with the mean.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Percentiles {
    /// 50th percentile (median) timing
    pub p50: Duration,
    /// 90th percentile timing
    pub p90: Duration,
    /// 99th percentile timing
    pub p99: Duration,
    /// Arithmetic mean of all timings
    pub mean: Duration,
}

/// Comprehensive statistics for a benchmark run.
///
/// All timing values are in nanoseconds for precision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    /// Arithmetic mean in nanoseconds
    pub mean: u128,
    /// Median (50th percentile) in nanoseconds
    pub median: u128,
    /// 90th percentile in nanoseconds
    pub p90: u128,
    /// 99th percentile in nanoseconds
    pub p99: u128,
    /// Standard deviation in nanoseconds
    pub std_dev: f64,
    /// Variance in nanoseconds squared
    pub variance: f64,
    /// Minimum timing in nanoseconds
    pub min: u128,
    /// Maximum timing in nanoseconds
    pub max: u128,
    /// Number of samples collected
    pub sample_count: usize,
}

/// Complete result of a benchmark run.
///
/// Contains all timing data, statistics, and metadata for a single benchmark execution.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    /// Benchmark function name
    pub name: String,
    /// Module path where the benchmark is defined
    pub module: String,
    /// Number of samples collected
    pub samples: usize,
    /// Percentile statistics computed from all timings
    pub percentiles: Percentiles,
    /// Raw timing data for each sample
    pub all_timings: Vec<Duration>,
    /// CPU state samples collected during the run
    #[serde(default)]
    pub cpu_samples: Vec<CpuSnapshot>,
    /// Total warmup duration in milliseconds
    #[serde(default)]
    pub warmup_ms: Option<u128>,
    /// Number of iterations performed during warmup
    #[serde(default)]
    pub warmup_iterations: Option<u64>,
}

/// Comparison between current benchmark run and baseline.
///
/// Contains statistical measures to determine if performance has regressed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    /// Mean timing from the current run
    pub current_mean: Duration,
    /// Mean timing from the baseline
    pub baseline_mean: Duration,
    /// Percentage change from baseline (positive = slower)
    pub percentage_change: f64,
    /// Number of baseline samples used for comparison
    #[serde(default)]
    pub baseline_count: usize,
    /// Z-score for statistical significance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_score: Option<f64>,
    /// 95% confidence interval for the change
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_interval: Option<(f64, f64)>,
    /// Probability that a real change occurred
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_probability: Option<f64>,
}

/// A registered benchmark function.
///
/// This struct is used by the `inventory` crate for compile-time benchmark registration.
/// The `#[bench]` macro from `simplebench-macros` generates these registrations automatically.
///
/// The `run` function encapsulates the entire measurement process: it receives config,
/// performs warmup, runs measurement iterations, and returns a complete `BenchResult`.
/// This design allows benchmarks with setup to run setup once before measurement begins.
pub struct SimpleBench {
    /// Name of the benchmark function
    pub name: &'static str,
    /// Module path where the benchmark is defined
    pub module: &'static str,
    /// The benchmark runner function that performs measurement and returns results
    pub run: fn(&crate::config::BenchmarkConfig) -> BenchResult,
}

inventory::collect!(SimpleBench);

/// Benchmark metadata for JSON listing.
///
/// A simplified representation of a benchmark for discovery/listing purposes.
#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkInfo {
    /// Name of the benchmark function
    pub name: String,
    /// Module path where the benchmark is defined
    pub module: String,
}

/// List all registered benchmarks as JSON to stdout
///
/// Used by the orchestrator to discover benchmark names before execution.
pub fn list_benchmarks_json() {
    let benchmarks: Vec<BenchmarkInfo> = inventory::iter::<SimpleBench>()
        .map(|b| BenchmarkInfo {
            name: b.name.to_string(),
            module: b.module.to_string(),
        })
        .collect();
    println!("{}", serde_json::to_string(&benchmarks).unwrap());
}

/// Run a single benchmark and output JSON result to stdout
///
/// The benchmark to run is specified via SIMPLEBENCH_BENCH_FILTER env var (exact match).
/// The core to pin to is specified via SIMPLEBENCH_PIN_CORE env var.
pub fn run_single_benchmark_json(config: &crate::config::BenchmarkConfig) {
    let bench_name = std::env::var("SIMPLEBENCH_BENCH_FILTER")
        .expect("SIMPLEBENCH_BENCH_FILTER must be set for single benchmark execution");

    let pin_core: usize = std::env::var("SIMPLEBENCH_PIN_CORE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1); // Default to core 1, not 0 (reserved)

    // Set CPU affinity
    if let Err(e) = affinity::set_thread_affinity([pin_core]) {
        eprintln!(
            "Warning: Failed to set affinity to core {}: {:?}",
            pin_core, e
        );
    }

    // Find and run the benchmark
    for bench in inventory::iter::<SimpleBench>() {
        if bench.name == bench_name {
            // The benchmark's run function handles warmup, measurement, and returns results
            let result = (bench.run)(config);
            println!("{}", serde_json::to_string(&result).unwrap());
            return;
        }
    }

    eprintln!("ERROR: Benchmark '{}' not found", bench_name);
    std::process::exit(1);
}

pub(crate) fn calculate_percentiles(timings: &[Duration]) -> Percentiles {
    let mut sorted_timings = timings.to_vec();
    sorted_timings.sort();

    let len = sorted_timings.len();
    let p50_idx = (len * 50) / 100;
    let p90_idx = (len * 90) / 100;
    let p99_idx = (len * 99) / 100;

    // Calculate mean
    let sum_nanos: u128 = timings.iter().map(|d| d.as_nanos()).sum();
    let mean_nanos = sum_nanos / (len as u128);
    let mean = Duration::from_nanos(mean_nanos as u64);

    Percentiles {
        p50: sorted_timings[p50_idx.min(len - 1)],
        p90: sorted_timings[p90_idx.min(len - 1)],
        p99: sorted_timings[p99_idx.min(len - 1)],
        mean,
    }
}

/// Calculate comprehensive statistics from raw timing samples
pub fn calculate_statistics(samples: &[u128]) -> Statistics {
    let sample_count = samples.len();

    if sample_count == 0 {
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

    // Sort for percentile calculations
    let mut sorted = samples.to_vec();
    sorted.sort();

    // Calculate percentiles
    let p50_idx = (sample_count * 50) / 100;
    let p90_idx = (sample_count * 90) / 100;
    let p99_idx = (sample_count * 99) / 100;

    let median = sorted[p50_idx.min(sample_count - 1)];
    let p90 = sorted[p90_idx.min(sample_count - 1)];
    let p99 = sorted[p99_idx.min(sample_count - 1)];

    // Calculate mean
    let sum: u128 = samples.iter().sum();
    let mean = sum / (sample_count as u128);

    // Calculate variance and standard deviation
    let mean_f64 = mean as f64;
    let variance: f64 = samples
        .iter()
        .map(|&s| {
            let diff = s as f64 - mean_f64;
            diff * diff
        })
        .sum::<f64>()
        / (sample_count as f64);

    let std_dev = variance.sqrt();

    // Min and max
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

/// Run all benchmarks with configuration and stream results
///
/// This is the primary entry point for the generated runner.
/// Prints each benchmark result immediately as it completes.
pub fn run_and_stream_benchmarks(config: &crate::config::BenchmarkConfig) -> Vec<BenchResult> {
    use crate::baseline::{BaselineManager, ComparisonResult};
    use crate::output::{
        print_benchmark_result_line, print_comparison_line, print_new_baseline_line,
        print_streaming_summary,
    };
    use colored::*;

    match affinity::set_thread_affinity([0]) {
        Ok(_) => println!(
            "{} {}\n",
            "Set affinity to core".green().bold(),
            "0".cyan().bold()
        ),
        Err(e) => println!("Failed to set core affinity {e:?}"),
    };

    // Verify benchmark environment
    crate::cpu_monitor::verify_benchmark_environment(0);

    let mut results = Vec::new();
    let mut comparisons = Vec::new();

    // Initialize baseline manager
    let baseline_manager = match BaselineManager::new() {
        Ok(bm) => Some(bm),
        Err(e) => {
            eprintln!("Warning: Could not initialize baseline manager: {}", e);
            eprintln!("Running without baseline comparison.");
            None
        }
    };

    // Get benchmark filter if specified
    let bench_filter = std::env::var("SIMPLEBENCH_BENCH_FILTER").ok();

    // Count how many benchmarks match the filter
    let total_benchmarks: usize = inventory::iter::<SimpleBench>().count();
    let filtered_count = if let Some(ref filter) = bench_filter {
        inventory::iter::<SimpleBench>()
            .filter(|b| b.name.contains(filter))
            .count()
    } else {
        total_benchmarks
    };

    println!(
        "{} {} {}",
        "Running benchmarks with".green().bold(),
        config.measurement.samples,
        "samples".green().bold()
    );

    if let Some(ref filter) = bench_filter {
        println!(
            "{} {} ({} matched filter: \"{}\")\n",
            "Filtering to".dimmed(),
            filtered_count,
            if filtered_count == 1 {
                "benchmark"
            } else {
                "benchmarks"
            },
            filter
        );
    } else {
        println!();
    }

    // Run each benchmark and print immediately
    for bench in inventory::iter::<SimpleBench> {
        // Apply filter if specified
        if let Some(ref filter) = bench_filter {
            if !bench.name.contains(filter) {
                continue; // Skip this benchmark
            }
        }
        // Run benchmark - the run function handles warmup, measurement, and returns results
        let result = (bench.run)(config);

        // Print benchmark result immediately
        print_benchmark_result_line(&result);

        // Compare with baseline using CPD and print comparison
        if let Some(ref bm) = baseline_manager {
            let crate_name = result.module.split("::").next().unwrap_or("unknown");

            // Load recent baselines for window-based comparison
            let mut is_regression = false;
            if let Ok(historical) =
                bm.load_recent_baselines(crate_name, &result.name, config.comparison.window_size)
            {
                if !historical.is_empty() {
                    // Use CPD-based comparison
                    let comparison_result = crate::baseline::detect_regression_with_cpd(
                        &result,
                        &historical,
                        config.comparison.threshold,
                        config.comparison.confidence_level,
                        config.comparison.cp_threshold,
                        config.comparison.hazard_rate,
                    );

                    is_regression = comparison_result.is_regression;

                    if let Some(ref comparison) = comparison_result.comparison {
                        print_comparison_line(
                            comparison,
                            &result.name,
                            comparison_result.is_regression,
                        );
                    }

                    comparisons.push(comparison_result);
                } else {
                    // First run - no baseline
                    print_new_baseline_line(&result.name);

                    comparisons.push(ComparisonResult {
                        benchmark_name: result.name.clone(),
                        comparison: None,
                        is_regression: false,
                    });
                }
            }

            // Save new baseline with regression flag
            if let Err(e) = bm.save_baseline(crate_name, &result, is_regression) {
                eprintln!(
                    "Warning: Failed to save baseline for {}: {}",
                    result.name, e
                );
            }
        }

        results.push(result);
        println!(); // Blank line between benchmarks
    }

    // Print summary footer
    if !comparisons.is_empty() {
        print_streaming_summary(&comparisons, &config.comparison);

        // Show filter stats if filtering was applied
        if let Some(ref filter) = bench_filter {
            println!(
                "\n{} {} of {} total benchmarks (filter: \"{}\")",
                "Ran".dimmed(),
                filtered_count,
                total_benchmarks,
                filter
            );
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_percentiles() {
        let timings = vec![
            Duration::from_millis(1),
            Duration::from_millis(2),
            Duration::from_millis(3),
            Duration::from_millis(4),
            Duration::from_millis(5),
            Duration::from_millis(6),
            Duration::from_millis(7),
            Duration::from_millis(8),
            Duration::from_millis(9),
            Duration::from_millis(10),
        ];

        let percentiles = calculate_percentiles(&timings);

        // For 10 samples: p50 at index 5 (6ms), p90 at index 9 (10ms), p99 at index 9 (10ms)
        // Mean: (1+2+3+4+5+6+7+8+9+10)/10 = 55/10 = 5.5ms
        assert_eq!(percentiles.p50, Duration::from_millis(6));
        assert_eq!(percentiles.p90, Duration::from_millis(10));
        assert_eq!(percentiles.p99, Duration::from_millis(10));
        assert_eq!(percentiles.mean, Duration::from_micros(5500));
    }

    #[test]
    fn test_calculate_percentiles_single_element() {
        let timings = vec![Duration::from_millis(5)];
        let percentiles = calculate_percentiles(&timings);

        assert_eq!(percentiles.p50, Duration::from_millis(5));
        assert_eq!(percentiles.p90, Duration::from_millis(5));
        assert_eq!(percentiles.p99, Duration::from_millis(5));
        assert_eq!(percentiles.mean, Duration::from_millis(5));
    }
}
