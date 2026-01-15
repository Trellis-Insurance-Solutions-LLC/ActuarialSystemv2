//! Actuarial System - High-performance projection engine for FIA products with GLWB riders
//!
//! This library provides:
//! - Single and multi-policy liability projections
//! - Decrement modeling (mortality, lapse, partial withdrawals)
//! - Reserve calculations (VM-22, CARVM, economic scenarios)
//! - Asset modeling and portfolio analytics
//! - Multi-scenario simulation framework

pub mod policy;
pub mod assumptions;
pub mod projection;
pub mod scenario;

// Re-export commonly used types
pub use policy::Policy;
pub use assumptions::{Assumptions, MortalityTable, SurrenderChargeSchedule, LapseModel};
pub use projection::{ProjectionEngine, ProjectionResult, CashflowRow};
pub use scenario::ScenarioRunner;
