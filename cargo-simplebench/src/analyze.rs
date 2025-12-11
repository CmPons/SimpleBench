use anyhow::{Context, Result};
use colored::*;
use simplebench_runtime::baseline::BaselineManager;
use simplebench_runtime::{CpuAnalysis, Statistics};
use std::path::Path;

pub fn run_analysis(
    workspace_root: &Path,
    benchmark_name: &str,
    run_timestamp: Option<String>,
    last_n: Option<usize>,
) -> Result<()> {
    let baseline_manager = BaselineManager::with_root_dir(workspace_root.join(".benches"))?;

    // Try to find the benchmark by searching all crate directories
    let (crate_name, bench_name) = find_benchmark(&baseline_manager, benchmark_name)?;

    if let Some(timestamp) = run_timestamp {
        // Analyze specific run
        analyze_single_run(&baseline_manager, &crate_name, &bench_name, &timestamp)?;
    } else if let Some(n) = last_n {
        // Compare last N runs
        analyze_multiple_runs(&baseline_manager, &crate_name, &bench_name, n)?;
    } else {
        // Analyze latest run + show history
        analyze_latest_with_history(&baseline_manager, &crate_name, &bench_name)?;
    }

    Ok(())
}

/// Find the benchmark by searching through all crate directories
fn find_benchmark(
    baseline_manager: &BaselineManager,
    benchmark_name: &str,
) -> Result<(String, String)> {
    // Check if the benchmark name already includes crate name (contains "_")
    if benchmark_name.contains('_') {
        // Try to split into crate and bench name
        if let Some(idx) = benchmark_name.find('_') {
            let crate_name = &benchmark_name[..idx];
            let bench_name = &benchmark_name[idx + 1..];

            // Check if this combination exists
            if baseline_manager.has_baseline(crate_name, bench_name) {
                return Ok((crate_name.to_string(), bench_name.to_string()));
            }
        }
    }

    // If not found, search through machine directory for matching benchmark
    anyhow::bail!(
        "Benchmark '{}' not found. Use format: <crate_name>_<benchmark_name>",
        benchmark_name
    )
}

/// Analyze a single run and display detailed statistics
fn analyze_single_run(
    baseline_manager: &BaselineManager,
    crate_name: &str,
    bench_name: &str,
    timestamp: &str,
) -> Result<()> {
    let run_data = baseline_manager
        .load_run(crate_name, bench_name, timestamp)?
        .context(format!("Run '{}' not found", timestamp))?;

    println!("{}", format!("Benchmark: {}::{}", crate_name, bench_name).cyan().bold());
    println!("{}", format!("Run: {}", timestamp).dimmed());
    println!("{}", format!("Samples: {}", run_data.statistics.sample_count).dimmed());
    println!();

    print_statistics(&run_data.statistics);

    // Print CPU analysis if available
    if !run_data.cpu_samples.is_empty() {
        println!();
        print_cpu_analysis(&run_data.cpu_samples);
    }

    println!();
    print_outlier_analysis(&run_data.samples, &run_data.statistics);

    Ok(())
}

/// Analyze the latest run and show historical comparison
fn analyze_latest_with_history(
    baseline_manager: &BaselineManager,
    crate_name: &str,
    bench_name: &str,
) -> Result<()> {
    let latest = baseline_manager
        .load_baseline(crate_name, bench_name)?
        .context("No baseline found for this benchmark")?;

    println!("{}", format!("Benchmark: {}::{}", crate_name, bench_name).cyan().bold());
    println!("{}", format!("Latest Run: {}", latest.timestamp).dimmed());
    println!("{}", format!("Samples: {}", latest.statistics.sample_count).dimmed());
    println!();

    print_statistics(&latest.statistics);
    println!();
    print_outlier_analysis(&latest.samples, &latest.statistics);
    println!();

    // Show historical comparison
    let runs = baseline_manager.list_runs(crate_name, bench_name)?;
    if runs.len() > 1 {
        let n = runs.len().min(5);
        println!("{}", format!("Historical Comparison (last {} runs):", n).green().bold());
        print_historical_table(baseline_manager, crate_name, bench_name, &runs[runs.len().saturating_sub(n)..])?;
    }

    Ok(())
}

/// Analyze and compare multiple runs
fn analyze_multiple_runs(
    baseline_manager: &BaselineManager,
    crate_name: &str,
    bench_name: &str,
    n: usize,
) -> Result<()> {
    let runs = baseline_manager.list_runs(crate_name, bench_name)?;

    if runs.is_empty() {
        anyhow::bail!("No runs found for benchmark {}::{}", crate_name, bench_name);
    }

    let runs_to_analyze = &runs[runs.len().saturating_sub(n)..];

    println!("{}", format!("Benchmark: {}::{}", crate_name, bench_name).cyan().bold());
    println!("{}", format!("Comparing last {} runs:", runs_to_analyze.len()).dimmed());
    println!();

    print_historical_table(baseline_manager, crate_name, bench_name, runs_to_analyze)?;

    Ok(())
}

/// Print summary statistics in a formatted table
fn print_statistics(stats: &Statistics) {
    println!("{}", "Summary Statistics".cyan().bold());
    println!("{}", "─".repeat(50).dimmed());

    let variance_pct = if stats.mean > 0 {
        (stats.std_dev / stats.mean as f64) * 100.0
    } else {
        0.0
    };

    println!("  {}  {}", "Mean:".cyan(), format_ns(stats.mean));
    println!("  {}  {}", "Median (p50):".cyan(), format_ns(stats.median));
    println!("  {}  {}", "p90:".cyan(), format_ns(stats.p90));
    println!("  {}  {}", "p99:".cyan(), format_ns(stats.p99));
    println!();
    println!("  {}  {} ({:.1}%)", "Std Dev:".cyan(), format_ns(stats.std_dev as u128), variance_pct);
    println!("  {}  {}", "Variance:".cyan(), format_ns_squared(stats.variance));
    println!("  {}  {} - {}", "Range:".cyan(), format_ns(stats.min), format_ns(stats.max));
    println!("{}", "─".repeat(50).dimmed());
}

/// Print outlier analysis in a formatted table
fn print_outlier_analysis(samples: &[u128], stats: &Statistics) {
    println!("{}", "Outlier Analysis".cyan().bold());
    println!("{}", "─".repeat(50).dimmed());

    // IQR method
    let mut sorted = samples.to_vec();
    sorted.sort();

    let q1_idx = (sorted.len() * 25) / 100;
    let q3_idx = (sorted.len() * 75) / 100;
    let q1 = sorted[q1_idx.min(sorted.len() - 1)];
    let q3 = sorted[q3_idx.min(sorted.len() - 1)];
    let iqr = q3.saturating_sub(q1) as f64;

    let lower_fence = (q1 as f64 - 1.5 * iqr).max(0.0) as u128;
    let upper_fence = (q3 as f64 + 1.5 * iqr) as u128;

    let iqr_outliers: Vec<(usize, u128)> = samples
        .iter()
        .enumerate()
        .filter(|(_, &s)| s < lower_fence || s > upper_fence)
        .map(|(i, &s)| (i, s))
        .collect();

    println!("  {}", "IQR Method (1.5× threshold):".yellow());
    println!("    {}  {}", "Lower fence:".dimmed(), format_ns(lower_fence));
    println!("    {}  {}", "Upper fence:".dimmed(), format_ns(upper_fence));
    println!("    {}  {} ({:.1}%)",
        "Outliers:".dimmed(),
        iqr_outliers.len(),
        (iqr_outliers.len() as f64 / samples.len() as f64) * 100.0
    );
    println!();

    // Z-score method
    let mean_f64 = stats.mean as f64;
    let std_dev = stats.std_dev;

    let z_outliers: Vec<(usize, u128)> = samples
        .iter()
        .enumerate()
        .filter(|(_, &s)| {
            if std_dev > 0.0 {
                ((s as f64 - mean_f64) / std_dev).abs() > 3.0
            } else {
                false
            }
        })
        .map(|(i, &s)| (i, s))
        .collect();

    println!("  {}", "Z-Score Method (3σ threshold):".yellow());
    println!("    {}  {} ({:.1}%)",
        "Outliers:".dimmed(),
        z_outliers.len(),
        (z_outliers.len() as f64 / samples.len() as f64) * 100.0
    );

    // Print flagged samples
    if !iqr_outliers.is_empty() {
        println!();
        println!("  {}", "Flagged samples (IQR):".red());
        for (idx, sample) in iqr_outliers.iter().take(5) {
            let diff_pct = if stats.median > 0 {
                ((*sample as f64 - stats.median as f64) / stats.median as f64) * 100.0
            } else {
                0.0
            };
            println!("    #{}: {} ({:+.1}%)",
                idx,
                format_ns(*sample),
                diff_pct
            );
        }
        if iqr_outliers.len() > 5 {
            println!("    {} more outliers...", iqr_outliers.len() - 5);
        }
    }

    println!("{}", "─".repeat(50).dimmed());
}

/// Print historical comparison table
fn print_historical_table(
    baseline_manager: &BaselineManager,
    crate_name: &str,
    bench_name: &str,
    timestamps: &[String],
) -> Result<()> {
    println!(
        "{:<22} {:>12} {:>12} {:>12} {:>10}",
        "Run".bold(),
        "Mean".bold(),
        "Median".bold(),
        "p90".bold(),
        "Variance".bold()
    );
    println!("{}", "─".repeat(72).dimmed());

    for timestamp in timestamps {
        if let Some(run_data) = baseline_manager.load_run(crate_name, bench_name, timestamp)? {
            let stats = &run_data.statistics;
            let variance_pct = if stats.mean > 0 {
                (stats.std_dev / stats.mean as f64) * 100.0
            } else {
                0.0
            };

            println!(
                "{:<22} {:>12} {:>12} {:>12} {:>9.1}%",
                timestamp,
                format_ns(stats.mean),
                format_ns(stats.median),
                format_ns(stats.p90),
                variance_pct
            );

            // Print CPU info if available
            if !run_data.cpu_samples.is_empty() {
                let analysis = CpuAnalysis::from_snapshots(&run_data.cpu_samples, None);
                if let Some(cpu_stats) = analysis.format_stats_line() {
                    println!("  {}{}", "    ".dimmed(), cpu_stats.dimmed());
                }
                // Show warnings if any
                for warning in &analysis.warnings {
                    println!("  {}{}", "    ".dimmed(), warning.format());
                }
            }
        }
    }

    Ok(())
}

/// Format nanoseconds in a human-readable way
fn format_ns(ns: u128) -> String {
    if ns < 1_000 {
        format!("{} ns", ns)
    } else if ns < 1_000_000 {
        format!("{:.2} µs", ns as f64 / 1_000.0)
    } else if ns < 1_000_000_000 {
        format!("{:.2} ms", ns as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", ns as f64 / 1_000_000_000.0)
    }
}

/// Format variance (ns²) in a human-readable way
fn format_ns_squared(variance: f64) -> String {
    if variance < 1_000_000.0 {
        format!("{:.0} ns²", variance)
    } else if variance < 1_000_000_000_000.0 {
        format!("{:.2} µs²", variance / 1_000_000.0)
    } else {
        format!("{:.2} ms²", variance / 1_000_000_000_000.0)
    }
}

/// Print CPU analysis for a set of samples
fn print_cpu_analysis(cpu_samples: &[simplebench_runtime::CpuSnapshot]) {
    let analysis = CpuAnalysis::from_snapshots(cpu_samples, None);

    println!("{}", "CPU Analysis".cyan().bold());
    println!("{}", "─".repeat(50).dimmed());

    if let Some(ref freq_stats) = analysis.frequency_stats {
        println!("  {}", "Frequency:".yellow());
        println!("    {}  {:.0} MHz", "Min:".dimmed(), freq_stats.min_mhz);
        println!("    {}  {:.0} MHz", "Max:".dimmed(), freq_stats.max_mhz);
        println!("    {}  {:.0} MHz", "Mean:".dimmed(), freq_stats.mean_mhz);
        println!("    {}  {:.1}%", "Variance:".dimmed(), freq_stats.variance_percent);
        println!();
    }

    if let Some(ref temp_stats) = analysis.temperature_stats {
        println!("  {}", "Temperature:".yellow());
        println!("    {}  {:.0}°C", "Min:".dimmed(), temp_stats.min_celsius);
        println!("    {}  {:.0}°C", "Max:".dimmed(), temp_stats.max_celsius);
        println!("    {}  {:.0}°C", "Mean:".dimmed(), temp_stats.mean_celsius);
        println!("    {}  +{:.0}°C", "Increase:".dimmed(), temp_stats.increase_celsius);
        println!();
    }

    if !analysis.warnings.is_empty() {
        println!("  {}", "Warnings:".red().bold());
        for warning in &analysis.warnings {
            println!("    {}", warning.format());
        }
    }

    println!("{}", "─".repeat(50).dimmed());
}
