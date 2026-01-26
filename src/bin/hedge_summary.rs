//! Output total hedge gains by policy ID

use actuarial_system::{
    Assumptions,
    projection::{ProjectionEngine, ProjectionConfig, CreditingApproach, HedgeParams, DEFAULT_FIXED_ANNUAL_RATE, DEFAULT_INDEXED_ANNUAL_RATE},
};
use actuarial_system::policy::load_default_inforce;
use rayon::prelude::*;
use std::fs::File;
use std::io::Write;

fn main() {
    let policies = load_default_inforce().expect("Failed to load policies");
    let assumptions = Assumptions::default_pricing();
    let config = ProjectionConfig {
        projection_months: 768, // Run to terminal age 121
        crediting: CreditingApproach::PolicyBased {
            fixed_annual_rate: DEFAULT_FIXED_ANNUAL_RATE,
            indexed_annual_rate: DEFAULT_INDEXED_ANNUAL_RATE,
        },
        detailed_output: false,
        treasury_change: 0.0,
        fixed_lapse_rate: None,
        hedge_params: Some(HedgeParams::default()),
        reserve_config: None,
    };

    // Run projections in parallel and collect (policy_id, total_hedge_gains)
    let results: Vec<(u32, f64)> = policies
        .par_iter()
        .map(|policy| {
            let engine = ProjectionEngine::new(assumptions.clone(), config.clone());
            let result = engine.project_policy(policy);
            let total_hedge: f64 = result.cashflows.iter().map(|r| r.hedge_gains).sum();
            (policy.policy_id, total_hedge)
        })
        .collect();

    let mut file = File::create("hedge_gains_by_policy.csv").unwrap();
    writeln!(file, "PolicyID,TotalHedgeGains").unwrap();
    
    let mut sorted_results = results;
    sorted_results.sort_by_key(|(id, _)| *id);
    
    for (policy_id, total_hedge) in &sorted_results {
        writeln!(file, "{},{:.6}", policy_id, total_hedge).unwrap();
    }
    
    println!("Written {} policies to hedge_gains_by_policy.csv", sorted_results.len());
}
