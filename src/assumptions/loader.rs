//! CSV-based assumption loader
//!
//! Loads actuarial assumptions from CSV files in data/assumptions/

use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::path::Path;

/// Default path to assumptions directory
pub const DEFAULT_ASSUMPTIONS_PATH: &str = "data/assumptions";

/// Load mortality base rates from CSV
/// Returns Vec<(female_rate, male_rate)> indexed by age
pub fn load_mortality_base_rates(path: &Path) -> Result<Vec<(f64, f64)>, Box<dyn Error>> {
    let file = File::open(path.join("mortality_base_rates.csv"))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut rates = vec![(0.0, 0.0); 121];

    for result in reader.records() {
        let record = result?;
        let age: usize = record[0].parse()?;
        let female: f64 = record[1].parse()?;
        let male: f64 = record[2].parse()?;

        if age < rates.len() {
            rates[age] = (female, male);
        }
    }

    Ok(rates)
}

/// Load mortality improvement rates from CSV
/// Returns Vec<(female_rate, male_rate)> indexed by age
pub fn load_mortality_improvement(path: &Path) -> Result<Vec<(f64, f64)>, Box<dyn Error>> {
    let file = File::open(path.join("mortality_improvement.csv"))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut rates = vec![(0.0, 0.0); 121];

    for result in reader.records() {
        let record = result?;
        let age: usize = record[0].parse()?;
        let female: f64 = record[1].parse()?;
        let male: f64 = record[2].parse()?;

        if age < rates.len() {
            rates[age] = (female, male);
        }
    }

    Ok(rates)
}

/// Load mortality age factors from CSV
/// Returns Vec<f64> indexed by age
pub fn load_mortality_age_factors(path: &Path) -> Result<Vec<f64>, Box<dyn Error>> {
    let file = File::open(path.join("mortality_age_factors.csv"))?;
    let mut reader = csv::Reader::from_reader(file);

    // Default to 1.0 for all ages
    let mut factors = vec![1.0; 121];

    for result in reader.records() {
        let record = result?;
        let age: usize = record[0].parse()?;
        let factor: f64 = record[1].parse()?;

        if age < factors.len() {
            factors[age] = factor;
        }
    }

    Ok(factors)
}

/// Load surrender charges from CSV
/// Returns Vec<f64> indexed by policy year (1-indexed in file, 0-indexed in vec)
pub fn load_surrender_charges(path: &Path) -> Result<Vec<f64>, Box<dyn Error>> {
    let file = File::open(path.join("surrender_charges.csv"))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut charges = vec![0.0; 20]; // Support up to 20 years

    for result in reader.records() {
        let record = result?;
        let year: usize = record[0].parse()?;
        let charge: f64 = record[1].parse()?;

        if year > 0 && year <= charges.len() {
            charges[year - 1] = charge;
        }
    }

    Ok(charges)
}

/// Load RMD rates from CSV
/// Returns Vec<(age, rate)> for ages with RMD requirements
pub fn load_rmd_rates(path: &Path) -> Result<Vec<(u8, f64)>, Box<dyn Error>> {
    let file = File::open(path.join("rmd_rates.csv"))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut rates = Vec::new();

    for result in reader.records() {
        let record = result?;
        let age: u8 = record[0].parse()?;
        let rate: f64 = record[1].parse()?;
        rates.push((age, rate));
    }

    Ok(rates)
}

/// Load free withdrawal utilization from CSV
/// Returns Vec<f64> indexed by policy year (1-indexed in file)
pub fn load_free_withdrawal_util(path: &Path) -> Result<Vec<f64>, Box<dyn Error>> {
    let file = File::open(path.join("free_withdrawal_util.csv"))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut utils = Vec::new();

    for result in reader.records() {
        let record = result?;
        let _year: usize = record[0].parse()?;
        let util: f64 = record[1].parse()?;
        utils.push(util);
    }

    Ok(utils)
}

/// Load payout factors from CSV
/// Returns HashMap<age, factor>
pub fn load_payout_factors(path: &Path) -> Result<HashMap<u8, f64>, Box<dyn Error>> {
    let file = File::open(path.join("payout_factors.csv"))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut factors = HashMap::new();

    for result in reader.records() {
        let record = result?;
        let age: u8 = record[0].parse()?;
        let factor: f64 = record[1].parse()?;
        factors.insert(age, factor);
    }

    Ok(factors)
}

/// Load surrender predictive model coefficients from CSV
/// Returns HashMap<term_name, coefficient>
pub fn load_surrender_model(path: &Path) -> Result<HashMap<String, f64>, Box<dyn Error>> {
    let file = File::open(path.join("surrender_predictive_model.csv"))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut coefficients = HashMap::new();

    for result in reader.records() {
        let record = result?;
        let term = record[0].to_string();
        let coef: f64 = record[1].parse()?;
        coefficients.insert(term, coef);
    }

    Ok(coefficients)
}

/// Load all assumptions from the given directory
pub struct LoadedAssumptions {
    pub mortality_base_rates: Vec<(f64, f64)>,
    pub mortality_improvement: Vec<(f64, f64)>,
    pub mortality_age_factors: Vec<f64>,
    pub surrender_charges: Vec<f64>,
    pub rmd_rates: Vec<(u8, f64)>,
    pub free_withdrawal_util: Vec<f64>,
    pub payout_factors: HashMap<u8, f64>,
    pub surrender_model: HashMap<String, f64>,
}

impl LoadedAssumptions {
    /// Load all assumptions from the default path
    pub fn load_default() -> Result<Self, Box<dyn Error>> {
        Self::load_from(Path::new(DEFAULT_ASSUMPTIONS_PATH))
    }

    /// Load all assumptions from a specific path
    pub fn load_from(path: &Path) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            mortality_base_rates: load_mortality_base_rates(path)?,
            mortality_improvement: load_mortality_improvement(path)?,
            mortality_age_factors: load_mortality_age_factors(path)?,
            surrender_charges: load_surrender_charges(path)?,
            rmd_rates: load_rmd_rates(path)?,
            free_withdrawal_util: load_free_withdrawal_util(path)?,
            payout_factors: load_payout_factors(path)?,
            surrender_model: load_surrender_model(path)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_default_assumptions() {
        let result = LoadedAssumptions::load_default();
        assert!(result.is_ok(), "Failed to load assumptions: {:?}", result.err());

        let assumptions = result.unwrap();

        // Check mortality base rates loaded
        assert!(assumptions.mortality_base_rates.len() >= 100);
        assert!(assumptions.mortality_base_rates[77].1 > 0.0); // Male age 77

        // Check improvement rates loaded
        assert!(assumptions.mortality_improvement.len() >= 100);

        // Check age factors loaded
        assert!(assumptions.mortality_age_factors.len() >= 50);

        // Check surrender charges loaded
        assert!(assumptions.surrender_charges.len() >= 10);

        // Check RMD rates loaded
        assert!(!assumptions.rmd_rates.is_empty());

        // Check payout factors loaded
        assert!(!assumptions.payout_factors.is_empty());
    }
}
