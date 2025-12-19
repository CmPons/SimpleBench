use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Configuration for benchmark measurement parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasurementConfig {
    /// Number of timing samples to collect per benchmark
    #[serde(default = "default_samples")]
    pub samples: usize,

    /// Warmup duration in seconds (default: 3 seconds, matching Criterion)
    #[serde(default = "default_warmup_duration")]
    pub warmup_duration_secs: u64,
}

fn default_samples() -> usize {
    1000
}
fn default_warmup_duration() -> u64 {
    3 // 3 seconds, matching Criterion's default
}

impl Default for MeasurementConfig {
    fn default() -> Self {
        Self {
            samples: default_samples(),
            warmup_duration_secs: default_warmup_duration(),
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

    /// Window size for historical comparison (default: 10)
    #[serde(default = "default_window_size")]
    pub window_size: usize,

    /// Statistical confidence level (default: 0.95 = 95%)
    #[serde(default = "default_confidence_level")]
    pub confidence_level: f64,

    /// Change point probability threshold (default: 0.8 = 80%)
    #[serde(default = "default_cp_threshold")]
    pub cp_threshold: f64,

    /// Bayesian hazard rate (default: 0.1 = change every 10 runs)
    #[serde(default = "default_hazard_rate")]
    pub hazard_rate: f64,
}

fn default_threshold() -> f64 {
    5.0
}

fn default_window_size() -> usize {
    10
}

fn default_confidence_level() -> f64 {
    0.95
}

fn default_cp_threshold() -> f64 {
    0.8
}

fn default_hazard_rate() -> f64 {
    0.1
}

impl Default for ComparisonConfig {
    fn default() -> Self {
        Self {
            threshold: default_threshold(),
            ci_mode: false,
            window_size: default_window_size(),
            confidence_level: default_confidence_level(),
            cp_threshold: default_cp_threshold(),
            hazard_rate: default_hazard_rate(),
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

        if let Ok(warmup) = std::env::var("SIMPLEBENCH_WARMUP_DURATION") {
            if let Ok(val) = warmup.parse() {
                self.measurement.warmup_duration_secs = val;
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

        // CPD-specific overrides
        if let Ok(window) = std::env::var("SIMPLEBENCH_WINDOW") {
            if let Ok(val) = window.parse() {
                self.comparison.window_size = val;
            }
        }

        if let Ok(confidence) = std::env::var("SIMPLEBENCH_CONFIDENCE") {
            if let Ok(val) = confidence.parse() {
                self.comparison.confidence_level = val;
            }
        }

        if let Ok(cp_threshold) = std::env::var("SIMPLEBENCH_CP_THRESHOLD") {
            if let Ok(val) = cp_threshold.parse() {
                self.comparison.cp_threshold = val;
            }
        }

        if let Ok(hazard_rate) = std::env::var("SIMPLEBENCH_HAZARD_RATE") {
            if let Ok(val) = hazard_rate.parse() {
                self.comparison.hazard_rate = val;
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
        assert_eq!(config.measurement.samples, 1000);
        assert_eq!(config.measurement.warmup_duration_secs, 3);
        assert_eq!(config.comparison.threshold, 5.0);
        assert!(!config.comparison.ci_mode);
    }

    #[test]
    fn test_save_and_load_config() {
        let config = BenchmarkConfig::default();
        let temp_file = NamedTempFile::new().unwrap();

        config.save(temp_file.path()).unwrap();
        let loaded = BenchmarkConfig::from_file(temp_file.path()).unwrap();

        assert_eq!(loaded.measurement.samples, 1000);
        assert_eq!(loaded.measurement.warmup_duration_secs, 3);
    }

    #[test]
    fn test_env_overrides() {
        env::set_var("SIMPLEBENCH_SAMPLES", "300");
        env::set_var("SIMPLEBENCH_WARMUP_DURATION", "5");
        env::set_var("SIMPLEBENCH_CI", "1");
        env::set_var("SIMPLEBENCH_THRESHOLD", "10.0");

        let mut config = BenchmarkConfig::default();
        config.apply_env_overrides();

        assert_eq!(config.measurement.samples, 300);
        assert_eq!(config.measurement.warmup_duration_secs, 5);
        assert!(config.comparison.ci_mode);
        assert_eq!(config.comparison.threshold, 10.0);

        // Clean up
        env::remove_var("SIMPLEBENCH_SAMPLES");
        env::remove_var("SIMPLEBENCH_WARMUP_DURATION");
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
        assert_eq!(config.measurement.warmup_duration_secs, 3);
        assert!(!config.comparison.ci_mode);
    }
}
