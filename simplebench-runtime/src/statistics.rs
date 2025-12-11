/// Statistical functions for baseline comparison and regression detection
///
/// This module provides core statistical operations used by both the statistical
/// window approach and Bayesian change point detection.

/// Calculate the arithmetic mean of a slice of values
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Calculate the variance of a slice of values
pub fn variance(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }

    let m = mean(values);
    values.iter().map(|&x| (x - m).powi(2)).sum::<f64>() / values.len() as f64
}

/// Calculate the standard deviation of a slice of values
pub fn standard_deviation(values: &[f64]) -> f64 {
    variance(values).sqrt()
}

/// Calculate the z-score: how many standard deviations a value is from the mean
pub fn z_score(value: f64, mean: f64, stddev: f64) -> f64 {
    if stddev < 1e-10 {
        // Avoid division by zero for very low variance
        return 0.0;
    }
    (value - mean) / stddev
}

/// Calculate confidence interval bounds
///
/// Returns (lower_bound, upper_bound) for the given confidence level.
/// Common confidence levels:
/// - 0.90 = 90% confidence (z = 1.645 one-tailed, 1.96 two-tailed)
/// - 0.95 = 95% confidence (z = 1.645 one-tailed, 1.96 two-tailed)
/// - 0.99 = 99% confidence (z = 2.326 one-tailed, 2.576 two-tailed)
pub fn confidence_interval(
    mean: f64,
    stddev: f64,
    confidence_level: f64,
) -> (f64, f64) {
    // Map confidence level to z-critical value (one-tailed for regression detection)
    let z_critical = if (confidence_level - 0.90).abs() < 0.01 {
        1.282  // 90% one-tailed
    } else if (confidence_level - 0.95).abs() < 0.01 {
        1.645  // 95% one-tailed
    } else if (confidence_level - 0.99).abs() < 0.01 {
        2.326  // 99% one-tailed
    } else {
        // Default to 95% two-tailed for other values
        1.96
    };

    let margin = z_critical * stddev;
    (mean - margin, mean + margin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mean() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(mean(&values), 3.0);

        let empty: Vec<f64> = vec![];
        assert_eq!(mean(&empty), 0.0);
    }

    #[test]
    fn test_variance() {
        let values = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let var = variance(&values);
        // Expected variance: 4.0
        assert!((var - 4.0).abs() < 0.01);
    }

    #[test]
    fn test_standard_deviation() {
        let values = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let std = standard_deviation(&values);
        // Expected stddev: 2.0
        assert!((std - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_z_score() {
        let z = z_score(10.0, 5.0, 2.0);
        assert_eq!(z, 2.5);

        // Test with zero stddev (edge case)
        let z_zero = z_score(10.0, 5.0, 0.0);
        assert_eq!(z_zero, 0.0);
    }

    #[test]
    fn test_confidence_interval() {
        let (lower, upper) = confidence_interval(100.0, 10.0, 0.95);
        // 95% CI should be approximately [83.55, 116.45] for one-tailed
        assert!((lower - 83.55).abs() < 1.0);
        assert!((upper - 116.45).abs() < 1.0);
    }
}
