use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use crate::{BenchResult, Percentiles};
use crate::config::ComparisonConfig;

/// Get the MAC address of the primary network interface and hash it for privacy
///
/// Returns a SHA256 hash (first 16 hex characters) of the MAC address to serve as
/// a stable machine identifier without exposing the actual MAC address.
fn get_primary_mac_address() -> Result<String, std::io::Error> {
    let interface = default_net::get_default_interface()
        .map_err(|e| std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Failed to get default network interface: {}", e)
        ))?;

    let mac_addr = interface.mac_addr
        .ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Default interface has no MAC address"
        ))?;

    // Format as lowercase with dashes: aa-bb-cc-dd-ee-ff
    // The default format uses colons, so replace them with dashes
    let mac_string = format!("{}", mac_addr).replace(':', "-").to_lowercase();

    // Hash the MAC address for privacy protection
    hash_mac_address(&mac_string)
}

/// Hash a MAC address using SHA256 for privacy protection
///
/// Returns the first 16 characters of the hex digest as a stable machine identifier
fn hash_mac_address(mac: &str) -> Result<String, std::io::Error> {
    let mut hasher = Sha256::new();
    hasher.update(mac.as_bytes());
    let result = hasher.finalize();

    // Use first 16 characters of hex digest (64 bits of entropy)
    Ok(format!("{:x}", result)[..16].to_string())
}

/// Storage format for baseline benchmark results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineData {
    pub benchmark_name: String,
    pub module: String,
    pub percentiles: Percentiles,
    pub iterations: usize,
    pub samples: usize,
    pub timestamp: String,
    #[serde(alias = "hostname")]
    pub machine_id: String,
}

impl BaselineData {
    pub fn from_bench_result(result: &BenchResult, machine_id: String) -> Self {
        Self {
            benchmark_name: result.name.clone(),
            module: result.module.clone(),
            percentiles: result.percentiles.clone(),
            iterations: result.iterations,
            samples: result.samples,
            timestamp: chrono::Utc::now().to_rfc3339(),
            machine_id,
        }
    }

    pub fn to_bench_result(&self) -> BenchResult {
        BenchResult {
            name: self.benchmark_name.clone(),
            module: self.module.clone(),
            percentiles: self.percentiles.clone(),
            iterations: self.iterations,
            samples: self.samples,
            all_timings: vec![], // Not stored in baseline
        }
    }
}

/// Manages baseline storage in .benches/ directory
#[derive(Debug)]
pub struct BaselineManager {
    root_dir: PathBuf,
    machine_id: String,
}

impl BaselineManager {
    /// Create a new baseline manager
    ///
    /// By default, uses .benches/ in the current directory
    pub fn new() -> Result<Self, std::io::Error> {
        let machine_id = get_primary_mac_address()?;

        Ok(Self {
            root_dir: PathBuf::from(".benches"),
            machine_id,
        })
    }

    /// Create a baseline manager with a custom root directory
    pub fn with_root_dir<P: AsRef<Path>>(root_dir: P) -> Result<Self, std::io::Error> {
        let machine_id = get_primary_mac_address()?;

        Ok(Self {
            root_dir: root_dir.as_ref().to_path_buf(),
            machine_id,
        })
    }

    /// Get the directory path for this machine's baselines
    fn machine_dir(&self) -> PathBuf {
        self.root_dir.join(&self.machine_id)
    }

    /// Get the file path for a specific benchmark baseline
    fn baseline_path(&self, crate_name: &str, benchmark_name: &str) -> PathBuf {
        let filename = format!("{}_{}.json", crate_name, benchmark_name);
        self.machine_dir().join(filename)
    }

    /// Ensure the baseline directory exists
    fn ensure_dir_exists(&self) -> Result<(), std::io::Error> {
        fs::create_dir_all(self.machine_dir())
    }

    /// Save a benchmark result as a baseline
    pub fn save_baseline(&self, crate_name: &str, result: &BenchResult) -> Result<(), std::io::Error> {
        self.ensure_dir_exists()?;

        let baseline = BaselineData::from_bench_result(result, self.machine_id.clone());
        let json = serde_json::to_string_pretty(&baseline)?;

        let path = self.baseline_path(crate_name, &result.name);
        fs::write(path, json)?;

        Ok(())
    }

    /// Load a baseline for a specific benchmark
    pub fn load_baseline(&self, crate_name: &str, benchmark_name: &str) -> Result<Option<BaselineData>, std::io::Error> {
        let path = self.baseline_path(crate_name, benchmark_name);

        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(path)?;
        let baseline: BaselineData = serde_json::from_str(&contents)?;

        Ok(Some(baseline))
    }

    /// Check if a baseline exists for a benchmark
    pub fn has_baseline(&self, crate_name: &str, benchmark_name: &str) -> bool {
        self.baseline_path(crate_name, benchmark_name).exists()
    }

    /// List all baselines for a crate
    pub fn list_baselines(&self, crate_name: &str) -> Result<Vec<String>, std::io::Error> {
        let machine_dir = self.machine_dir();

        if !machine_dir.exists() {
            return Ok(vec![]);
        }

        let prefix = format!("{}_", crate_name);
        let mut baselines = Vec::new();

        for entry in fs::read_dir(machine_dir)? {
            let entry = entry?;
            let filename = entry.file_name().to_string_lossy().to_string();

            if filename.starts_with(&prefix) && filename.ends_with(".json") {
                // Extract benchmark name from filename
                let benchmark_name = filename
                    .strip_prefix(&prefix)
                    .and_then(|s| s.strip_suffix(".json"))
                    .unwrap_or(&filename)
                    .to_string();
                baselines.push(benchmark_name);
            }
        }

        Ok(baselines)
    }
}

impl Default for BaselineManager {
    fn default() -> Self {
        Self::new().expect("Failed to get primary MAC address")
    }
}

/// Result of baseline comparison for a single benchmark
#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub benchmark_name: String,
    pub comparison: Option<crate::Comparison>,
    pub is_regression: bool,
}

/// Process benchmarks with baseline comparison
///
/// This function:
/// 1. Loads existing baselines
/// 2. Compares current results with baselines
/// 3. Saves new baselines
/// 4. Returns comparison results
pub fn process_with_baselines(
    results: &[crate::BenchResult],
    config: &ComparisonConfig,
) -> Result<Vec<ComparisonResult>, std::io::Error> {
    let baseline_manager = BaselineManager::new()?;
    let mut comparisons = Vec::new();

    for result in results {
        // Extract crate name from module path (first component)
        let crate_name = result.module.split("::").next().unwrap_or("unknown");

        // Try to load baseline
        let comparison_result = match baseline_manager.load_baseline(crate_name, &result.name)? {
            Some(baseline_data) => {
                let baseline = baseline_data.to_bench_result();
                let comparison = crate::compare_with_baseline(result, &baseline);

                // Use configurable threshold
                let is_regression = comparison.percentage_change > config.threshold;

                ComparisonResult {
                    benchmark_name: result.name.clone(),
                    comparison: Some(comparison),
                    is_regression,
                }
            }
            None => {
                // No baseline exists - first run
                ComparisonResult {
                    benchmark_name: result.name.clone(),
                    comparison: None,
                    is_regression: false,
                }
            }
        };

        comparisons.push(comparison_result);

        // Save current result as baseline
        baseline_manager.save_baseline(crate_name, result)?;
    }

    Ok(comparisons)
}

/// Check if any regressions were detected and exit in CI mode
pub fn check_regressions_and_exit(comparisons: &[ComparisonResult], config: &ComparisonConfig) {
    if !config.ci_mode {
        return;
    }

    let has_regression = comparisons.iter().any(|c| c.is_regression);

    if has_regression {
        use colored::Colorize;
        eprintln!();
        eprintln!(
            "{}",
            format!("FAILED: Performance regression detected (threshold: {}%)", config.threshold)
                .red()
                .bold()
        );
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    fn create_test_result(name: &str) -> BenchResult {
        BenchResult {
            name: name.to_string(),
            module: "test_module".to_string(),
            iterations: 100,
            samples: 10,
            percentiles: Percentiles {
                p50: Duration::from_millis(5),
                p90: Duration::from_millis(10),
                p99: Duration::from_millis(15),
            },
            all_timings: vec![],
        }
    }

    #[test]
    fn test_baseline_data_conversion() {
        let result = create_test_result("test_bench");
        let machine_id = "0123456789abcdef".to_string(); // 16-char hex hash

        let baseline = BaselineData::from_bench_result(&result, machine_id.clone());

        assert_eq!(baseline.benchmark_name, "test_bench");
        assert_eq!(baseline.module, "test_module");
        assert_eq!(baseline.machine_id, machine_id);
        assert_eq!(baseline.iterations, 100);
        assert_eq!(baseline.samples, 10);

        let converted = baseline.to_bench_result();
        assert_eq!(converted.name, result.name);
        assert_eq!(converted.module, result.module);
        assert_eq!(converted.percentiles.p90, result.percentiles.p90);
    }

    #[test]
    fn test_save_and_load_baseline() {
        let temp_dir = TempDir::new().unwrap();
        let manager = BaselineManager::with_root_dir(temp_dir.path()).unwrap();

        let result = create_test_result("test_bench");

        // Save baseline
        manager.save_baseline("my_crate", &result).unwrap();

        // Load baseline
        let loaded = manager.load_baseline("my_crate", "test_bench").unwrap();
        assert!(loaded.is_some());

        let baseline = loaded.unwrap();
        assert_eq!(baseline.benchmark_name, "test_bench");
        assert_eq!(baseline.module, "test_module");
        assert_eq!(baseline.percentiles.p90, Duration::from_millis(10));
    }

    #[test]
    fn test_load_nonexistent_baseline() {
        let temp_dir = TempDir::new().unwrap();
        let manager = BaselineManager::with_root_dir(temp_dir.path()).unwrap();

        let loaded = manager.load_baseline("my_crate", "nonexistent").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_has_baseline() {
        let temp_dir = TempDir::new().unwrap();
        let manager = BaselineManager::with_root_dir(temp_dir.path()).unwrap();

        let result = create_test_result("test_bench");

        assert!(!manager.has_baseline("my_crate", "test_bench"));

        manager.save_baseline("my_crate", &result).unwrap();

        assert!(manager.has_baseline("my_crate", "test_bench"));
    }

    #[test]
    fn test_list_baselines() {
        let temp_dir = TempDir::new().unwrap();
        let manager = BaselineManager::with_root_dir(temp_dir.path()).unwrap();

        let result1 = create_test_result("bench1");
        let result2 = create_test_result("bench2");

        manager.save_baseline("my_crate", &result1).unwrap();
        manager.save_baseline("my_crate", &result2).unwrap();

        let mut baselines = manager.list_baselines("my_crate").unwrap();
        baselines.sort();

        assert_eq!(baselines, vec!["bench1", "bench2"]);
    }

    #[test]
    fn test_get_primary_mac_address() {
        // Test that we can get a hashed machine ID
        let result = get_primary_mac_address();

        // Should succeed on systems with network interfaces
        assert!(result.is_ok(), "Failed to get machine ID: {:?}", result);

        let machine_id = result.unwrap();

        // Should be 16 characters (first 16 chars of SHA256 hex digest)
        assert_eq!(machine_id.len(), 16, "Machine ID should be 16 characters: {}", machine_id);

        // Should be lowercase hex
        assert_eq!(machine_id, machine_id.to_lowercase(), "Machine ID should be lowercase");
        assert!(machine_id.chars().all(|c| c.is_ascii_hexdigit()),
                "Machine ID should contain only hex digits");
    }

    #[test]
    fn test_mac_address_format() {
        // Test that BaselineManager can be created successfully
        let manager_result = BaselineManager::new();
        assert!(manager_result.is_ok(), "Failed to create BaselineManager: {:?}", manager_result);

        let manager = manager_result.unwrap();

        // Verify machine_id is properly formatted (16 character hex hash)
        assert_eq!(manager.machine_id.len(), 16, "Machine ID should be 16 characters");
        assert_eq!(manager.machine_id, manager.machine_id.to_lowercase());
        assert!(manager.machine_id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
