//! Discount curve handling for reserve calculations
//!
//! Supports:
//! - Single valuation rate (standard CARVM)
//! - Separate rates for death benefits vs elective benefits
//! - Full spot rate curves (for advanced calculations)

use serde::{Deserialize, Serialize};

/// Discount curve for reserve calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscountCurve {
    /// Primary valuation interest rate (annual)
    /// Used for elective benefits (income, surrender)
    pub valuation_rate: f64,

    /// Optional: separate rate for death benefits (annual)
    /// If None, uses valuation_rate
    pub death_benefit_rate: Option<f64>,

    /// Optional: spot rate curve for more precise discounting
    /// Index = month, value = annual spot rate to that point
    pub spot_rates: Option<Vec<f64>>,
}

impl DiscountCurve {
    /// Create a simple discount curve with single rate
    pub fn single_rate(annual_rate: f64) -> Self {
        Self {
            valuation_rate: annual_rate,
            death_benefit_rate: None,
            spot_rates: None,
        }
    }

    /// Create discount curve with separate death benefit rate
    pub fn with_death_benefit_rate(valuation_rate: f64, death_benefit_rate: f64) -> Self {
        Self {
            valuation_rate,
            death_benefit_rate: Some(death_benefit_rate),
            spot_rates: None,
        }
    }

    /// Create discount curve from spot rate curve
    pub fn from_spot_curve(spot_rates: Vec<f64>) -> Self {
        let valuation_rate = spot_rates.first().copied().unwrap_or(0.0);
        Self {
            valuation_rate,
            death_benefit_rate: None,
            spot_rates: Some(spot_rates),
        }
    }

    /// Get monthly discount factor for elective benefits
    pub fn elective_discount_factor(&self) -> f64 {
        1.0 / (1.0 + self.valuation_rate / 12.0)
    }

    /// Get monthly discount factor for death benefits
    pub fn death_benefit_discount_factor(&self) -> f64 {
        let rate = self.death_benefit_rate.unwrap_or(self.valuation_rate);
        1.0 / (1.0 + rate / 12.0)
    }

    /// Calculate discount factor to a specific month for elective benefits
    pub fn discount_to_month_elective(&self, months: u32) -> f64 {
        if let Some(ref spots) = self.spot_rates {
            if (months as usize) < spots.len() {
                let spot = spots[months as usize];
                return (1.0 + spot).powf(-(months as f64) / 12.0);
            }
        }

        self.elective_discount_factor().powi(months as i32)
    }

    /// Calculate discount factor to a specific month for death benefits
    pub fn discount_to_month_death(&self, months: u32) -> f64 {
        self.death_benefit_discount_factor().powi(months as i32)
    }

    /// Calculate present value of a stream of elective benefits
    pub fn pv_elective_stream(&self, benefits: &[(u32, f64)]) -> f64 {
        benefits
            .iter()
            .map(|(month, amount)| amount * self.discount_to_month_elective(*month))
            .sum()
    }

    /// Calculate present value of a stream of death benefits
    /// Takes (month, probability, amount) tuples
    pub fn pv_death_benefit_stream(&self, benefits: &[(u32, f64, f64)]) -> f64 {
        benefits
            .iter()
            .map(|(month, prob, amount)| prob * amount * self.discount_to_month_death(*month))
            .sum()
    }
}

impl Default for DiscountCurve {
    fn default() -> Self {
        Self::single_rate(0.0475) // 4.75% default valuation rate
    }
}

/// Helper functions for present value calculations
pub struct PVCalculator;

impl PVCalculator {
    /// Calculate PV of a level annuity
    /// Payments of `amount` for `n_months`, first payment immediate
    pub fn pv_annuity_due(amount: f64, n_months: u32, monthly_rate: f64) -> f64 {
        if monthly_rate.abs() < 1e-10 {
            return amount * n_months as f64;
        }

        let v = 1.0 / (1.0 + monthly_rate);
        amount * (1.0 - v.powi(n_months as i32)) / (1.0 - v)
    }

    /// Calculate PV of a level annuity (ordinary - payments at end of period)
    pub fn pv_annuity_ordinary(amount: f64, n_months: u32, monthly_rate: f64) -> f64 {
        Self::pv_annuity_due(amount, n_months, monthly_rate) / (1.0 + monthly_rate)
    }

    /// Calculate PV of a life annuity with mortality
    /// Takes vector of (month, survival_prob, payment)
    pub fn pv_life_annuity(
        payments: &[(u32, f64, f64)],
        discount_curve: &DiscountCurve,
    ) -> f64 {
        payments
            .iter()
            .map(|(month, surv_prob, payment)| {
                surv_prob * payment * discount_curve.discount_to_month_elective(*month)
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_rate_curve() {
        let curve = DiscountCurve::single_rate(0.05);
        assert!((curve.valuation_rate - 0.05).abs() < 1e-10);
        assert!(curve.death_benefit_rate.is_none());
    }

    #[test]
    fn test_discount_factors() {
        let curve = DiscountCurve::single_rate(0.06); // 6% annual

        let monthly_v = curve.elective_discount_factor();
        let expected = 1.0 / (1.0 + 0.06 / 12.0);
        assert!((monthly_v - expected).abs() < 1e-10);

        let v_12 = curve.discount_to_month_elective(12);
        let expected_12: f64 = (1.0_f64 / (1.0 + 0.06 / 12.0)).powi(12);
        assert!((v_12 - expected_12).abs() < 1e-10);
    }

    #[test]
    fn test_separate_death_rate() {
        let curve = DiscountCurve::with_death_benefit_rate(0.05, 0.04);

        let elective_v = curve.elective_discount_factor();
        let death_v = curve.death_benefit_discount_factor();

        assert!(death_v > elective_v); // Lower rate = higher discount factor
    }

    #[test]
    fn test_pv_annuity() {
        // $100/month for 12 months at 6% annual
        let pv = PVCalculator::pv_annuity_ordinary(100.0, 12, 0.06 / 12.0);

        // Expected: 100 * (1 - 1.005^-12) / 0.005 ≈ 1162.62
        assert!((pv - 1162.62).abs() < 1.0);
    }
}
