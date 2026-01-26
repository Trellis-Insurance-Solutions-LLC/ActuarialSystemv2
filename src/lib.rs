//! Actuarial System - High-performance projection engine for FIA products with GLWB riders
//!
//! This library provides:
//! - Single and multi-policy liability projections
//! - Decrement modeling (mortality, lapse, partial withdrawals)
//! - Reserve calculations (CARVM, AG33, AG35, VM-22)
//! - Asset modeling and portfolio analytics
//! - Multi-scenario simulation framework

pub mod policy;
pub mod assumptions;
pub mod projection;
pub mod scenario;
pub mod reserves;

// Re-export commonly used types
pub use policy::Policy;
pub use assumptions::{Assumptions, MortalityTable, SurrenderChargeSchedule, LapseModel};
pub use projection::{ProjectionEngine, ProjectionResult, CashflowRow};
pub use scenario::ScenarioRunner;

// Re-export reserve types
pub use reserves::{
    CARVMCalculator,
    CARVMConfig,
    CARVMMethod,
    ReserveResult,
    ReserveCalculator,
    ReserveCalcConfig,
};
