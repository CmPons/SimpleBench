use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

pub mod measurement;
pub mod output;

pub use measurement::*;
pub use output::*;

// Re-export inventory for use by the macro
pub use inventory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Percentiles {
    pub p50: Duration,
    pub p90: Duration,
    pub p99: Duration,
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
    pub current_p90: Duration,
    pub baseline_p90: Duration,
    pub percentage_change: f64,
    pub is_regression: bool,
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
    
    Percentiles {
        p50: sorted_timings[p50_idx.min(len - 1)],
        p90: sorted_timings[p90_idx.min(len - 1)],
        p99: sorted_timings[p99_idx.min(len - 1)],
    }
}

pub fn compare_with_baseline(current: &BenchResult, baseline: &BenchResult) -> Comparison {
    let current_p90_nanos = current.percentiles.p90.as_nanos() as f64;
    let baseline_p90_nanos = baseline.percentiles.p90.as_nanos() as f64;
    
    let percentage_change = if baseline_p90_nanos > 0.0 {
        ((current_p90_nanos - baseline_p90_nanos) / baseline_p90_nanos) * 100.0
    } else {
        0.0
    };
    
    let is_regression = percentage_change > 10.0; // 10% threshold
    
    Comparison {
        current_p90: current.percentiles.p90,
        baseline_p90: baseline.percentiles.p90,
        percentage_change,
        is_regression,
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
        assert_eq!(percentiles.p50, Duration::from_millis(6));
        assert_eq!(percentiles.p90, Duration::from_millis(10));
        assert_eq!(percentiles.p99, Duration::from_millis(10));
    }
    
    #[test]
    fn test_calculate_percentiles_single_element() {
        let timings = vec![Duration::from_millis(5)];
        let percentiles = calculate_percentiles(&timings);
        
        assert_eq!(percentiles.p50, Duration::from_millis(5));
        assert_eq!(percentiles.p90, Duration::from_millis(5));
        assert_eq!(percentiles.p99, Duration::from_millis(5));
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
                p90: Duration::from_millis(10),  // Same as baseline
                p99: Duration::from_millis(15),
            },
            all_timings: vec![],
        };
        
        let comparison = compare_with_baseline(&current, &baseline);
        
        assert_eq!(comparison.percentage_change, 0.0);
        assert!(!comparison.is_regression);
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
                p90: Duration::from_millis(12),  // 20% slower
                p99: Duration::from_millis(15),
            },
            all_timings: vec![],
        };
        
        let comparison = compare_with_baseline(&current, &baseline);
        
        assert_eq!(comparison.percentage_change, 20.0);
        assert!(comparison.is_regression); // 20% > 10% threshold
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
                p90: Duration::from_millis(8),  // 20% faster
                p99: Duration::from_millis(15),
            },
            all_timings: vec![],
        };
        
        let comparison = compare_with_baseline(&current, &baseline);
        
        assert_eq!(comparison.percentage_change, -20.0);
        assert!(!comparison.is_regression); // Improvement, not regression
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