# Historical Data & Analysis Tools Implementation Plan

## Overview

Enhance SimpleBench with comprehensive data retention and analysis tools to debug variance issues. This involves restructuring baseline storage to preserve all raw samples and run history, plus adding analytical tools for post-run investigation.

## Goals

1. **Full Sample Retention**: Store complete timing data (ALL samples, regardless of count), not just percentiles
2. **Historical Tracking**: Keep every run instead of overwriting, with timestamp-based organization
3. **Rich Analysis Tools**: Add `analyze` subcommand for detailed statistical analysis
4. **Better Debugging**: Enable investigation of variance patterns across runs

**Note on Storage**: Disk space is not a concern. Whether a benchmark runs 100 samples or 100,000 samples, we store EVERY sample. This complete data retention enables thorough post-run analysis and variance investigation.

## Current State

**Current Directory Structure:**
```
.benches/
  <mac-address>/
    <crate_name>_<benchmark_name>.json
```

**Current JSON Format (baseline.rs):**
```json
{
  "timestamp": "2025-01-15T10:30:00Z",
  "p50": 1250,
  "p90": 1500,
  "p99": 1800
}
```

**Limitations:**
- Raw samples are discarded after percentile calculation
- Each new run overwrites previous baseline
- No variance/outlier information preserved
- Cannot analyze historical trends or run-to-run variance

## Proposed Changes

### New Directory Structure
```
.benches/
  <mac-address>/
    <crate_name>_<benchmark_name>/
      2025-01-15T10-30-00.json
      2025-01-15T14-22-15.json
      2025-01-16T09-15-30.json
```

### New JSON Format
```json
{
  "timestamp": "2025-01-15T10:30:00Z",
  "samples": [1200, 1250, 1300, ...],  // ALL raw samples in nanoseconds (100, 100,000, or any count)
  "statistics": {
    "mean": 1275,
    "median": 1250,
    "p90": 1500,
    "p99": 1800,
    "std_dev": 125.5,
    "variance": 15750.25,
    "min": 1150,
    "max": 1850,
    "sample_count": 100  // Total number of samples collected
  }
}
```

### New Subcommand: `analyze`

**Usage:**
```bash
cargo simplebench analyze <benchmark_name>
cargo simplebench analyze game_math_vector_add
cargo simplebench analyze --run 2025-01-15T10-30-00  # Specific run
cargo simplebench analyze --last 5                   # Last 5 runs
```

**Output Example:**
```
Benchmark: game_math::vector_add
Latest Run: 2025-01-15T14:22:15
Samples: 100

╭─ Summary Statistics ─────────────────╮
│ Mean:              1,275 ns          │
│ Median (p50):      1,250 ns          │
│ p90:               1,500 ns          │
│ p99:               1,800 ns          │
│                                      │
│ Std Deviation:     125.5 ns (9.8%)   │
│ Variance:          15,750 ns²        │
│ Range:             1,150 - 1,850 ns  │
╰──────────────────────────────────────╯

╭─ Outlier Analysis ───────────────────╮
│ IQR Method (1.5× threshold):         │
│   Lower fence: 1,000 ns              │
│   Upper fence: 1,750 ns              │
│   Outliers: 3 high (3.0%)            │
│                                      │
│ Z-Score Method (3σ threshold):       │
│   Outliers: 2 extreme (2.0%)         │
│                                      │
│ Flagged samples:                     │
│   #87: 1,820 ns (+45.6% from median) │
│   #92: 1,850 ns (+48.0% from median) │
╰──────────────────────────────────────╯

Historical Comparison (last 5 runs):
  Run                    Mean      Median    p90       Variance
  2025-01-15T14:22:15   1,275 ns  1,250 ns  1,500 ns  9.8%
  2025-01-15T10:30:00   1,290 ns  1,260 ns  1,520 ns  11.2%
  2025-01-14T16:45:10   1,265 ns  1,245 ns  1,485 ns  8.5%
  2025-01-14T09:20:05   1,310 ns  1,270 ns  1,550 ns  13.1%
  2025-01-13T11:15:42   1,280 ns  1,255 ns  1,505 ns  10.3%
```

## Implementation Tasks

### Task 1: Update Data Structures (simplebench-runtime)

**Files to modify:**
- `simplebench-runtime/src/lib.rs` - Add `BenchRunData` struct
- `simplebench-runtime/src/measurement.rs` - Return raw samples
- `simplebench-runtime/src/baseline.rs` - Update serialization format

**New Types:**
```rust
// lib.rs
#[derive(Serialize, Deserialize, Debug)]
pub struct BenchRunData {
    pub timestamp: String,
    pub samples: Vec<u128>,  // ALL raw samples in nanoseconds (stores every single sample, regardless of count)
    pub statistics: Statistics,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Statistics {
    pub mean: u128,
    pub median: u128,
    pub p90: u128,
    pub p99: u128,
    pub std_dev: f64,
    pub variance: f64,
    pub min: u128,
    pub max: u128,
    pub sample_count: usize,  // Number of samples (for display/validation)
}
```

**Changes to measurement.rs:**
```rust
// Keep ALL samples instead of just percentiles (regardless of sample count configuration)
pub fn measure_function(f: fn()) -> BenchRunData {
    let mut samples = vec![];
    // ... collect samples (could be 100, 100,000, or any count) ...

    let statistics = calculate_statistics(&samples);

    BenchRunData {
        timestamp: Utc::now().to_rfc3339(),
        samples,  // Store complete array - disk space is not a concern
        statistics,
    }
}

fn calculate_statistics(samples: &[u128]) -> Statistics {
    // Calculate mean, median, percentiles, std_dev, variance, min, max
    let sample_count = samples.len();
    // ... calculations ...

    Statistics {
        mean,
        median,
        p90,
        p99,
        std_dev,
        variance,
        min,
        max,
        sample_count,
    }
}
```

### Task 2: Update Directory Structure (simplebench-runtime)

**Files to modify:**
- `simplebench-runtime/src/baseline.rs` - Implement new storage strategy

**Key Changes:**

1. **Timestamp-based filenames:**
```rust
// Old: .benches/<mac>/crate_name_bench_name.json
// New: .benches/<mac>/crate_name_bench_name/2025-01-15T10-30-00.json

fn get_run_path(workspace_root: &Path, crate_name: &str, bench_name: &str) -> PathBuf {
    let machine_id = get_machine_id();
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S");

    workspace_root
        .join(".benches")
        .join(machine_id)
        .join(format!("{}_{}", crate_name, bench_name))
        .join(format!("{}.json", timestamp))
}
```

2. **Save without overwriting:**
```rust
pub fn save_run(data: &BenchRunData, workspace_root: &Path, crate_name: &str, bench_name: &str) -> Result<()> {
    let path = get_run_path(workspace_root, crate_name, bench_name);

    // Create parent directories
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write new file (never overwrites)
    let json = serde_json::to_string_pretty(data)?;
    fs::write(&path, json)?;
    Ok(())
}
```

3. **Get most recent baseline:**
```rust
pub fn get_latest_baseline(workspace_root: &Path, crate_name: &str, bench_name: &str) -> Option<BenchRunData> {
    let machine_id = get_machine_id();
    let bench_dir = workspace_root
        .join(".benches")
        .join(machine_id)
        .join(format!("{}_{}", crate_name, bench_name));

    if !bench_dir.exists() {
        return None;
    }

    // Find most recent JSON file
    let mut runs: Vec<_> = fs::read_dir(&bench_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .collect();

    runs.sort_by_key(|e| e.file_name());
    let latest = runs.last()?;

    let json = fs::read_to_string(latest.path()).ok()?;
    serde_json::from_str(&json).ok()
}
```

### Task 3: Update Baseline Comparison Logic

**Files to modify:**
- `simplebench-runtime/src/baseline.rs` - Update `check_regression` function
- `cargo-simplebench/src/main.rs` - Update runner to use new baseline logic

**Changes:**
```rust
pub fn check_regression(
    current: &BenchRunData,
    baseline: &BenchRunData,
    threshold: f64
) -> bool {
    let current_p90 = current.statistics.p90 as f64;
    let baseline_p90 = baseline.statistics.p90 as f64;

    let percent_diff = ((current_p90 - baseline_p90) / baseline_p90) * 100.0;
    percent_diff > threshold
}
```

### Task 4: Implement `analyze` Subcommand

**Files to modify:**
- `cargo-simplebench/src/main.rs` - Add analyze subcommand
- Create `cargo-simplebench/src/analyze.rs` - Analysis logic

**Subcommand Structure:**
```rust
// main.rs
enum Command {
    Run {
        ci: bool,
        threshold: f64,
    },
    Analyze {
        benchmark_name: String,
        run_timestamp: Option<String>,
        last_n: Option<usize>,
    },
}

fn main() {
    match parse_args() {
        Command::Run { ci, threshold } => run_benchmarks(ci, threshold),
        Command::Analyze { benchmark_name, run_timestamp, last_n } => {
            analyze::run_analysis(benchmark_name, run_timestamp, last_n)
        }
    }
}
```

**Analysis Module (analyze.rs):**
```rust
pub fn run_analysis(benchmark_name: String, run_timestamp: Option<String>, last_n: Option<usize>) {
    let workspace_root = find_workspace_root();
    let machine_id = get_machine_id();

    // Find benchmark directory
    let bench_dir = find_benchmark_dir(&workspace_root, &machine_id, &benchmark_name)?;

    if let Some(timestamp) = run_timestamp {
        // Analyze specific run
        analyze_single_run(&bench_dir, &timestamp);
    } else if let Some(n) = last_n {
        // Compare last N runs
        analyze_multiple_runs(&bench_dir, n);
    } else {
        // Analyze latest run + show history
        analyze_latest_with_history(&bench_dir);
    }
}

fn analyze_single_run(bench_dir: &Path, timestamp: &str) {
    let run_data = load_run_data(bench_dir, timestamp)?;

    println!("╭─ Summary Statistics ─────────────────╮");
    print_statistics(&run_data.statistics);
    println!("╰──────────────────────────────────────╯\n");

    println!("╭─ Outlier Analysis ───────────────────╮");
    detect_and_print_outliers(&run_data.samples);
    println!("╰──────────────────────────────────────╯");
}

fn detect_and_print_outliers(samples: &[u128]) {
    // IQR method
    let q1 = percentile(samples, 25.0);
    let q3 = percentile(samples, 75.0);
    let iqr = q3 - q1;
    let lower_fence = q1 - (1.5 * iqr as f64) as u128;
    let upper_fence = q3 + (1.5 * iqr as f64) as u128;

    let outliers: Vec<_> = samples.iter()
        .enumerate()
        .filter(|(_, &s)| s < lower_fence || s > upper_fence)
        .collect();

    println!("  IQR Method (1.5× threshold):");
    println!("    Lower fence: {} ns", format_ns(lower_fence));
    println!("    Upper fence: {} ns", format_ns(upper_fence));
    println!("    Outliers: {} ({:.1}%)", outliers.len(),
             (outliers.len() as f64 / samples.len() as f64) * 100.0);

    // Z-score method
    let mean = samples.iter().sum::<u128>() as f64 / samples.len() as f64;
    let variance = samples.iter()
        .map(|&s| (s as f64 - mean).powi(2))
        .sum::<f64>() / samples.len() as f64;
    let std_dev = variance.sqrt();

    let z_outliers: Vec<_> = samples.iter()
        .enumerate()
        .filter(|(_, &s)| ((s as f64 - mean) / std_dev).abs() > 3.0)
        .collect();

    println!("\n  Z-Score Method (3σ threshold):");
    println!("    Outliers: {} ({:.1}%)", z_outliers.len(),
             (z_outliers.len() as f64 / samples.len() as f64) * 100.0);

    // Print flagged samples
    if !outliers.is_empty() {
        println!("\n  Flagged samples:");
        for (idx, &sample) in outliers.iter().take(5) {
            let median = percentile(samples, 50.0);
            let diff_pct = ((sample as f64 - median as f64) / median as f64) * 100.0;
            println!("    #{}: {} ns ({:+.1}% from median)",
                     idx, format_ns(*sample), diff_pct);
        }
    }
}
```

### Task 5: Update Output Formatting

**Files to modify:**
- `simplebench-runtime/src/output.rs` - Add variance percentage to output

**Enhanced Output:**
```rust
pub fn print_bench_result(result: &BenchRunData, baseline: Option<&BenchRunData>) {
    let stats = &result.statistics;

    println!("  Mean: {} ns (±{:.1}%)",
             format_ns(stats.mean),
             (stats.std_dev / stats.mean as f64) * 100.0);
    println!("  p50:  {} ns", format_ns(stats.median));
    println!("  p90:  {} ns", format_ns(stats.p90));
    println!("  p99:  {} ns", format_ns(stats.p99));

    if let Some(baseline) = baseline {
        // ... regression comparison ...
    }
}
```

## Performance and Storage Considerations

### File Size Expectations
- **100 samples**: ~2-5 KB per JSON file
- **1,000 samples**: ~20-50 KB per JSON file
- **10,000 samples**: ~200-500 KB per JSON file
- **100,000 samples**: ~2-5 MB per JSON file

### Design Philosophy
**Disk space is NOT a constraint.** The priority is complete data retention for thorough variance analysis. Benefits of storing all samples:

1. **Post-run analysis**: Investigate variance without re-running benchmarks
2. **Outlier identification**: See exact sample indices and values that caused variance
3. **Distribution analysis**: Understand timing patterns (bimodal, skewed, etc.)
4. **Historical trends**: Compare sample distributions across runs
5. **Statistical validation**: Verify percentile calculations and detect anomalies

### No Compression or Truncation
- Store samples in plain JSON (human-readable, tool-friendly)
- No sample truncation or summarization
- No lossy compression
- If a benchmark configuration uses 100,000 samples, we store all 100,000

## Testing Strategy

### 1. Data Structure Testing
```bash
cd test-workspace
rm -rf .benches/  # Clean slate
cargo simplebench  # First run
# Verify:
# - Directory created: .benches/<mac>/game_math_vector_add/
# - File exists: .benches/<mac>/game_math_vector_add/<timestamp>.json
# - JSON contains ALL samples (current config uses 100, but could be 100,000+)
# - "sample_count" field matches length of "samples" array

cargo simplebench  # Second run
# Verify:
# - Second timestamped file created (not overwritten)
# - Both files exist in same directory
# - Each file stores complete sample set independently
```

### 2. Analysis Subcommand Testing
```bash
cargo simplebench analyze game_math_vector_add
# Verify:
# - Shows latest run statistics
# - Displays outlier analysis
# - Shows historical comparison

cargo simplebench analyze game_math_vector_add --last 3
# Verify:
# - Compares last 3 runs
# - Shows variance trends
```

### 3. Regression Testing
```bash
cargo simplebench  # Run 1 (establishes baseline)
cargo simplebench  # Run 2 (should pass)
# Verify:
# - Compares against Run 1 (most recent previous)
# - No regression detected (assuming stable timings)
```

### 4. Migration Testing
```bash
# Start with old baseline format
echo '{"timestamp": "...", "p50": 1000, "p90": 1200, "p99": 1500}' > \
  .benches/<mac>/old_bench.json

cargo simplebench
# Verify:
# - New format coexists with old format
# - No crashes from missing 'samples' field
# - Graceful handling (warn user to re-establish baselines)
```

## Implementation Order

1. **Task 1**: Update data structures to store all raw samples
   - Modify `BenchRunData` struct
   - Update `measure_function` to preserve samples
   - Add `calculate_statistics` function

2. **Task 2**: Change directory structure for historical runs
   - Update `get_run_path` to use timestamp-based paths
   - Implement `save_run` to create new files
   - Implement `get_latest_baseline` to find most recent

3. **Task 3**: Update baseline comparison logic
   - Modify `check_regression` to use new `BenchRunData` format
   - Update runner to save/load new format

4. **Task 4**: Implement basic `analyze` subcommand
   - Add CLI argument parsing for analyze
   - Implement `analyze_single_run` with statistics display
   - Add formatted output for summary statistics

5. **Task 5**: Add outlier detection
   - Implement IQR-based outlier detection
   - Implement Z-score-based outlier detection
   - Add formatted output for outlier analysis

6. **Task 6**: Add historical comparison
   - Implement `analyze_multiple_runs`
   - Add table output for run-to-run comparison
   - Calculate variance trends

7. **Task 7**: Testing and validation
   - Test with test-workspace
   - Verify data persistence across runs
   - Validate statistical calculations

## Success Criteria

- ✅ All raw samples preserved in JSON files (100, 100,000, or any count)
- ✅ Each run creates new timestamped file (no overwrites)
- ✅ Baseline = most recent run in directory
- ✅ `cargo simplebench analyze <name>` shows detailed statistics
- ✅ Outlier detection identifies anomalous samples
- ✅ Historical comparison shows variance trends
- ✅ Test workspace validation passes
- ✅ Backward compatibility: handles missing old baselines gracefully
- ✅ Handles large sample counts (100,000+) without performance issues
- ✅ File size matches expectation: ~20 bytes per sample (e.g., 2MB for 100k samples)

## Future Enhancements (Out of Scope)

- Export analysis to CSV/JSON for external tools
- Graphical visualization of sample distributions
- Statistical tests (t-test, Mann-Whitney U)
- Automated variance quality scoring
- Cross-machine baseline comparison
