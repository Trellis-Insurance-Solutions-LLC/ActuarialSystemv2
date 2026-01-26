//! Core types for reserve calculations

use serde::{Deserialize, Serialize};

/// State of a policy for reserve calculation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyState {
    /// Pre-income, can elect income or surrender
    Accumulation,
    /// Taking GLWB withdrawals
    IncomeActive,
    /// Contract terminated via surrender
    Surrendered,
    /// Contract ended via death or maturity
    Matured,
}

impl Default for PolicyState {
    fn default() -> Self {
        PolicyState::Accumulation
    }
}

/// Projection state for reserve calculation
///
/// Extends the concept from `ProjectionState` with reserve-specific fields.
/// Used to track state during benefit stream calculations.
#[derive(Debug, Clone)]
pub struct ReserveProjectionState {
    /// Current projection month (from valuation date)
    pub month: u32,

    /// Current policy state
    pub policy_state: PolicyState,

    /// Current account value
    pub account_value: f64,

    /// Current benefit base
    pub benefit_base: f64,

    /// Cumulative systematic withdrawals taken
    pub cumulative_withdrawals: f64,

    /// Remaining free withdrawal amount available
    /// (For products where free PWD may be the optimal path)
    pub remaining_free_amount: f64,

    /// Cumulative survival probability from valuation date
    pub survival_probability: f64,

    /// Attained age at this point
    pub attained_age: u8,

    /// Policy year at this point
    pub policy_year: u32,
}

impl ReserveProjectionState {
    /// Create initial state from policy values
    pub fn initial(
        account_value: f64,
        benefit_base: f64,
        attained_age: u8,
        policy_year: u32,
        income_activated: bool,
    ) -> Self {
        Self {
            month: 0,
            policy_state: if income_activated {
                PolicyState::IncomeActive
            } else {
                PolicyState::Accumulation
            },
            account_value,
            benefit_base,
            cumulative_withdrawals: 0.0,
            remaining_free_amount: account_value * 0.10, // Typical 10% free withdrawal
            survival_probability: 1.0,
            attained_age,
            policy_year,
        }
    }

    /// Calculate ITM-ness (benefit base / account value)
    pub fn itm_ness(&self) -> f64 {
        if self.account_value <= 0.0 {
            f64::MAX
        } else {
            self.benefit_base / self.account_value
        }
    }
}

/// Result of a reserve calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveResult {
    /// Policy identifier
    pub policy_id: u32,

    /// Month of valuation (0 = issue date)
    pub valuation_date: u32,

    /// Gross reserve before any adjustments
    pub gross_reserve: f64,

    /// Net reserve after reinsurance, etc.
    pub net_reserve: f64,

    /// Optimal income activation month from optimization
    /// u32::MAX indicates "never activate" is optimal
    pub optimal_activation_month: u32,

    /// Breakdown of reserve by benefit type
    pub reserve_components: ReserveComponents,

    /// Method used for calculation
    pub method: ReserveMethod,

    /// Whether this result came from cache roll-forward
    pub from_cache: bool,

    /// Cash surrender value at valuation date (for reference)
    pub csv_at_valuation: f64,
}

impl ReserveResult {
    /// Check if CSV is binding (reserve = CSV)
    pub fn is_csv_binding(&self) -> bool {
        (self.gross_reserve - self.csv_at_valuation).abs() < 0.01
    }
}

/// Breakdown of reserve by benefit type
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReserveComponents {
    /// PV of guaranteed death benefits (non-elective)
    pub death_benefit_pv: f64,

    /// PV of GLWB income stream (elective)
    pub income_benefit_pv: f64,

    /// CSV component (if binding)
    pub surrender_value_pv: f64,

    /// Combined elective benefit PV
    pub elective_benefit_pv: f64,

    /// Free partial withdrawal PV (if optimal path includes PWD)
    pub free_pwd_pv: f64,
}

impl ReserveComponents {
    /// Total reserve from components
    pub fn total(&self) -> f64 {
        self.death_benefit_pv + self.elective_benefit_pv
    }
}

/// Method used for reserve calculation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReserveMethod {
    /// Basic CARVM
    CARVM,

    /// AG33 - CARVM for contracts with elective benefits
    AG33,

    /// AG35 Type 1 - Basic computational method
    AG35Type1,

    /// AG35 Type 2 - Requires "Hedged as Required" certification
    AG35Type2,

    /// VM-22 Principles-Based Reserve
    VM22 {
        /// Scenario ID used for this calculation
        scenario_id: u32,
    },
}

impl Default for ReserveMethod {
    fn default() -> Self {
        ReserveMethod::CARVM
    }
}

/// Configuration for reserve-aware projection
#[derive(Debug, Clone)]
pub struct ReserveProjectionConfig {
    /// Maximum projection months
    pub max_projection_months: u32,

    /// Valuation month (0 = from issue)
    pub valuation_month: u32,

    /// Force income activation at specific month (for path testing)
    /// None = let optimizer decide, Some(m) = activate at month m
    pub forced_activation_month: Option<u32>,

    /// Whether to track detailed benefit streams
    pub detailed_output: bool,
}

impl Default for ReserveProjectionConfig {
    fn default() -> Self {
        Self {
            max_projection_months: 768, // 64 years
            valuation_month: 0,
            forced_activation_month: None,
            detailed_output: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_state_default() {
        assert_eq!(PolicyState::default(), PolicyState::Accumulation);
    }

    #[test]
    fn test_reserve_projection_state_itm() {
        let state = ReserveProjectionState {
            month: 0,
            policy_state: PolicyState::Accumulation,
            account_value: 100_000.0,
            benefit_base: 130_000.0,
            cumulative_withdrawals: 0.0,
            remaining_free_amount: 10_000.0,
            survival_probability: 1.0,
            attained_age: 65,
            policy_year: 1,
        };

        assert!((state.itm_ness() - 1.3).abs() < 0.001);
    }

    #[test]
    fn test_reserve_components_total() {
        let components = ReserveComponents {
            death_benefit_pv: 5_000.0,
            income_benefit_pv: 0.0,
            surrender_value_pv: 0.0,
            elective_benefit_pv: 95_000.0,
            free_pwd_pv: 0.0,
        };

        assert!((components.total() - 100_000.0).abs() < 0.01);
    }
}
