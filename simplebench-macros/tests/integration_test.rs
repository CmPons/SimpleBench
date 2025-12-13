use simplebench_macros::bench;
use simplebench_runtime::{BenchResult, SimpleBench};

#[bench]
fn bench_addition() {
    let _ = 1 + 1;
}

#[bench]
fn bench_multiplication() {
    let _ = 2 * 3;
}

#[bench]
fn bench_vector_allocation() {
    let _v: Vec<i32> = Vec::with_capacity(100);
}

#[bench]
fn bench_string_concatenation() {
    let mut s = String::new();
    for i in 0..10 {
        s.push_str(&i.to_string());
    }
}

#[test]
fn test_benchmarks_are_registered() {
    // Collect all registered benchmarks via inventory
    let bench_names: Vec<&str> = simplebench_runtime::inventory::iter::<SimpleBench>()
        .map(|b| b.name)
        .collect();

    // Should have at least our 4 test benchmarks
    assert!(
        bench_names.len() >= 4,
        "Expected at least 4 benchmarks, got {}",
        bench_names.len()
    );

    // Check that our benchmarks are present
    assert!(
        bench_names.contains(&"bench_addition"),
        "bench_addition not found"
    );
    assert!(
        bench_names.contains(&"bench_multiplication"),
        "bench_multiplication not found"
    );
    assert!(
        bench_names.contains(&"bench_vector_allocation"),
        "bench_vector_allocation not found"
    );
    assert!(
        bench_names.contains(&"bench_string_concatenation"),
        "bench_string_concatenation not found"
    );
}

#[test]
fn test_benchmark_module_paths() {
    // Verify that module paths are captured correctly
    for bench in simplebench_runtime::inventory::iter::<SimpleBench>() {
        assert!(!bench.module.is_empty(), "Module should not be empty for {}", bench.name);
        assert!(!bench.name.is_empty(), "Name should not be empty");
    }
}

#[test]
fn test_json_serialization() {
    use simplebench_runtime::{load_result_from_file, save_result_to_file, Percentiles};
    use std::time::Duration;
    use tempfile::NamedTempFile;

    // Create a mock BenchResult for testing serialization
    let result = BenchResult {
        name: "test_bench".to_string(),
        module: "test_module".to_string(),
        iterations: 100,
        samples: 10,
        percentiles: Percentiles {
            p50: Duration::from_millis(5),
            p90: Duration::from_millis(10),
            p99: Duration::from_millis(15),
            mean: Duration::from_millis(8),
        },
        all_timings: vec![Duration::from_millis(5); 10],
        cpu_samples: vec![],
        warmup_ms: Some(100),
        warmup_iterations: Some(1000),
    };

    let temp_file = NamedTempFile::new().unwrap();

    // Save to JSON
    save_result_to_file(&result, temp_file.path()).unwrap();

    // Load from JSON
    let loaded = load_result_from_file(temp_file.path()).unwrap();

    assert_eq!(result.name, loaded.name);
    assert_eq!(result.module, loaded.module);
    assert_eq!(result.iterations, loaded.iterations);
    assert_eq!(result.samples, loaded.samples);
}
