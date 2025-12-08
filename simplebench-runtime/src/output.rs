use std::fs;
use std::path::Path;
use serde_json;
use crate::{BenchResult, Comparison};

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
    format!(
        "{}::{} - p50: {}, p90: {}, p99: {} ({} samples × {} iterations)",
        result.module,
        result.name,
        format_duration_human_readable(result.percentiles.p50),
        format_duration_human_readable(result.percentiles.p90),
        format_duration_human_readable(result.percentiles.p99),
        result.samples,
        result.iterations
    )
}

pub fn format_comparison_result(comparison: &Comparison, benchmark_name: &str) -> String {
    let change_symbol = if comparison.percentage_change > 0.0 { "↗" } else { "↘" };
    let change_color = if comparison.is_regression { "REGRESSION" } else { "OK" };
    
    format!(
        "{} [{}] {} {:.1}% ({} -> {})",
        benchmark_name,
        change_color,
        change_symbol,
        comparison.percentage_change.abs(),
        format_duration_human_readable(comparison.baseline_p90),
        format_duration_human_readable(comparison.current_p90)
    )
}

pub fn print_summary(results: &[BenchResult], comparisons: Option<&[Comparison]>) {
    println!("SimpleBench Results:");
    println!("===================");
    
    for (i, result) in results.iter().enumerate() {
        println!("{}", format_benchmark_result(result));
        
        if let Some(comparisons) = comparisons {
            if i < comparisons.len() {
                println!("  {}", format_comparison_result(&comparisons[i], &result.name));
            }
        }
        println!();
    }
    
    if let Some(comparisons) = comparisons {
        let regressions = comparisons.iter().filter(|c| c.is_regression).count();
        if regressions > 0 {
            println!("⚠️  {} regression(s) detected!", regressions);
        } else {
            println!("✅ No regressions detected");
        }
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
        assert!(formatted.contains("10 samples × 100 iterations"));
    }
}