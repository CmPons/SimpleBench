use simplebench_macros::mbench;
use simplebench_runtime::{run_all_benchmarks, BenchResult};

#[mbench]
fn bench_addition() {
    let _ = 1 + 1;
}

#[mbench]
fn bench_multiplication() {
    let _ = 2 * 3;
}

#[mbench]
fn bench_vector_allocation() {
    let _v: Vec<i32> = Vec::with_capacity(100);
}

#[mbench]
fn bench_string_concatenation() {
    let mut s = String::new();
    for i in 0..10 {
        s.push_str(&i.to_string());
    }
}

#[test]
fn test_benchmarks_are_registered() {
    let results = run_all_benchmarks(100, 10);

    // Should have at least our 4 test benchmarks
    assert!(results.len() >= 4, "Expected at least 4 benchmarks, got {}", results.len());

    // Check that our benchmarks are present
    let bench_names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(bench_names.contains(&"bench_addition"), "bench_addition not found");
    assert!(bench_names.contains(&"bench_multiplication"), "bench_multiplication not found");
    assert!(bench_names.contains(&"bench_vector_allocation"), "bench_vector_allocation not found");
    assert!(bench_names.contains(&"bench_string_concatenation"), "bench_string_concatenation not found");
}

#[test]
fn test_benchmark_results_structure() {
    let results = run_all_benchmarks(100, 10);

    for result in &results {
        // Verify result structure
        assert_eq!(result.iterations, 100);
        assert_eq!(result.samples, 10);
        assert_eq!(result.all_timings.len(), 10);

        // Verify percentiles are in order
        assert!(result.percentiles.p50 <= result.percentiles.p90);
        assert!(result.percentiles.p90 <= result.percentiles.p99);

        // Verify module path is captured
        assert!(!result.module.is_empty());

        // Verify name is not empty
        assert!(!result.name.is_empty());
    }
}

#[test]
fn test_benchmark_timings_are_reasonable() {
    let results = run_all_benchmarks(10, 5);

    for result in &results {
        // All timings should be non-zero (we did some work)
        for timing in &result.all_timings {
            assert!(timing.as_nanos() > 0, "Timing should be > 0 for {}", result.name);
        }

        // Timings should be reasonable (not absurdly large)
        // For simple operations, even with 10 iterations, should be under 1 second
        assert!(result.percentiles.p99.as_secs() < 1,
                "Timing too large for {} (p99: {:?})", result.name, result.percentiles.p99);
    }
}

#[test]
fn test_comparison_functionality() {
    use simplebench_runtime::{compare_with_baseline, Percentiles};
    use std::time::Duration;

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
            p90: Duration::from_millis(13),  // 30% slower
            p99: Duration::from_millis(15),
        },
        all_timings: vec![],
    };

    let comparison = compare_with_baseline(&current, &baseline);

    assert_eq!(comparison.percentage_change, 30.0);
    assert!(comparison.is_regression);
}

#[test]
fn test_json_serialization() {
    use simplebench_runtime::{save_result_to_file, load_result_from_file};
    use tempfile::NamedTempFile;

    let results = run_all_benchmarks(50, 5);

    if let Some(result) = results.first() {
        let temp_file = NamedTempFile::new().unwrap();

        // Save to JSON
        save_result_to_file(result, temp_file.path()).unwrap();

        // Load from JSON
        let loaded = load_result_from_file(temp_file.path()).unwrap();

        assert_eq!(result.name, loaded.name);
        assert_eq!(result.module, loaded.module);
        assert_eq!(result.iterations, loaded.iterations);
        assert_eq!(result.samples, loaded.samples);
    }
}