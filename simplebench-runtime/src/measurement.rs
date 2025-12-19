use crate::progress::{emit_progress, ProgressMessage, ProgressPhase};
use crate::{calculate_percentiles, config::BenchmarkConfig, BenchResult, CpuMonitor, CpuSnapshot};
use std::time::{Duration, Instant};

/// Warmup benchmark using time-based exponential doubling (Criterion-style)
/// Returns (elapsed_ms, total_iterations) for reporting
fn warmup_benchmark<F>(bench_fn: &F, warmup_duration: Duration, iterations: usize) -> (u128, u64)
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

    (start.elapsed().as_millis(), total_iterations)
}

/// Get the CPU core this thread is pinned to (if any)
fn get_pinned_core() -> usize {
    // Check env var set by orchestrator
    std::env::var("SIMPLEBENCH_PIN_CORE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Warmup using a closure (generic version for new measurement functions)
fn warmup_closure<F>(
    func: &mut F,
    duration: Duration,
    iterations: usize,
    bench_name: &str,
) -> (u128, u64)
where
    F: FnMut(),
{
    let start = Instant::now();
    let mut total_iterations = 0u64;
    let mut batch_size = 1u64;
    let mut last_report = Instant::now();
    let target_ms = duration.as_millis() as u64;

    while start.elapsed() < duration {
        for _ in 0..batch_size {
            for _ in 0..iterations {
                func();
            }
        }
        total_iterations += batch_size * (iterations as u64);
        batch_size *= 2;

        // Emit progress every 100ms
        if last_report.elapsed() >= Duration::from_millis(100) {
            emit_progress(&ProgressMessage {
                bench: bench_name,
                phase: ProgressPhase::Warmup {
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    target_ms,
                },
            });
            last_report = Instant::now();
        }
    }

    (start.elapsed().as_millis(), total_iterations)
}

/// Measure a closure, collecting timing samples with CPU monitoring
fn measure_closure<F>(
    func: &mut F,
    iterations: usize,
    samples: usize,
    bench_name: &str,
) -> (Vec<Duration>, Vec<CpuSnapshot>)
where
    F: FnMut(),
{
    let mut all_timings = Vec::with_capacity(samples);
    let mut cpu_samples = Vec::with_capacity(samples);

    // Initialize CPU monitor for the pinned core
    let cpu_core = get_pinned_core();
    let monitor = CpuMonitor::new(cpu_core);

    // Report progress every ~1% of samples (minimum every sample for small counts)
    let report_interval = (samples / 100).max(1);

    for sample_idx in 0..samples {
        // Emit progress BEFORE timing (so we don't affect measurements)
        if sample_idx % report_interval == 0 {
            emit_progress(&ProgressMessage {
                bench: bench_name,
                phase: ProgressPhase::Samples {
                    current: sample_idx as u32,
                    total: samples as u32,
                },
            });
        }

        // Read CPU frequency BEFORE measurement (while CPU is active)
        let freq_before = monitor.read_frequency();

        let start = Instant::now();
        for _ in 0..iterations {
            func();
        }
        let elapsed = start.elapsed();
        all_timings.push(elapsed);

        // Read frequency after as well, use the higher of the two
        let freq_after = monitor.read_frequency();
        let frequency_khz = match (freq_before, freq_after) {
            (Some(before), Some(after)) => Some(before.max(after)),
            (Some(f), None) | (None, Some(f)) => Some(f),
            (None, None) => None,
        };

        let snapshot = CpuSnapshot {
            timestamp: Instant::now(),
            frequency_khz,
            temperature_millic: monitor.read_temperature(),
        };
        cpu_samples.push(snapshot);
    }

    // Emit completion message
    emit_progress(&ProgressMessage {
        bench: bench_name,
        phase: ProgressPhase::Complete,
    });

    (all_timings, cpu_samples)
}

/// Measure a simple benchmark (no setup) using the new architecture.
///
/// This function is called by the generated benchmark wrapper for benchmarks
/// without setup code. The config is passed in, and a complete BenchResult is returned.
pub fn measure_simple<F>(
    config: &BenchmarkConfig,
    name: &str,
    module: &str,
    mut func: F,
) -> BenchResult
where
    F: FnMut(),
{
    // Warmup
    let (warmup_ms, warmup_iters) = warmup_closure(
        &mut func,
        Duration::from_secs(config.measurement.warmup_duration_secs),
        config.measurement.iterations,
        name,
    );

    // Measurement
    let (all_timings, cpu_samples) = measure_closure(
        &mut func,
        config.measurement.iterations,
        config.measurement.samples,
        name,
    );

    let percentiles = calculate_percentiles(&all_timings);

    BenchResult {
        name: name.to_string(),
        module: module.to_string(),
        iterations: config.measurement.iterations,
        samples: config.measurement.samples,
        percentiles,
        all_timings,
        cpu_samples,
        warmup_ms: Some(warmup_ms),
        warmup_iterations: Some(warmup_iters),
    }
}

/// Measure a benchmark with setup code that runs once before measurement.
///
/// This function is called by the generated benchmark wrapper for benchmarks
/// with the `setup` attribute. Setup runs exactly once, then the benchmark
/// function receives a reference to the setup data for each iteration.
pub fn measure_with_setup<T, S, B>(
    config: &BenchmarkConfig,
    name: &str,
    module: &str,
    setup: S,
    mut bench: B,
) -> BenchResult
where
    S: FnOnce() -> T,
    B: FnMut(&T),
{
    // Run setup ONCE before any measurement
    let data = setup();

    // Create closure that borrows the setup data
    let mut func = || bench(&data);

    // Warmup
    let (warmup_ms, warmup_iters) = warmup_closure(
        &mut func,
        Duration::from_secs(config.measurement.warmup_duration_secs),
        config.measurement.iterations,
        name,
    );

    // Measurement
    let (all_timings, cpu_samples) = measure_closure(
        &mut func,
        config.measurement.iterations,
        config.measurement.samples,
        name,
    );

    let percentiles = calculate_percentiles(&all_timings);

    BenchResult {
        name: name.to_string(),
        module: module.to_string(),
        iterations: config.measurement.iterations,
        samples: config.measurement.samples,
        percentiles,
        all_timings,
        cpu_samples,
        warmup_ms: Some(warmup_ms),
        warmup_iterations: Some(warmup_iters),
    }
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
    // Perform time-based warmup and store stats
    let (warmup_ms, warmup_iters) =
        warmup_benchmark(&func, Duration::from_secs(warmup_duration_secs), iterations);

    let mut result = measure_function_impl(name, module, func, iterations, samples);

    // Store warmup stats in result for later printing
    result.warmup_ms = Some(warmup_ms);
    result.warmup_iterations = Some(warmup_iters);

    result
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

    // Initialize CPU monitor for the pinned core
    let cpu_core = get_pinned_core();
    let monitor = CpuMonitor::new(cpu_core);

    for _ in 0..samples {
        // Read CPU frequency BEFORE measurement (while CPU is active)
        let freq_before = monitor.read_frequency();

        let start = Instant::now();
        for _ in 0..iterations {
            func();
        }
        let elapsed = start.elapsed();
        all_timings.push(elapsed);

        // Read frequency after as well, use the higher of the two
        let freq_after = monitor.read_frequency();
        let frequency_khz = match (freq_before, freq_after) {
            (Some(before), Some(after)) => Some(before.max(after)),
            (Some(f), None) | (None, Some(f)) => Some(f),
            (None, None) => None,
        };

        let snapshot = CpuSnapshot {
            timestamp: Instant::now(),
            frequency_khz,
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
        warmup_ms: None,
        warmup_iterations: None,
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
        return Err(
            "Samples should not exceed 1,000,000 for reasonable execution time".to_string(),
        );
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
