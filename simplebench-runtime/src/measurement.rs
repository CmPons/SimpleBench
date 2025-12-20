use crate::progress::{emit_progress, ProgressMessage, ProgressPhase};
use crate::{calculate_percentiles, config::BenchmarkConfig, BenchResult, CpuMonitor, CpuSnapshot};
use std::time::{Duration, Instant};

/// Get the CPU core this thread is pinned to (if any)
fn get_pinned_core() -> usize {
    // Check env var set by orchestrator
    std::env::var("SIMPLEBENCH_PIN_CORE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Warmup using a closure (generic version for new measurement functions)
fn warmup_closure<F>(func: &mut F, duration: Duration, bench_name: &str) -> (u128, u64)
where
    F: FnMut(),
{
    let start = Instant::now();
    let mut total_iterations = 0u64;
    let mut last_report = Instant::now();
    let target_ms = duration.as_millis() as u64;

    while start.elapsed() < duration {
        func();
        total_iterations += 1;

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
        func();
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
        name,
    );

    // Measurement
    let (all_timings, cpu_samples) = measure_closure(&mut func, config.measurement.samples, name);

    let percentiles = calculate_percentiles(&all_timings);

    BenchResult {
        name: name.to_string(),
        module: module.to_string(),
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
        name,
    );

    // Measurement
    let (all_timings, cpu_samples) = measure_closure(&mut func, config.measurement.samples, name);

    let percentiles = calculate_percentiles(&all_timings);

    BenchResult {
        name: name.to_string(),
        module: module.to_string(),
        samples: config.measurement.samples,
        percentiles,
        all_timings,
        cpu_samples,
        warmup_ms: Some(warmup_ms),
        warmup_iterations: Some(warmup_iters),
    }
}

/// Warmup with setup running before each call (for setup_each benchmarks)
fn warmup_with_setup<T, S, B>(
    setup: &mut S,
    bench: &mut B,
    duration: Duration,
    bench_name: &str,
) -> (u128, u64)
where
    S: FnMut() -> T,
    B: FnMut(T),
{
    let start = Instant::now();
    let mut total_iterations = 0u64;
    let mut last_report = Instant::now();
    let target_ms = duration.as_millis() as u64;

    while start.elapsed() < duration {
        let data = setup();
        bench(data);
        total_iterations += 1;

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

/// Warmup with setup running before each call, borrowing version
fn warmup_with_setup_ref<T, S, B>(
    setup: &mut S,
    bench: &mut B,
    duration: Duration,
    bench_name: &str,
) -> (u128, u64)
where
    S: FnMut() -> T,
    B: FnMut(&T),
{
    let start = Instant::now();
    let mut total_iterations = 0u64;
    let mut last_report = Instant::now();
    let target_ms = duration.as_millis() as u64;

    while start.elapsed() < duration {
        let data = setup();
        bench(&data);
        total_iterations += 1;

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

/// Measure a benchmark where setup runs before every sample (owning version).
///
/// The benchmark function takes ownership of the data produced by setup.
/// This allows benchmarking operations that consume or mutate their input.
pub fn measure_with_setup_each<T, S, B>(
    config: &BenchmarkConfig,
    name: &str,
    module: &str,
    mut setup: S,
    mut bench: B,
) -> BenchResult
where
    S: FnMut() -> T,
    B: FnMut(T),
{
    // Warmup: run setup + bench together
    let (warmup_ms, warmup_iters) = warmup_with_setup(
        &mut setup,
        &mut bench,
        Duration::from_secs(config.measurement.warmup_duration_secs),
        name,
    );

    // Measurement
    let samples = config.measurement.samples;
    let mut all_timings = Vec::with_capacity(samples);
    let mut cpu_samples = Vec::with_capacity(samples);

    // Initialize CPU monitor for the pinned core
    let cpu_core = get_pinned_core();
    let monitor = CpuMonitor::new(cpu_core);

    // Report progress every ~1% of samples
    let report_interval = (samples / 100).max(1);

    for sample_idx in 0..samples {
        // Emit progress BEFORE timing
        if sample_idx % report_interval == 0 {
            emit_progress(&ProgressMessage {
                bench: name,
                phase: ProgressPhase::Samples {
                    current: sample_idx as u32,
                    total: samples as u32,
                },
            });
        }

        // Setup runs before each sample
        let data = setup();

        // Read CPU frequency BEFORE measurement
        let freq_before = monitor.read_frequency();

        let start = Instant::now();
        bench(data); // Consumes data
        let elapsed = start.elapsed();
        all_timings.push(elapsed);

        // Read frequency after as well
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
        bench: name,
        phase: ProgressPhase::Complete,
    });

    let percentiles = calculate_percentiles(&all_timings);

    BenchResult {
        name: name.to_string(),
        module: module.to_string(),
        samples,
        percentiles,
        all_timings,
        cpu_samples,
        warmup_ms: Some(warmup_ms),
        warmup_iterations: Some(warmup_iters),
    }
}

/// Measure a benchmark where setup runs before every sample (borrowing version).
///
/// The benchmark function borrows the data produced by setup.
/// Use this when you need fresh data each sample but don't consume it.
pub fn measure_with_setup_each_ref<T, S, B>(
    config: &BenchmarkConfig,
    name: &str,
    module: &str,
    mut setup: S,
    mut bench: B,
) -> BenchResult
where
    S: FnMut() -> T,
    B: FnMut(&T),
{
    // Warmup: run setup + bench together
    let (warmup_ms, warmup_iters) = warmup_with_setup_ref(
        &mut setup,
        &mut bench,
        Duration::from_secs(config.measurement.warmup_duration_secs),
        name,
    );

    // Measurement
    let samples = config.measurement.samples;
    let mut all_timings = Vec::with_capacity(samples);
    let mut cpu_samples = Vec::with_capacity(samples);

    // Initialize CPU monitor for the pinned core
    let cpu_core = get_pinned_core();
    let monitor = CpuMonitor::new(cpu_core);

    // Report progress every ~1% of samples
    let report_interval = (samples / 100).max(1);

    for sample_idx in 0..samples {
        // Emit progress BEFORE timing
        if sample_idx % report_interval == 0 {
            emit_progress(&ProgressMessage {
                bench: name,
                phase: ProgressPhase::Samples {
                    current: sample_idx as u32,
                    total: samples as u32,
                },
            });
        }

        // Setup runs before each sample
        let data = setup();

        // Read CPU frequency BEFORE measurement
        let freq_before = monitor.read_frequency();

        let start = Instant::now();
        bench(&data); // Borrows data
        let elapsed = start.elapsed();
        all_timings.push(elapsed);

        // Read frequency after as well
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

        drop(data); // Explicit drop (happens anyway)
    }

    // Emit completion message
    emit_progress(&ProgressMessage {
        bench: name,
        phase: ProgressPhase::Complete,
    });

    let percentiles = calculate_percentiles(&all_timings);

    BenchResult {
        name: name.to_string(),
        module: module.to_string(),
        samples,
        percentiles,
        all_timings,
        cpu_samples,
        warmup_ms: Some(warmup_ms),
        warmup_iterations: Some(warmup_iters),
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

pub fn validate_measurement_params(samples: usize) -> Result<(), String> {
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
        assert!(validate_measurement_params(100).is_ok());
        assert!(validate_measurement_params(0).is_err());
        assert!(validate_measurement_params(1_000_001).is_err());
        assert!(validate_measurement_params(100_000).is_ok());
    }

    #[test]
    fn test_measure_simple_basic() {
        let config = BenchmarkConfig {
            measurement: crate::config::MeasurementConfig {
                samples: 10,
                warmup_duration_secs: 0, // Skip warmup for test speed
            },
            ..Default::default()
        };

        let result = measure_simple(&config, "test_bench", "test_module", || {
            // Simple work
            let _ = (0..100).sum::<i32>();
        });

        assert_eq!(result.name, "test_bench");
        assert_eq!(result.module, "test_module");
        assert_eq!(result.samples, 10);
        assert_eq!(result.all_timings.len(), 10);

        // All measurements should be reasonable (not zero, not extremely large)
        for timing in &result.all_timings {
            assert!(*timing > Duration::from_nanos(0));
            assert!(*timing < Duration::from_secs(1));
        }
    }
}
