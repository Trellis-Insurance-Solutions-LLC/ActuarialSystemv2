//! Policy data structures matching the pricing inforce format

use serde::{Deserialize, Serialize};

/// Default GLWB start year (99 = never activates)
fn default_glwb_start_year() -> u32 {
    99
}

/// Qualified status of the policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualStatus {
    /// Qualified (IRA, etc.)
    Q,
    /// Non-qualified
    N,
}

impl QualStatus {
    pub fn is_qualified(&self) -> bool {
        matches!(self, QualStatus::Q)
    }
}

/// Gender of the policyholder
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gender {
    Male,
    Female,
}

/// Crediting strategy for the annuity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreditingStrategy {
    /// Indexed crediting (S&P 500, etc.)
    Indexed,
    /// Fixed rate crediting
    Fixed,
}

/// Rollup type for benefit base
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollupType {
    /// Simple interest rollup
    Simple,
    /// Compound interest rollup
    Compound,
}

/// Benefit base bucket for lapse model segmentation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BenefitBaseBucket {
    /// [0, 50000)
    Under50k,
    /// [50000, 100000)
    From50kTo100k,
    /// [100000, 200000)
    From100kTo200k,
    /// [200000, 500000)
    From200kTo500k,
    /// [500000, Inf)
    Over500k,
}

impl BenefitBaseBucket {
    /// Determine bucket from benefit base amount
    pub fn from_amount(amount: f64) -> Self {
        if amount < 50_000.0 {
            BenefitBaseBucket::Under50k
        } else if amount < 100_000.0 {
            BenefitBaseBucket::From50kTo100k
        } else if amount < 200_000.0 {
            BenefitBaseBucket::From100kTo200k
        } else if amount < 500_000.0 {
            BenefitBaseBucket::From200kTo500k
        } else {
            BenefitBaseBucket::Over500k
        }
    }

    /// Get the string representation matching Excel format
    pub fn as_str(&self) -> &'static str {
        match self {
            BenefitBaseBucket::Under50k => "[0, 50000)",
            BenefitBaseBucket::From50kTo100k => "[50000, 100000)",
            BenefitBaseBucket::From100kTo200k => "[100000, 200000)",
            BenefitBaseBucket::From200kTo500k => "[200000, 500000)",
            BenefitBaseBucket::Over500k => "[500000, Inf)",
        }
    }
}

/// A single policy record from the pricing inforce
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    /// Unique policy identifier
    pub policy_id: u32,

    /// Qualified status (Q = qualified, N = non-qualified)
    pub qual_status: QualStatus,

    /// Issue age of the policyholder
    pub issue_age: u8,

    /// Gender of the policyholder
    pub gender: Gender,

    /// Initial benefit base at policy inception
    pub initial_benefit_base: f64,

    /// Initial number of policies (fractional for weighted cohorts)
    pub initial_pols: f64,

    /// Initial premium amount
    pub initial_premium: f64,

    /// Benefit base bucket for segmentation
    pub benefit_base_bucket: BenefitBaseBucket,

    /// Percentage weight within cohort
    pub percentage: f64,

    /// Crediting strategy (Indexed or Fixed)
    pub crediting_strategy: CreditingStrategy,

    /// Surrender charge period in years
    pub sc_period: u8,

    /// Valuation rate for discounting
    pub val_rate: f64,

    /// Minimum guaranteed interest rate
    pub mgir: f64,

    /// Bonus percentage on benefit base at inception
    pub bonus: f64,

    /// Rollup type (Simple or Compound)
    pub rollup_type: RollupType,

    /// Current policy duration in months (for seasoned policies)
    #[serde(default)]
    pub duration_months: u32,

    /// Whether GLWB income has been activated (for mid-projection starts)
    #[serde(default)]
    pub income_activated: bool,

    /// Policy year when GLWB income activates (99 = never)
    #[serde(default = "default_glwb_start_year")]
    pub glwb_start_year: u32,

    /// Current account value (for mid-projection starts)
    #[serde(default)]
    pub current_av: Option<f64>,

    /// Current benefit base (for mid-projection starts)
    #[serde(default)]
    pub current_benefit_base: Option<f64>,
}

impl Policy {
    /// Create a new policy with required fields
    pub fn new(
        policy_id: u32,
        qual_status: QualStatus,
        issue_age: u8,
        gender: Gender,
        initial_benefit_base: f64,
        initial_pols: f64,
        initial_premium: f64,
        crediting_strategy: CreditingStrategy,
        sc_period: u8,
        val_rate: f64,
        mgir: f64,
        bonus: f64,
        rollup_type: RollupType,
    ) -> Self {
        Self::with_glwb_start(
            policy_id, qual_status, issue_age, gender, initial_benefit_base,
            initial_pols, initial_premium, crediting_strategy, sc_period,
            val_rate, mgir, bonus, rollup_type, 99, // Default: never activates
        )
    }

    /// Create a new policy with GLWB activation year specified
    pub fn with_glwb_start(
        policy_id: u32,
        qual_status: QualStatus,
        issue_age: u8,
        gender: Gender,
        initial_benefit_base: f64,
        initial_pols: f64,
        initial_premium: f64,
        crediting_strategy: CreditingStrategy,
        sc_period: u8,
        val_rate: f64,
        mgir: f64,
        bonus: f64,
        rollup_type: RollupType,
        glwb_start_year: u32,
    ) -> Self {
        // Bucket is based on per-life benefit base, not aggregate
        let bb_per_life = if initial_pols > 0.0 {
            initial_benefit_base / initial_pols
        } else {
            initial_benefit_base
        };
        let benefit_base_bucket = BenefitBaseBucket::from_amount(bb_per_life);

        Self {
            policy_id,
            qual_status,
            issue_age,
            gender,
            initial_benefit_base,
            initial_pols,
            initial_premium,
            benefit_base_bucket,
            percentage: 1.0,
            crediting_strategy,
            sc_period,
            val_rate,
            mgir,
            bonus,
            rollup_type,
            duration_months: 0,
            income_activated: false,
            glwb_start_year,
            current_av: None,
            current_benefit_base: None,
        }
    }

    /// Get the starting account value for projection
    pub fn starting_av(&self) -> f64 {
        self.current_av.unwrap_or(self.initial_premium)
    }

    /// Get the starting benefit base for projection
    pub fn starting_benefit_base(&self) -> f64 {
        self.current_benefit_base.unwrap_or(self.initial_benefit_base)
    }

    /// Calculate attained age at a given projection month
    /// Excel formula: =IssueAge + PolicyYear - 1
    /// Age increments at the START of each policy year (month 13, 25, etc.)
    pub fn attained_age(&self, projection_month: u32) -> u8 {
        let policy_year = self.policy_year(projection_month);
        self.issue_age.saturating_add((policy_year - 1) as u8)
    }

    /// Calculate policy year at a given projection month
    pub fn policy_year(&self, projection_month: u32) -> u32 {
        let total_months = self.duration_months + projection_month;
        // Use saturating_sub to handle the case when total_months is 0 (new issue at month 0)
        total_months.saturating_sub(1) / 12 + 1
    }

    /// Calculate month within policy year at a given projection month
    pub fn month_in_policy_year(&self, projection_month: u32) -> u32 {
        let total_months = self.duration_months + projection_month;
        // Use saturating_sub to handle the case when total_months is 0
        (total_months.saturating_sub(1) % 12) + 1
    }

    /// Check if policy is still in surrender charge period
    pub fn in_sc_period(&self, projection_month: u32) -> bool {
        self.policy_year(projection_month) <= self.sc_period as u32
    }

    /// Check if GLWB income should be activated at a given projection month
    /// Income activates at the START of the glwb_start_year
    pub fn should_activate_income(&self, projection_month: u32) -> bool {
        if self.income_activated {
            return true; // Already activated
        }
        self.policy_year(projection_month) >= self.glwb_start_year
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benefit_base_bucket() {
        assert_eq!(BenefitBaseBucket::from_amount(25_000.0), BenefitBaseBucket::Under50k);
        assert_eq!(BenefitBaseBucket::from_amount(75_000.0), BenefitBaseBucket::From50kTo100k);
        assert_eq!(BenefitBaseBucket::from_amount(150_000.0), BenefitBaseBucket::From100kTo200k);
        assert_eq!(BenefitBaseBucket::from_amount(300_000.0), BenefitBaseBucket::From200kTo500k);
        assert_eq!(BenefitBaseBucket::from_amount(1_000_000.0), BenefitBaseBucket::Over500k);
    }

    #[test]
    fn test_policy_timing() {
        let policy = Policy::new(
            1,
            QualStatus::Q,
            77,
            Gender::Male,
            27178.16,
            0.039,
            20906.28,
            CreditingStrategy::Indexed,
            10,
            0.0475,
            0.01,
            0.3,
            RollupType::Simple,
        );

        // Month 1: policy year 1, month 1
        assert_eq!(policy.policy_year(1), 1);
        assert_eq!(policy.month_in_policy_year(1), 1);
        assert_eq!(policy.attained_age(1), 77);

        // Month 12: policy year 1, month 12, still age 77
        assert_eq!(policy.policy_year(12), 1);
        assert_eq!(policy.month_in_policy_year(12), 12);
        assert_eq!(policy.attained_age(12), 77);

        // Month 13: policy year 2, month 1
        assert_eq!(policy.policy_year(13), 2);
        assert_eq!(policy.month_in_policy_year(13), 1);
        assert_eq!(policy.attained_age(13), 78);
    }
}
