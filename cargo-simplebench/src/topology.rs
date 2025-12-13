//! CPU topology detection for parallel benchmark execution
//!
//! This module detects physical CPU cores and returns a list of usable cores,
//! excluding core 0 which is reserved for system processes.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Get a list of usable CPU cores for benchmark execution.
///
/// Returns one logical CPU per physical core, excluding core 0.
/// Falls back to `vec![1]` if detection fails.
pub fn get_usable_cores() -> Vec<usize> {
    match detect_physical_cores() {
        Ok(cores) => {
            if cores.is_empty() {
                vec![1] // Fallback
            } else {
                cores
            }
        }
        Err(_) => vec![1], // Fallback on any error
    }
}

/// Detect physical cores by reading sysfs topology information.
///
/// On Linux, reads /sys/devices/system/cpu/cpuN/topology/thread_siblings_list
/// to identify which logical CPUs share a physical core (hyperthreading).
/// Returns one CPU per physical core, excluding core 0.
fn detect_physical_cores() -> Result<Vec<usize>, std::io::Error> {
    let cpu_base = Path::new("/sys/devices/system/cpu");

    if !cpu_base.exists() {
        // Not Linux or sysfs not available
        return Ok(vec![1]);
    }

    // Track which physical cores we've already selected a CPU from
    let mut seen_siblings: HashSet<String> = HashSet::new();
    let mut usable_cores: Vec<usize> = Vec::new();

    // Enumerate CPUs (cpu0, cpu1, ...)
    let mut cpu_dirs: Vec<_> = fs::read_dir(cpu_base)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            name_str.starts_with("cpu") && name_str[3..].chars().all(|c| c.is_ascii_digit())
        })
        .collect();

    // Sort by CPU number
    cpu_dirs.sort_by_key(|entry| {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        name_str[3..].parse::<usize>().unwrap_or(0)
    });

    for entry in cpu_dirs {
        let cpu_name = entry.file_name();
        let cpu_name_str = cpu_name.to_string_lossy();
        let cpu_num: usize = match cpu_name_str[3..].parse() {
            Ok(n) => n,
            Err(_) => continue,
        };

        // Skip core 0 - reserved for system
        if cpu_num == 0 {
            continue;
        }

        // Read thread_siblings_list to identify physical core
        let siblings_path = entry.path().join("topology/thread_siblings_list");
        let siblings = match fs::read_to_string(&siblings_path) {
            Ok(s) => s.trim().to_string(),
            Err(_) => continue, // Skip if can't read topology
        };

        // If we haven't seen this sibling group, add this CPU
        if !seen_siblings.contains(&siblings) {
            seen_siblings.insert(siblings);
            usable_cores.push(cpu_num);
        }
    }

    // Sort for consistent ordering
    usable_cores.sort();

    Ok(usable_cores)
}

/// Get the total number of logical CPUs available.
#[allow(dead_code)]
pub fn get_cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_usable_cores_not_empty() {
        let cores = get_usable_cores();
        assert!(!cores.is_empty(), "Should return at least one core");
    }

    #[test]
    fn test_get_usable_cores_excludes_zero() {
        let cores = get_usable_cores();
        assert!(
            !cores.contains(&0),
            "Should not include core 0 (reserved for system)"
        );
    }

    #[test]
    fn test_get_usable_cores_sorted() {
        let cores = get_usable_cores();
        let mut sorted = cores.clone();
        sorted.sort();
        assert_eq!(cores, sorted, "Cores should be in sorted order");
    }
}
