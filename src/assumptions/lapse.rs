//! Lapse/surrender predictive model
//!
//! Implements the lapse model matching the Excel "Surrender predictive model" structure.
//! Uses a log link (not logit): annual_prob = exp(linear_predictor)

use crate::policy::BenefitBaseBucket;

/// Lapse model with predictive factors
#[derive(Debug, Clone)]
pub struct LapseModel {
    /// Model coefficients
    pub coefficients: LapseCoefficients,

    /// Bucket-specific coefficients
    pub bucket_coefficients: BucketCoefficients,

    /// Pre-calculated base linear predictor by policy year (excluding ITM and bucket terms)
    /// Index 0 = policy year 1, etc.
    /// These are for the reference bucket [0, 50000)
    precalc_by_year: Vec<f64>,
}

/// Coefficients for the predictive lapse model
#[derive(Debug, Clone)]
pub struct LapseCoefficients {
    /// ITM-ness effect for clamped range [0.5, 1]
    pub itm_low: f64,
    /// ITM-ness effect for clamped range [1, 2]
    pub itm_high: f64,
    /// Main income effect (IncomeStartedY) - large negative value
    pub income_main: f64,
    /// Income × ITM low interaction
    pub income_itm_low: f64,
}

impl Default for LapseCoefficients {
    fn default() -> Self {
        Self {
            itm_low: -3.16184447006944,
            itm_high: -1.15717209704794,
            income_main: -2.41891458766257,  // IncomeStartedY coefficient
            income_itm_low: 1.53610221716995,
        }
    }
}

/// Coefficients for bucket adjustments in the lapse model
/// Buckets: [0, 50000), [50000, 100000), [100000, 200000), [200000, Inf)
/// The precalc_by_year values are calculated for [200000, Inf) bucket (index 3)
#[derive(Debug, Clone)]
pub struct BucketCoefficients {
    /// Main bucket effects (additive to intercept)
    /// Order: [0, 50000), [50000, 100000), [100000, 200000), [200000, Inf)
    pub main: [f64; 4],

    /// Duration polynomial interactions: poly1 coefficient by bucket
    pub poly1: [f64; 4],

    /// Duration polynomial interactions: poly2 coefficient by bucket
    pub poly2: [f64; 4],

    /// Income interaction by bucket
    pub income: [f64; 4],

    /// Shock year indicator interaction by bucket
    pub shock_year: [f64; 4],

    /// Post-shock poly1 interaction by bucket
    pub post_shock_poly1: [f64; 4],

    /// Post-shock poly2 interaction by bucket
    pub post_shock_poly2: [f64; 4],
}

impl Default for BucketCoefficients {
    fn default() -> Self {
        // From surrender_predictive_model.csv
        // Index 0: [0, 50000) - reference bucket in R model (all zeros)
        // Index 1: [50000, 100000)
        // Index 2: [100000, 200000)
        // Index 3: [200000, Inf) - this is what precalc values are based on
        Self {
            main: [0.0, -0.157400822647813, -0.249985676390188, -0.338729473320792],
            poly1: [0.0, 0.0682532283448409, 0.0763149050501966, 0.0577584207560845],
            poly2: [0.0, 0.00291547472994642, 0.00234424354188925, 0.00156198120112268],
            income: [0.0, -0.0925723455729023, -0.134728779396966, -0.0656576115761846],
            shock_year: [0.0, 0.577678462673537, 0.469928825868869, 0.472851885434387],
            post_shock_poly1: [0.0, 0.544650716473373, 0.705070763116629, 0.75719904977134],
            post_shock_poly2: [0.0, -0.908776562309262, -0.826641853779992, -0.839435686720885],
        }
    }
}

impl BucketCoefficients {
    /// Get the bucket index for coefficient lookup
    /// Index 0: [0, 50000), Index 1: [50000, 100000), Index 2: [100000, 200000), Index 3: [200000, Inf)
    fn bucket_index(bucket: BenefitBaseBucket) -> usize {
        match bucket {
            BenefitBaseBucket::Under50k => 0,
            BenefitBaseBucket::From50kTo100k => 1,
            BenefitBaseBucket::From100kTo200k => 2,
            BenefitBaseBucket::From200kTo500k => 3,  // Uses [200000, Inf) coefficients
            BenefitBaseBucket::Over500k => 3,        // Uses [200000, Inf) coefficients
        }
    }

    /// Calculate the raw bucket terms for a given bucket index
    fn raw_bucket_terms(
        &self,
        idx: usize,
        poly1: f64,
        poly2: f64,
        shock_ind: f64,
        post_shock_poly1: f64,
        post_shock_poly2: f64,
        income_ind: f64,
    ) -> f64 {
        self.main[idx]
            + self.poly1[idx] * poly1
            + self.poly2[idx] * poly2
            + self.income[idx] * income_ind
            + self.shock_year[idx] * shock_ind
            + self.post_shock_poly1[idx] * post_shock_poly1
            + self.post_shock_poly2[idx] * post_shock_poly2
    }

    /// Calculate bucket adjustment relative to [200000, Inf) which is the precalc base
    /// The precalc values already include [200000, Inf) bucket effects (index 3) for non-income case
    ///
    /// Key insight: When income is NOT activated, we use full bucket terms (main + poly + shock + post-shock).
    /// When income IS activated, the polynomial bucket×duration interactions don't apply.
    /// CRITICAL: The precalc already includes base bucket polynomial terms, so we must subtract
    /// those when income activates (since they shouldn't apply in the income-on scenario).
    pub fn adjustment(
        &self,
        bucket: BenefitBaseBucket,
        policy_year: u32,
        sc_period: u32,
        income_activated: bool,
    ) -> f64 {
        let target_idx = Self::bucket_index(bucket);

        // Duration polynomial terms: pmin(0, Duration - SCP)
        let duration = policy_year as i32;
        let scp = sc_period as i32;
        let duration_minus_scp = (duration - scp).min(0) as f64;
        let poly1 = duration_minus_scp;
        let poly2 = duration_minus_scp * duration_minus_scp;

        // Shock year indicator: Duration == SCP + 1
        let is_shock_year = policy_year == sc_period + 1;
        let shock_ind = if is_shock_year { 1.0 } else { 0.0 };

        // Post-shock polynomial: if_else(Duration > SCP, 1, 0) / pmax(1, pmin(3, Duration - SCP))
        let post_shock_term = if policy_year > sc_period {
            let denom = ((policy_year - sc_period) as f64).max(1.0).min(3.0);
            1.0 / denom
        } else {
            0.0
        };
        let post_shock_poly1 = post_shock_term;
        let post_shock_poly2 = post_shock_term * post_shock_term;

        if income_activated {
            // When income is activated, polynomial bucket interactions don't apply.
            // The precalc includes base bucket (idx 3) polynomial terms that we must remove.
            // We add: target main effect + target income interaction
            // We subtract: base bucket's polynomial terms (already in precalc)

            // Base bucket terms that precalc already includes (and we need to remove)
            let base_poly_terms = self.poly1[3] * poly1
                + self.poly2[3] * poly2
                + self.shock_year[3] * shock_ind
                + self.post_shock_poly1[3] * post_shock_poly1
                + self.post_shock_poly2[3] * post_shock_poly2;

            // Target bucket: only main effect + income interaction (no poly terms when income on)
            let target_terms = self.main[target_idx] + self.income[target_idx];

            // Base bucket: main effect (no poly since we're removing them from precalc)
            let base_main = self.main[3];

            return (target_terms - base_main) - base_poly_terms;
        }

        // When income is NOT activated, use full bucket terms (main + poly + shock + post-shock)
        let base_bucket_terms = self.raw_bucket_terms(3, poly1, poly2, shock_ind, post_shock_poly1, post_shock_poly2, 0.0);
        let target_bucket_terms = self.raw_bucket_terms(target_idx, poly1, poly2, shock_ind, post_shock_poly1, post_shock_poly2, 0.0);
        target_bucket_terms - base_bucket_terms
    }
}

impl LapseModel {
    /// Create from loaded CSV assumptions
    /// Note: The lapse model uses pre-calculated values from the surrender predictive model.
    /// The CSV provides raw R model coefficients; for now we use the pre-calibrated values.
    pub fn from_loaded(loaded: &super::loader::LoadedAssumptions) -> Self {
        // Extract key ITM coefficients from loaded model if available
        let mut coefficients = LapseCoefficients::default();

        if let Some(&itm_low) = loaded.surrender_model.get("I(pmax(0.5, pmin(1, ITMness)))") {
            coefficients.itm_low = itm_low;
        }
        if let Some(&itm_high) = loaded.surrender_model.get("I(pmax(1, pmin(2, ITMness)))") {
            coefficients.itm_high = itm_high;
        }
        if let Some(&income_main) = loaded.surrender_model.get("IncomeStartedY") {
            coefficients.income_main = income_main;
        }
        if let Some(&income_itm) = loaded.surrender_model.get("IncomeStartedY:I(pmax(0.5, pmin(1, ITMness)))") {
            coefficients.income_itm_low = income_itm;
        }

        Self {
            coefficients,
            bucket_coefficients: BucketCoefficients::default(),
            // Pre-calculated values for bucket [200000, Inf) (index 3)
            // These exclude ITM terms but INCLUDE bucket effects for [200000, Inf)
            // Bucket adjustments are calculated as differences from this base
            precalc_by_year: vec![
                -1.4257937264401424,  // Year 1
                -0.9061294780969887,  // Year 2
                -0.3805864186366955,  // Year 3
                0.15083545194073789,  // Year 4
                0.329461260874028,    // Year 5
                0.513965880924458,    // Year 6
                0.704349312092028,    // Year 7
                0.9006115543767378,   // Year 8
                1.1027526077785876,   // Year 9
                1.310772472297577,    // Year 10
                2.9366733874333395,   // Year 11 (shock year)
                2.083416198115829,    // Year 12
                2.1066423172719184,   // Year 13+
            ],
        }
    }

    /// Create default predictive model matching Excel calibration
    pub fn default_predictive_model() -> Self {
        Self {
            coefficients: LapseCoefficients::default(),
            bucket_coefficients: BucketCoefficients::default(),
            // Pre-calculated values for bucket [200000, Inf) (index 3)
            // These exclude ITM terms but INCLUDE bucket effects for [200000, Inf)
            // Bucket adjustments are calculated as differences from this base
            precalc_by_year: vec![
                -1.4257937264401424,  // Year 1
                -0.9061294780969887,  // Year 2
                -0.3805864186366955,  // Year 3
                0.15083545194073789,  // Year 4
                0.329461260874028,    // Year 5
                0.513965880924458,    // Year 6
                0.704349312092028,    // Year 7
                0.9006115543767378,   // Year 8
                1.1027526077785876,   // Year 9
                1.310772472297577,    // Year 10
                2.9366733874333395,   // Year 11 (shock year)
                2.083416198115829,    // Year 12
                2.1066423172719184,   // Year 13+
            ],
        }
    }

    /// Get pre-calculated base value for a policy year (excluding ITM terms)
    fn precalc_for_year(&self, policy_year: u32) -> f64 {
        let idx = (policy_year as usize).saturating_sub(1);
        self.precalc_by_year
            .get(idx)
            .copied()
            .unwrap_or_else(|| *self.precalc_by_year.last().unwrap_or(&0.0))
    }

    /// Calculate the base component (linear predictor scale)
    /// This adds ITM coefficients at base level (assuming ITM effects at their intercept)
    /// and bucket-specific adjustments
    pub fn base_component_with_bucket(
        &self,
        policy_year: u32,
        income_activated: bool,
        bucket: BenefitBaseBucket,
        sc_period: u32,
    ) -> f64 {
        let c = &self.coefficients;

        // Get pre-calculated value for this policy year (for reference bucket)
        let precalc = self.precalc_for_year(policy_year);

        // Add ITM coefficients at base level (these get adjusted by dynamic component)
        // When income is activated, add the large negative IncomeStartedY effect
        let income_ind = if income_activated { 1.0 } else { 0.0 };

        // Add bucket-specific adjustment
        let bucket_adj = self.bucket_coefficients.adjustment(bucket, policy_year, sc_period, income_activated);

        precalc + c.itm_low + c.itm_high + c.income_main * income_ind + c.income_itm_low * income_ind + bucket_adj
    }

    /// Calculate the base component for reference bucket [0, 50000)
    /// Use base_component_with_bucket for other buckets
    pub fn base_component(
        &self,
        policy_year: u32,
        income_activated: bool,
    ) -> f64 {
        self.base_component_with_bucket(policy_year, income_activated, BenefitBaseBucket::Under50k, 10)
    }

    /// Calculate the dynamic component based on actual ITM-ness
    /// This adjusts the base ITM assumptions for actual ITM level
    pub fn dynamic_component(
        &self,
        itm_ness: f64,
        income_activated: bool,
    ) -> f64 {
        let c = &self.coefficients;

        // Clamped ITM values
        let itm_low_clamped = itm_ness.max(0.5).min(1.0);
        let itm_high_clamped = itm_ness.max(1.0).min(2.0);

        let income_ind = if income_activated { 1.0 } else { 0.0 };

        // Dynamic adjusts from base (ITM=1) to actual ITM
        // Base assumed: itm_low=1.0, itm_high=1.0
        // Dynamic = coef * (actual - 1)
        c.itm_high * (itm_high_clamped - 1.0)
            + c.itm_low * (itm_low_clamped - 1.0)
            + c.income_itm_low * income_ind * (itm_low_clamped - 1.0)
    }

    /// Calculate annual lapse probability using log link with bucket adjustment
    /// p = exp(base + dynamic)
    pub fn annual_lapse_prob_with_bucket(
        &self,
        policy_year: u32,
        income_activated: bool,
        itm_ness: f64,
        bucket: BenefitBaseBucket,
        sc_period: u32,
    ) -> f64 {
        let base = self.base_component_with_bucket(policy_year, income_activated, bucket, sc_period);
        let dynamic = self.dynamic_component(itm_ness, income_activated);
        let linear_predictor = base + dynamic;

        // Log link: p = exp(eta)
        // Cap at reasonable maximum to avoid overflow
        linear_predictor.min(0.0).exp().min(1.0)
    }

    /// Calculate annual lapse probability for reference bucket
    pub fn annual_lapse_prob(
        &self,
        policy_year: u32,
        income_activated: bool,
        itm_ness: f64,
    ) -> f64 {
        self.annual_lapse_prob_with_bucket(policy_year, income_activated, itm_ness, BenefitBaseBucket::Under50k, 10)
    }

    /// Calculate monthly lapse rate
    /// Converts annual probability to monthly using: 1 - (1 - p_annual)^skew
    /// where skew is normally 1/12, but 0.4 for shock year month 1
    /// Returns 0 for projection month 1 (Excel hardcodes this)
    pub fn monthly_lapse_rate(
        &self,
        projection_month: u32,
        policy_year: u32,
        income_activated: bool,
        itm_ness: f64,
    ) -> f64 {
        // Month 1 has no lapse (Excel rule)
        if projection_month == 1 {
            return 0.0;
        }

        // Account value must be positive for lapse
        if itm_ness <= 0.0 {
            return 0.0;
        }

        let annual_prob = self.annual_lapse_prob(policy_year, income_activated, itm_ness);

        // Default skew is 1/12 (uniform monthly distribution)
        let skew = 1.0 / 12.0;

        // Convert annual to monthly using skew
        1.0 - (1.0 - annual_prob).powf(skew)
    }

    /// Calculate monthly lapse rate with shock year skew and bucket adjustment
    /// In the shock year (first year without surrender charges), skew varies by month:
    /// Month 1: 40%, Month 2: 30%, Month 3: 20%, Months 4-12: 1/120 each
    pub fn monthly_lapse_rate_with_skew(
        &self,
        projection_month: u32,
        policy_year: u32,
        month_in_policy_year: u32,
        income_activated: bool,
        itm_ness: f64,
        sc_period: u32,
        bucket: BenefitBaseBucket,
    ) -> f64 {
        // Month 1 has no lapse (Excel rule)
        if projection_month == 1 {
            return 0.0;
        }

        // Account value must be positive for lapse
        if itm_ness <= 0.0 {
            return 0.0;
        }

        let annual_prob = self.annual_lapse_prob_with_bucket(policy_year, income_activated, itm_ness, bucket, sc_period);

        // Determine skew based on shock year
        // Shock year is the first year after SC period ends (year sc_period + 1)
        let shock_year = sc_period + 1;
        let skew = if policy_year == shock_year {
            // Shock year has front-loaded skew (totals to 100%)
            match month_in_policy_year {
                1 => 0.4,           // 40%
                2 => 0.3,           // 30%
                3 => 0.2,           // 20%
                _ => 0.1 / 9.0,     // ~1.11% for months 4-12 (10% / 9 months)
            }
        } else {
            // Normal monthly distribution
            1.0 / 12.0
        };

        // Convert annual to monthly using skew
        1.0 - (1.0 - annual_prob).powf(skew)
    }

    /// Get the skew factor for a given policy year and month
    /// Used for display/debugging purposes
    pub fn get_skew(&self, policy_year: u32, month_in_policy_year: u32, sc_period: u32) -> f64 {
        let shock_year = sc_period + 1;
        if policy_year == shock_year {
            match month_in_policy_year {
                1 => 0.4,
                2 => 0.3,
                3 => 0.2,
                _ => 0.1 / 9.0,
            }
        } else {
            1.0 / 12.0
        }
    }
}

/// Calculate ITM-ness (in-the-money-ness) for GLWB
/// ITM = Benefit Base / Account Value
/// >1 means guarantee is valuable (in the money)
pub fn calculate_itm_ness(benefit_base: f64, account_value: f64) -> f64 {
    if account_value <= 0.0 {
        return 1.0; // Avoid division by zero
    }
    benefit_base / account_value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_month_1_zero_lapse() {
        let model = LapseModel::default_predictive_model();

        let rate = model.monthly_lapse_rate(1, 1, false, 1.3);
        assert_eq!(rate, 0.0, "Month 1 should have zero lapse");
    }

    #[test]
    fn test_lapse_month_2() {
        let model = LapseModel::default_predictive_model();

        // Policy 2800: ITM = 27178.16 / 20906.28 = 1.30
        let itm = 27178.16 / 20906.28;
        let rate = model.monthly_lapse_rate(2, 1, false, itm);

        // Excel shows 0.000189 for month 2
        assert!(
            (rate - 0.000189).abs() < 0.00005,
            "Month 2 lapse mismatch: {} vs 0.000189",
            rate
        );
    }

    #[test]
    fn test_base_component() {
        let model = LapseModel::default_predictive_model();

        let base = model.base_component(1, false);
        // Excel shows -5.7448 for year 1, GLWB not activated
        assert!(
            (base - (-5.7448)).abs() < 0.01,
            "Base component mismatch: {} vs -5.7448",
            base
        );
    }

    #[test]
    fn test_dynamic_component() {
        let model = LapseModel::default_predictive_model();

        // ITM = 1.30
        let dynamic = model.dynamic_component(1.30, false);
        // Excel shows -0.347 for ITM=1.30, GLWB not activated
        assert!(
            (dynamic - (-0.347)).abs() < 0.01,
            "Dynamic component mismatch: {} vs -0.347",
            dynamic
        );
    }

    #[test]
    fn test_shock_year_higher_lapse() {
        let model = LapseModel::default_predictive_model();

        // Year 10 vs Year 11 (shock year)
        let rate_10 = model.monthly_lapse_rate(120, 10, false, 1.3);
        let rate_11 = model.monthly_lapse_rate(132, 11, false, 1.3);

        assert!(
            rate_11 > rate_10,
            "Shock year (11) should have higher lapse than year 10"
        );
    }

    #[test]
    fn test_itm_ness() {
        assert_eq!(calculate_itm_ness(120_000.0, 100_000.0), 1.2);
        assert_eq!(calculate_itm_ness(80_000.0, 100_000.0), 0.8);
    }
}
