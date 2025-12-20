//! Output formatting for benchmark results
//!
//! This module handles all user-facing output from the orchestrator,
//! including benchmark results, comparisons, and summaries.

use colored::*;
use simplebench_runtime::{
    baseline::ComparisonResult, config::ComparisonConfig, BenchResult, Comparison,
};
use std::time::Duration;

/// Format a duration in human-readable form
pub fn format_duration(duration: Duration) -> String {
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

/// Print a single benchmark result (called as each benchmark completes)
pub fn print_benchmark_result(result: &BenchResult, core: usize) {
    let bench_name = format!("{}::{}", result.module, result.name);
    let mean_str = format_duration(result.percentiles.mean);
    let p50_str = format_duration(result.percentiles.p50);
    let p90_str = format_duration(result.percentiles.p90);
    let p99_str = format_duration(result.percentiles.p99);

    // Calculate coefficient of variation (CV) from raw timings if available
    let cv_str = if !result.all_timings.is_empty() {
        let samples_ns: Vec<u128> = result.all_timings.iter().map(|d| d.as_nanos()).collect();
        let stats = simplebench_runtime::calculate_statistics(&samples_ns);
        let cv_pct = if stats.mean > 0 {
            (stats.std_dev / stats.mean as f64) * 100.0
        } else {
            0.0
        };
        format!(" (CV: {:.1}%)", cv_pct)
    } else {
        String::new()
    };

    println!(
        "{} {} mean: {}{}, p50: {}, p90: {}, p99: {} [core {}]",
        "BENCH".green().bold(),
        bench_name.cyan(),
        mean_str.cyan().bold(),
        cv_str.dimmed(),
        p50_str.dimmed(),
        p90_str.dimmed(),
        p99_str.dimmed(),
        core.to_string().yellow()
    );

    // Print warmup stats if available
    if let (Some(warmup_ms), Some(warmup_iters)) = (result.warmup_ms, result.warmup_iterations) {
        println!(
            "        {} {}ms ({} iterations)",
            "Warmup:".dimmed(),
            warmup_ms,
            warmup_iters
        );
    }

    // Print CPU stats if available
    if let Some(cpu_stats) = format_cpu_stats(&result.cpu_samples) {
        println!("        {}", cpu_stats.dimmed());
    }
}

/// Format CPU statistics from samples
fn format_cpu_stats(cpu_samples: &[simplebench_runtime::CpuSnapshot]) -> Option<String> {
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

/// Print comparison result for a benchmark
pub fn print_comparison(comparison: &Comparison, benchmark_name: &str, is_regression: bool) {
    let change_symbol = if comparison.percentage_change > 0.0 {
        "↗"
    } else {
        "↘"
    };
    let percentage_str = format!("{:.1}%", comparison.percentage_change.abs());
    let baseline_str = format_duration(comparison.baseline_mean);
    let current_str = format_duration(comparison.current_mean);

    let baseline_suffix = if comparison.baseline_count > 1 {
        format!(" (n={})", comparison.baseline_count)
    } else {
        String::new()
    };

    let base_line = if is_regression {
        format!(
            "        {} {} {} (mean: {} -> {}{})",
            "REGRESS".red().bold(),
            change_symbol,
            percentage_str.red().bold(),
            baseline_str.dimmed(),
            current_str.red(),
            baseline_suffix.dimmed()
        )
    } else if comparison.percentage_change < -5.0 {
        format!(
            "        {} {} {} (mean: {} -> {}{})",
            "IMPROVE".green().bold(),
            change_symbol,
            percentage_str.green(),
            baseline_str.dimmed(),
            current_str.green(),
            baseline_suffix.dimmed()
        )
    } else {
        format!(
            "        {} {} {} (mean: {} -> {}{})",
            "STABLE".cyan(),
            change_symbol,
            percentage_str.dimmed(),
            baseline_str.dimmed(),
            current_str.dimmed(),
            baseline_suffix.dimmed()
        )
    };

    // Add statistical info if available
    let mut stats_parts = Vec::new();

    if let Some(z_score) = comparison.z_score {
        stats_parts.push(format!("z={:.2}", z_score));
    }

    if let Some(cp_prob) = comparison.change_probability {
        stats_parts.push(format!("cp={:.0}%", cp_prob * 100.0));
    }

    if !stats_parts.is_empty() {
        println!("{}", base_line);
        println!("        {}", stats_parts.join(", ").dimmed());
    } else {
        println!("{}", base_line);
    }

    // Suppress unused variable warning
    let _ = benchmark_name;
}

/// Print "NEW" message for first baseline
pub fn print_new_baseline(benchmark_name: &str) {
    println!(
        "        {} {} (establishing baseline)",
        "NEW".blue().bold(),
        benchmark_name.bright_white()
    );
}

/// Print comparison result (handles both existing comparison and new baseline cases)
pub fn print_comparison_result(comparison_result: &ComparisonResult) {
    if let Some(ref comparison) = comparison_result.comparison {
        print_comparison(comparison, &comparison_result.benchmark_name, comparison_result.is_regression);
    } else {
        print_new_baseline(&comparison_result.benchmark_name);
    }
}

/// Print summary footer
pub fn print_summary(comparisons: &[ComparisonResult], config: &ComparisonConfig) {
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

/// Print header showing benchmark count and core usage
pub fn print_run_header(benchmark_count: usize, core_count: usize, parallel: bool) {
    let mode = if parallel { "parallel" } else { "sequential" };
    println!(
        "\n{} {} {} on {} core(s) ({})",
        "Running".green().bold(),
        benchmark_count,
        if benchmark_count == 1 {
            "benchmark"
        } else {
            "benchmarks"
        },
        core_count.to_string().cyan().bold(),
        mode.yellow()
    );
    println!();
}
