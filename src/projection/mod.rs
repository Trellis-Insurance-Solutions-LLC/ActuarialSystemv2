//! Projection engine for single and multi-policy projections

mod state;
mod engine;
mod cashflows;

pub use state::ProjectionState;
pub use engine::{ProjectionEngine, ProjectionConfig, CreditingApproach};
pub use cashflows::{CashflowRow, ProjectionResult};
