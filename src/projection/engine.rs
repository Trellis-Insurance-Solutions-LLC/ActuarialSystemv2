//! Core projection engine for monthly liability cashflow projections

use crate::assumptions::Assumptions;
use crate::policy::Policy;
use super::state::ProjectionState;
use super::cashflows::{CashflowRow, ProjectionResult};

/// Configuration for a projection run
#[derive(Debug, Clone)]
pub struct ProjectionConfig {
    /// Number of months to project
    pub projection_months: u32,

    /// Credited rate approach
    pub crediting: CreditingApproach,

    /// Whether to track detailed cashflows
    pub detailed_output: bool,

    /// Treasury rate change assumption (for lapse model)
    pub treasury_change: f64,

    /// Override lapse with fixed annual rate (for testing)
    /// If Some, uses this rate with even 1/12 monthly skew
    pub fixed_lapse_rate: Option<f64>,
}

/// Approach for crediting interest to account value
#[derive(Debug, Clone)]
pub enum CreditingApproach {
    /// Option budget approach: fixed spread over risk-free
    OptionBudget {
        /// Annual option budget rate
        budget_rate: f64,
        /// Equity kicker (additional return if equity performance is positive)
        equity_kicker: f64,
    },
    /// Scenario-based crediting with floors, caps, and participation
    ScenarioBased {
        /// Floor rate (minimum credited)
        floor: f64,
        /// Cap rate (maximum credited)
        cap: f64,
        /// Participation rate
        participation: f64,
        /// Index return for the period
        index_return: f64,
    },
    /// Fixed crediting rate (monthly)
    Fixed(f64),
    /// Indexed annual crediting (applied at month 12 only)
    /// Excel: =IF(D11=12, annual_rate * IF(C11>10, 0.5, 1), 0)
    IndexedAnnual {
        /// Annual credited rate for years 1-10
        annual_rate: f64,
    },
}

impl Default for ProjectionConfig {
    fn default() -> Self {
        Self {
            projection_months: 360, // 30 years
            crediting: CreditingApproach::OptionBudget {
                budget_rate: 0.0,  // Net zero crediting in year 1
                equity_kicker: 0.0,
            },
            detailed_output: true,
            treasury_change: 0.0,
            fixed_lapse_rate: None,
        }
    }
}

/// Main projection engine
pub struct ProjectionEngine {
    assumptions: Assumptions,
    config: ProjectionConfig,
}

impl ProjectionEngine {
    /// Create a new projection engine with given assumptions and config
    pub fn new(assumptions: Assumptions, config: ProjectionConfig) -> Self {
        Self { assumptions, config }
    }

    /// Run projection for a single policy
    pub fn project_policy(&self, policy: &Policy) -> ProjectionResult {
        let mut result = ProjectionResult::new(policy.policy_id);
        let mut state = ProjectionState::from_policy(policy);

        for _month in 1..=self.config.projection_months {
            // Advance state to next month
            state.advance_month(policy);

            // Calculate and record cashflows
            let row = self.calculate_month(policy, &mut state);
            result.add_row(row);

            // Stop if no lives remaining
            if state.lives <= 1e-10 {
                break;
            }
        }

        result
    }

    /// Calculate cashflows for a single month
    fn calculate_month(&self, policy: &Policy, state: &mut ProjectionState) -> CashflowRow {
        let mut row = CashflowRow::new(state.projection_month);

        // Set timing
        row.policy_year = state.policy_year;
        row.month_in_policy_year = state.month_in_policy_year;
        row.attained_age = state.attained_age;

        // Set BOP values
        row.bop_av = state.bop_av;
        row.bop_benefit_base = state.bop_benefit_base;
        row.pre_decrement_av = state.pre_decrement_av();
        row.lives = state.lives;

        // Premium (only in month 1 for single premium product)
        if state.projection_month == 1 {
            row.premium = policy.initial_premium;
        }

        // Calculate decrements
        self.calculate_decrements(policy, state, &mut row);

        // Calculate persistency and apply decrements
        self.apply_decrements(state, &mut row);

        // Calculate cashflows
        self.calculate_cashflows(policy, state, &mut row);

        // Accumulate YTD systematic withdrawal for correct monthly distribution
        state.ytd_systematic_wd += row.systematic_withdrawal;

        // Update state for next month
        state.eop_av = row.eop_av;
        state.av_persistency = row.av_persistency;
        state.bb_persistency = row.bb_persistency;
        state.lives_persistency = row.lives_persistency;
        // row.lives already has this month's persistency applied (from apply_decrements)
        // So just use it directly as the BOP lives for next month
        state.lives = row.lives;

        // Save current BOP values for next month's lagged ITM calculation
        // MUST happen BEFORE update_benefit_base modifies bop_benefit_base
        state.prior_bop_av = state.bop_av;
        state.prior_bop_bb = state.bop_benefit_base;

        // Update benefit base with rollup
        self.update_benefit_base(policy, state, &row);

        row
    }

    /// Calculate all decrement rates for the month
    fn calculate_decrements(&self, policy: &Policy, state: &ProjectionState, row: &mut CashflowRow) {
        // Mortality
        let baseline_annual = self.assumptions.mortality.baseline_annual_rate(
            state.attained_age,
            policy.gender,
        );
        row.baseline_mortality = baseline_annual;
        row.mortality_improvement = 0.015; // 1.5% annual improvement

        // Final mortality with improvement applied
        row.final_mortality = self.assumptions.mortality.monthly_rate(
            state.attained_age,
            policy.gender,
            state.projection_month,
        );

        // Surrender charge
        row.surrender_charge = self.assumptions.product.base.surrender_charges.get_rate(state.policy_year);

        // Free partial withdrawal percentage (incorporating RMD for qualified contracts)
        // Excel Column J: =IF(C11=1,0,IF($C$4="Q",MAX(base_free%,RMD_rate),base_free%))
        row.fpw_pct = self.assumptions.pwd.get_fpw_pct(
            state.policy_year,
            state.attained_age,
            policy.qual_status,
        );

        // GLWB activation status
        row.glwb_activated = state.income_activated;

        // Non-systematic PWD rate (0 for month 1 of each policy year per Excel)
        row.non_systematic_pwd_rate = self.assumptions.pwd.monthly_pwd_rate_adjusted(
            state.policy_year,
            state.month_in_policy_year,
            state.attained_age,
            policy.qual_status,
            state.income_activated,
        );

        // Lapse components
        // Use prior period's ITM to match Excel's behavior (row N uses row N-1's BB/AV)
        let itm = state.prior_itm();

        // Lapse skew from model (shock year: 40%/30%/20%/0.83%..., otherwise 1/12)
        row.lapse_skew = self.assumptions.lapse.get_skew(
            state.policy_year,
            state.month_in_policy_year,
            policy.sc_period as u32,
        );
        row.base_lapse_component = self.assumptions.lapse.base_component_with_bucket(
            state.policy_year,
            state.income_activated,
            policy.benefit_base_bucket,
            policy.sc_period as u32,
        );
        row.dynamic_lapse_component = self.assumptions.lapse.dynamic_component(itm, state.income_activated);

        // Final monthly lapse rate
        // No lapses when AV = 0 (nothing to surrender)
        row.final_lapse_rate = if state.bop_av <= 0.0 {
            0.0
        } else if let Some(annual_rate) = self.config.fixed_lapse_rate {
            // Fixed lapse rate for testing: even 1/12 monthly skew, 0 in month 1
            if state.projection_month == 1 {
                0.0
            } else {
                1.0 - (1.0 - annual_rate).powf(1.0 / 12.0)
            }
        } else {
            // Normal predictive model with shock year skew
            self.assumptions.lapse.monthly_lapse_rate_with_skew(
                state.projection_month,
                state.policy_year,
                state.month_in_policy_year,
                state.income_activated,
                itm,
                policy.sc_period as u32,
                policy.benefit_base_bucket,
            )
        };

        // Rider charge rate - annual, only applied when MOD(projection_month, 12) = 0
        // Excel: =IF(K12=1,1.5%,0.5%)*IF(MOD(B12,12)=0,1,0)
        row.rider_charge_rate = if state.projection_month % 12 == 0 {
            if state.income_activated { 0.015 } else { 0.005 }
        } else {
            0.0
        };

        // Credited rate
        row.credited_rate = self.calculate_credited_rate(policy, state);

        // Systematic withdrawal (if income activated)
        row.systematic_withdrawal = if state.income_activated {
            let annual_max = self.assumptions.product.glwb.max_annual_withdrawal(
                state.bop_benefit_base,
                state.attained_age,
            );
            (annual_max - state.ytd_systematic_wd).max(0.0) / (13.0 - state.month_in_policy_year as f64)
        } else {
            0.0
        };

        // Rollup rate (monthly)
        row.rollup_rate = if state.policy_year <= policy.sc_period as u32 && !state.income_activated {
            policy.bonus / 12.0 * 10.0 // Simple 10% annual rollup
        } else {
            0.0
        };
    }

    /// Calculate credited rate based on configuration
    fn calculate_credited_rate(&self, _policy: &Policy, state: &ProjectionState) -> f64 {
        match &self.config.crediting {
            CreditingApproach::OptionBudget { budget_rate, equity_kicker } => {
                (*budget_rate + *equity_kicker) / 12.0
            }
            CreditingApproach::ScenarioBased { floor, cap, participation, index_return } => {
                let raw_credit = index_return * participation;
                raw_credit.max(*floor).min(*cap) / 12.0
            }
            CreditingApproach::Fixed(rate) => rate / 12.0,
            CreditingApproach::IndexedAnnual { annual_rate } => {
                // Credit earned in year N is applied at month 1 of year N+1 (i.e., month 13, 25, 37...)
                // Years 1-10 performance get full rate, years 11+ performance get half rate
                // The credit at month 13 is for year 1 performance (full rate)
                // The credit at month 121 is for year 10 performance (full rate)
                // The credit at month 133 is for year 11 performance (half rate)
                if state.month_in_policy_year == 1 && state.policy_year > 1 {
                    let crediting_for_year = state.policy_year - 1; // Year whose performance we're crediting
                    let rate_multiplier = if crediting_for_year <= 10 { 1.0 } else { 0.5 };
                    *annual_rate * rate_multiplier
                } else {
                    0.0
                }
            }
        }
    }

    /// Apply decrements and calculate persistency
    /// Excel: Z = Lives persistency = (1-H)*(1-S) where H=mortality, S=lapse
    fn apply_decrements(&self, state: &ProjectionState, row: &mut CashflowRow) {
        let mortality_decrement = row.final_mortality;
        let lapse_decrement = row.final_lapse_rate;
        let _pwd_decrement = row.non_systematic_pwd_rate;

        // Persistency factors - use multiplication per Excel formula
        // Lives persistency = (1-mortality)*(1-lapse)
        let monthly_persistency = (1.0 - mortality_decrement) * (1.0 - lapse_decrement);

        row.av_persistency = state.av_persistency * monthly_persistency;
        row.bb_persistency = state.bb_persistency * monthly_persistency;
        row.lives_persistency = state.lives_persistency * monthly_persistency;

        // Updated lives
        row.lives = state.lives * row.lives_persistency / state.lives_persistency;
    }

    /// Calculate dollar cashflows using Excel's proportional allocation approach
    /// Excel allocates total decrement pool proportionally based on rates
    fn calculate_cashflows(&self, policy: &Policy, state: &ProjectionState, row: &mut CashflowRow) {
        let bop_av = state.bop_av;
        let lives = state.lives;

        // Excel column V: Systematic withdrawal (only if GLWB activated)
        let systematic_wd = row.systematic_withdrawal;

        // Excel column AB: Pre-decrement AV = (BOP_AV - Systematic_WD) * (1 + Credited_Rate)
        let pre_dec_av = (bop_av - systematic_wd).max(0.0) * (1.0 + row.credited_rate);
        row.pre_decrement_av = pre_dec_av;

        // Rider charge expressed as rate: T * P / O (annual rate * BB / AV)
        let rider_rate = if bop_av > 0.0 {
            row.rider_charge_rate * state.bop_benefit_base / bop_av
        } else {
            0.0
        };

        // Excel column X: AV persistency = (1-H)*(1-S)*(1-L)*(1-rider_rate)
        let av_persistency = (1.0 - row.final_mortality)
            * (1.0 - row.final_lapse_rate)
            * (1.0 - row.non_systematic_pwd_rate)
            * (1.0 - rider_rate);

        // Total decrement pool = Pre_dec_AV * (1 - AV_persistency)
        let decrement_pool = pre_dec_av * (1.0 - av_persistency);

        // Sum of all rates for proportional allocation
        let sum_of_rates = row.final_mortality
            + row.final_lapse_rate
            + row.non_systematic_pwd_rate
            + rider_rate;

        // Proportional allocation of decrements (per-policy amounts, not multiplied by lives)
        // These match Excel columns AC-AG
        let (mort_dec, lapse_dec, pwd_dec, rider_dec, surr_chg_dec) = if sum_of_rates > 0.0 {
            let allocation_base = decrement_pool / sum_of_rates;

            // Mortality = Pool * H / sum
            let mort = allocation_base * row.final_mortality;

            // FPW% is already 0 for year 1 from get_fpw_pct
            let fpw_pct = row.fpw_pct;

            // Lapse (net of SC) = Pool * S / sum * (FPW% + (1-FPW%)*(1-SC))
            let net_of_sc_factor = fpw_pct + (1.0 - fpw_pct) * (1.0 - row.surrender_charge);
            let lapse = allocation_base * row.final_lapse_rate * net_of_sc_factor;

            // Surrender charges = Pool * S / sum * (1-FPW%) * SC
            let surr_chg = allocation_base * row.final_lapse_rate * (1.0 - fpw_pct) * row.surrender_charge;

            // PWD = Pool * L / sum + Systematic_WD
            let pwd = allocation_base * row.non_systematic_pwd_rate + systematic_wd;

            // Rider charges = Pool * rider_rate / sum
            let rider = allocation_base * rider_rate;

            (mort, lapse, pwd, rider, surr_chg)
        } else {
            (0.0, 0.0, systematic_wd, 0.0, 0.0)
        };

        // Store per-policy decrement amounts (these are what Excel shows in AC-AH)
        row.mortality_dec = mort_dec;
        row.lapse_dec = lapse_dec;
        row.pwd_dec = pwd_dec;
        row.rider_charges_dec = rider_dec;
        row.surrender_charges_dec = surr_chg_dec;

        // Excel column AH: Interest credits = Pre_dec_AV - MAX(0, BOP_AV - Systematic_WD)
        let interest_credits = pre_dec_av - (bop_av - systematic_wd).max(0.0);
        row.interest_credits_dec = interest_credits;

        // Total cashflows (per-policy * lives)
        row.mortality_cf = mort_dec * lives;
        row.lapse_cf = lapse_dec * lives;
        row.pwd_cf = pwd_dec * lives;
        row.rider_charges_cf = rider_dec * lives;
        row.surrender_charges_cf = surr_chg_dec * lives;
        row.interest_credits_cf = interest_credits * lives;

        // Excel column AI: EOP AV = MAX(0, BOP_AV + Interest_credits - sum(decrements))
        // Floor at 0: once AV is exhausted, the guarantee kicks in
        // Note: For single-policy projection, we track per-policy EOP AV
        row.eop_av = (bop_av + interest_credits - (mort_dec + lapse_dec + pwd_dec + rider_dec + surr_chg_dec)).max(0.0);

        // Expenses (simplified)
        row.expenses = lives * 10.0 / 12.0; // $10/policy/year

        // Commission (only in first year)
        if state.policy_year == 1 && state.month_in_policy_year == 1 {
            row.commission = policy.initial_premium * 0.05; // 5% first year commission
        }

        // Total net cashflow
        row.total_net_cashflow = row.premium
            - row.mortality_cf
            - row.lapse_cf
            - row.pwd_cf
            + row.rider_charges_cf
            + row.surrender_charges_cf
            - row.expenses
            - row.commission;
    }

    /// Update benefit base for next month
    /// Excel formula: =P11*Y11*(1+IF(AND(D11=12,K11=0),1,0)*W11)
    /// where Y11 = (1-H11)*(1-S11)*(1-L11) = BB persistency
    /// Rollup only applies at month 12 when GLWB not activated
    fn update_benefit_base(&self, policy: &Policy, state: &mut ProjectionState, row: &CashflowRow) {
        // Calculate BB persistency for this month: (1-mort)*(1-lapse)*(1-pwd)
        let monthly_bb_persistency = (1.0 - row.final_mortality)
            * (1.0 - row.final_lapse_rate)
            * (1.0 - row.non_systematic_pwd_rate);

        // Apply BB persistency
        state.bop_benefit_base = state.bop_benefit_base * monthly_bb_persistency;

        if state.income_activated {
            // After income activation, BB is only reduced by persistency (mortality, lapse)
            // Systematic withdrawals come from AV, not BB
            // No rollup after income activation
        } else if state.month_in_policy_year == 12 && state.policy_year <= policy.sc_period as u32 {
            // Rollup at month 12 during SC period when GLWB not activated
            // 10% simple interest on premium, applied multiplicatively to persisted BB
            // Excel: W = (1+Bonus+0.1*MIN(10,PY))/(1+Bonus+0.1*MIN(10,PY-1))-1
            let py = (state.policy_year as f64).min(10.0);
            let py_prev = ((state.policy_year - 1) as f64).min(10.0);
            let rollup_factor = (1.0 + policy.bonus + 0.10 * py)
                              / (1.0 + policy.bonus + 0.10 * py_prev);
            state.bop_benefit_base = state.bop_benefit_base * rollup_factor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{Policy, QualStatus, Gender, CreditingStrategy, RollupType};

    fn test_policy() -> Policy {
        Policy::new(
            2800,
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
        )
    }

    #[test]
    fn test_projection_runs() {
        let assumptions = Assumptions::default_pricing();
        let config = ProjectionConfig {
            projection_months: 12,
            ..Default::default()
        };

        let engine = ProjectionEngine::new(assumptions, config);
        let policy = test_policy();

        let result = engine.project_policy(&policy);

        assert_eq!(result.cashflows.len(), 12);
        assert!(result.cashflows[0].bop_av > 0.0);
        assert!(result.cashflows[0].lives > 0.0);
    }

    #[test]
    fn test_decrements_positive() {
        let assumptions = Assumptions::default_pricing();
        let config = ProjectionConfig {
            projection_months: 1,
            ..Default::default()
        };

        let engine = ProjectionEngine::new(assumptions, config);
        let policy = test_policy();

        let result = engine.project_policy(&policy);
        let row = &result.cashflows[0];

        // All decrement rates should be positive and less than 1
        assert!(row.final_mortality > 0.0 && row.final_mortality < 1.0);
        assert!(row.final_lapse_rate >= 0.0 && row.final_lapse_rate < 1.0);
    }

    #[test]
    fn test_av_decreases_over_time() {
        let assumptions = Assumptions::default_pricing();
        let config = ProjectionConfig {
            projection_months: 120,
            ..Default::default()
        };

        let engine = ProjectionEngine::new(assumptions, config);
        let policy = test_policy();

        let result = engine.project_policy(&policy);

        // With no crediting, AV should decrease over time due to charges
        let first_av = result.cashflows[0].bop_av;
        let last_av = result.cashflows.last().unwrap().eop_av;

        assert!(last_av < first_av);
    }
}
