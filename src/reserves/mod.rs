//! Reserve calculation module for CARVM, AG33, AG35, and VM-22
//!
//! This module provides seriatim reserve calculations under multiple regulatory frameworks:
//! - **CARVM**: Commissioners Annuity Reserve Valuation Method (optimal policyholder behavior)
//! - **AG33**: CARVM for contracts with elective benefits (integrated benefit streams)
//! - **AG35**: CARVM for equity indexed annuities (Type 1 and Type 2 methods)
//! - **VM-22**: Principles-based reserves (stochastic scenarios, company assumptions)
//!
//! # Architecture
//!
//! The reserve calculation follows a separation of concerns pattern:
//! 1. **Death benefits** (non-elective): Calculated separately with mortality-weighted discounting
//! 2. **Elective benefits** (income, surrender): Optimized using dynamic programming
//! 3. **Caching**: Roll-forward optimization for efficient multi-timestep calculations
//!
//! # Example
//!
//! ```rust,ignore
//! use actuarial_system::reserves::{CARVMCalculator, CARVMConfig, CARVMMethod};
//! use actuarial_system::{Policy, Assumptions};
//!
//! let assumptions = Assumptions::default_pricing();
//! let config = CARVMConfig {
//!     method: CARVMMethod::Hybrid,
//!     max_projection_months: 768,
//!     use_caching: true,
//!     revalidation_frequency: 12,
//! };
//!
//! let mut calculator = CARVMCalculator::new(assumptions, config);
//! let reserve = calculator.calculate_reserve(&policy, 0);
//! println!("Reserve: {:.2}", reserve.gross_reserve);
//! ```

mod types;
mod discount;
mod benefits;
mod carvm;
mod cache;

// Re-export public types
pub use types::{
    PolicyState,
    ReserveProjectionState,
    ReserveResult,
    ReserveComponents,
    ReserveMethod,
};

pub use discount::DiscountCurve;

pub use carvm::{
    CARVMCalculator,
    CARVMConfig,
    CARVMMethod,
};

pub use cache::{
    CachedReservePath,
    RollForwardResult,
};

pub use benefits::BenefitCalculator;

// Re-export the config for external use
// (ReserveCalcConfig is defined below in this file)

/// Simplified reserve calculation configuration for use in ProjectionConfig
///
/// This provides a high-level toggle for reserve calculations.
/// When included in ProjectionConfig, reserves will be calculated alongside projections.
#[derive(Debug, Clone)]
pub struct ReserveCalcConfig {
    /// Reserve method to use
    pub method: ReserveMethod,

    /// CARVM-specific configuration (used when method is CARVM, AG33, or AG35)
    pub carvm_config: CARVMConfig,

    /// Valuation month (typically 0 for issue-date reserves)
    pub valuation_month: u32,
}

impl Default for ReserveCalcConfig {
    fn default() -> Self {
        Self {
            method: ReserveMethod::CARVM,
            carvm_config: CARVMConfig::default(),
            valuation_month: 0,
        }
    }
}

impl ReserveCalcConfig {
    /// Create a quick CARVM config (brute force, no caching, limited projection)
    /// Good for spot-checking reserves without full optimization
    pub fn quick() -> Self {
        Self {
            method: ReserveMethod::CARVM,
            carvm_config: CARVMConfig {
                method: CARVMMethod::BruteForce,
                max_projection_months: 360, // 30 years
                use_caching: false,
                max_deferral_years: 15,
                ..Default::default()
            },
            valuation_month: 0,
        }
    }

    /// Create a full CARVM config with caching (for production use)
    pub fn full() -> Self {
        Self {
            method: ReserveMethod::CARVM,
            carvm_config: CARVMConfig {
                method: CARVMMethod::Hybrid,
                max_projection_months: 768, // 64 years
                use_caching: true,
                max_deferral_years: 30,
                ..Default::default()
            },
            valuation_month: 0,
        }
    }

    /// Set the valuation month
    pub fn at_month(mut self, month: u32) -> Self {
        self.valuation_month = month;
        self
    }
}

/// Trait for reserve calculators
///
/// Implement this trait to create custom reserve calculation methods.
pub trait ReserveCalculator {
    /// Calculate reserve for a policy at a given valuation month
    fn calculate_reserve(
        &mut self,
        policy: &crate::policy::Policy,
        valuation_month: u32,
    ) -> ReserveResult;

    /// Calculate reserves for multiple policies (can be parallelized)
    fn calculate_reserves_batch(
        &mut self,
        policies: &[crate::policy::Policy],
        valuation_month: u32,
    ) -> Vec<ReserveResult> {
        policies
            .iter()
            .map(|p| self.calculate_reserve(p, valuation_month))
            .collect()
    }

    /// Clear any cached data
    fn clear_cache(&mut self);
}
