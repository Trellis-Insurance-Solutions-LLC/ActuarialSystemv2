//! AWS Lambda handler for running block projections
//!
//! This Lambda function accepts projection configuration and returns the Cost of Funds (IRR)
//! along with summary statistics.

use actuarial_system::{
    Assumptions,
    projection::{
        ProjectionEngine, ProjectionConfig, CreditingApproach, HedgeParams,
        calculate_cost_of_funds, DEFAULT_FIXED_ANNUAL_RATE, DEFAULT_INDEXED_ANNUAL_RATE,
    },
    policy::load_policies_from_reader,
};
use lambda_runtime::{service_fn, Error, LambdaEvent};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// Input configuration for the projection
#[derive(Debug, Deserialize)]
pub struct ProjectionRequest {
    /// Number of months to project (default: 768 for terminal age)
    #[serde(default = "default_projection_months")]
    pub projection_months: u32,

    /// Annual crediting rate for fixed products (default: 2.75%)
    #[serde(default = "default_fixed_rate")]
    pub fixed_annual_rate: f64,

    /// Annual crediting rate for indexed products (default: 3.78%)
    #[serde(default = "default_indexed_rate")]
    pub indexed_annual_rate: f64,

    /// Treasury rate change for lapse sensitivity (default: 0)
    #[serde(default)]
    pub treasury_change: f64,

    /// Optional fixed lapse rate override (bypasses dynamic model)
    #[serde(default)]
    pub fixed_lapse_rate: Option<f64>,

    /// Inforce data as CSV string (optional - uses default if not provided)
    #[serde(default)]
    pub inforce_csv: Option<String>,
}

fn default_projection_months() -> u32 { 768 }
fn default_fixed_rate() -> f64 { DEFAULT_FIXED_ANNUAL_RATE }
fn default_indexed_rate() -> f64 { DEFAULT_INDEXED_ANNUAL_RATE }

/// Output from the projection
#[derive(Debug, Serialize)]
pub struct ProjectionResponse {
    /// Cost of Funds (IRR of net cashflows) as annual percentage
    pub cost_of_funds_pct: Option<f64>,

    /// Total number of policies projected
    pub policy_count: usize,

    /// Projection duration in months
    pub projection_months: u32,

    /// Summary statistics
    pub summary: ProjectionSummary,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,

    /// Error message if projection failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectionSummary {
    /// Total initial premium
    pub total_premium: f64,
    /// Total initial AV
    pub total_initial_av: f64,
    /// Total initial benefit base
    pub total_initial_bb: f64,
    /// Total initial lives
    pub total_initial_lives: f64,
    /// Total net cashflows (undiscounted sum)
    pub total_net_cashflows: f64,
    /// Month 1 net cashflow
    pub month_1_cashflow: f64,
    /// Final month lives
    pub final_lives: f64,
    /// Final month AV
    pub final_av: f64,
}

/// Lambda handler function
async fn handler(event: LambdaEvent<ProjectionRequest>) -> Result<ProjectionResponse, Error> {
    let start = std::time::Instant::now();
    let request = event.payload;

    // Load policies
    let policies = if let Some(csv_data) = &request.inforce_csv {
        let cursor = Cursor::new(csv_data.as_bytes());
        match load_policies_from_reader(cursor) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ProjectionResponse {
                    cost_of_funds_pct: None,
                    policy_count: 0,
                    projection_months: request.projection_months,
                    summary: ProjectionSummary {
                        total_premium: 0.0,
                        total_initial_av: 0.0,
                        total_initial_bb: 0.0,
                        total_initial_lives: 0.0,
                        total_net_cashflows: 0.0,
                        month_1_cashflow: 0.0,
                        final_lives: 0.0,
                        final_av: 0.0,
                    },
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("Failed to parse inforce CSV: {}", e)),
                });
            }
        }
    } else {
        // Use embedded default inforce (for now, return error - in production would use S3)
        return Ok(ProjectionResponse {
            cost_of_funds_pct: None,
            policy_count: 0,
            projection_months: request.projection_months,
            summary: ProjectionSummary {
                total_premium: 0.0,
                total_initial_av: 0.0,
                total_initial_bb: 0.0,
                total_initial_lives: 0.0,
                total_net_cashflows: 0.0,
                month_1_cashflow: 0.0,
                final_lives: 0.0,
                final_av: 0.0,
            },
            execution_time_ms: start.elapsed().as_millis() as u64,
            error: Some("No inforce_csv provided. Please include inforce data in request.".to_string()),
        });
    };

    let policy_count = policies.len();

    // Set up assumptions and config
    let assumptions = Assumptions::default_pricing();
    let config = ProjectionConfig {
        projection_months: request.projection_months,
        crediting: CreditingApproach::PolicyBased {
            fixed_annual_rate: request.fixed_annual_rate,
            indexed_annual_rate: request.indexed_annual_rate,
        },
        detailed_output: false,
        treasury_change: request.treasury_change,
        fixed_lapse_rate: request.fixed_lapse_rate,
        hedge_params: Some(HedgeParams::default()),
    };

    // Run projections in parallel
    let results: Vec<_> = policies
        .par_iter()
        .map(|policy| {
            let engine = ProjectionEngine::new(assumptions.clone(), config.clone());
            engine.project_policy(policy)
        })
        .collect();

    // Aggregate results
    let num_months = request.projection_months as usize;
    let mut aggregated_cashflows = vec![0.0_f64; num_months];
    let mut total_initial_av = 0.0;
    let mut total_initial_bb = 0.0;
    let mut total_initial_lives = 0.0;
    let mut total_premium = 0.0;
    let mut final_lives = 0.0;
    let mut final_av = 0.0;

    for result in &results {
        for row in &result.cashflows {
            let idx = (row.projection_month - 1) as usize;
            if idx < num_months {
                aggregated_cashflows[idx] += row.total_net_cashflow;
            }
        }

        if let Some(first) = result.cashflows.first() {
            total_initial_av += first.bop_av;
            total_initial_bb += first.bop_benefit_base;
            total_initial_lives += first.lives;
            total_premium += first.premium;
        }

        if let Some(last) = result.cashflows.last() {
            final_lives += last.lives;
            final_av += last.eop_av;
        }
    }

    let total_net_cashflows: f64 = aggregated_cashflows.iter().sum();
    let month_1_cashflow = aggregated_cashflows.first().copied().unwrap_or(0.0);

    // Calculate Cost of Funds (IRR)
    let cost_of_funds = calculate_cost_of_funds(&aggregated_cashflows);
    let cost_of_funds_pct = cost_of_funds.map(|r| r * 100.0);

    let execution_time_ms = start.elapsed().as_millis() as u64;

    Ok(ProjectionResponse {
        cost_of_funds_pct,
        policy_count,
        projection_months: request.projection_months,
        summary: ProjectionSummary {
            total_premium,
            total_initial_av,
            total_initial_bb,
            total_initial_lives,
            total_net_cashflows,
            month_1_cashflow,
            final_lives,
            final_av,
        },
        execution_time_ms,
        error: None,
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize logging
    env_logger::init();

    // Run the Lambda runtime
    lambda_runtime::run(service_fn(handler)).await
}
