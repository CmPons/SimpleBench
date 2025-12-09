use std::fs;
use std::path::Path;
use serde_json;
use colored::*;
use crate::{BenchResult, Comparison};
use crate::baseline::ComparisonResult;

pub fn save_result_to_file<P: AsRef<Path>>(result: &BenchResult, path: P) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(result)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn load_result_from_file<P: AsRef<Path>>(path: P) -> Result<BenchResult, Box<dyn std::error::Error>> {
    let json = fs::read_to_string(path)?;
    let result = serde_json::from_str(&json)?;
    Ok(result)
}

pub fn save_results_to_file<P: AsRef<Path>>(results: &[BenchResult], path: P) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(results)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn load_results_from_file<P: AsRef<Path>>(path: P) -> Result<Vec<BenchResult>, Box<dyn std::error::Error>> {
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
    let p50_str = format_duration_human_readable(result.percentiles.p50);
    let p90_str = format_duration_human_readable(result.percentiles.p90);
    let p99_str = format_duration_human_readable(result.percentiles.p99);

    format!(
        "{} {} {} p50: {}, p90: {}, p99: {}",
        "BENCH".green().bold(),
        bench_name.cyan(),
        format!("[{} samples × {} iters]", result.samples, result.iterations).dimmed(),
        p50_str.cyan(),
        p90_str.cyan(),
        p99_str.cyan()
    )
}

pub fn format_comparison_result(comparison: &Comparison, benchmark_name: &str, is_regression: bool) -> String {
    let change_symbol = if comparison.percentage_change > 0.0 { "↗" } else { "↘" };
    let percentage_str = format!("{:.1}%", comparison.percentage_change.abs());
    let baseline_str = format_duration_human_readable(comparison.baseline_p90);
    let current_str = format_duration_human_readable(comparison.current_p90);

    if is_regression {
        format!(
            "        {} {} {} {} ({} -> {})",
            "REGRESS".red().bold(),
            benchmark_name.bright_white(),
            change_symbol,
            percentage_str.red().bold(),
            baseline_str.dimmed(),
            current_str.red()
        )
    } else if comparison.percentage_change < -5.0 {
        // Show improvements of >5% in green
        format!(
            "        {} {} {} {} ({} -> {})",
            "IMPROVE".green().bold(),
            benchmark_name.bright_white(),
            change_symbol,
            percentage_str.green(),
            baseline_str.dimmed(),
            current_str.green()
        )
    } else {
        // Minor changes in yellow
        format!(
            "        {} {} {} {} ({} -> {})",
            "STABLE".yellow(),
            benchmark_name.bright_white(),
            change_symbol,
            percentage_str.dimmed(),
            baseline_str.dimmed(),
            current_str.dimmed()
        )
    }
}

pub fn print_benchmark_start(bench_name: &str, module: &str) {
    println!("   {} {}::{}",
        "Running".cyan().bold(),
        module.dimmed(),
        bench_name
    );
}

pub fn print_summary(results: &[BenchResult], comparisons: Option<&[ComparisonResult]>) {
    // Print header
    println!("{} {} {}",
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
                    println!("{}", format_comparison_result(comparison, &result.name, comparisons[i].is_regression));
                } else {
                    // First run - no baseline to compare against
                    println!("        {} {} (establishing baseline)",
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
        let improvements = comparisons.iter()
            .filter(|c| {
                c.comparison.as_ref()
                    .map(|comp| comp.percentage_change < -5.0)
                    .unwrap_or(false)
            })
            .count();
        let new_benchmarks = comparisons.iter().filter(|c| c.comparison.is_none()).count();
        let stable = comparisons.len() - regressions - improvements - new_benchmarks;

        println!("{} {} total: {} {}, {} {}, {} {}{}",
            "Summary:".cyan().bold(),
            results.len(),
            stable,
            "stable".dimmed(),
            improvements,
            "improved".green(),
            regressions,
            if regressions > 0 { "regressed".red().bold() } else { "regressed".dimmed() },
            if new_benchmarks > 0 {
                format!(", {} {}", new_benchmarks, "new".blue())
            } else {
                String::new()
            }
        );

        if regressions > 0 {
            println!("{} {} regression(s) detected", "warning:".yellow().bold(), regressions);
        }
    } else {
        println!("{} running {} benchmarks",
            "Finished".green().bold(),
            results.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use crate::Percentiles;
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
            },
            all_timings: vec![Duration::from_millis(5); 10],
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
        assert_eq!(format_duration_human_readable(Duration::from_nanos(500)), "500ns");
        assert_eq!(format_duration_human_readable(Duration::from_micros(500)), "500.00μs");
        assert_eq!(format_duration_human_readable(Duration::from_millis(500)), "500.00ms");
        assert_eq!(format_duration_human_readable(Duration::from_secs(5)), "5.00s");
    }
    
    #[test]
    fn test_format_benchmark_result() {
        let result = create_test_result();
        let formatted = format_benchmark_result(&result);

        assert!(formatted.contains("test_module::test_bench"));
        assert!(formatted.contains("p50:"));
        assert!(formatted.contains("p90:"));
        assert!(formatted.contains("p99:"));
        assert!(formatted.contains("10 samples × 100 iters"));
    }
}