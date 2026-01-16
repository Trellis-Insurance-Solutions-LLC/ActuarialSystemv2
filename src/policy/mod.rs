//! Policy data structures and inforce loading

mod data;
pub mod loader;

pub use data::{Policy, QualStatus, Gender, CreditingStrategy, RollupType, BenefitBaseBucket};
pub use loader::{load_policies, load_policies_from_reader, load_default_inforce};
