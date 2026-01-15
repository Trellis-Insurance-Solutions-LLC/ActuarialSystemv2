//! Scenario runner for efficient batch projections
//!
//! Pre-loads assumptions once, then allows running many projections with
//! different configurations without re-reading CSV files.

use crate::{Assumptions, Policy};
use crate::projection::{ProjectionEngine, ProjectionConfig, ProjectionResult};

/// Pre-loaded scenario runner for efficient batch projections
///
/// # Example
/// ```ignore
/// let runner = ScenarioRunner::from_csv()?;
///
/// // Run many scenarios with different configs
/// for rate in [0.03, 0.04, 0.05] {
///     let config = ProjectionConfig { ... };
///     let result = runner.run(&policy, config);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ScenarioRunner {
    /// Pre-loaded base assumptions
    base_assumptions: Assumptions,
}

impl ScenarioRunner {
    /// Create runner with default in-memory assumptions
    pub fn new() -> Self {
        Self {
            base_assumptions: Assumptions::default_pricing(),
        }
    }

    /// Create runner by loading assumptions from CSV files
    pub fn from_csv() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            base_assumptions: Assumptions::from_csv()?,
        })
    }

    /// Create runner from specific assumptions directory
    pub fn from_csv_path(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            base_assumptions: Assumptions::from_csv_path(path)?,
        })
    }

    /// Create runner with pre-built assumptions
    pub fn with_assumptions(assumptions: Assumptions) -> Self {
        Self {
            base_assumptions: assumptions,
        }
    }

    /// Run a single projection with the given config
    /// Clones the base assumptions internally (very fast ~0.3μs)
    pub fn run(&self, policy: &Policy, config: ProjectionConfig) -> ProjectionResult {
        let engine = ProjectionEngine::new(self.base_assumptions.clone(), config);
        engine.project_policy(policy)
    }

    /// Run projections for multiple policies with the same config
    pub fn run_batch(&self, policies: &[Policy], config: ProjectionConfig) -> Vec<ProjectionResult> {
        let engine = ProjectionEngine::new(self.base_assumptions.clone(), config);
        policies.iter().map(|p| engine.project_policy(p)).collect()
    }

    /// Run multiple scenarios (different configs) for a single policy
    pub fn run_scenarios(&self, policy: &Policy, configs: &[ProjectionConfig]) -> Vec<ProjectionResult> {
        configs
            .iter()
            .map(|config| {
                let engine = ProjectionEngine::new(self.base_assumptions.clone(), config.clone());
                engine.project_policy(policy)
            })
            .collect()
    }

    /// Get reference to base assumptions for inspection/modification
    pub fn assumptions(&self) -> &Assumptions {
        &self.base_assumptions
    }

    /// Get mutable reference to base assumptions for customization
    pub fn assumptions_mut(&mut self) -> &mut Assumptions {
        &mut self.base_assumptions
    }
}

impl Default for ScenarioRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{QualStatus, Gender, CreditingStrategy, RollupType};
    use crate::projection::CreditingApproach;

    fn test_policy() -> Policy {
        Policy::new(
            1,
            QualStatus::Q,
            65,
            Gender::Male,
            100_000.0,
            1.0,
            100_000.0,
            CreditingStrategy::Indexed,
            10,
            0.0475,
            0.01,
            0.3,
            RollupType::Simple,
        )
    }

    #[test]
    fn test_scenario_runner_batch() {
        let runner = ScenarioRunner::new();
        let policy = test_policy();

        let configs: Vec<_> = [0.03, 0.04, 0.05]
            .iter()
            .map(|&rate| ProjectionConfig {
                projection_months: 120,
                crediting: CreditingApproach::IndexedAnnual { annual_rate: rate },
                detailed_output: false,
                treasury_change: 0.0,
                fixed_lapse_rate: Some(0.05),
            })
            .collect();

        let results = runner.run_scenarios(&policy, &configs);
        assert_eq!(results.len(), 3);

        // Higher credit rate should result in higher final AV
        assert!(results[2].summary().final_av > results[0].summary().final_av);
    }
}
