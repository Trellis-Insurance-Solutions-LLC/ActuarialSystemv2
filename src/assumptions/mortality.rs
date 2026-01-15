//! Mortality assumptions based on IAM 2012 Basic table with configurable factors
//!
//! The mortality model separates:
//! - Base mortality rates (IAM 2012 Basic table)
//! - Age-graded multiplicative factors
//! - Mortality improvement rates
//!
//! This allows each component to be adjusted independently for sensitivity testing.

use crate::policy::Gender;

/// Mortality table with separate base rates and adjustment factors
#[derive(Debug, Clone)]
pub struct MortalityTable {
    /// Base annual mortality rates by age (index = age)
    /// Stored as (female_rate, male_rate)
    base_rates: Vec<(f64, f64)>,

    /// Multiplicative factors by attained age to adjust base rates
    age_factors: Vec<f64>,

    /// Annual mortality improvement rates by age (index = age)
    /// Stored as (female_rate, male_rate) - varies by age and gender
    improvement_rates: Vec<(f64, f64)>,

    /// Method for converting annual to monthly rates
    conversion_method: MonthlyConversion,

    /// Base year of the mortality table (for improvement calculations)
    table_base_year: u32,

    /// Projection start year
    projection_year: u32,
}

/// Method for converting annual mortality rates to monthly
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MonthlyConversion {
    /// Standard actuarial: q_monthly = 1 - (1 - q_annual)^(1/12)
    Standard,
    /// Simple division: q_monthly = q_annual / 12
    SimpleDivision,
    /// Excel method: q_monthly = q_annual * age_factor / 12
    /// This applies the age factor twice (once in baseline, once in conversion)
    ExcelMethod,
}

impl MortalityTable {
    /// Create IAM 2012 Basic table with default factors matching Excel calibration
    pub fn iam_2012_with_improvement() -> Self {
        Self {
            base_rates: Self::iam_2012_base_rates(),
            age_factors: Self::default_age_factors(),
            improvement_rates: Self::default_improvement_rates(),
            conversion_method: MonthlyConversion::Standard,
            table_base_year: 2012,
            projection_year: 2026,
        }
    }

    /// Create from loaded CSV assumptions
    pub fn from_loaded(loaded: &super::loader::LoadedAssumptions) -> Self {
        Self {
            base_rates: loaded.mortality_base_rates.clone(),
            age_factors: loaded.mortality_age_factors.clone(),
            improvement_rates: loaded.mortality_improvement.clone(),
            conversion_method: MonthlyConversion::Standard,
            table_base_year: 2012,
            projection_year: 2026,
        }
    }

    /// Create with custom configuration
    pub fn new(
        base_rates: Vec<(f64, f64)>,
        age_factors: Vec<f64>,
        improvement_rate: f64,
        conversion_method: MonthlyConversion,
    ) -> Self {
        // Convert single rate to age-varying (for backward compatibility)
        let improvement_rates = vec![(improvement_rate, improvement_rate); 121];
        Self {
            base_rates,
            age_factors,
            improvement_rates,
            conversion_method,
            table_base_year: 2012,
            projection_year: 2026,
        }
    }

    /// Set the table base year and projection year for improvement calculations
    pub fn set_improvement_years(&mut self, table_base_year: u32, projection_year: u32) {
        self.table_base_year = table_base_year;
        self.projection_year = projection_year;
    }

    /// Get table base year
    pub fn table_base_year(&self) -> u32 {
        self.table_base_year
    }

    /// Get projection year
    pub fn projection_year(&self) -> u32 {
        self.projection_year
    }

    /// Get a mutable reference to age factors for calibration
    pub fn age_factors_mut(&mut self) -> &mut Vec<f64> {
        &mut self.age_factors
    }

    /// Get the age factors
    pub fn age_factors(&self) -> &[f64] {
        &self.age_factors
    }

    /// Set age factors
    pub fn set_age_factors(&mut self, factors: Vec<f64>) {
        self.age_factors = factors;
    }

    /// Set a specific age factor
    pub fn set_age_factor(&mut self, age: usize, factor: f64) {
        if age < self.age_factors.len() {
            self.age_factors[age] = factor;
        }
    }

    /// Set improvement rate (applies same rate to all ages)
    pub fn set_improvement_rate(&mut self, rate: f64) {
        self.improvement_rates = vec![(rate, rate); 121];
    }

    /// Get improvement rate for a specific age and gender
    pub fn improvement_rate(&self, age: u8, gender: Gender) -> f64 {
        let idx = age as usize;
        if idx >= self.improvement_rates.len() {
            return 0.0;
        }
        let (female, male) = self.improvement_rates[idx];
        match gender {
            Gender::Female => female,
            Gender::Male => male,
        }
    }

    /// Set conversion method
    pub fn set_conversion_method(&mut self, method: MonthlyConversion) {
        self.conversion_method = method;
    }

    /// Apply a scalar multiplier to all age factors
    pub fn scale_age_factors(&mut self, multiplier: f64) {
        for factor in &mut self.age_factors {
            *factor *= multiplier;
        }
    }

    /// Get monthly mortality rate for a given age and gender
    ///
    /// # Arguments
    /// * `attained_age` - Current age of the policyholder
    /// * `gender` - Gender for mortality table lookup
    /// * `projection_month` - Month number in projection (1-indexed)
    ///
    /// # Improvement calculation
    /// Uses formula: years = (projection_year - table_base_year - 1) + projection_month/12
    /// This matches the Excel formula: (2026-2012-1+B11/12)
    pub fn monthly_rate(&self, attained_age: u8, gender: Gender, projection_month: u32) -> f64 {
        let age = attained_age as usize;
        if age >= self.base_rates.len() {
            return 1.0 / 12.0; // Simplified extreme age handling
        }

        // Get base annual rate
        let (female_rate, male_rate) = self.base_rates[age];
        let base_annual = match gender {
            Gender::Female => female_rate,
            Gender::Male => male_rate,
        };

        // Get age factor
        let age_factor = self.age_factors.get(age).copied().unwrap_or(1.0);

        // Apply age factor to get best estimate annual rate
        let best_estimate_annual = base_annual * age_factor;

        // Calculate years of improvement from table base year to projection
        // Formula: (projection_year - table_base_year - 1) + projection_month/12
        let years_improvement = (self.projection_year - self.table_base_year - 1) as f64
            + projection_month as f64 / 12.0;

        // Get age-specific improvement rate
        let improvement_rate = self.improvement_rate(attained_age, gender);

        // Apply mortality improvement
        let improvement_factor = (1.0 - improvement_rate).powf(years_improvement);
        let improved_annual = best_estimate_annual * improvement_factor;

        // Convert to monthly based on selected method
        match self.conversion_method {
            MonthlyConversion::Standard => {
                // Standard actuarial: q_monthly = 1 - (1 - q_annual)^(1/12)
                1.0 - (1.0 - improved_annual).powf(1.0 / 12.0)
            }
            MonthlyConversion::SimpleDivision => {
                improved_annual / 12.0
            }
            MonthlyConversion::ExcelMethod => {
                // Legacy method - applies age factor twice
                best_estimate_annual * age_factor / 12.0 * improvement_factor
            }
        }
    }

    /// Get the baseline annual mortality rate (with age factor applied)
    pub fn baseline_annual_rate(&self, attained_age: u8, gender: Gender) -> f64 {
        let age = attained_age as usize;
        if age >= self.base_rates.len() {
            return 1.0;
        }

        let (female_rate, male_rate) = self.base_rates[age];
        let base_rate = match gender {
            Gender::Female => female_rate,
            Gender::Male => male_rate,
        };

        let age_factor = self.age_factors.get(age).copied().unwrap_or(1.0);
        base_rate * age_factor
    }

    /// Get the raw base rate (before age factor)
    pub fn raw_base_rate(&self, attained_age: u8, gender: Gender) -> f64 {
        let age = attained_age as usize;
        if age >= self.base_rates.len() {
            return 1.0;
        }

        let (female_rate, male_rate) = self.base_rates[age];
        match gender {
            Gender::Female => female_rate,
            Gender::Male => male_rate,
        }
    }

    /// Get age factor for a specific age
    pub fn get_age_factor(&self, attained_age: u8) -> f64 {
        self.age_factors.get(attained_age as usize).copied().unwrap_or(1.0)
    }

    /// IAM 2012 Basic mortality table from Excel
    fn iam_2012_base_rates() -> Vec<(f64, f64)> {
        vec![
            // Age 0-9
            (0.001801, 0.001783), (0.00045, 0.000446), (0.000287, 0.000306),
            (0.000199, 0.000254), (0.000152, 0.000193), (0.000139, 0.000186),
            (0.00013, 0.000184), (0.000122, 0.000177), (0.000105, 0.000159),
            (0.000098, 0.000143),
            // Age 10-19
            (0.000094, 0.000126), (0.000096, 0.000123), (0.000105, 0.000147),
            (0.00012, 0.000188), (0.000146, 0.000236), (0.000174, 0.000282),
            (0.000199, 0.000325), (0.00022, 0.000364), (0.000234, 0.000399),
            (0.000245, 0.00043),
            // Age 20-29
            (0.000253, 0.000459), (0.00026, 0.000492), (0.000266, 0.000526),
            (0.000272, 0.000569), (0.000275, 0.000616), (0.000277, 0.000669),
            (0.000284, 0.000728), (0.00029, 0.000764), (0.0003, 0.000789),
            (0.000313, 0.000808),
            // Age 30-39
            (0.000333, 0.000824), (0.000357, 0.000834), (0.000375, 0.000838),
            (0.00039, 0.000828), (0.000405, 0.000808), (0.000424, 0.000789),
            (0.000447, 0.000783), (0.000476, 0.0008), (0.000514, 0.000837),
            (0.00056, 0.000889),
            // Age 40-49
            (0.000613, 0.000955), (0.000667, 0.001029), (0.000723, 0.00111),
            (0.000774, 0.001188), (0.000823, 0.001268), (0.000866, 0.001355),
            (0.000917, 0.001464), (0.000983, 0.001615), (0.001072, 0.001808),
            (0.001168, 0.002032),
            // Age 50-59
            (0.00129, 0.002285), (0.001453, 0.002557), (0.001622, 0.002828),
            (0.001792, 0.003088), (0.001972, 0.003345), (0.002166, 0.003616),
            (0.002393, 0.003922), (0.002666, 0.004272), (0.003, 0.004681),
            (0.003393, 0.005146),
            // Age 60-69
            (0.003844, 0.005662), (0.004352, 0.006237), (0.004899, 0.006854),
            (0.005482, 0.00751), (0.006118, 0.00822), (0.006829, 0.009007),
            (0.007279, 0.009497), (0.007821, 0.010085), (0.008475, 0.010787),
            (0.009234, 0.011625),
            // Age 70-79
            (0.010083, 0.012619), (0.011011, 0.013798), (0.01203, 0.015195),
            (0.013154, 0.016834), (0.014415, 0.018733), (0.015869, 0.020905),
            (0.017555, 0.023367), (0.0195, 0.026155), (0.021758, 0.029306),
            (0.024412, 0.032858),
            // Age 80-89
            (0.027579, 0.036927), (0.031501, 0.041703), (0.036122, 0.046957),
            (0.041477, 0.052713), (0.047589, 0.059148), (0.054441, 0.066505),
            (0.061972, 0.075015), (0.070155, 0.084823), (0.078963, 0.095987),
            (0.088336, 0.108482),
            // Age 90-99
            (0.098197, 0.122214), (0.108323, 0.136799), (0.119188, 0.152409),
            (0.131334, 0.169078), (0.145521, 0.186882), (0.162722, 0.205844),
            (0.18212, 0.219247), (0.199661, 0.238612), (0.217946, 0.258341),
            (0.236834, 0.278219),
            // Age 100-109
            (0.256357, 0.298452), (0.283802, 0.32361), (0.304716, 0.344191),
            (0.325819, 0.364633), (0.346936, 0.384783), (0.367898, 0.4),
            (0.387607, 0.4), (0.4, 0.4), (0.4, 0.4), (0.4, 0.4),
            // Age 110-120
            (0.4, 0.4), (0.4, 0.4), (0.4, 0.4), (0.4, 0.4), (0.4, 0.4),
            (0.4, 0.4), (0.4, 0.4), (0.4, 0.4), (0.4, 0.4), (0.4, 0.4),
            (0.4, 0.4),
        ]
    }

    /// Default age factors: 0.6 for ages ≤60, grading to 1.0 at age 90
    pub fn default_age_factors() -> Vec<f64> {
        let mut factors = vec![1.0; 121];

        // Ages 0-60: factor of 0.6
        for age in 0..=60 {
            factors[age] = 0.6;
        }

        // Ages 61-89: linear grade from 0.6 to 1.0 over 30 years
        for age in 61..=89 {
            let years_from_60 = (age - 60) as f64;
            factors[age] = 0.6 + (0.4 * years_from_60 / 30.0);
        }

        // Ages 90+: factor of 1.0
        for age in 90..=120 {
            factors[age] = 1.0;
        }

        factors
    }

    /// Default improvement rates by age and gender
    /// Rates vary by age: ~1% at young ages, peak ~1.3-1.5% at middle ages, declining at older ages
    pub fn default_improvement_rates() -> Vec<(f64, f64)> {
        let mut rates = vec![(0.01, 0.01); 121]; // Default 1% for all ages

        // Ages 51-80: higher improvement rates
        for age in 51..=80 {
            let female = if age <= 52 { 0.01 }
                else if age <= 58 { 0.012 }
                else { 0.013 };
            let male = if age <= 50 { 0.01 }
                else if age <= 52 { 0.011 }
                else if age <= 54 { 0.012 }
                else if age <= 56 { 0.013 }
                else if age <= 58 { 0.014 }
                else { 0.015 };
            rates[age] = (female, male);
        }

        // Ages 81-90: declining improvement rates
        rates[81] = (0.012, 0.014);
        rates[82] = (0.012, 0.013);
        rates[83] = (0.011, 0.013);
        rates[84] = (0.010, 0.012);
        rates[85] = (0.010, 0.011);
        rates[86] = (0.009, 0.010);
        rates[87] = (0.008, 0.009);
        rates[88] = (0.007, 0.009);
        rates[89] = (0.007, 0.008);
        rates[90] = (0.006, 0.007);

        // Ages 91-103: minimal improvement
        rates[91] = (0.006, 0.007);
        rates[92] = (0.005, 0.006);
        rates[93] = (0.005, 0.005);
        rates[94] = (0.004, 0.005);
        rates[95] = (0.004, 0.004);
        rates[96] = (0.004, 0.004);
        rates[97] = (0.003, 0.003);
        rates[98] = (0.003, 0.003);
        rates[99] = (0.002, 0.002);
        rates[100] = (0.002, 0.002);
        rates[101] = (0.002, 0.002);
        rates[102] = (0.001, 0.001);
        rates[103] = (0.001, 0.001);

        // Ages 104+: no improvement
        for age in 104..=120 {
            rates[age] = (0.0, 0.0);
        }

        rates
    }

    /// Create flat age factors (all 1.0) for using raw table rates
    pub fn flat_age_factors() -> Vec<f64> {
        vec![1.0; 121]
    }

    /// Create custom graded age factors
    pub fn graded_age_factors(
        start_age: usize,
        end_age: usize,
        start_factor: f64,
        end_factor: f64,
    ) -> Vec<f64> {
        let mut factors = vec![start_factor; 121];

        let grade_years = (end_age - start_age) as f64;
        for age in start_age..=end_age {
            let years_from_start = (age - start_age) as f64;
            factors[age] = start_factor + (end_factor - start_factor) * years_from_start / grade_years;
        }

        for age in (end_age + 1)..=120 {
            factors[age] = end_factor;
        }

        factors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baseline_annual_rate() {
        let table = MortalityTable::iam_2012_with_improvement();

        // Age 77 male: base = 0.026155, factor at 77 = 0.6 + (17/30)*0.4 = 0.8267
        let baseline = table.baseline_annual_rate(77, Gender::Male);
        let expected = 0.026155 * (0.6 + 17.0 * 0.4 / 30.0);
        assert!((baseline - expected).abs() < 1e-6);
        assert!((baseline - 0.0216).abs() < 0.001); // Should be ~0.0216
    }

    #[test]
    fn test_age_factor_grading() {
        let factors = MortalityTable::default_age_factors();

        assert_eq!(factors[50], 0.6);
        assert_eq!(factors[60], 0.6);
        assert!((factors[75] - 0.8).abs() < 0.01); // 0.6 + (15/30)*0.4 = 0.8
        assert_eq!(factors[90], 1.0);
        assert_eq!(factors[100], 1.0);
    }

    #[test]
    fn test_excel_mortality_formula() {
        let table = MortalityTable::iam_2012_with_improvement();

        // Month 1, age 77 male
        // Excel formula: =1-(1-F11*(1-G11)^(2026-2012-1+B11/12))^(1/12)
        // years = 2026-2012-1+1/12 = 13.0833
        // improvement = 0.985^13.0833 = 0.8196
        // improved_annual = 0.0216 * 0.8196 = 0.01771
        // monthly = 1-(1-0.01771)^(1/12) = 0.001489
        let monthly = table.monthly_rate(77, Gender::Male, 1);
        assert!((monthly - 0.0014907).abs() < 0.00001, "Month 1 mortality mismatch: {}", monthly);

        // Month 2
        let monthly_2 = table.monthly_rate(77, Gender::Male, 2);
        assert!((monthly_2 - 0.0014888).abs() < 0.00001, "Month 2 mortality mismatch: {}", monthly_2);
    }

    #[test]
    fn test_mortality_improvement() {
        let table = MortalityTable::iam_2012_with_improvement();

        let rate_month_1 = table.monthly_rate(77, Gender::Male, 1);
        let rate_month_13 = table.monthly_rate(77, Gender::Male, 13);

        // Rate should decrease with improvement
        assert!(rate_month_13 < rate_month_1);

        // Over 12 months, ratio should be approximately (1-0.015) = 0.985
        let ratio = rate_month_13 / rate_month_1;
        assert!((ratio - 0.985).abs() < 0.002, "Improvement ratio: {}", ratio);
    }

    #[test]
    fn test_custom_age_factors() {
        let mut table = MortalityTable::iam_2012_with_improvement();

        // Scale all factors by 1.1
        table.scale_age_factors(1.1);

        let new_factor = table.get_age_factor(60);
        assert!((new_factor - 0.66).abs() < 0.001); // 0.6 * 1.1 = 0.66
    }

    #[test]
    fn test_graded_factors() {
        let factors = MortalityTable::graded_age_factors(50, 80, 0.5, 1.0);

        assert_eq!(factors[50], 0.5);
        assert!((factors[65] - 0.75).abs() < 0.01); // halfway
        assert_eq!(factors[80], 1.0);
        assert_eq!(factors[90], 1.0);
    }
}
