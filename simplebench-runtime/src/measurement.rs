use std::time::{Duration, Instant};
use crate::{BenchResult, calculate_percentiles};

pub fn measure_with_warmup<F>(
    name: String,
    module: String,
    func: F,
    iterations: usize,
    samples: usize,
    warmup_iterations: usize,
) -> BenchResult
where
    F: Fn(),
{
    // Warmup phase
    for _ in 0..warmup_iterations {
        func();
    }

    measure_function_impl(name, module, func, iterations, samples)
}

pub fn measure_function_impl<F>(name: String, module: String, func: F, iterations: usize, samples: usize) -> BenchResult
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
    if samples > 10000 {
        return Err("Samples should not exceed 10000 for reasonable execution time".to_string());
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
        assert!(validate_measurement_params(100, 10001).is_err());
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