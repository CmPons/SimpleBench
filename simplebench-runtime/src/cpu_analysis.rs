//! CPU analysis for detecting thermal throttling, frequency variance, and cold starts

use crate::CpuSnapshot;

#[derive(Debug, Clone)]
pub struct FrequencyStats {
    pub min_mhz: f64,
    pub max_mhz: f64,
    pub mean_mhz: f64,
    pub stddev_mhz: f64,
    pub variance_percent: f64, // (max - min) / mean * 100
}

#[derive(Debug, Clone)]
pub struct TemperatureStats {
    pub min_celsius: f64,
    pub max_celsius: f64,
    pub mean_celsius: f64,
    pub increase_celsius: f64, // max - min
}

#[derive(Debug, Clone)]
pub enum CpuWarning {
    ColdStart {
        initial_temp_celsius: f64,
    },
    ThermalThrottling {
        temp_increase_celsius: f64,
        max_temp_celsius: f64,
    },
    FrequencyVariance {
        variance_percent: f64,
    },
    LowFrequency {
        mean_mhz: f64,
        max_available_mhz: f64,
        percent_of_max: f64,
    },
}

impl CpuWarning {
    pub fn format(&self) -> String {
        match self {
            CpuWarning::ColdStart {
                initial_temp_celsius,
            } => {
                format!(
                    "⚠ Cold start detected (initial: {:.0}°C)",
                    initial_temp_celsius
                )
            }
            CpuWarning::ThermalThrottling {
                temp_increase_celsius,
                max_temp_celsius,
            } => {
                format!(
                    "⚠ Thermal throttling detected (+{:.0}°C, max: {:.0}°C)",
                    temp_increase_celsius, max_temp_celsius
                )
            }
            CpuWarning::FrequencyVariance { variance_percent } => {
                format!(
                    "⚠ Frequency variance detected ({:.1}% variance)",
                    variance_percent
                )
            }
            CpuWarning::LowFrequency {
                mean_mhz,
                max_available_mhz,
                percent_of_max,
            } => {
                format!(
                    "⚠ Low frequency detected ({:.0} MHz, {:.0}% of max {:.0} MHz)",
                    mean_mhz, percent_of_max, max_available_mhz
                )
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CpuAnalysis {
    pub frequency_stats: Option<FrequencyStats>,
    pub temperature_stats: Option<TemperatureStats>,
    pub warnings: Vec<CpuWarning>,
}

impl CpuAnalysis {
    /// Analyze CPU snapshots and detect anomalies
    pub fn from_snapshots(snapshots: &[CpuSnapshot], max_freq_khz: Option<u64>) -> Self {
        let mut warnings = Vec::new();

        // Collect frequency data
        let frequencies: Vec<f64> = snapshots.iter().filter_map(|s| s.frequency_mhz()).collect();

        // Collect temperature data
        let temperatures: Vec<f64> = snapshots
            .iter()
            .filter_map(|s| s.temperature_celsius())
            .collect();

        // Calculate frequency statistics
        let frequency_stats = if !frequencies.is_empty() {
            let min_mhz = frequencies.iter().copied().fold(f64::INFINITY, f64::min);
            let max_mhz = frequencies
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            let mean_mhz = frequencies.iter().sum::<f64>() / frequencies.len() as f64;

            // Calculate standard deviation
            let variance = frequencies
                .iter()
                .map(|&f| {
                    let diff = f - mean_mhz;
                    diff * diff
                })
                .sum::<f64>()
                / frequencies.len() as f64;
            let stddev_mhz = variance.sqrt();

            // Variance as percentage of mean
            let variance_percent = if mean_mhz > 0.0 {
                ((max_mhz - min_mhz) / mean_mhz) * 100.0
            } else {
                0.0
            };

            // Detect frequency variance (>10%)
            if variance_percent > 10.0 {
                warnings.push(CpuWarning::FrequencyVariance { variance_percent });
            }

            // Detect low frequency (<50% of max)
            if let Some(max_freq_khz) = max_freq_khz {
                let max_available_mhz = max_freq_khz as f64 / 1000.0;
                let percent_of_max = (mean_mhz / max_available_mhz) * 100.0;

                if percent_of_max < 50.0 {
                    warnings.push(CpuWarning::LowFrequency {
                        mean_mhz,
                        max_available_mhz,
                        percent_of_max,
                    });
                }
            }

            Some(FrequencyStats {
                min_mhz,
                max_mhz,
                mean_mhz,
                stddev_mhz,
                variance_percent,
            })
        } else {
            None
        };

        // Calculate temperature statistics
        let temperature_stats = if !temperatures.is_empty() {
            let min_celsius = temperatures.iter().copied().fold(f64::INFINITY, f64::min);
            let max_celsius = temperatures
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            let mean_celsius = temperatures.iter().sum::<f64>() / temperatures.len() as f64;
            let increase_celsius = max_celsius - min_celsius;

            // Detect cold start (initial temp <50°C)
            if let Some(&initial_temp) = temperatures.first() {
                if initial_temp < 50.0 {
                    warnings.push(CpuWarning::ColdStart {
                        initial_temp_celsius: initial_temp,
                    });
                }
            }

            // Detect thermal throttling (>20°C increase or >85°C max)
            if increase_celsius > 20.0 || max_celsius > 85.0 {
                warnings.push(CpuWarning::ThermalThrottling {
                    temp_increase_celsius: increase_celsius,
                    max_temp_celsius: max_celsius,
                });
            }

            Some(TemperatureStats {
                min_celsius,
                max_celsius,
                mean_celsius,
                increase_celsius,
            })
        } else {
            None
        };

        CpuAnalysis {
            frequency_stats,
            temperature_stats,
            warnings,
        }
    }

    /// Format stats as a single-line string
    pub fn format_stats_line(&self) -> Option<String> {
        let mut parts = Vec::new();

        if let Some(ref freq) = self.frequency_stats {
            parts.push(format!(
                "{:.0}-{:.0} MHz (mean: {:.0} MHz, variance: {:.1}%)",
                freq.min_mhz, freq.max_mhz, freq.mean_mhz, freq.variance_percent
            ));
        }

        if let Some(ref temp) = self.temperature_stats {
            parts.push(format!(
                "{:.0}-{:.0}°C (increase: +{:.0}°C)",
                temp.min_celsius, temp.max_celsius, temp.increase_celsius
            ));
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_frequency_analysis() {
        let snapshots = vec![
            CpuSnapshot {
                timestamp: Instant::now(),
                frequency_khz: Some(4_000_000), // 4000 MHz
                temperature_millic: None,
            },
            CpuSnapshot {
                timestamp: Instant::now(),
                frequency_khz: Some(4_500_000), // 4500 MHz
                temperature_millic: None,
            },
            CpuSnapshot {
                timestamp: Instant::now(),
                frequency_khz: Some(4_600_000), // 4600 MHz
                temperature_millic: None,
            },
        ];

        let analysis = CpuAnalysis::from_snapshots(&snapshots, Some(5_000_000));

        assert!(analysis.frequency_stats.is_some());
        let freq_stats = analysis.frequency_stats.unwrap();
        assert_eq!(freq_stats.min_mhz, 4000.0);
        assert_eq!(freq_stats.max_mhz, 4600.0);
        assert!((freq_stats.mean_mhz - 4366.67).abs() < 1.0);
    }

    #[test]
    fn test_cold_start_detection() {
        let snapshots = vec![
            CpuSnapshot {
                timestamp: Instant::now(),
                frequency_khz: None,
                temperature_millic: Some(45_000), // 45°C - cold start
            },
            CpuSnapshot {
                timestamp: Instant::now(),
                frequency_khz: None,
                temperature_millic: Some(55_000), // 55°C
            },
        ];

        let analysis = CpuAnalysis::from_snapshots(&snapshots, None);

        assert!(!analysis.warnings.is_empty());
        assert!(matches!(analysis.warnings[0], CpuWarning::ColdStart { .. }));
    }

    #[test]
    fn test_frequency_variance_detection() {
        let snapshots = vec![
            CpuSnapshot {
                timestamp: Instant::now(),
                frequency_khz: Some(2_000_000), // 2000 MHz
                temperature_millic: None,
            },
            CpuSnapshot {
                timestamp: Instant::now(),
                frequency_khz: Some(4_500_000), // 4500 MHz - large variance
                temperature_millic: None,
            },
        ];

        let analysis = CpuAnalysis::from_snapshots(&snapshots, None);

        assert!(!analysis.warnings.is_empty());
        let has_variance_warning = analysis
            .warnings
            .iter()
            .any(|w| matches!(w, CpuWarning::FrequencyVariance { .. }));
        assert!(has_variance_warning);
    }

    #[test]
    fn test_thermal_throttling_detection() {
        let snapshots = vec![
            CpuSnapshot {
                timestamp: Instant::now(),
                frequency_khz: None,
                temperature_millic: Some(60_000), // 60°C
            },
            CpuSnapshot {
                timestamp: Instant::now(),
                frequency_khz: None,
                temperature_millic: Some(90_000), // 90°C - throttling
            },
        ];

        let analysis = CpuAnalysis::from_snapshots(&snapshots, None);

        assert!(!analysis.warnings.is_empty());
        let has_throttling_warning = analysis
            .warnings
            .iter()
            .any(|w| matches!(w, CpuWarning::ThermalThrottling { .. }));
        assert!(has_throttling_warning);
    }
}
