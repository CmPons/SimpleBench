use crate::{calculate_percentiles, BenchResult, CpuMonitor, CpuSnapshot};
use std::time::{Duration, Instant};

/// Warmup benchmark using time-based exponential doubling (Criterion-style)
fn warmup_benchmark<F>(bench_fn: &F, warmup_duration: Duration, iterations: usize)
where
    F: Fn(),
{
    let start = Instant::now();
    let mut total_iterations = 0u64;
    let mut batch_size = 1u64;

    while start.elapsed() < warmup_duration {
        // Run benchmark function batch_size times
        for _ in 0..batch_size {
            for _ in 0..iterations {
                bench_fn();
            }
        }

        total_iterations += batch_size * (iterations as u64);
        batch_size *= 2; // Exponential doubling
    }

    eprintln!(
        "  Warmup: {}ms ({} iterations)",
        start.elapsed().as_millis(),
        total_iterations
    );
}

pub fn measure_with_warmup<F>(
    name: String,
    module: String,
    func: F,
    iterations: usize,
    samples: usize,
    warmup_duration_secs: u64,
) -> BenchResult
where
    F: Fn(),
{
    // Perform time-based warmup
    warmup_benchmark(&func, Duration::from_secs(warmup_duration_secs), iterations);

    measure_function_impl(name, module, func, iterations, samples)
}

pub fn measure_function_impl<F>(
    name: String,
    module: String,
    func: F,
    iterations: usize,
    samples: usize,
) -> BenchResult
where
    F: Fn(),
{
    let mut all_timings = Vec::with_capacity(samples);
    let mut cpu_samples = Vec::with_capacity(samples);

    // Initialize CPU monitor (will be no-op on non-Linux)
    let monitor = CpuMonitor::new(0);

    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iterations {
            func();
        }
        let elapsed = start.elapsed();
        all_timings.push(elapsed);

        // Capture CPU state after each sample
        let snapshot = CpuSnapshot {
            timestamp: Instant::now(),
            frequency_khz: monitor.read_frequency(),
            temperature_millic: monitor.read_temperature(),
        };
        cpu_samples.push(snapshot);
    }

    let percentiles = calculate_percentiles(&all_timings);

    BenchResult {
        name,
        module,
        iterations,
        samples,
        percentiles,
        all_timings,
        cpu_samples,
    }
}

pub fn measure_single_iteration<F>(func: F) -> Duration
where
    F: FnOnce(),
{
    let start = Instant::now();
    func();
    start.elapsed()
}

pub fn validate_measurement_params(iterations: usize, samples: usize) -> Result<(), String> {
    if iterations == 0 {
        return Err("Iterations must be greater than 0".to_string());
    }
    if samples == 0 {
        return Err("Samples must be greater than 0".to_string());
    }
    if samples > 1_000_000 {
        return Err("Samples should not exceed 1,000,000 for reasonable execution time".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_measure_single_iteration() {
        let duration = measure_single_iteration(|| {
            thread::sleep(Duration::from_millis(1));
        });

        assert!(duration >= Duration::from_millis(1));
        assert!(duration < Duration::from_millis(10)); // Should be close to 1ms
    }

    #[test]
    fn test_validate_measurement_params() {
        assert!(validate_measurement_params(100, 100).is_ok());
        assert!(validate_measurement_params(0, 100).is_err());
        assert!(validate_measurement_params(100, 0).is_err());
        assert!(validate_measurement_params(100, 1_000_001).is_err());
        assert!(validate_measurement_params(5, 100_000).is_ok());
    }

    #[test]
    fn test_measure_function_basic() {
        let result = measure_function_impl(
            "test_bench".to_string(),
            "test_module".to_string(),
            || {
                // Simple work
                let _ = (0..100).sum::<i32>();
            },
            100,
            10,
        );

        assert_eq!(result.name, "test_bench");
        assert_eq!(result.module, "test_module");
        assert_eq!(result.iterations, 100);
        assert_eq!(result.samples, 10);
        assert_eq!(result.all_timings.len(), 10);

        // All measurements should be reasonable (not zero, not extremely large)
        for timing in &result.all_timings {
            assert!(*timing > Duration::from_nanos(0));
            assert!(*timing < Duration::from_secs(1));
        }
    }

}

