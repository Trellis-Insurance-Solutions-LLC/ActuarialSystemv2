//! Benefit stream calculations for reserve valuation
//!
//! Calculates present values of:
//! - Death benefits (non-elective, mortality-weighted)
//! - Income benefits (elective, GLWB systematic withdrawals)
//! - Surrender benefits (elective, CSV)
//!
//! Uses separation of concerns pattern to handle different discount rates
//! for elective vs non-elective benefits per AG33/AG35 requirements.

use crate::assumptions::Assumptions;
use crate::policy::Policy;
use super::discount::DiscountCurve;
use super::types::PolicyState;

/// Calculator for benefit stream present values
pub struct BenefitCalculator<'a> {
    assumptions: &'a Assumptions,
    discount_curve: DiscountCurve,
    max_projection_months: u32,
}

impl<'a> BenefitCalculator<'a> {
    /// Create a new benefit calculator
    pub fn new(
        assumptions: &'a Assumptions,
        discount_curve: DiscountCurve,
        max_projection_months: u32,
    ) -> Self {
        Self {
            assumptions,
            discount_curve,
            max_projection_months,
        }
    }

    /// Create with policy's valuation rate
    pub fn from_policy(assumptions: &'a Assumptions, policy: &Policy) -> Self {
        Self::new(
            assumptions,
            DiscountCurve::single_rate(policy.val_rate),
            768, // Default 64 years
        )
    }

    // ========================================================================
    // DEATH BENEFIT CALCULATIONS (Non-Elective)
    // ========================================================================

    /// Calculate PV of death benefits along a given path
    ///
    /// Death benefits are NON-ELECTIVE, so we use mortality-weighted discounting.
    /// The death benefit amount may depend on whether the policy is in accumulation
    /// or income phase.
    ///
    /// # Arguments
    /// * `policy` - The policy to calculate for
    /// * `valuation_month` - Starting month for calculation
    /// * `activation_month` - Month when income activates (None = never)
    /// * `starting_av` - Account value at valuation month
    /// * `starting_bb` - Benefit base at valuation month
    pub fn death_benefit_pv(
        &self,
        policy: &Policy,
        valuation_month: u32,
        activation_month: Option<u32>,
        starting_av: f64,
        starting_bb: f64,
    ) -> f64 {
        let mut death_pv = 0.0;
        let mut survival_prob = 1.0;

        // Track projected state over time
        let mut projected_av = starting_av;
        let mut projected_bb = starting_bb;

        let v_death = self.discount_curve.death_benefit_discount_factor();

        for t in valuation_month..self.max_projection_months {
            let months_from_val = t - valuation_month;

            // Determine policy state at this month
            let state = if activation_month.map_or(false, |am| t >= am) {
                PolicyState::IncomeActive
            } else {
                PolicyState::Accumulation
            };

            // Get mortality rate
            let attained_age = policy.attained_age(t);
            let q = self.assumptions.mortality.monthly_rate(attained_age, policy.gender, t);

            // Calculate death benefit amount for this state
            let db = self.death_benefit_amount(
                policy,
                t,
                state,
                projected_av,
                projected_bb,
            );

            // PV contribution: survival to t × probability of death × DB × discount
            death_pv += survival_prob * q * db * v_death.powi(months_from_val as i32);

            // Update survival probability
            survival_prob *= 1.0 - q;

            // Stop if everyone has died (or negligible survival)
            if survival_prob < 1e-10 {
                break;
            }

            // Project AV and BB forward (simplified - would use actual projection)
            self.project_state_forward(
                policy,
                t,
                state,
                &mut projected_av,
                &mut projected_bb,
            );
        }

        death_pv
    }

    /// Calculate death benefit amount for a given state
    ///
    /// For this product spec:
    /// - Death benefit = AV (no surrender charges applied)
    /// - Benefit base is only used for GLWB income calculation, not death benefit
    fn death_benefit_amount(
        &self,
        _policy: &Policy,
        _month: u32,
        state: PolicyState,
        account_value: f64,
        _benefit_base: f64,
    ) -> f64 {
        match state {
            PolicyState::Accumulation | PolicyState::IncomeActive => {
                // Death benefit = Account Value (no surrender charges)
                // The benefit base is only relevant for the GLWB income stream
                account_value
            }
            PolicyState::Surrendered | PolicyState::Matured => 0.0,
        }
    }

    // ========================================================================
    // INCOME BENEFIT CALCULATIONS (Elective)
    // ========================================================================

    /// Calculate PV of income benefits if income activates at a specific month
    ///
    /// # Arguments
    /// * `policy` - The policy to calculate for
    /// * `valuation_month` - Starting month for discounting
    /// * `activation_month` - Month when income starts
    /// * `starting_bb` - Benefit base at activation (frozen at that point)
    pub fn income_benefit_pv(
        &self,
        policy: &Policy,
        valuation_month: u32,
        activation_month: u32,
        starting_bb: f64,
    ) -> f64 {
        if activation_month < valuation_month {
            // Already past activation - this shouldn't happen in normal use
            return 0.0;
        }

        let mut income_pv = 0.0;
        let mut survival_prob = 1.0;

        // Get payout rate at activation age
        let activation_age = policy.attained_age(activation_month);
        let payout_rate = self.assumptions.product.glwb.payout_factors.get_single_life(activation_age);

        // Monthly income amount (benefit base × annual payout rate / 12)
        let monthly_income = starting_bb * payout_rate / 12.0;

        let v_elective = self.discount_curve.elective_discount_factor();

        // Project forward from valuation month
        // Income payments start at activation_month
        for t in valuation_month..self.max_projection_months {
            let months_from_val = t - valuation_month;

            // Get mortality rate
            let attained_age = policy.attained_age(t);
            let q = self.assumptions.mortality.monthly_rate(attained_age, policy.gender, t);

            // Income only paid after activation
            if t >= activation_month {
                // Income payment at start of month (annuity due)
                income_pv += survival_prob * monthly_income * v_elective.powi(months_from_val as i32);
            }

            // Update survival
            survival_prob *= 1.0 - q;

            if survival_prob < 1e-10 {
                break;
            }
        }

        income_pv
    }

    /// Calculate PV of income benefits if already in income phase
    pub fn remaining_income_pv(
        &self,
        policy: &Policy,
        valuation_month: u32,
        current_bb: f64,
        locked_payout_rate: f64,
    ) -> f64 {
        let mut income_pv = 0.0;
        let mut survival_prob = 1.0;

        let monthly_income = current_bb * locked_payout_rate / 12.0;
        let v_elective = self.discount_curve.elective_discount_factor();

        for t in valuation_month..self.max_projection_months {
            let months_from_val = t - valuation_month;

            let attained_age = policy.attained_age(t);
            let q = self.assumptions.mortality.monthly_rate(attained_age, policy.gender, t);

            // Income payment
            income_pv += survival_prob * monthly_income * v_elective.powi(months_from_val as i32);

            survival_prob *= 1.0 - q;

            if survival_prob < 1e-10 {
                break;
            }
        }

        income_pv
    }

    // ========================================================================
    // SURRENDER VALUE CALCULATIONS (Elective)
    // ========================================================================

    /// Calculate cash surrender value at a given month
    pub fn cash_surrender_value(
        &self,
        policy: &Policy,
        month: u32,
        account_value: f64,
    ) -> f64 {
        let policy_year = policy.policy_year(month);
        let sc_rate = self.assumptions.product.base.surrender_charges.get_rate(policy_year);

        account_value * (1.0 - sc_rate)
    }

    // ========================================================================
    // HELPER METHODS
    // ========================================================================

    /// Project AV and BB forward by one month (simplified)
    ///
    /// This is a simplified projection for reserve calculations.
    /// For full accuracy, would use the actual projection engine.
    fn project_state_forward(
        &self,
        policy: &Policy,
        month: u32,
        state: PolicyState,
        av: &mut f64,
        bb: &mut f64,
    ) {
        let attained_age = policy.attained_age(month);
        let policy_year = policy.policy_year(month);
        let month_in_py = policy.month_in_policy_year(month);

        // Mortality decrement
        let q = self.assumptions.mortality.monthly_rate(attained_age, policy.gender, month);

        // Rider charge (annual, applied at month 12)
        let rider_charge = if month % 12 == 0 {
            let rate = if state == PolicyState::IncomeActive {
                self.assumptions.product.glwb.post_activation_charge
            } else {
                self.assumptions.product.glwb.pre_activation_charge
            };
            *bb * rate
        } else {
            0.0
        };

        // Systematic withdrawal if in income
        let systematic_wd = if state == PolicyState::IncomeActive {
            let payout_rate = self.assumptions.product.glwb.payout_factors.get_single_life(attained_age);
            *bb * payout_rate / 12.0
        } else {
            0.0
        };

        // Update AV (simplified - ignoring credited interest for conservative estimate)
        *av = (*av - systematic_wd - rider_charge).max(0.0);

        // Update BB
        // During accumulation, BB grows via rollup; after income, BB is frozen
        if state == PolicyState::Accumulation {
            // Benefit base rollup at month 12 during SC period
            if month_in_py == 12 && policy_year <= policy.sc_period as u32 {
                let bb_bonus = self.assumptions.product.glwb.bonus_rate;
                let rollup_rate = self.assumptions.product.glwb.rollup_rate;
                let py = (policy_year as f64).min(10.0);
                let py_prev = ((policy_year - 1) as f64).min(10.0);
                let rollup_factor = (1.0 + bb_bonus + rollup_rate * py)
                    / (1.0 + bb_bonus + rollup_rate * py_prev);
                *bb *= rollup_factor;
            }
        }
        // In income phase, BB is frozen (no changes)

        // Apply mortality decrement to both
        *av *= 1.0 - q;
        *bb *= 1.0 - q;
    }

    /// Calculate total reserve for a specific activation path
    ///
    /// Combines death benefit PV and elective benefit PV
    pub fn total_reserve_for_path(
        &self,
        policy: &Policy,
        valuation_month: u32,
        activation_month: Option<u32>,
        starting_av: f64,
        starting_bb: f64,
    ) -> f64 {
        // Death benefit PV (non-elective)
        let death_pv = self.death_benefit_pv(
            policy,
            valuation_month,
            activation_month,
            starting_av,
            starting_bb,
        );

        // Elective benefit PV
        let elective_pv = if let Some(am) = activation_month {
            // Project BB to activation month, then calculate income PV
            // Simplified: use starting BB (would need projection for accuracy)
            self.income_benefit_pv(policy, valuation_month, am, starting_bb)
        } else {
            // Never activate - elective benefit is surrender
            // For CARVM, we test this as one of the paths
            0.0
        };

        death_pv + elective_pv
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{QualStatus, Gender, CreditingStrategy, RollupType};

    fn test_policy() -> Policy {
        Policy::new(
            1,
            QualStatus::Q,
            65,
            Gender::Male,
            130_000.0,  // BB
            1.0,        // pols
            100_000.0,  // premium
            CreditingStrategy::Indexed,
            10,
            0.0475,     // val_rate
            0.01,
            0.3,
            RollupType::Simple,
        )
    }

    #[test]
    fn test_death_benefit_amount() {
        let assumptions = Assumptions::default_pricing();
        let policy = test_policy();
        let calc = BenefitCalculator::from_policy(&assumptions, &policy);

        // Death benefit = AV (benefit base not used for death benefit in this product)
        let db = calc.death_benefit_amount(&policy, 1, PolicyState::Accumulation, 100_000.0, 130_000.0);
        assert!((db - 100_000.0).abs() < 1.0, "DB should equal AV");

        // Death benefit is AV regardless of BB
        let db_higher_av = calc.death_benefit_amount(&policy, 1, PolicyState::Accumulation, 150_000.0, 130_000.0);
        assert!((db_higher_av - 150_000.0).abs() < 1.0, "DB should equal AV");

        // In income phase, DB is still AV
        let db_income = calc.death_benefit_amount(&policy, 1, PolicyState::IncomeActive, 80_000.0, 130_000.0);
        assert!((db_income - 80_000.0).abs() < 1.0, "DB in income phase should equal AV");
    }

    #[test]
    fn test_csv_calculation() {
        let assumptions = Assumptions::default_pricing();
        let policy = test_policy();
        let calc = BenefitCalculator::from_policy(&assumptions, &policy);

        // In year 1, SC is typically ~10%
        let csv = calc.cash_surrender_value(&policy, 1, 100_000.0);
        // CSV should be less than AV due to surrender charge
        assert!(csv < 100_000.0);
        assert!(csv > 85_000.0); // But not too much less
    }
}
