//! Cashflow output structures for projections

use serde::{Deserialize, Serialize};

/// A single row of projection output for one month
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashflowRow {
    // Timing
    pub projection_month: u32,
    pub policy_year: u32,
    pub month_in_policy_year: u32,
    pub attained_age: u8,

    // Decrements (rates)
    pub baseline_mortality: f64,
    pub mortality_improvement: f64,
    pub final_mortality: f64,
    pub surrender_charge: f64,
    pub fpw_pct: f64,
    pub glwb_activated: bool,
    pub non_systematic_pwd_rate: f64,
    pub lapse_skew: f64,
    pub base_lapse_component: f64,
    pub dynamic_lapse_component: f64,
    pub final_lapse_rate: f64,

    // Account values and benefit base
    pub premium: f64,
    pub bop_av: f64,
    pub bop_benefit_base: f64,
    pub pre_decrement_av: f64,

    // Rider calculations
    pub rider_charge_rate: f64,
    pub credited_rate: f64,
    pub systematic_withdrawal: f64,
    pub rollup_rate: f64,

    // Persistency
    pub av_persistency: f64,
    pub bb_persistency: f64,
    pub lives_persistency: f64,
    pub lives: f64,

    // Per-policy decrement amounts (Excel columns AC-AH)
    pub mortality_dec: f64,
    pub lapse_dec: f64,
    pub pwd_dec: f64,
    pub rider_charges_dec: f64,
    pub surrender_charges_dec: f64,
    pub interest_credits_dec: f64,

    // Cashflows (dollar amounts = per-policy * lives)
    pub mortality_cf: f64,
    pub lapse_cf: f64,
    pub pwd_cf: f64,
    pub rider_charges_cf: f64,
    pub surrender_charges_cf: f64,
    pub interest_credits_cf: f64,
    pub eop_av: f64,

    // Expenses
    pub expenses: f64,
    pub commission: f64,
    pub chargebacks: f64,
    pub bonus_comp: f64,

    // Summary
    pub total_net_cashflow: f64,
    pub net_index_credit_reimbursement: f64,
    pub hedge_gains: f64,
}

impl CashflowRow {
    /// Create a new cashflow row with default values
    pub fn new(projection_month: u32) -> Self {
        Self {
            projection_month,
            policy_year: 1,
            month_in_policy_year: 1,
            attained_age: 0,
            baseline_mortality: 0.0,
            mortality_improvement: 0.0,
            final_mortality: 0.0,
            surrender_charge: 0.0,
            fpw_pct: 0.0,
            glwb_activated: false,
            non_systematic_pwd_rate: 0.0,
            lapse_skew: 0.0,
            base_lapse_component: 0.0,
            dynamic_lapse_component: 0.0,
            final_lapse_rate: 0.0,
            premium: 0.0,
            bop_av: 0.0,
            bop_benefit_base: 0.0,
            pre_decrement_av: 0.0,
            rider_charge_rate: 0.0,
            credited_rate: 0.0,
            systematic_withdrawal: 0.0,
            rollup_rate: 0.0,
            av_persistency: 1.0,
            bb_persistency: 1.0,
            lives_persistency: 1.0,
            lives: 0.0,
            mortality_dec: 0.0,
            lapse_dec: 0.0,
            pwd_dec: 0.0,
            rider_charges_dec: 0.0,
            surrender_charges_dec: 0.0,
            interest_credits_dec: 0.0,
            mortality_cf: 0.0,
            lapse_cf: 0.0,
            pwd_cf: 0.0,
            rider_charges_cf: 0.0,
            surrender_charges_cf: 0.0,
            interest_credits_cf: 0.0,
            eop_av: 0.0,
            expenses: 0.0,
            commission: 0.0,
            chargebacks: 0.0,
            bonus_comp: 0.0,
            total_net_cashflow: 0.0,
            net_index_credit_reimbursement: 0.0,
            hedge_gains: 0.0,
        }
    }
}

/// Complete projection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionResult {
    /// Policy identifier
    pub policy_id: u32,

    /// Monthly cashflow rows
    pub cashflows: Vec<CashflowRow>,

    /// Total present value of liabilities
    pub pv_liabilities: f64,

    /// Total present value of premiums
    pub pv_premiums: f64,
}

impl ProjectionResult {
    pub fn new(policy_id: u32) -> Self {
        Self {
            policy_id,
            cashflows: Vec::new(),
            pv_liabilities: 0.0,
            pv_premiums: 0.0,
        }
    }

    /// Add a cashflow row
    pub fn add_row(&mut self, row: CashflowRow) {
        self.cashflows.push(row);
    }

    /// Get summary statistics
    pub fn summary(&self) -> ProjectionSummary {
        let total_premium: f64 = self.cashflows.iter().map(|r| r.premium).sum();
        let total_mortality: f64 = self.cashflows.iter().map(|r| r.mortality_cf).sum();
        let total_lapse: f64 = self.cashflows.iter().map(|r| r.lapse_cf).sum();
        let total_pwd: f64 = self.cashflows.iter().map(|r| r.pwd_cf).sum();
        let total_rider_charges: f64 = self.cashflows.iter().map(|r| r.rider_charges_cf).sum();
        let total_net_cf: f64 = self.cashflows.iter().map(|r| r.total_net_cashflow).sum();

        let final_av = self.cashflows.last().map(|r| r.eop_av).unwrap_or(0.0);
        let final_lives = self.cashflows.last().map(|r| r.lives).unwrap_or(0.0);

        ProjectionSummary {
            total_months: self.cashflows.len() as u32,
            total_premium,
            total_mortality,
            total_lapse,
            total_pwd,
            total_rider_charges,
            total_net_cf,
            final_av,
            final_lives,
        }
    }
}

/// Summary statistics for a projection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionSummary {
    pub total_months: u32,
    pub total_premium: f64,
    pub total_mortality: f64,
    pub total_lapse: f64,
    pub total_pwd: f64,
    pub total_rider_charges: f64,
    pub total_net_cf: f64,
    pub final_av: f64,
    pub final_lives: f64,
}
