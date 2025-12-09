use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Configuration for benchmark measurement parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasurementConfig {
    /// Number of timing samples to collect per benchmark
    #[serde(default = "default_samples")]
    pub samples: usize,

    /// Number of iterations per sample (None = auto-scale)
    #[serde(default)]
    pub iterations: Option<usize>,

    /// Number of warmup iterations before measurement
    #[serde(default = "default_warmup_iterations")]
    pub warmup_iterations: usize,

    /// Target duration per sample in milliseconds (for auto-scaling)
    #[serde(default = "default_target_sample_duration_ms")]
    pub target_sample_duration_ms: u64,
}

fn default_samples() -> usize { 200 }
fn default_warmup_iterations() -> usize { 50 }
fn default_target_sample_duration_ms() -> u64 { 10 }

impl Default for MeasurementConfig {
    fn default() -> Self {
        Self {
            samples: default_samples(),
            iterations: None, // Auto-scale by default
            warmup_iterations: default_warmup_iterations(),
            target_sample_duration_ms: default_target_sample_duration_ms(),
        }
    }
}

/// Configuration for baseline comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonConfig {
    /// Regression threshold percentage
    #[serde(default = "default_threshold")]
    pub threshold: f64,

    /// CI mode: fail on regressions
    #[serde(default)]
    pub ci_mode: bool,
}

fn default_threshold() -> f64 { 5.0 }

impl Default for ComparisonConfig {
    fn default() -> Self {
        Self {
            threshold: default_threshold(),
            ci_mode: false,
        }
    }
}

/// Complete SimpleBench configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BenchmarkConfig {
    #[serde(default)]
    pub measurement: MeasurementConfig,

    #[serde(default)]
    pub comparison: ComparisonConfig,
}

impl BenchmarkConfig {
    /// Load configuration with priority: env vars > config file > defaults
    ///
    /// This is called by the generated runner at startup.
    pub fn load() -> Self {
        // Start with defaults
        let mut config = Self::default();

        // Try to load from config file
        if let Ok(file_config) = Self::from_file("simplebench.toml") {
            config = file_config;
        }

        // Override with environment variables
        config.apply_env_overrides();

        config
    }

    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        let config: BenchmarkConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Apply environment variable overrides
    ///
    /// This allows CLI args (passed via env vars) to override config file values.
    pub fn apply_env_overrides(&mut self) {
        // Measurement overrides
        if let Ok(samples) = std::env::var("SIMPLEBENCH_SAMPLES") {
            if let Ok(val) = samples.parse() {
                self.measurement.samples = val;
            }
        }

        if let Ok(iterations) = std::env::var("SIMPLEBENCH_ITERATIONS") {
            if let Ok(val) = iterations.parse() {
                self.measurement.iterations = Some(val);
            }
        }

        if let Ok(warmup) = std::env::var("SIMPLEBENCH_WARMUP_ITERATIONS") {
            if let Ok(val) = warmup.parse() {
                self.measurement.warmup_iterations = val;
            }
        }

        if let Ok(duration) = std::env::var("SIMPLEBENCH_TARGET_DURATION_MS") {
            if let Ok(val) = duration.parse() {
                self.measurement.target_sample_duration_ms = val;
            }
        }

        // Comparison overrides
        if std::env::var("SIMPLEBENCH_CI").is_ok() {
            self.comparison.ci_mode = true;
        }

        if let Ok(threshold) = std::env::var("SIMPLEBENCH_THRESHOLD") {
            if let Ok(val) = threshold.parse() {
                self.comparison.threshold = val;
            }
        }
    }

    /// Save configuration to a TOML file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let toml = toml::to_string_pretty(self)?;
        fs::write(path, toml)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = BenchmarkConfig::default();
        assert_eq!(config.measurement.samples, 200);
        assert_eq!(config.measurement.iterations, None);
        assert_eq!(config.measurement.warmup_iterations, 50);
        assert_eq!(config.measurement.target_sample_duration_ms, 10);
        assert_eq!(config.comparison.threshold, 5.0);
        assert_eq!(config.comparison.ci_mode, false);
    }

    #[test]
    fn test_save_and_load_config() {
        let config = BenchmarkConfig::default();
        let temp_file = NamedTempFile::new().unwrap();

        config.save(temp_file.path()).unwrap();
        let loaded = BenchmarkConfig::from_file(temp_file.path()).unwrap();

        assert_eq!(loaded.measurement.samples, 200);
        assert_eq!(loaded.measurement.warmup_iterations, 50);
    }

    #[test]
    fn test_env_overrides() {
        env::set_var("SIMPLEBENCH_SAMPLES", "300");
        env::set_var("SIMPLEBENCH_ITERATIONS", "1000");
        env::set_var("SIMPLEBENCH_WARMUP_ITERATIONS", "100");
        env::set_var("SIMPLEBENCH_CI", "1");
        env::set_var("SIMPLEBENCH_THRESHOLD", "10.0");

        let mut config = BenchmarkConfig::default();
        config.apply_env_overrides();

        assert_eq!(config.measurement.samples, 300);
        assert_eq!(config.measurement.iterations, Some(1000));
        assert_eq!(config.measurement.warmup_iterations, 100);
        assert_eq!(config.comparison.ci_mode, true);
        assert_eq!(config.comparison.threshold, 10.0);

        // Clean up
        env::remove_var("SIMPLEBENCH_SAMPLES");
        env::remove_var("SIMPLEBENCH_ITERATIONS");
        env::remove_var("SIMPLEBENCH_WARMUP_ITERATIONS");
        env::remove_var("SIMPLEBENCH_CI");
        env::remove_var("SIMPLEBENCH_THRESHOLD");
    }

    #[test]
    fn test_partial_config_file() {
        let toml_content = r#"
            [measurement]
            samples = 150

            [comparison]
            threshold = 7.5
        "#;

        let temp_file = NamedTempFile::new().unwrap();
        fs::write(temp_file.path(), toml_content).unwrap();

        let config = BenchmarkConfig::from_file(temp_file.path()).unwrap();

        // Specified values
        assert_eq!(config.measurement.samples, 150);
        assert_eq!(config.comparison.threshold, 7.5);

        // Default values for unspecified fields
        assert_eq!(config.measurement.iterations, None);
        assert_eq!(config.measurement.warmup_iterations, 50);
        assert_eq!(config.comparison.ci_mode, false);
    }
}
