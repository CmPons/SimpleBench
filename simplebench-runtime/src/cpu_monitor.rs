//! CPU monitoring for Linux systems
//!
//! Provides CPU frequency and thermal monitoring on Linux via sysfs.
//! Gracefully degrades on non-Linux platforms.

use std::fs;
use std::time::Instant;

/// Monitor for a specific CPU core
pub struct CpuMonitor {
    cpu_core: usize,
    thermal_zone: Option<usize>,
}

impl CpuMonitor {
    /// Create monitor for specific CPU core
    pub fn new(cpu_core: usize) -> Self {
        let thermal_zone = Self::discover_thermal_zones().first().copied();
        Self {
            cpu_core,
            thermal_zone,
        }
    }

    /// Read current frequency in kHz (returns None if unavailable)
    pub fn read_frequency(&self) -> Option<u64> {
        #[cfg(target_os = "linux")]
        {
            let path = format!(
                "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq",
                self.cpu_core
            );
            fs::read_to_string(path)
                .ok()
                .and_then(|s| s.trim().parse().ok())
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// Read current governor (returns None if unavailable)
    pub fn read_governor(&self) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            let path = format!(
                "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor",
                self.cpu_core
            );
            fs::read_to_string(path).ok().map(|s| s.trim().to_string())
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// Read frequency range (min, max in kHz)
    pub fn read_frequency_range(&self) -> Option<(u64, u64)> {
        #[cfg(target_os = "linux")]
        {
            let min_path = format!(
                "/sys/devices/system/cpu/cpu{}/cpufreq/cpuinfo_min_freq",
                self.cpu_core
            );
            let max_path = format!(
                "/sys/devices/system/cpu/cpu{}/cpufreq/cpuinfo_max_freq",
                self.cpu_core
            );

            let min = fs::read_to_string(min_path)
                .ok()
                .and_then(|s| s.trim().parse().ok())?;
            let max = fs::read_to_string(max_path)
                .ok()
                .and_then(|s| s.trim().parse().ok())?;

            Some((min, max))
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// Read current temperature in millidegrees Celsius (returns None if unavailable)
    pub fn read_temperature(&self) -> Option<i32> {
        #[cfg(target_os = "linux")]
        {
            let zone = self.thermal_zone?;
            let path = format!("/sys/class/thermal/thermal_zone{}/temp", zone);
            fs::read_to_string(path)
                .ok()
                .and_then(|s| s.trim().parse().ok())
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// Find available thermal zones (returns zone indices)
    pub fn discover_thermal_zones() -> Vec<usize> {
        #[cfg(target_os = "linux")]
        {
            (0..20)
                .filter(|&i| {
                    let path = format!("/sys/class/thermal/thermal_zone{}/temp", i);
                    fs::metadata(path).is_ok()
                })
                .collect()
        }
        #[cfg(not(target_os = "linux"))]
        {
            Vec::new()
        }
    }
}

/// Snapshot of CPU state at a point in time
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CpuSnapshot {
    #[serde(skip, default = "Instant::now")]
    pub timestamp: Instant,
    pub frequency_khz: Option<u64>,
    pub temperature_millic: Option<i32>,
}

impl CpuSnapshot {
    /// Get frequency in MHz
    pub fn frequency_mhz(&self) -> Option<f64> {
        self.frequency_khz.map(|khz| khz as f64 / 1000.0)
    }

    /// Get temperature in Celsius
    pub fn temperature_celsius(&self) -> Option<f64> {
        self.temperature_millic.map(|millic| millic as f64 / 1000.0)
    }
}

impl Default for CpuSnapshot {
    fn default() -> Self {
        Self {
            timestamp: Instant::now(),
            frequency_khz: None,
            temperature_millic: None,
        }
    }
}

/// Verify and report benchmark environment
pub fn verify_benchmark_environment(cpu_core: usize) {
    use colored::*;

    println!("{}", "Verifying benchmark environment...".green().bold());

    #[cfg(target_os = "linux")]
    {
        println!("  {} {}", "Platform:".dimmed(), "Linux (full monitoring support)".cyan());

        let monitor = CpuMonitor::new(cpu_core);

        // Check governor
        if let Some(governor) = monitor.read_governor() {
            print!("  {} {} {}", "CPU".dimmed(), cpu_core, "governor:".dimmed());
            if governor == "performance" {
                println!(" {}", governor.green());
            } else {
                println!(" {}", governor.yellow());
                println!("    {} {}", "⚠".yellow(), "Not using 'performance' governor".yellow());
                println!("    {} {}", "Consider:".dimmed(), "sudo cpupower frequency-set -g performance".dimmed());
            }
        }

        // Check frequency range
        if let Some((min_khz, max_khz)) = monitor.read_frequency_range() {
            println!(
                "  {} {} {} {} {} {} {}",
                "CPU".dimmed(),
                cpu_core,
                "frequency range:".dimmed(),
                (min_khz / 1000).to_string().cyan(),
                "MHz -".dimmed(),
                (max_khz / 1000).to_string().cyan(),
                "MHz".dimmed()
            );
        }

        // Check current frequency
        if let Some(freq_khz) = monitor.read_frequency() {
            println!(
                "  {} {} {} {} {}",
                "CPU".dimmed(),
                cpu_core,
                "current frequency:".dimmed(),
                (freq_khz / 1000).to_string().cyan(),
                "MHz".dimmed()
            );
        }

        // Check thermal zones
        let zones = CpuMonitor::discover_thermal_zones();
        if !zones.is_empty() {
            println!("  {} {} {}", "Found".dimmed(), zones.len().to_string().cyan(), "thermal zone(s)".dimmed());
            for zone in zones.iter().take(3) {
                let path = format!("/sys/class/thermal/thermal_zone{}/temp", zone);
                if let Ok(temp_str) = fs::read_to_string(path) {
                    if let Ok(temp_millic) = temp_str.trim().parse::<i32>() {
                        println!("    {} {} {}°C", "Zone".dimmed(), zone, (temp_millic / 1000).to_string().cyan());
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let os = std::env::consts::OS;
        println!("  {} {} {}", "Platform:".dimmed(), os.cyan(), "(limited monitoring support)".dimmed());
        println!("    {} {}", "ℹ".blue(), "CPU frequency/thermal monitoring not available on this platform".dimmed());
    }

    println!("{}\n", "Environment check complete.".green());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_monitor_creation() {
        let monitor = CpuMonitor::new(0);
        // Should not panic on any platform
        let _ = monitor.read_frequency();
        let _ = monitor.read_governor();
        let _ = monitor.read_frequency_range();
        let _ = monitor.read_temperature();
    }

    #[test]
    fn test_thermal_zone_discovery() {
        let zones = CpuMonitor::discover_thermal_zones();
        // On Linux, should find at least one zone (usually)
        // On other platforms, should return empty vec
        #[cfg(target_os = "linux")]
        {
            // May or may not find zones depending on system
            println!("Found {} thermal zones", zones.len());
        }
        #[cfg(not(target_os = "linux"))]
        {
            assert!(zones.is_empty());
        }
    }

    #[test]
    fn test_cpu_snapshot() {
        let snapshot = CpuSnapshot {
            timestamp: Instant::now(),
            frequency_khz: Some(4500000),
            temperature_millic: Some(55000),
        };

        assert_eq!(snapshot.frequency_mhz(), Some(4500.0));
        assert_eq!(snapshot.temperature_celsius(), Some(55.0));
    }

    #[test]
    fn test_verify_environment() {
        // Should not panic on any platform
        verify_benchmark_environment(0);
    }
}
