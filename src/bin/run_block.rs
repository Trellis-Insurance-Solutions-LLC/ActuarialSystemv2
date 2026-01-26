//! Run projection for entire block from pricing_inforce.csv
//!
//! Outputs monthly aggregated cashflows for comparison with Excel

use actuarial_system::{
    Assumptions,
    projection::{
        ProjectionEngine, ProjectionConfig, CashflowRow, CreditingApproach, HedgeParams,
        DEFAULT_FIXED_ANNUAL_RATE, DEFAULT_INDEXED_ANNUAL_RATE,
    },
};
use actuarial_system::policy::load_default_inforce;
use rayon::prelude::*;
use std::fs::File;
use std::io::Write;
use std::time::Instant;

/// Aggregated monthly results across all policies
#[derive(Debug, Clone, Default)]
struct AggregatedRow {
    month: u32,
    total_bop_av: f64,
    total_bop_bb: f64,
    total_lives: f64,
    total_mortality: f64,
    total_lapse: f64,
    total_pwd: f64,
    total_rider_charges: f64,
    total_surrender_charges: f64,
    total_interest: f64,
    total_eop_av: f64,
    // New fields
    total_expenses: f64,
    total_agent_commission: f64,
    total_imo_override: f64,
    total_wholesaler_override: f64,
    total_bonus_comp: f64,
    total_chargebacks: f64,
    total_hedge_gains: f64,
    total_net_cashflow: f64,
}

fn main() {
    env_logger::init();

    let start = Instant::now();
    println!("Loading policies from pricing_inforce.csv...");

    let policies = load_default_inforce().expect("Failed to load policies");
    println!("Loaded {} policies in {:?}", policies.len(), start.elapsed());

    // Load assumptions
    let assumptions = Assumptions::default_pricing();

    // Standard projection config - uses policy's crediting strategy
    let config = ProjectionConfig {
        projection_months: 768, // Run to terminal age 121 for youngest issue age 57
        crediting: CreditingApproach::PolicyBased {
            fixed_annual_rate: DEFAULT_FIXED_ANNUAL_RATE,
            indexed_annual_rate: DEFAULT_INDEXED_ANNUAL_RATE,
        },
        detailed_output: false, // Don't need detailed lapse components
        treasury_change: 0.0,
        fixed_lapse_rate: None,
        hedge_params: Some(HedgeParams::default()),
        reserve_config: None,
    };

    println!("Running projections...");
    let proj_start = Instant::now();

    // Run projections in parallel
    let results: Vec<Vec<CashflowRow>> = policies
        .par_iter()
        .map(|policy| {
            let engine = ProjectionEngine::new(assumptions.clone(), config.clone());
            let result = engine.project_policy(policy);
            result.cashflows
        })
        .collect();

    println!("Projections complete in {:?}", proj_start.elapsed());

    // Aggregate results by month
    println!("Aggregating results...");
    let mut aggregated: Vec<AggregatedRow> = (1..=768)
        .map(|m| AggregatedRow { month: m, ..Default::default() })
        .collect();

    for cashflows in &results {
        for row in cashflows {
            let idx = (row.projection_month - 1) as usize;
            if idx < aggregated.len() {
                let agg = &mut aggregated[idx];
                agg.total_bop_av += row.bop_av;
                agg.total_bop_bb += row.bop_benefit_base;
                agg.total_lives += row.lives;
                agg.total_mortality += row.mortality_dec;
                agg.total_lapse += row.lapse_dec;
                agg.total_pwd += row.pwd_dec;
                agg.total_rider_charges += row.rider_charges_dec;
                agg.total_surrender_charges += row.surrender_charges_dec;
                agg.total_interest += row.interest_credits_dec;
                agg.total_eop_av += row.eop_av;
                // New fields
                agg.total_expenses += row.expenses;
                agg.total_agent_commission += row.agent_commission;
                agg.total_imo_override += row.imo_override;
                agg.total_wholesaler_override += row.wholesaler_override;
                agg.total_bonus_comp += row.bonus_comp;
                agg.total_chargebacks += row.chargebacks;
                agg.total_hedge_gains += row.hedge_gains;
                agg.total_net_cashflow += row.total_net_cashflow;
            }
        }
    }

    // Write output
    let output_path = "block_projection_output.csv";
    let mut file = File::create(output_path).expect("Failed to create output file");

    writeln!(file, "Month,BOP_AV,BOP_BB,Lives,Mortality,Lapse,PWD,RiderCharges,SurrCharges,Interest,EOP_AV,Expenses,AgentComm,IMOOverride,WholesalerOverride,BonusComp,Chargebacks,HedgeGains,NetCashflow").unwrap();

    for row in &aggregated {
        writeln!(
            file,
            "{},{:.2},{:.2},{:.8},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
            row.month,
            row.total_bop_av,
            row.total_bop_bb,
            row.total_lives,
            row.total_mortality,
            row.total_lapse,
            row.total_pwd,
            row.total_rider_charges,
            row.total_surrender_charges,
            row.total_interest,
            row.total_eop_av,
            row.total_expenses,
            row.total_agent_commission,
            row.total_imo_override,
            row.total_wholesaler_override,
            row.total_bonus_comp,
            row.total_chargebacks,
            row.total_hedge_gains,
            row.total_net_cashflow,
        ).unwrap();
    }

    println!("Output written to {}", output_path);

    // Print summary stats
    println!("\nBlock Summary:");
    println!("  Month 1:   Lives={:.4}, BOP_AV=${:.0}, BOP_BB=${:.0}",
             aggregated[0].total_lives,
             aggregated[0].total_bop_av,
             aggregated[0].total_bop_bb);
    println!("  Month 120: Lives={:.4}, BOP_AV=${:.0}",
             aggregated[119].total_lives,
             aggregated[119].total_bop_av);
    println!("  Month 360: Lives={:.4}, BOP_AV=${:.0}",
             aggregated[359].total_lives,
             aggregated[359].total_bop_av);
    println!("  Month 528: Lives={:.4}, BOP_AV=${:.0} (oldest issue age 77 reaches 121)",
             aggregated[527].total_lives,
             aggregated[527].total_bop_av);
    println!("  Month 768: Lives={:.4}, BOP_AV=${:.0} (youngest issue age 57 reaches 121)",
             aggregated[767].total_lives,
             aggregated[767].total_bop_av);

    println!("\nTotal time: {:?}", start.elapsed());
}
