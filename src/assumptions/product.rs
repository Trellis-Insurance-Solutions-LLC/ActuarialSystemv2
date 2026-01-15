//! Product features including surrender charges, payout factors, and rider terms

use std::collections::HashMap;

/// Surrender charge schedule by policy year
#[derive(Debug, Clone)]
pub struct SurrenderChargeSchedule {
    /// Surrender charge rates by policy year (1-indexed)
    charges: Vec<f64>,
}

impl SurrenderChargeSchedule {
    /// Create from loaded CSV data
    pub fn from_loaded(charges: &[f64]) -> Self {
        Self {
            charges: charges.to_vec(),
        }
    }

    /// Create default 10-year surrender charge schedule
    pub fn default_10_year() -> Self {
        Self {
            // Year 1-10 charges, year 11+ is 0
            charges: vec![
                0.09, // Year 1
                0.09, // Year 2
                0.08, // Year 3
                0.07, // Year 4
                0.06, // Year 5
                0.05, // Year 6
                0.04, // Year 7
                0.03, // Year 8
                0.02, // Year 9
                0.01, // Year 10
            ],
        }
    }

    /// Get surrender charge rate for a given policy year
    pub fn get_rate(&self, policy_year: u32) -> f64 {
        if policy_year == 0 {
            return self.charges.first().copied().unwrap_or(0.0);
        }
        let idx = (policy_year as usize).saturating_sub(1);
        self.charges.get(idx).copied().unwrap_or(0.0)
    }

    /// Check if still in surrender charge period
    pub fn in_sc_period(&self, policy_year: u32) -> bool {
        self.get_rate(policy_year) > 0.0
    }

    /// Get the total SC period length in years
    pub fn sc_period_years(&self) -> u32 {
        self.charges.len() as u32
    }
}

/// GLWB payout factors by attained age
#[derive(Debug, Clone)]
pub struct PayoutFactors {
    /// Single life payout factors by age band
    single_life: HashMap<(u8, u8), f64>,
    /// Joint life payout factors by age band (optional)
    joint_life: Option<HashMap<(u8, u8), f64>>,
}

impl PayoutFactors {
    /// Create from loaded CSV data (HashMap<age, factor>)
    pub fn from_loaded(factors: &std::collections::HashMap<u8, f64>) -> Self {
        // Convert direct age->factor mapping to age bands
        // For now, store as single-year bands
        let mut single_life = HashMap::new();
        for (&age, &factor) in factors {
            single_life.insert((age, age), factor);
        }
        Self {
            single_life,
            joint_life: None,
        }
    }

    /// Create default payout factors from product features
    pub fn default() -> Self {
        let mut single_life = HashMap::new();

        // Age bands and factors from Product features sheet
        single_life.insert((50, 55), 0.046);
        single_life.insert((56, 60), 0.050);
        single_life.insert((61, 65), 0.055);
        single_life.insert((66, 70), 0.060);
        single_life.insert((71, 75), 0.065);
        single_life.insert((76, 80), 0.070);
        single_life.insert((81, 85), 0.080);
        single_life.insert((86, 120), 0.090);

        Self {
            single_life,
            joint_life: None,
        }
    }

    /// Get single life payout factor for attained age
    pub fn get_single_life(&self, attained_age: u8) -> f64 {
        for ((min_age, max_age), factor) in &self.single_life {
            if attained_age >= *min_age && attained_age <= *max_age {
                return *factor;
            }
        }
        // Default to highest age band if beyond range
        0.090
    }

    /// Get joint life payout factor for attained age (if available)
    pub fn get_joint_life(&self, attained_age: u8) -> Option<f64> {
        self.joint_life.as_ref().and_then(|jl| {
            for ((min_age, max_age), factor) in jl {
                if attained_age >= *min_age && attained_age <= *max_age {
                    return Some(*factor);
                }
            }
            None
        })
    }
}

/// GLWB rider features
#[derive(Debug, Clone)]
pub struct GlwbFeatures {
    /// Minimum age for income activation
    pub min_activation_age: u8,

    /// Bonus percentage applied to initial premium for benefit base
    pub bonus_rate: f64,

    /// Annual rollup rate for benefit base
    pub rollup_rate: f64,

    /// Maximum years for rollup
    pub rollup_years: u8,

    /// Is rollup simple or compound interest
    pub simple_rollup: bool,

    /// Rider charge rate before income activation (annual)
    pub pre_activation_charge: f64,

    /// Rider charge rate after income activation (annual)
    pub post_activation_charge: f64,

    /// Payout factors by age
    pub payout_factors: PayoutFactors,
}

impl Default for GlwbFeatures {
    fn default() -> Self {
        Self {
            min_activation_age: 50,
            bonus_rate: 0.30,           // 30% bonus
            rollup_rate: 0.10,          // 10% annual rollup
            rollup_years: 10,           // 10 years of rollup
            simple_rollup: true,        // Simple interest
            pre_activation_charge: 0.005,  // 0.5% per annum
            post_activation_charge: 0.015, // 1.5% per annum
            payout_factors: PayoutFactors::default(),
        }
    }
}

impl GlwbFeatures {
    /// Calculate monthly rider charge rate based on activation status
    pub fn monthly_rider_charge(&self, income_activated: bool) -> f64 {
        let annual_rate = if income_activated {
            self.post_activation_charge
        } else {
            self.pre_activation_charge
        };
        annual_rate / 12.0
    }

    /// Calculate monthly rollup factor for benefit base
    /// Returns the factor to multiply benefit base by (> 1.0 means growth)
    pub fn monthly_rollup_factor(&self, policy_year: u32, income_activated: bool) -> f64 {
        // No rollup after income activation or beyond rollup period
        if income_activated || policy_year > self.rollup_years as u32 {
            return 1.0;
        }

        if self.simple_rollup {
            // Simple interest: add (rollup_rate / 12) of INITIAL benefit base each month
            // This is handled differently - return the monthly addition rate
            // For simple rollup, we track the monthly increment separately
            1.0 + self.rollup_rate / 12.0
        } else {
            // Compound interest: multiply by (1 + rate)^(1/12)
            (1.0 + self.rollup_rate).powf(1.0 / 12.0)
        }
    }

    /// Calculate maximum withdrawal amount for the year
    pub fn max_annual_withdrawal(&self, benefit_base: f64, attained_age: u8) -> f64 {
        let payout_rate = self.payout_factors.get_single_life(attained_age);
        benefit_base * payout_rate
    }
}

/// Base product features (non-rider)
#[derive(Debug, Clone)]
pub struct BaseProductFeatures {
    /// Surrender charge schedule
    pub surrender_charges: SurrenderChargeSchedule,

    /// Free partial withdrawal percentage per year
    pub free_withdrawal_pct: f64,

    /// Minimum premium
    pub min_premium: f64,

    /// Maximum premium
    pub max_premium: f64,

    /// Minimum issue age
    pub min_issue_age: u8,

    /// Maximum issue age
    pub max_issue_age: u8,
}

impl Default for BaseProductFeatures {
    fn default() -> Self {
        Self {
            surrender_charges: SurrenderChargeSchedule::default_10_year(),
            free_withdrawal_pct: 0.05, // 5% free withdrawal
            min_premium: 25_000.0,
            max_premium: 1_000_000.0,
            min_issue_age: 40,
            max_issue_age: 80,
        }
    }
}

/// Combined product features
#[derive(Debug, Clone)]
pub struct ProductFeatures {
    pub base: BaseProductFeatures,
    pub glwb: GlwbFeatures,
}

impl Default for ProductFeatures {
    fn default() -> Self {
        Self {
            base: BaseProductFeatures::default(),
            glwb: GlwbFeatures::default(),
        }
    }
}

impl ProductFeatures {
    /// Create from loaded CSV assumptions
    pub fn from_loaded(loaded: &super::loader::LoadedAssumptions) -> Self {
        let mut features = Self::default();
        features.base.surrender_charges = SurrenderChargeSchedule::from_loaded(&loaded.surrender_charges);
        features.glwb.payout_factors = PayoutFactors::from_loaded(&loaded.payout_factors);
        features
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surrender_charges() {
        let sc = SurrenderChargeSchedule::default_10_year();

        assert_eq!(sc.get_rate(1), 0.09);
        assert_eq!(sc.get_rate(5), 0.06);
        assert_eq!(sc.get_rate(10), 0.01);
        assert_eq!(sc.get_rate(11), 0.0);
        assert_eq!(sc.get_rate(20), 0.0);
    }

    #[test]
    fn test_payout_factors() {
        let pf = PayoutFactors::default();

        assert_eq!(pf.get_single_life(52), 0.046);
        assert_eq!(pf.get_single_life(65), 0.055);
        assert_eq!(pf.get_single_life(77), 0.070);
        assert_eq!(pf.get_single_life(90), 0.090);
    }

    #[test]
    fn test_glwb_rollup() {
        let glwb = GlwbFeatures::default();

        // During rollup period, not activated
        let factor = glwb.monthly_rollup_factor(1, false);
        assert!((factor - (1.0 + 0.10 / 12.0)).abs() < 1e-10);

        // After income activation - no rollup
        assert_eq!(glwb.monthly_rollup_factor(1, true), 1.0);

        // After rollup period - no rollup
        assert_eq!(glwb.monthly_rollup_factor(11, false), 1.0);
    }
}
