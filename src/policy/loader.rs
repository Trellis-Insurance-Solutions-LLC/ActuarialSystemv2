//! Load policies from pricing_inforce.csv

use super::{Policy, QualStatus, Gender, CreditingStrategy, RollupType, BenefitBaseBucket};
use csv::Reader;
use std::error::Error;
use std::path::Path;

/// Raw CSV row matching pricing_inforce.csv columns
#[derive(Debug, serde::Deserialize)]
struct CsvRow {
    #[serde(rename = "QualStatus")]
    qual_status: String,
    #[serde(rename = "IssueAge")]
    issue_age: u8,
    #[serde(rename = "Gender")]
    gender: String,
    #[serde(rename = "InitialBB")]
    initial_bb: f64,
    #[serde(rename = "InitialPols")]
    initial_pols: f64,
    #[serde(rename = "InitialPremium")]
    initial_premium: f64,
    #[serde(rename = "Benefit_Base_Bucket")]
    benefit_base_bucket: String,
    #[serde(rename = "Percentage")]
    percentage: f64,
    #[serde(rename = "CreditingStrategy")]
    crediting_strategy: String,
    #[serde(rename = "PolicyID")]
    policy_id: u32,
    #[serde(rename = "SCPeriod")]
    sc_period: u8,
    #[serde(rename = "valRate")]
    val_rate: f64,
    #[serde(rename = "MGIR")]
    mgir: f64,
    #[serde(rename = "Bonus")]
    bonus: f64,
    #[serde(rename = "RollupType")]
    rollup_type: String,
    #[serde(rename = "Rollup")]
    _rollup: f64,
    #[serde(rename = "RollupDuration")]
    _rollup_duration: u32,
    #[serde(rename = "GLWBStartYear")]
    glwb_start_year: u32,
    #[serde(rename = "WaitPeriod")]
    _wait_period: u32,
}

impl CsvRow {
    fn to_policy(self) -> Result<Policy, Box<dyn Error>> {
        let qual_status = match self.qual_status.as_str() {
            "Q" => QualStatus::Q,
            "N" => QualStatus::N,
            other => return Err(format!("Unknown QualStatus: {}", other).into()),
        };

        let gender = match self.gender.as_str() {
            "Male" => Gender::Male,
            "Female" => Gender::Female,
            other => return Err(format!("Unknown Gender: {}", other).into()),
        };

        let crediting_strategy = match self.crediting_strategy.as_str() {
            "Indexed" => CreditingStrategy::Indexed,
            "Fixed" => CreditingStrategy::Fixed,
            other => return Err(format!("Unknown CreditingStrategy: {}", other).into()),
        };

        let rollup_type = match self.rollup_type.as_str() {
            "Simple" => RollupType::Simple,
            "Compound" => RollupType::Compound,
            other => return Err(format!("Unknown RollupType: {}", other).into()),
        };

        let benefit_base_bucket = match self.benefit_base_bucket.as_str() {
            "[0, 50000)" => BenefitBaseBucket::Under50k,
            "[50000, 100000)" => BenefitBaseBucket::From50kTo100k,
            "[100000, 200000)" => BenefitBaseBucket::From100kTo200k,
            "[200000, 500000)" => BenefitBaseBucket::From200kTo500k,
            "[500000, Inf)" => BenefitBaseBucket::Over500k,
            other => return Err(format!("Unknown Benefit_Base_Bucket: {}", other).into()),
        };

        Ok(Policy {
            policy_id: self.policy_id,
            qual_status,
            issue_age: self.issue_age,
            gender,
            initial_benefit_base: self.initial_bb,
            initial_pols: self.initial_pols,
            initial_premium: self.initial_premium,
            benefit_base_bucket,
            percentage: self.percentage,
            crediting_strategy,
            sc_period: self.sc_period,
            val_rate: self.val_rate,
            mgir: self.mgir,
            bonus: self.bonus,
            rollup_type,
            duration_months: 0,
            income_activated: false,
            glwb_start_year: self.glwb_start_year,
            current_av: None,
            current_benefit_base: None,
        })
    }
}

/// Load all policies from a CSV file
pub fn load_policies<P: AsRef<Path>>(path: P) -> Result<Vec<Policy>, Box<dyn Error>> {
    let mut reader = Reader::from_path(path)?;
    let mut policies = Vec::new();

    for result in reader.deserialize() {
        let row: CsvRow = result?;
        let policy = row.to_policy()?;
        policies.push(policy);
    }

    Ok(policies)
}

/// Load policies from any reader (e.g., string buffer, network stream)
pub fn load_policies_from_reader<R: std::io::Read>(reader: R) -> Result<Vec<Policy>, Box<dyn Error>> {
    let mut csv_reader = Reader::from_reader(reader);
    let mut policies = Vec::new();

    for result in csv_reader.deserialize() {
        let row: CsvRow = result?;
        let policy = row.to_policy()?;
        policies.push(policy);
    }

    Ok(policies)
}

/// Load policies from the default pricing_inforce.csv location
pub fn load_default_inforce() -> Result<Vec<Policy>, Box<dyn Error>> {
    load_policies("pricing_inforce.csv")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_policies() {
        let policies = load_default_inforce().expect("Failed to load policies");
        assert_eq!(policies.len(), 2800);

        // Check first policy
        let p1 = &policies[0];
        assert_eq!(p1.policy_id, 1);

        // Check policy 10 (index 9)
        let p10 = &policies[9];
        assert_eq!(p10.policy_id, 10);
        assert_eq!(p10.issue_age, 57);
        assert_eq!(p10.glwb_start_year, 5);
    }
}
