use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod baseline;
pub mod changepoint;
pub mod config;
pub mod cpu_analysis;
pub mod cpu_monitor;
pub mod measurement;
pub mod output;
pub mod statistics;

pub use baseline::*;
pub use changepoint::*;
pub use config::*;
pub use cpu_analysis::*;
pub use cpu_monitor::*;
pub use measurement::*;
pub use output::*;
pub use statistics::*;

// Re-export inventory for use by the macro
pub use inventory;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Percentiles {
    pub p50: Duration,
    pub p90: Duration,
    pub p99: Duration,
    pub mean: Duration,
}

/// Comprehensive statistics for a benchmark run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    pub mean: u128,    // nanoseconds
    pub median: u128,  // nanoseconds (p50)
    pub p90: u128,     // nanoseconds
    pub p99: u128,     // nanoseconds
    pub std_dev: f64,  // standard deviation in nanoseconds
    pub variance: f64, // variance in nanoseconds²
    pub min: u128,     // nanoseconds
    pub max: u128,     // nanoseconds
    pub sample_count: usize,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub name: String,
    pub module: String,
    pub iterations: usize,
    pub samples: usize,
    pub percentiles: Percentiles,
    pub all_timings: Vec<Duration>,
    #[serde(default)]
    pub cpu_samples: Vec<CpuSnapshot>,
    #[serde(default)]
    pub warmup_ms: Option<u128>,
    #[serde(default)]
    pub warmup_iterations: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub current_mean: Duration,
    pub baseline_mean: Duration,
    pub percentage_change: f64,
    #[serde(default)]
    pub baseline_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_interval: Option<(f64, f64)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_probability: Option<f64>,
}

pub struct SimpleBench {
    pub name: &'static str,
    pub module: &'static str,
    pub func: fn(),
}

inventory::collect!(SimpleBench);

/// Benchmark info for JSON listing
#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkInfo {
    pub name: String,
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
        eprintln!("Warning: Failed to set affinity to core {}: {:?}", pin_core, e);
    }

    // Find and run the benchmark
    for bench in inventory::iter::<SimpleBench>() {
        if bench.name == bench_name {
            let result = measure_with_warmup(
                bench.name.to_string(),
                bench.module.to_string(),
                bench.func,
                config.measurement.iterations,
                config.measurement.samples,
                config.measurement.warmup_duration_secs,
            );
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
        "{} {} {} {} {}",
        "Running benchmarks with".green().bold(),
        config.measurement.samples,
        "samples ×".green().bold(),
        config.measurement.iterations,
        "iterations".green().bold()
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
        // Run benchmark
        let result = measure_with_warmup(
            bench.name.to_string(),
            bench.module.to_string(),
            bench.func,
            config.measurement.iterations,
            config.measurement.samples,
            config.measurement.warmup_duration_secs,
        );

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
