//! Actuarial assumptions including mortality, lapse, and product features

mod mortality;
mod lapse;
mod product;
mod pwd;
pub mod loader;

pub use mortality::{MortalityTable, MonthlyConversion};
pub use lapse::{LapseModel, calculate_itm_ness};
pub use product::{SurrenderChargeSchedule, PayoutFactors, ProductFeatures};
pub use pwd::{PwdAssumptions, RmdTable};
pub use loader::LoadedAssumptions;

use std::path::Path;

/// Container for all projection assumptions
#[derive(Debug, Clone)]
pub struct Assumptions {
    pub mortality: MortalityTable,
    pub lapse: LapseModel,
    pub product: ProductFeatures,
    pub pwd: PwdAssumptions,
}

impl Assumptions {
    /// Create assumptions with default values matching the Excel reference
    pub fn default_pricing() -> Self {
        Self {
            mortality: MortalityTable::iam_2012_with_improvement(),
            lapse: LapseModel::default_predictive_model(),
            product: ProductFeatures::default(),
            pwd: PwdAssumptions::default(),
        }
    }

    /// Load assumptions from CSV files in the default location (data/assumptions/)
    pub fn from_csv() -> Result<Self, Box<dyn std::error::Error>> {
        Self::from_csv_path(Path::new(loader::DEFAULT_ASSUMPTIONS_PATH))
    }

    /// Load assumptions from CSV files in a specific directory
    pub fn from_csv_path(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let loaded = LoadedAssumptions::load_from(path)?;

        Ok(Self {
            mortality: MortalityTable::from_loaded(&loaded),
            lapse: LapseModel::from_loaded(&loaded),
            product: ProductFeatures::from_loaded(&loaded),
            pwd: PwdAssumptions::from_loaded(&loaded),
        })
    }
}
