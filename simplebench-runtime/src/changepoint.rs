/// Bayesian Online Change Point Detection
///
/// Based on Adams & MacKay (2007) "Bayesian Online Changepoint Detection"
/// Simplified implementation for detecting performance regressions in benchmark data.

use crate::statistics::mean;

/// Bayesian Online Change Point Detection core algorithm
pub struct BayesianCPD {
    /// Prior probability of a change point occurring (hazard rate)
    hazard_rate: f64,
}

impl BayesianCPD {
    /// Create a new Bayesian CPD detector
    ///
    /// # Arguments
    /// * `hazard_rate` - Prior probability of change point (e.g., 0.1 = expect change every 10 runs)
    pub fn new(hazard_rate: f64) -> Self {
        Self {
            hazard_rate,
        }
    }

    /// Update with a new observation and return change point probability
    ///
    /// # Arguments
    /// * `value` - New observation (e.g., current benchmark mean)
    /// * `historical` - Historical observations (e.g., previous benchmark means)
    ///
    /// # Returns
    /// Probability that a change point occurred (0.0 to 1.0)
    pub fn update(&mut self, value: f64, historical: &[f64]) -> f64 {
        if historical.is_empty() {
            return 0.0;
        }

        // Calculate predictive probability using all historical data
        let likelihood = self.student_t_likelihood(value, historical);

        // For simplified implementation: compare new value against historical distribution
        // High likelihood = fits the pattern (low change probability)
        // Low likelihood = doesn't fit (high change probability)

        // Convert likelihood to change probability (invert the likelihood)
        // If value is very different from historical mean, likelihood will be low
        let unlikelihood = 1.0 - likelihood.min(1.0);

        // Combine data evidence with prior (hazard rate)
        // Bayesian update: P(CP|data) ∝ P(data|CP) * P(CP)
        // Where P(CP) = hazard_rate
        // And P(data|CP) = 1 - likelihood (assuming uniform prior under change)

        // Weight the evidence by the hazard rate as a prior
        let prior_weight = self.hazard_rate;
        let evidence_weight = unlikelihood;

        // Combine: higher hazard rate gives more weight to the prior expectation of change
        let change_prob = (evidence_weight * 0.7) + (prior_weight * 0.3) + (evidence_weight * prior_weight * 0.5);

        change_prob.min(1.0)
    }

    /// Calculate Student's t-distribution likelihood
    ///
    /// This is used as the predictive distribution for new observations.
    /// Student's t is more robust to outliers than normal distribution.
    fn student_t_likelihood(&self, value: f64, historical: &[f64]) -> f64 {
        if historical.is_empty() {
            return 0.5;
        }

        let n = historical.len() as f64;
        let hist_mean = mean(historical);
        let variance = if n > 1.0 {
            historical
                .iter()
                .map(|&x| (x - hist_mean).powi(2))
                .sum::<f64>()
                / n
        } else {
            1.0 // Default variance for single observation
        };

        if variance < 1e-10 {
            // Very low variance, use normal approximation
            let stddev = variance.sqrt().max(1e-5);
            let z = ((value - hist_mean) / stddev).abs();
            return (-0.5 * z * z).exp();
        }

        // Student's t-distribution with n-1 degrees of freedom
        let df = (n - 1.0).max(1.0);
        let t = (value - hist_mean) / variance.sqrt();
        let t_squared = t * t;

        // Simplified Student's t PDF (good enough for our purposes)
        let coef = (1.0 + t_squared / df).powf(-(df + 1.0) / 2.0);
        coef
    }
}

/// Simplified API: calculate change point probability for a new value
///
/// This is a convenience function that creates a new BayesianCPD instance
/// and calculates the change point probability in a single call.
///
/// # Arguments
/// * `new_value` - Current observation (e.g., current benchmark mean)
/// * `historical` - Historical observations (e.g., previous benchmark means)
/// * `hazard_rate` - Prior probability of change point
///
/// # Returns
/// Probability that a change point occurred (0.0 to 1.0)
pub fn bayesian_change_point_probability(
    new_value: f64,
    historical: &[f64],
    hazard_rate: f64,
) -> f64 {
    let mut cpd = BayesianCPD::new(hazard_rate);
    cpd.update(new_value, historical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stable_data_low_change_probability() {
        // Stable data should have low change probability
        let historical = vec![1.0, 1.01, 0.99, 1.0, 1.02, 0.98, 1.0, 1.01];
        let new_value = 1.0;
        let prob = bayesian_change_point_probability(new_value, &historical, 0.1);

        // Should be low probability since new value fits the pattern
        assert!(prob < 0.5, "Expected low change probability, got {}", prob);
    }

    #[test]
    fn test_clear_change_point() {
        // Historical data around 1.0, then sudden jump to 2.0
        let historical = vec![1.0, 1.01, 0.99, 1.0, 1.02, 0.98, 1.0, 1.01];
        let new_value = 2.0;
        let prob = bayesian_change_point_probability(new_value, &historical, 0.1);

        // Should be high probability since new value is very different
        assert!(prob > 0.3, "Expected high change probability, got {}", prob);
    }

    #[test]
    fn test_empty_historical() {
        let historical: Vec<f64> = vec![];
        let new_value = 1.0;
        let prob = bayesian_change_point_probability(new_value, &historical, 0.1);

        // Should return 0.0 for empty history
        assert_eq!(prob, 0.0);
    }

    #[test]
    fn test_gradual_drift() {
        // Gradual increase should show lower change probability than sudden jump
        let historical = vec![1.0, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3];
        let new_value = 1.35;
        let prob_gradual = bayesian_change_point_probability(new_value, &historical, 0.1);

        let historical_stable = vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let new_value_jump = 1.35;
        let prob_sudden = bayesian_change_point_probability(new_value_jump, &historical_stable, 0.1);

        // Sudden jump should have higher change probability than gradual drift
        assert!(prob_sudden > prob_gradual,
            "Sudden jump ({}) should have higher change probability than gradual drift ({})",
            prob_sudden, prob_gradual);
    }

    #[test]
    fn test_higher_hazard_rate_increases_change_probability() {
        let historical = vec![1.0, 1.0, 1.0, 1.0, 1.0];
        let new_value = 1.2;

        let prob_low_hazard = bayesian_change_point_probability(new_value, &historical, 0.05);
        let prob_high_hazard = bayesian_change_point_probability(new_value, &historical, 0.3);

        // Higher hazard rate should increase change point probability
        assert!(prob_high_hazard > prob_low_hazard,
            "Higher hazard rate ({}) should produce higher change probability than lower hazard rate ({})",
            prob_high_hazard, prob_low_hazard);
    }
}
