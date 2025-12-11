use crate::baseline::ComparisonResult;
use crate::{BenchResult, Comparison};
use colored::*;
use serde_json;
use std::fs;
use std::path::Path;

pub fn save_result_to_file<P: AsRef<Path>>(
    result: &BenchResult,
    path: P,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(result)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn load_result_from_file<P: AsRef<Path>>(
    path: P,
) -> Result<BenchResult, Box<dyn std::error::Error>> {
    let json = fs::read_to_string(path)?;
    let result = serde_json::from_str(&json)?;
    Ok(result)
}

pub fn save_results_to_file<P: AsRef<Path>>(
    results: &[BenchResult],
    path: P,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(results)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn load_results_from_file<P: AsRef<Path>>(
    path: P,
) -> Result<Vec<BenchResult>, Box<dyn std::error::Error>> {
    let json = fs::read_to_string(path)?;
    let results = serde_json::from_str(&json)?;
    Ok(results)
}

pub fn format_duration_human_readable(duration: std::time::Duration) -> String {
    let nanos = duration.as_nanos();

    if nanos < 1_000 {
        format!("{}ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.2}μs", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2}ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2}s", nanos as f64 / 1_000_000_000.0)
    }
}

pub fn format_benchmark_result(result: &BenchResult) -> String {
    let bench_name = format!("{}::{}", result.module, result.name);
    let mean_str = format_duration_human_readable(result.percentiles.mean);
    let p50_str = format_duration_human_readable(result.percentiles.p50);
    let p90_str = format_duration_human_readable(result.percentiles.p90);
    let p99_str = format_duration_human_readable(result.percentiles.p99);

    // Calculate coefficient of variation (CV) from raw timings if available
    let cv_str = if !result.all_timings.is_empty() {
        let samples_ns: Vec<u128> = result.all_timings.iter().map(|d| d.as_nanos()).collect();
        let stats = crate::calculate_statistics(&samples_ns);
        let cv_pct = if stats.mean > 0 {
            (stats.std_dev / stats.mean as f64) * 100.0
        } else {
            0.0
        };
        format!(", CV: {:.1}%", cv_pct)
    } else {
        String::new()
    };

    format!(
        "{} {} mean: {}{}, p50: {}, p90: {}, p99: {}",
        "BENCH".green().bold(),
        bench_name.cyan(),
        mean_str.cyan().bold(),
        if !cv_str.is_empty() {
            format!(" ({})", cv_str.trim_start_matches(", "))
                .dimmed()
                .to_string()
        } else {
            String::new()
        },
        p50_str.dimmed(),
        p90_str.dimmed(),
        p99_str.dimmed()
    )
}

/// Format CPU statistics from samples
pub fn format_cpu_stats(cpu_samples: &[crate::CpuSnapshot]) -> Option<String> {
    if cpu_samples.is_empty() {
        return None;
    }

    let frequencies: Vec<f64> = cpu_samples
        .iter()
        .filter_map(|s| s.frequency_mhz())
        .collect();

    let temperatures: Vec<f64> = cpu_samples
        .iter()
        .filter_map(|s| s.temperature_celsius())
        .collect();

    let mut parts = Vec::new();

    // Frequency stats
    if !frequencies.is_empty() {
        let min_freq = frequencies.iter().copied().fold(f64::INFINITY, f64::min);
        let max_freq = frequencies
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let mean_freq = frequencies.iter().sum::<f64>() / frequencies.len() as f64;
        parts.push(format!(
            "CPU: {:.0}-{:.0} MHz (mean: {:.0} MHz)",
            min_freq, max_freq, mean_freq
        ));
    }

    // Temperature stats
    if !temperatures.is_empty() {
        let min_temp = temperatures.iter().copied().fold(f64::INFINITY, f64::min);
        let max_temp = temperatures
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let temp_increase = max_temp - min_temp;
        parts.push(format!(
            "Temp: {:.0}-{:.0}°C (+{:.0}°C)",
            min_temp, max_temp, temp_increase
        ));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

pub fn format_comparison_result(
    comparison: &Comparison,
    _benchmark_name: &str,
    is_regression: bool,
) -> String {
    let change_symbol = if comparison.percentage_change > 0.0 {
        "↗"
    } else {
        "↘"
    };
    let percentage_str = format!("{:.1}%", comparison.percentage_change.abs());
    let baseline_str = format_duration_human_readable(comparison.baseline_mean);
    let current_str = format_duration_human_readable(comparison.current_mean);

    if is_regression {
        format!(
            "        {} {} {} (mean: {} -> {})",
            "REGRESS".red().bold(),
            change_symbol,
            percentage_str.red().bold(),
            baseline_str.dimmed(),
            current_str.red()
        )
    } else if comparison.percentage_change < -5.0 {
        // Show improvements of >5% in green
        format!(
            "        {} {} {} (mean: {} -> {})",
            "IMPROVE".green().bold(),
            change_symbol,
            percentage_str.green(),
            baseline_str.dimmed(),
            current_str.green()
        )
    } else {
        // Minor changes in yellow
        format!(
            "        {} {} {} (mean: {} -> {})",
            "STABLE".cyan(),
            change_symbol,
            percentage_str.dimmed(),
            baseline_str.dimmed(),
            current_str.dimmed()
        )
    }
}

pub fn print_benchmark_start(bench_name: &str, module: &str) {
    println!(
        "   {} {}::{}",
        "Running".cyan().bold(),
        module.dimmed(),
        bench_name
    );
}

/// Print a single benchmark result line (for streaming output)
pub fn print_benchmark_result_line(result: &BenchResult) {
    println!("{}", format_benchmark_result(result));

    // Print warmup stats if available
    if let (Some(warmup_ms), Some(warmup_iters)) = (result.warmup_ms, result.warmup_iterations) {
        println!(
            "        {} {}ms ({} iterations)",
            "Warmup:".dimmed(),
            warmup_ms,
            warmup_iters
        );
    }

    // Print CPU stats if available (Linux only)
    if let Some(cpu_stats) = format_cpu_stats(&result.cpu_samples) {
        println!("        {}", cpu_stats.dimmed());
    }
}

/// Print a single comparison line (for streaming output)
pub fn print_comparison_line(comparison: &Comparison, benchmark_name: &str, is_regression: bool) {
    println!(
        "{}",
        format_comparison_result(comparison, benchmark_name, is_regression)
    );
}

/// Print "NEW" message for first baseline
pub fn print_new_baseline_line(benchmark_name: &str) {
    println!(
        "        {} {} (establishing baseline)",
        "NEW".blue().bold(),
        benchmark_name.bright_white()
    );
}

/// Print summary footer for streaming mode
pub fn print_streaming_summary(
    comparisons: &[ComparisonResult],
    config: &crate::config::ComparisonConfig,
) {
    let regressions = comparisons.iter().filter(|c| c.is_regression).count();
    let improvements = comparisons
        .iter()
        .filter(|c| {
            c.comparison
                .as_ref()
                .map(|comp| comp.percentage_change < -5.0)
                .unwrap_or(false)
        })
        .count();
    let new_benchmarks = comparisons
        .iter()
        .filter(|c| c.comparison.is_none())
        .count();
    let stable = comparisons.len() - regressions - improvements - new_benchmarks;

    println!("{}", "─".repeat(80).dimmed());
    println!(
        "{} {} total: {} {}, {} {}, {} {}{}",
        "Summary:".cyan().bold(),
        comparisons.len(),
        stable,
        "stable".dimmed(),
        improvements,
        "improved".green(),
        regressions,
        if regressions > 0 {
            "regressed".red().bold()
        } else {
            "regressed".dimmed()
        },
        if new_benchmarks > 0 {
            format!(", {} {}", new_benchmarks, "new".blue())
        } else {
            String::new()
        }
    );

    if regressions > 0 {
        println!(
            "{} {} regression(s) detected (threshold: {}%)",
            "Warning:".yellow().bold(),
            regressions,
            config.threshold
        );
    }
}

pub fn print_summary(results: &[BenchResult], comparisons: Option<&[ComparisonResult]>) {
    // Print header
    println!(
        "{} {} {}",
        "Running".green().bold(),
        results.len(),
        "Benchmarks".green().bold()
    );
    println!();

    // Print individual benchmark results
    for (i, result) in results.iter().enumerate() {
        println!("{}", format_benchmark_result(result));

        if let Some(comparisons) = comparisons {
            if i < comparisons.len() {
                if let Some(comparison) = &comparisons[i].comparison {
                    println!(
                        "{}",
                        format_comparison_result(
                            comparison,
                            &result.name,
                            comparisons[i].is_regression
                        )
                    );
                } else {
                    // First run - no baseline to compare against
                    println!(
                        "        {} {} (establishing baseline)",
                        "NEW".blue().bold(),
                        result.name.bright_white()
                    );
                }
            }
        }
    }

    println!();

    // Print summary footer
    if let Some(comparisons) = comparisons {
        let regressions = comparisons.iter().filter(|c| c.is_regression).count();
        let improvements = comparisons
            .iter()
            .filter(|c| {
                c.comparison
                    .as_ref()
                    .map(|comp| comp.percentage_change < -5.0)
                    .unwrap_or(false)
            })
            .count();
        let new_benchmarks = comparisons
            .iter()
            .filter(|c| c.comparison.is_none())
            .count();
        let stable = comparisons.len() - regressions - improvements - new_benchmarks;

        println!(
            "{} {} total: {} {}, {} {}, {} {}{}",
            "Summary:".cyan().bold(),
            results.len(),
            stable,
            "stable".dimmed(),
            improvements,
            "improved".green(),
            regressions,
            if regressions > 0 {
                "regressed".red().bold()
            } else {
                "regressed".dimmed()
            },
            if new_benchmarks > 0 {
                format!(", {} {}", new_benchmarks, "new".blue())
            } else {
                String::new()
            }
        );

        if regressions > 0 {
            println!(
                "{} {} regression(s) detected",
                "warning:".yellow().bold(),
                regressions
            );
        }
    } else {
        println!(
            "{} running {} benchmarks",
            "Finished".green().bold(),
            results.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Percentiles;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    fn create_test_result() -> BenchResult {
        BenchResult {
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
            ..Default::default()
        }
    }

    #[test]
    fn test_save_and_load_result() {
        let result = create_test_result();
        let temp_file = NamedTempFile::new().unwrap();

        save_result_to_file(&result, temp_file.path()).unwrap();
        let loaded_result = load_result_from_file(temp_file.path()).unwrap();

        assert_eq!(result.name, loaded_result.name);
        assert_eq!(result.module, loaded_result.module);
        assert_eq!(result.iterations, loaded_result.iterations);
        assert_eq!(result.samples, loaded_result.samples);
    }

    #[test]
    fn test_save_and_load_results() {
        let results = vec![create_test_result(), create_test_result()];
        let temp_file = NamedTempFile::new().unwrap();

        save_results_to_file(&results, temp_file.path()).unwrap();
        let loaded_results = load_results_from_file(temp_file.path()).unwrap();

        assert_eq!(results.len(), loaded_results.len());
        assert_eq!(results[0].name, loaded_results[0].name);
    }

    #[test]
    fn test_format_duration_human_readable() {
        assert_eq!(
            format_duration_human_readable(Duration::from_nanos(500)),
            "500ns"
        );
        assert_eq!(
            format_duration_human_readable(Duration::from_micros(500)),
            "500.00μs"
        );
        assert_eq!(
            format_duration_human_readable(Duration::from_millis(500)),
            "500.00ms"
        );
        assert_eq!(
            format_duration_human_readable(Duration::from_secs(5)),
            "5.00s"
        );
    }

    #[test]
    fn test_format_benchmark_result() {
        let result = create_test_result();
        let formatted = format_benchmark_result(&result);

        assert!(formatted.contains("test_module::test_bench"));
        assert!(formatted.contains("mean:"));
        assert!(formatted.contains("p50:"));
        assert!(formatted.contains("p90:"));
        assert!(formatted.contains("p99:"));
    }
}
