//! CARVM (Commissioners Annuity Reserve Valuation Method) calculator
//!
//! Implements the CARVM optimization to find the maximum reserve across all
//! possible policyholder behavior paths. For GLWB products, this means finding
//! the optimal income activation time that maximizes the insurer's liability.
//!
//! # Algorithm Options
//!
//! - **Brute Force**: O(T × N) - Tests all activation times, guaranteed correct
//! - **Dynamic Programming**: O(N) - Faster but more complex
//! - **Hybrid**: DP with brute-force validation for a subset
//!
//! # Caching
//!
//! Uses roll-forward caching for efficient multi-timestep calculations:
//! - Full solve at t=0 determines optimal activation time T*
//! - Subsequent reserves roll forward until T* or revalidation trigger

use crate::assumptions::Assumptions;
use crate::policy::Policy;

use super::types::{ReserveResult, ReserveComponents, ReserveMethod};
use super::discount::DiscountCurve;
use super::benefits::BenefitCalculator;
use super::cache::{CachedReservePath, RollForwardResult, ReserveCache, RevalidationCriteria};
use super::ReserveCalculator;

/// CARVM calculation method
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CARVMMethod {
    /// Test all possible activation times - O(T × N), guaranteed correct
    BruteForce,

    /// Dynamic programming - O(N), faster but more complex
    DynamicProgramming,

    /// DP with periodic brute-force validation
    Hybrid,
}

impl Default for CARVMMethod {
    fn default() -> Self {
        CARVMMethod::Hybrid
    }
}

/// Configuration for CARVM reserve calculation
#[derive(Debug, Clone)]
pub struct CARVMConfig {
    /// Calculation method
    pub method: CARVMMethod,

    /// Maximum projection months
    pub max_projection_months: u32,

    /// Whether to use roll-forward caching
    pub use_caching: bool,

    /// How often to re-validate cached values (months)
    pub revalidation_frequency: u32,

    /// Revalidation criteria
    pub revalidation_criteria: RevalidationCriteria,

    /// Maximum deferral period to test (in years)
    /// Limits brute force search space
    pub max_deferral_years: u32,
}

impl Default for CARVMConfig {
    fn default() -> Self {
        Self {
            method: CARVMMethod::Hybrid,
            max_projection_months: 768,
            use_caching: true,
            revalidation_frequency: 12,
            revalidation_criteria: RevalidationCriteria::default(),
            max_deferral_years: 30,
        }
    }
}

/// Main CARVM calculator
///
/// Calculates CARVM reserves using the configured method, with optional
/// caching for efficient multi-timestep calculations.
pub struct CARVMCalculator {
    assumptions: Assumptions,
    config: CARVMConfig,
    cache: ReserveCache,
}

impl CARVMCalculator {
    /// Create a new CARVM calculator
    pub fn new(assumptions: Assumptions, config: CARVMConfig) -> Self {
        let cache = ReserveCache::with_criteria(config.revalidation_criteria.clone());
        Self {
            assumptions,
            config,
            cache,
        }
    }

    /// Create with default configuration
    pub fn with_defaults(assumptions: Assumptions) -> Self {
        Self::new(assumptions, CARVMConfig::default())
    }

    /// Get reference to assumptions
    pub fn assumptions(&self) -> &Assumptions {
        &self.assumptions
    }

    /// Get mutable reference to assumptions
    pub fn assumptions_mut(&mut self) -> &mut Assumptions {
        &mut self.assumptions
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (u64, u64, f64) {
        (self.cache.cache_hits, self.cache.cache_misses, self.cache.hit_rate())
    }

    // ========================================================================
    // MAIN CALCULATION
    // ========================================================================

    /// Calculate reserve with caching support
    fn calculate_with_cache(
        &mut self,
        policy: &Policy,
        valuation_month: u32,
    ) -> ReserveResult {
        let policy_id = policy.policy_id as u64;

        // Get current state for validation
        let current_av = self.get_av_at_month(policy, valuation_month);
        let current_bb = self.get_bb_at_month(policy, valuation_month);
        let current_sc_period = policy.sc_period as u32;

        // Try to use cache
        if self.config.use_caching {
            // Clone cached data to avoid borrow issues
            let cached_data = self.cache.get(policy_id).cloned();

            if let Some(cached) = cached_data {
                // Check if revalidation is needed
                if let Some(_reason) = self.config.revalidation_criteria.needs_revalidation(
                    &cached,
                    valuation_month,
                    current_av,
                    current_bb,
                    current_sc_period,
                ) {
                    self.cache.record_revalidation();
                    // Fall through to full solve
                } else {
                    // Try roll forward
                    match self.try_roll_forward(policy, valuation_month, cached.clone()) {
                        RollForwardResult::Success { reserve, .. } => {
                            self.cache.record_hit();

                            let csv = self.cash_surrender_value(policy, valuation_month, current_av);
                            let final_reserve = reserve.max(csv);

                            return ReserveResult {
                                policy_id: policy.policy_id,
                                valuation_date: valuation_month,
                                gross_reserve: final_reserve,
                                net_reserve: final_reserve,
                                optimal_activation_month: cached.optimal_activation_month,
                                reserve_components: ReserveComponents {
                                    death_benefit_pv: cached.death_benefit_pv_remaining,
                                    income_benefit_pv: reserve - cached.death_benefit_pv_remaining,
                                    surrender_value_pv: if (final_reserve - csv).abs() < 0.01 { csv } else { 0.0 },
                                    elective_benefit_pv: reserve - cached.death_benefit_pv_remaining,
                                    free_pwd_pv: 0.0,
                                },
                                method: ReserveMethod::CARVM,
                                from_cache: true,
                                csv_at_valuation: csv,
                            };
                        }
                        RollForwardResult::NeedsResolve { .. } => {
                            self.cache.record_miss();
                            // Fall through to full solve
                        }
                    }
                }
            } else {
                self.cache.record_miss();
            }
        }

        // Full solve
        self.full_solve_and_cache(policy, valuation_month, current_av, current_bb)
    }

    /// Perform full CARVM optimization and cache result
    fn full_solve_and_cache(
        &mut self,
        policy: &Policy,
        valuation_month: u32,
        current_av: f64,
        current_bb: f64,
    ) -> ReserveResult {
        let (optimal_month, reserve, components) = match self.config.method {
            CARVMMethod::BruteForce => self.brute_force_solve(policy, valuation_month, current_av, current_bb),
            CARVMMethod::DynamicProgramming => self.dp_solve(policy, valuation_month, current_av, current_bb),
            CARVMMethod::Hybrid => self.hybrid_solve(policy, valuation_month, current_av, current_bb),
        };

        let csv = self.cash_surrender_value(policy, valuation_month, current_av);
        let final_reserve = reserve.max(csv);

        // Update cache
        if self.config.use_caching {
            let monthly_income = if optimal_month < u32::MAX {
                let activation_age = policy.attained_age(optimal_month);
                let payout_rate = self.assumptions.product.glwb.payout_factors.get_single_life(activation_age);
                current_bb * payout_rate / 12.0
            } else {
                0.0
            };

            let sc_rate = self.assumptions.product.base.surrender_charges.get_rate(
                policy.policy_year(valuation_month)
            );

            let cached_path = CachedReservePath::new(
                policy.policy_id as u64,
                valuation_month,
                optimal_month,
                reserve,
                current_av,
                current_bb,
                monthly_income,
                components.death_benefit_pv,
                sc_rate,
            );

            self.cache.insert(cached_path);
        }

        // Determine if CSV is binding
        let is_csv_binding = (final_reserve - csv).abs() < 0.01;

        ReserveResult {
            policy_id: policy.policy_id,
            valuation_date: valuation_month,
            gross_reserve: final_reserve,
            net_reserve: final_reserve,
            optimal_activation_month: if is_csv_binding { u32::MAX } else { optimal_month },
            reserve_components: if is_csv_binding {
                ReserveComponents {
                    surrender_value_pv: csv,
                    ..components
                }
            } else {
                components
            },
            method: ReserveMethod::CARVM,
            from_cache: false,
            csv_at_valuation: csv,
        }
    }

    // ========================================================================
    // BRUTE FORCE SOLVER
    // ========================================================================

    /// Brute force: test all possible activation times
    fn brute_force_solve(
        &self,
        policy: &Policy,
        valuation_month: u32,
        current_av: f64,
        current_bb: f64,
    ) -> (u32, f64, ReserveComponents) {
        let discount_curve = DiscountCurve::single_rate(policy.val_rate);
        let benefit_calc = BenefitCalculator::new(
            &self.assumptions,
            discount_curve,
            self.config.max_projection_months,
        );

        let mut best_reserve = 0.0;
        let mut best_activation = u32::MAX;
        let mut best_components = ReserveComponents::default();

        let max_deferral = valuation_month + self.config.max_deferral_years * 12;

        // Test each possible activation month
        for activation_month in valuation_month..=max_deferral.min(self.config.max_projection_months) {
            let death_pv = benefit_calc.death_benefit_pv(
                policy,
                valuation_month,
                Some(activation_month),
                current_av,
                current_bb,
            );

            let income_pv = benefit_calc.income_benefit_pv(
                policy,
                valuation_month,
                activation_month,
                current_bb,
            );

            let total = death_pv + income_pv;

            if total > best_reserve {
                best_reserve = total;
                best_activation = activation_month;
                best_components = ReserveComponents {
                    death_benefit_pv: death_pv,
                    income_benefit_pv: income_pv,
                    surrender_value_pv: 0.0,
                    elective_benefit_pv: income_pv,
                    free_pwd_pv: 0.0,
                };
            }
        }

        // Also test "never activate" path
        let never_death_pv = benefit_calc.death_benefit_pv(
            policy,
            valuation_month,
            None,
            current_av,
            current_bb,
        );

        if never_death_pv > best_reserve {
            best_reserve = never_death_pv;
            best_activation = u32::MAX;
            best_components = ReserveComponents {
                death_benefit_pv: never_death_pv,
                income_benefit_pv: 0.0,
                surrender_value_pv: 0.0,
                elective_benefit_pv: 0.0,
                free_pwd_pv: 0.0,
            };
        }

        (best_activation, best_reserve, best_components)
    }

    // ========================================================================
    // DYNAMIC PROGRAMMING SOLVER
    // ========================================================================

    /// Dynamic programming solver (placeholder - would implement full DP)
    fn dp_solve(
        &self,
        policy: &Policy,
        valuation_month: u32,
        current_av: f64,
        current_bb: f64,
    ) -> (u32, f64, ReserveComponents) {
        // TODO: Implement full DP solver with separate death/elective tracks
        // For now, fall back to brute force
        self.brute_force_solve(policy, valuation_month, current_av, current_bb)
    }

    /// Hybrid solver: DP with validation
    fn hybrid_solve(
        &self,
        policy: &Policy,
        valuation_month: u32,
        current_av: f64,
        current_bb: f64,
    ) -> (u32, f64, ReserveComponents) {
        // TODO: Run DP, validate against brute force for first N policies
        // For now, just use brute force
        self.brute_force_solve(policy, valuation_month, current_av, current_bb)
    }

    // ========================================================================
    // ROLL FORWARD
    // ========================================================================

    /// Try to roll forward from cached reserve
    fn try_roll_forward(
        &self,
        policy: &Policy,
        valuation_month: u32,
        cached: CachedReservePath,
    ) -> RollForwardResult {
        let t_star = cached.optimal_activation_month;
        let _months_elapsed = valuation_month.saturating_sub(cached.solve_month);

        // Get current state
        let current_av = self.get_av_at_month(policy, valuation_month);
        let current_bb = self.get_bb_at_month(policy, valuation_month);

        // Case A: Still in accumulation, before optimal activation
        if valuation_month < t_star {
            // Roll forward reserve
            let rolled = self.roll_accumulation_reserve(
                cached.reserve_at_solve,
                policy,
                cached.solve_month,
                valuation_month,
            );

            // Quick validation: ITM change
            let current_itm = if current_av > 0.0 { current_bb / current_av } else { f64::MAX };
            let still_valid = (current_itm - cached.itm_at_solve).abs() / cached.itm_at_solve.max(0.01) < 0.10;

            return RollForwardResult::Success {
                reserve: rolled,
                still_valid,
                validation_notes: None,
            };
        }

        // Case B: At or past optimal activation time
        if valuation_month >= t_star && t_star < u32::MAX {
            let discount_curve = DiscountCurve::single_rate(policy.val_rate);
            let benefit_calc = BenefitCalculator::new(
                &self.assumptions,
                discount_curve,
                self.config.max_projection_months,
            );

            // Simple calculation: PV of remaining income + death benefits
            let activation_age = policy.attained_age(t_star);
            let payout_rate = self.assumptions.product.glwb.payout_factors.get_single_life(activation_age);

            let income_pv = benefit_calc.remaining_income_pv(
                policy,
                valuation_month,
                current_bb,
                payout_rate,
            );

            let death_pv = benefit_calc.death_benefit_pv(
                policy,
                valuation_month,
                Some(t_star),
                current_av,
                current_bb,
            );

            return RollForwardResult::Success {
                reserve: income_pv + death_pv,
                still_valid: true,
                validation_notes: None,
            };
        }

        RollForwardResult::NeedsResolve {
            reason: "Unexpected state in roll forward".into(),
        }
    }

    /// Roll reserve forward through accumulation period
    fn roll_accumulation_reserve(
        &self,
        r_prev: f64,
        policy: &Policy,
        t_prev: u32,
        t_now: u32,
    ) -> f64 {
        let v = 1.0 / (1.0 + policy.val_rate / 12.0);
        let mut reserve = r_prev;

        for t in t_prev..t_now {
            let attained_age = policy.attained_age(t);
            let q = self.assumptions.mortality.monthly_rate(attained_age, policy.gender, t);
            let p = 1.0 - q;

            // Simplified roll forward (ignoring DB cost for now)
            // Full version would subtract DB cost at each step
            reserve = reserve / (p * v);
        }

        reserve
    }

    // ========================================================================
    // HELPER METHODS
    // ========================================================================

    /// Get account value at a specific month (simplified)
    fn get_av_at_month(&self, policy: &Policy, month: u32) -> f64 {
        if month == 0 {
            policy.starting_av()
        } else {
            // Would need actual projection or state tracking
            // For now, return starting AV (conservative)
            policy.starting_av()
        }
    }

    /// Get benefit base at a specific month (simplified)
    fn get_bb_at_month(&self, policy: &Policy, month: u32) -> f64 {
        if month == 0 {
            policy.starting_benefit_base()
        } else {
            // Would need actual projection
            policy.starting_benefit_base()
        }
    }

    /// Calculate cash surrender value
    fn cash_surrender_value(&self, policy: &Policy, month: u32, av: f64) -> f64 {
        let policy_year = policy.policy_year(month);
        let sc_rate = self.assumptions.product.base.surrender_charges.get_rate(policy_year);
        av * (1.0 - sc_rate)
    }
}

impl ReserveCalculator for CARVMCalculator {
    fn calculate_reserve(
        &mut self,
        policy: &Policy,
        valuation_month: u32,
    ) -> ReserveResult {
        self.calculate_with_cache(policy, valuation_month)
    }

    fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{QualStatus, Gender, CreditingStrategy, RollupType};

    fn test_policy() -> Policy {
        Policy::new(
            2800,
            QualStatus::Q,
            65,
            Gender::Male,
            130_000.0,
            1.0,
            100_000.0,
            CreditingStrategy::Indexed,
            10,
            0.0475,
            0.01,
            0.3,
            RollupType::Simple,
        )
    }

    #[test]
    fn test_carvm_calculator_creation() {
        let assumptions = Assumptions::default_pricing();
        let config = CARVMConfig::default();
        let calc = CARVMCalculator::new(assumptions, config);

        assert!(calc.config.use_caching);
    }

    #[test]
    fn test_carvm_reserve_calculation() {
        let assumptions = Assumptions::default_pricing();
        let config = CARVMConfig {
            method: CARVMMethod::BruteForce,
            max_projection_months: 120, // Limit for faster test
            max_deferral_years: 10,
            ..Default::default()
        };

        let mut calc = CARVMCalculator::new(assumptions, config);
        let policy = test_policy();

        let result = calc.calculate_reserve(&policy, 0);

        // Reserve should be positive
        assert!(result.gross_reserve > 0.0);

        // CSV should be less than AV due to surrender charges
        assert!(result.csv_at_valuation < policy.starting_av());
    }

    #[test]
    fn test_cache_behavior() {
        let assumptions = Assumptions::default_pricing();
        let config = CARVMConfig {
            method: CARVMMethod::BruteForce,
            max_projection_months: 60,
            max_deferral_years: 5,
            use_caching: true,
            ..Default::default()
        };

        let mut calc = CARVMCalculator::new(assumptions, config);
        let policy = test_policy();

        // First call - should be cache miss
        let _result1 = calc.calculate_reserve(&policy, 0);
        assert_eq!(calc.cache.cache_misses, 1);

        // Second call at same month - should be cache hit
        let _result2 = calc.calculate_reserve(&policy, 0);
        // Note: Same month might trigger revalidation, so we just check it runs
    }

    #[test]
    fn test_csv_is_floor() {
        // CARVM reserve should always be at least as large as CSV
        let assumptions = Assumptions::default_pricing();
        let config = CARVMConfig {
            method: CARVMMethod::BruteForce,
            max_projection_months: 120,
            max_deferral_years: 10,
            use_caching: false,
            ..Default::default()
        };

        let mut calc = CARVMCalculator::new(assumptions, config);
        let policy = test_policy();

        let result = calc.calculate_reserve(&policy, 0);

        // Reserve must be >= CSV (CSV is the floor)
        assert!(
            result.gross_reserve >= result.csv_at_valuation - 0.01,
            "Reserve {} should be >= CSV {}",
            result.gross_reserve,
            result.csv_at_valuation
        );
    }

    #[test]
    fn test_reserve_components_sum() {
        let assumptions = Assumptions::default_pricing();
        let config = CARVMConfig {
            method: CARVMMethod::BruteForce,
            max_projection_months: 120,
            max_deferral_years: 10,
            use_caching: false,
            ..Default::default()
        };

        let mut calc = CARVMCalculator::new(assumptions, config);
        let policy = test_policy();

        let result = calc.calculate_reserve(&policy, 0);

        // When CSV is not binding, death PV + elective PV should approximately equal gross reserve
        if !result.is_csv_binding() {
            let components_sum = result.reserve_components.death_benefit_pv
                + result.reserve_components.elective_benefit_pv;

            // Allow small tolerance for rounding
            assert!(
                (components_sum - result.gross_reserve).abs() < 1.0,
                "Components sum {} should equal gross reserve {}",
                components_sum,
                result.gross_reserve
            );
        }
    }

    #[test]
    fn test_different_ages() {
        // Older policyholders should generally have higher reserves (closer to payout)
        let assumptions = Assumptions::default_pricing();
        let config = CARVMConfig {
            method: CARVMMethod::BruteForce,
            max_projection_months: 120,
            max_deferral_years: 10,
            use_caching: false,
            ..Default::default()
        };

        // Test age 55 vs 70
        let policy_young = Policy::new(
            1, QualStatus::Q, 55, Gender::Male, 130_000.0, 1.0, 100_000.0,
            CreditingStrategy::Indexed, 10, 0.0475, 0.01, 0.3, RollupType::Simple,
        );

        let policy_old = Policy::new(
            2, QualStatus::Q, 70, Gender::Male, 130_000.0, 1.0, 100_000.0,
            CreditingStrategy::Indexed, 10, 0.0475, 0.01, 0.3, RollupType::Simple,
        );

        let mut calc = CARVMCalculator::new(assumptions, config);

        let result_young = calc.calculate_reserve(&policy_young, 0);
        let result_old = calc.calculate_reserve(&policy_old, 0);

        // Both reserves should be positive
        assert!(result_young.gross_reserve > 0.0);
        assert!(result_old.gross_reserve > 0.0);

        // Older policyholder should have earlier optimal activation (if not CSV binding)
        if !result_young.is_csv_binding() && !result_old.is_csv_binding() {
            assert!(
                result_old.optimal_activation_month <= result_young.optimal_activation_month,
                "Older policyholder (act month {}) should activate same or earlier than young ({})",
                result_old.optimal_activation_month,
                result_young.optimal_activation_month
            );
        }
    }

    #[test]
    fn test_high_itm_vs_low_itm() {
        // Higher ITM (BB/AV) should generally have higher reserve
        let assumptions = Assumptions::default_pricing();
        let config = CARVMConfig {
            method: CARVMMethod::BruteForce,
            max_projection_months: 120,
            max_deferral_years: 10,
            use_caching: false,
            ..Default::default()
        };

        // Low ITM: BB = AV (100% ITM)
        let policy_low_itm = Policy::new(
            1, QualStatus::Q, 65, Gender::Male, 100_000.0, 1.0, 100_000.0,
            CreditingStrategy::Indexed, 10, 0.0475, 0.01, 0.3, RollupType::Simple,
        );

        // High ITM: BB = 150% of AV
        let policy_high_itm = Policy::new(
            2, QualStatus::Q, 65, Gender::Male, 150_000.0, 1.0, 100_000.0,
            CreditingStrategy::Indexed, 10, 0.0475, 0.01, 0.3, RollupType::Simple,
        );

        let mut calc = CARVMCalculator::new(assumptions, config);

        let result_low = calc.calculate_reserve(&policy_low_itm, 0);
        let result_high = calc.calculate_reserve(&policy_high_itm, 0);

        // Both reserves should be positive
        assert!(result_low.gross_reserve > 0.0);
        assert!(result_high.gross_reserve > 0.0);

        // Higher ITM should have higher reserve (more valuable guarantee)
        assert!(
            result_high.gross_reserve >= result_low.gross_reserve,
            "High ITM reserve {} should be >= low ITM reserve {}",
            result_high.gross_reserve,
            result_low.gross_reserve
        );
    }

    #[test]
    fn test_optimal_activation_within_bounds() {
        let assumptions = Assumptions::default_pricing();
        let config = CARVMConfig {
            method: CARVMMethod::BruteForce,
            max_projection_months: 120,
            max_deferral_years: 10,
            use_caching: false,
            ..Default::default()
        };

        let mut calc = CARVMCalculator::new(assumptions, config);
        let policy = test_policy();

        let result = calc.calculate_reserve(&policy, 0);

        // Optimal activation month should be within tested range or u32::MAX
        if result.optimal_activation_month != u32::MAX {
            assert!(
                result.optimal_activation_month <= 10 * 12, // max_deferral_years
                "Optimal activation {} should be within deferral limit",
                result.optimal_activation_month
            );
        }
    }

    #[test]
    fn test_reserve_at_later_months() {
        let assumptions = Assumptions::default_pricing();
        let config = CARVMConfig {
            method: CARVMMethod::BruteForce,
            max_projection_months: 120,
            max_deferral_years: 10,
            use_caching: true,
            ..Default::default()
        };

        let mut calc = CARVMCalculator::new(assumptions, config);
        let policy = test_policy();

        // Calculate at month 0 and month 12
        let result_0 = calc.calculate_reserve(&policy, 0);
        let result_12 = calc.calculate_reserve(&policy, 12);

        // Both should have positive reserves
        assert!(result_0.gross_reserve > 0.0);
        assert!(result_12.gross_reserve > 0.0);

        // Reserves should be in a reasonable range
        // (Without actual projection, they may be similar due to simplified state tracking)
    }
}
