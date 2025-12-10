use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

pub mod measurement;
pub mod output;
pub mod baseline;
pub mod config;

pub use measurement::*;
pub use output::*;
pub use baseline::*;
pub use config::*;

// Re-export inventory for use by the macro
pub use inventory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Percentiles {
    pub p50: Duration,
    pub p90: Duration,
    pub p99: Duration,
    pub mean: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub name: String,
    pub module: String,
    pub iterations: usize,
    pub samples: usize,
    pub percentiles: Percentiles,
    pub all_timings: Vec<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub current_mean: Duration,
    pub baseline_mean: Duration,
    pub percentage_change: f64,
}

pub struct SimpleBench {
    pub name: &'static str,
    pub module: &'static str,
    pub func: fn(),
}

inventory::collect!(SimpleBench);

pub fn measure_function<F>(name: String, module: String, func: F, iterations: usize, samples: usize) -> BenchResult 
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
    
    let percentiles = calculate_percentiles(&all_timings);
    
    BenchResult {
        name,
        module,
        iterations,
        samples,
        percentiles,
        all_timings,
    }
}

pub fn calculate_percentiles(timings: &[Duration]) -> Percentiles {
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

pub fn compare_with_baseline(current: &BenchResult, baseline: &BenchResult) -> Comparison {
    let current_mean_nanos = current.percentiles.mean.as_nanos() as f64;
    let baseline_mean_nanos = baseline.percentiles.mean.as_nanos() as f64;

    let percentage_change = if baseline_mean_nanos > 0.0 {
        ((current_mean_nanos - baseline_mean_nanos) / baseline_mean_nanos) * 100.0
    } else {
        0.0
    };

    Comparison {
        current_mean: current.percentiles.mean,
        baseline_mean: baseline.percentiles.mean,
        percentage_change,
    }
}

pub fn run_all_benchmarks(iterations: usize, samples: usize) -> Vec<BenchResult> {
    let mut results = Vec::new();

    for bench in inventory::iter::<SimpleBench> {
        let result = measure_function(
            bench.name.to_string(),
            bench.module.to_string(),
            bench.func,
            iterations,
            samples,
        );
        results.push(result);
    }

    results
}

/// Run all benchmarks with configuration (batch mode)
///
/// Collects all results and returns them without printing.
/// Use `run_and_stream_benchmarks` for the streaming version.
pub fn run_all_benchmarks_with_config(config: &crate::config::BenchmarkConfig) -> Vec<BenchResult> {
    let mut results = Vec::new();

    for bench in inventory::iter::<SimpleBench> {
        let result = measure_with_warmup(
            bench.name.to_string(),
            bench.module.to_string(),
            bench.func,
            config.measurement.iterations,
            config.measurement.samples,
            config.measurement.warmup_iterations,
        );
        results.push(result);
    }

    results
}

/// Run all benchmarks with configuration and stream results
///
/// This is the primary entry point for the generated runner.
/// Prints each benchmark result immediately as it completes.
pub fn run_and_stream_benchmarks(config: &crate::config::BenchmarkConfig) -> Vec<BenchResult> {
    use crate::baseline::{BaselineManager, ComparisonResult};
    use crate::output::{print_benchmark_result_line, print_comparison_line, print_new_baseline_line, print_streaming_summary};
    use colored::*;

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

    println!("{} benchmarks with {} samples × {} iterations\n",
        "Running".green().bold(),
        config.measurement.samples,
        config.measurement.iterations
    );

    // Run each benchmark and print immediately
    for bench in inventory::iter::<SimpleBench> {
        // Run benchmark
        let result = measure_with_warmup(
            bench.name.to_string(),
            bench.module.to_string(),
            bench.func,
            config.measurement.iterations,
            config.measurement.samples,
            config.measurement.warmup_iterations,
        );

        // Print benchmark result immediately
        print_benchmark_result_line(&result);

        // Compare with baseline and print comparison
        if let Some(ref bm) = baseline_manager {
            let crate_name = result.module.split("::").next().unwrap_or("unknown");

            if let Ok(Some(baseline_data)) = bm.load_baseline(crate_name, &result.name) {
                let baseline = baseline_data.to_bench_result();
                let comparison = compare_with_baseline(&result, &baseline);
                let is_regression = comparison.percentage_change > config.comparison.threshold;

                print_comparison_line(&comparison, &result.name, is_regression);

                comparisons.push(ComparisonResult {
                    benchmark_name: result.name.clone(),
                    comparison: Some(comparison),
                    is_regression,
                });
            } else {
                // First run - no baseline
                print_new_baseline_line(&result.name);

                comparisons.push(ComparisonResult {
                    benchmark_name: result.name.clone(),
                    comparison: None,
                    is_regression: false,
                });
            }

            // Save new baseline
            if let Err(e) = bm.save_baseline(crate_name, &result) {
                eprintln!("Warning: Failed to save baseline for {}: {}", result.name, e);
            }
        }

        results.push(result);
        println!();  // Blank line between benchmarks
    }

    // Print summary footer
    if !comparisons.is_empty() {
        print_streaming_summary(&comparisons, &config.comparison);
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
    
    #[test]
    fn test_measure_function() {
        let result = measure_function(
            "test_bench".to_string(),
            "test_module".to_string(),
            || {
                // Small amount of work
                let _ = (0..100).sum::<i32>();
            },
            10,
            5,
        );
        
        assert_eq!(result.name, "test_bench");
        assert_eq!(result.module, "test_module");
        assert_eq!(result.iterations, 10);
        assert_eq!(result.samples, 5);
        assert_eq!(result.all_timings.len(), 5);
        
        // Verify all timings are reasonable
        for timing in &result.all_timings {
            assert!(*timing > Duration::from_nanos(0));
            assert!(*timing < Duration::from_secs(1));
        }
    }
    
    #[test]
    fn test_compare_with_baseline_no_regression() {
        let baseline = BenchResult {
            name: "test".to_string(),
            module: "test".to_string(),
            iterations: 100,
            samples: 10,
            percentiles: Percentiles {
                p50: Duration::from_millis(5),
                p90: Duration::from_millis(10),
                p99: Duration::from_millis(15),
                mean: Duration::from_millis(8),
            },
            all_timings: vec![],
        };

        let current = BenchResult {
            name: "test".to_string(),
            module: "test".to_string(),
            iterations: 100,
            samples: 10,
            percentiles: Percentiles {
                p50: Duration::from_millis(5),
                p90: Duration::from_millis(10),
                p99: Duration::from_millis(15),
                mean: Duration::from_millis(8),  // Same as baseline
            },
            all_timings: vec![],
        };

        let comparison = compare_with_baseline(&current, &baseline);

        assert_eq!(comparison.percentage_change, 0.0);
    }

    #[test]
    fn test_compare_with_baseline_regression() {
        let baseline = BenchResult {
            name: "test".to_string(),
            module: "test".to_string(),
            iterations: 100,
            samples: 10,
            percentiles: Percentiles {
                p50: Duration::from_millis(5),
                p90: Duration::from_millis(10),
                p99: Duration::from_millis(15),
                mean: Duration::from_millis(8),
            },
            all_timings: vec![],
        };

        let current = BenchResult {
            name: "test".to_string(),
            module: "test".to_string(),
            iterations: 100,
            samples: 10,
            percentiles: Percentiles {
                p50: Duration::from_millis(5),
                p90: Duration::from_millis(12),
                p99: Duration::from_millis(15),
                mean: Duration::from_micros(9600),  // 20% slower
            },
            all_timings: vec![],
        };

        let comparison = compare_with_baseline(&current, &baseline);

        assert_eq!(comparison.percentage_change, 20.0);
    }

    #[test]
    fn test_compare_with_baseline_improvement() {
        let baseline = BenchResult {
            name: "test".to_string(),
            module: "test".to_string(),
            iterations: 100,
            samples: 10,
            percentiles: Percentiles {
                p50: Duration::from_millis(5),
                p90: Duration::from_millis(10),
                p99: Duration::from_millis(15),
                mean: Duration::from_millis(8),
            },
            all_timings: vec![],
        };

        let current = BenchResult {
            name: "test".to_string(),
            module: "test".to_string(),
            iterations: 100,
            samples: 10,
            percentiles: Percentiles {
                p50: Duration::from_millis(5),
                p90: Duration::from_millis(8),
                p99: Duration::from_millis(15),
                mean: Duration::from_micros(6400),  // 20% faster
            },
            all_timings: vec![],
        };

        let comparison = compare_with_baseline(&current, &baseline);

        assert_eq!(comparison.percentage_change, -20.0);
    }
    
    inventory::submit! {
        SimpleBench {
            name: "test_inventory_benchmark",
            module: "simplebench_runtime::tests",
            func: test_benchmark_function,
        }
    }
    
    fn test_benchmark_function() {
        let _ = (0..1000).sum::<i32>();
    }
    
    #[test]
    fn test_run_all_benchmarks() {
        let results = run_all_benchmarks(100, 5);
        
        // Should find at least our test benchmark
        assert!(!results.is_empty());
        
        // Check if our test benchmark is included
        let found_test_bench = results.iter().any(|r| r.name == "test_inventory_benchmark");
        assert!(found_test_bench);
    }
}