//! Projection state tracking for a single policy

use crate::policy::Policy;

/// State of a policy at a point in time during projection
#[derive(Debug, Clone)]
pub struct ProjectionState {
    /// Current projection month (1-indexed)
    pub projection_month: u32,

    /// Policy year (1-indexed)
    pub policy_year: u32,

    /// Month within policy year (1-12)
    pub month_in_policy_year: u32,

    /// Attained age
    pub attained_age: u8,

    /// Beginning of period account value
    pub bop_av: f64,

    /// Beginning of period benefit base
    pub bop_benefit_base: f64,

    /// End of period account value
    pub eop_av: f64,

    /// Number of lives (policies in force)
    pub lives: f64,

    /// AV persistency factor (cumulative survival for AV)
    pub av_persistency: f64,

    /// Benefit base persistency factor
    pub bb_persistency: f64,

    /// Lives persistency factor
    pub lives_persistency: f64,

    /// Whether GLWB income has been activated
    pub income_activated: bool,

    /// Cumulative systematic withdrawals taken this policy year
    pub ytd_systematic_wd: f64,

    /// Cumulative non-systematic withdrawals taken this policy year
    pub ytd_non_systematic_wd: f64,

    /// Initial benefit base (for simple rollup calculations)
    pub initial_benefit_base: f64,

    /// Prior period's BOP AV (for lagged ITM calculation - matches Excel's behavior)
    pub prior_bop_av: f64,

    /// Prior period's BOP BB (for lagged ITM calculation - matches Excel's behavior)
    pub prior_bop_bb: f64,
}

impl ProjectionState {
    /// Initialize state from a policy at projection start
    pub fn from_policy(policy: &Policy) -> Self {
        Self {
            projection_month: 0,
            policy_year: 1,
            month_in_policy_year: 0,
            attained_age: policy.issue_age,
            bop_av: policy.starting_av(),
            bop_benefit_base: policy.starting_benefit_base(),
            eop_av: policy.starting_av(),
            lives: policy.initial_pols,
            av_persistency: 1.0,
            bb_persistency: 1.0,
            lives_persistency: 1.0,
            income_activated: policy.income_activated,
            ytd_systematic_wd: 0.0,
            ytd_non_systematic_wd: 0.0,
            initial_benefit_base: policy.starting_benefit_base(),
            // Prior BOP values for lagged ITM calc (initial values for first month)
            prior_bop_av: policy.starting_av(),
            prior_bop_bb: policy.starting_benefit_base(),
        }
    }

    /// Advance to next month
    pub fn advance_month(&mut self, policy: &Policy) {
        // Note: prior_bop_av and prior_bop_bb are saved in the engine at end of calculate_month
        // BEFORE update_benefit_base modifies bop_benefit_base

        self.projection_month += 1;

        // Update timing
        self.policy_year = policy.policy_year(self.projection_month);
        self.month_in_policy_year = policy.month_in_policy_year(self.projection_month);
        self.attained_age = policy.attained_age(self.projection_month);

        // Check for GLWB activation at start of policy year
        if !self.income_activated && policy.should_activate_income(self.projection_month) {
            self.income_activated = true;
        }

        // Reset YTD amounts at start of policy year
        if self.month_in_policy_year == 1 {
            self.ytd_systematic_wd = 0.0;
            self.ytd_non_systematic_wd = 0.0;
        }

        // BOP values come from prior EOP
        self.bop_av = self.eop_av;
        // Benefit base is updated via rollup in the engine
    }

    /// Pre-decrement account value (before applying decrements)
    pub fn pre_decrement_av(&self) -> f64 {
        self.bop_av
    }

    /// Calculate ITM-ness (benefit base / account value)
    pub fn itm_ness(&self) -> f64 {
        if self.bop_av <= 0.0 {
            1.0
        } else {
            self.bop_benefit_base / self.bop_av
        }
    }

    /// Calculate prior period's ITM-ness (for lapse calculation)
    /// This matches Excel's behavior where dynamic component uses prior row's BB/AV
    pub fn prior_itm(&self) -> f64 {
        if self.prior_bop_av <= 0.0 {
            1.0
        } else {
            self.prior_bop_bb / self.prior_bop_av
        }
    }
}
