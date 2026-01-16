//! Calculate Cost of Funds for the block projection
//!
//! This binary runs the block projection and calculates the IRR (Cost of Funds)
//! Supports JSON output for API integration via --json flag
//! Accepts config via environment variables:
//!   PROJECTION_MONTHS, FIXED_ANNUAL_RATE, INDEXED_ANNUAL_RATE, TREASURY_CHANGE

use actuarial_system::{
    Assumptions,
    projection::{
        ProjectionEngine, ProjectionConfig, CreditingApproach, HedgeParams,
        calculate_cost_of_funds, DEFAULT_FIXED_ANNUAL_RATE, DEFAULT_INDEXED_ANNUAL_RATE,
    },
};
use actuarial_system::policy::load_default_inforce;
use rayon::prelude::*;
use serde::Serialize;
use std::env;
use std::time::Instant;

#[derive(Serialize)]
struct ProjectionResponse {
    cost_of_funds_pct: Option<f64>,
    policy_count: usize,
    projection_months: u32,
    summary: ProjectionSummary,
    execution_time_ms: u64,
}

#[derive(Serialize)]
struct ProjectionSummary {
    total_premium: f64,
    total_initial_av: f64,
    total_initial_bb: f64,
    total_initial_lives: f64,
    total_net_cashflows: f64,
    month_1_cashflow: f64,
    final_lives: f64,
    final_av: f64,
}

fn main() {
    env_logger::init();

    let json_output = env::args().any(|arg| arg == "--json");
    let start = Instant::now();

    // Read config from environment or use defaults
    let projection_months: u32 = env::var("PROJECTION_MONTHS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(768);

    let fixed_annual_rate: f64 = env::var("FIXED_ANNUAL_RATE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FIXED_ANNUAL_RATE);

    let indexed_annual_rate: f64 = env::var("INDEXED_ANNUAL_RATE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_INDEXED_ANNUAL_RATE);

    let treasury_change: f64 = env::var("TREASURY_CHANGE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    if !json_output {
        println!("Loading policies from pricing_inforce.csv...");
    }

    let policies = load_default_inforce().expect("Failed to load policies");

    if !json_output {
        println!("Loaded {} policies in {:?}", policies.len(), start.elapsed());
    }

    let policy_count = policies.len();

    // Load assumptions
    let assumptions = Assumptions::default_pricing();

    // Projection config from environment
    let config = ProjectionConfig {
        projection_months,
        crediting: CreditingApproach::PolicyBased {
            fixed_annual_rate,
            indexed_annual_rate,
        },
        detailed_output: false,
        treasury_change,
        fixed_lapse_rate: None,
        hedge_params: Some(HedgeParams::default()),
    };

    if !json_output {
        println!("Running projections...");
    }

    let proj_start = Instant::now();

    // Run projections in parallel and collect net cashflows
    let results: Vec<_> = policies
        .par_iter()
        .map(|policy| {
            let engine = ProjectionEngine::new(assumptions.clone(), config.clone());
            engine.project_policy(policy)
        })
        .collect();

    if !json_output {
        println!("Projections complete in {:?}", proj_start.elapsed());
    }

    // Aggregate results
    let num_months = config.projection_months as usize;
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

    if json_output {
        // Output JSON for API consumption
        let response = ProjectionResponse {
            cost_of_funds_pct,
            policy_count,
            projection_months,
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
        };
        println!("{}", serde_json::to_string(&response).unwrap());
    } else {
        // Human-readable output
        println!("\nNet Cashflow Summary:");
        println!("  Month 1:   ${:.2}", aggregated_cashflows[0]);
        if num_months > 11 {
            println!("  Month 12:  ${:.2}", aggregated_cashflows[11]);
        }
        if num_months > 59 {
            println!("  Month 60:  ${:.2}", aggregated_cashflows[59]);
        }
        if num_months > 119 {
            println!("  Month 120: ${:.2}", aggregated_cashflows[119]);
        }
        if num_months > 359 {
            println!("  Month 360: ${:.2}", aggregated_cashflows[359]);
        }
        if num_months > 767 {
            println!("  Month 768: ${:.2}", aggregated_cashflows[767]);
        }

        println!("\n  Total (undiscounted): ${:.2}", total_net_cashflows);

        println!("\nCalculating Cost of Funds (IRR)...");

        match cost_of_funds_pct {
            Some(pct) => {
                println!("\n========================================");
                println!("  COST OF FUNDS: {:.4}%", pct);
                println!("========================================");
            }
            None => {
                println!("\n  Could not calculate IRR (no solution found)");
            }
        }

        println!("\nTotal time: {:?}", start.elapsed());
    }
}
