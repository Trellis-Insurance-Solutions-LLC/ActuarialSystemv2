//! Compare Rust projections against Excel outputs for specific policies
//!
//! Usage: cargo run --bin compare_policies

use actuarial_system::{
    Assumptions,
    projection::{
        ProjectionEngine, ProjectionConfig, CashflowRow, CreditingApproach, HedgeParams,
        DEFAULT_FIXED_ANNUAL_RATE, DEFAULT_INDEXED_ANNUAL_RATE,
    },
};
use actuarial_system::policy::load_default_inforce;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

fn main() {
    let policy_ids = vec![4, 1404];

    println!("Loading policies from pricing_inforce.csv...");
    let all_policies = load_default_inforce().expect("Failed to load policies");

    let assumptions = Assumptions::default_pricing();
    let config = ProjectionConfig {
        projection_months: 36, // Just first 3 years for debugging
        crediting: CreditingApproach::PolicyBased {
            fixed_annual_rate: DEFAULT_FIXED_ANNUAL_RATE,
            indexed_annual_rate: DEFAULT_INDEXED_ANNUAL_RATE,
        },
        detailed_output: true,
        treasury_change: 0.0,
        fixed_lapse_rate: None,
        hedge_params: Some(HedgeParams::default()),
        reserve_config: None,
    };

    for policy_id in policy_ids {
        println!("\n{}", "=".repeat(60));
        println!("Policy {}", policy_id);
        println!("{}", "=".repeat(60));

        // Find the policy
        let policy = all_policies.iter()
            .find(|p| p.policy_id == policy_id)
            .expect(&format!("Policy {} not found", policy_id));

        println!("  Issue Age: {}, GLWB Start Year: {}, Crediting: {:?}",
                 policy.issue_age, policy.glwb_start_year, policy.crediting_strategy);

        // Run projection
        let engine = ProjectionEngine::new(assumptions.clone(), config.clone());
        let result = engine.project_policy(policy);

        // Write Rust output
        let rust_output_path = format!("cashflow_examples/rust_output_{}.csv", policy_id);
        write_rust_output(&rust_output_path, &result.cashflows);
        println!("  Rust output written to: {}", rust_output_path);

        // Load Excel output and compare
        let excel_path = format!("cashflow_examples/output_{}.csv", policy_id);
        compare_outputs(&excel_path, &result.cashflows, policy_id);
    }
}

fn write_rust_output(path: &str, cashflows: &[CashflowRow]) {
    let mut file = File::create(path).expect("Failed to create output file");

    // Header matching Excel format
    writeln!(file, "Projection month,Policy year,Month in policy year,Attained age,\
        Baseline mortality,Mortality improvement,Final mortality,Surrender charge,FPW %,\
        GLWB activated,Non-systematic PWD rate,Lapse skew,Premium,BOP AV,BOP Benefit base,\
        Base component,Dynamic component,Final lapse rate,Rider charge,Credited rate,\
        Systematic withdrawal,Rollup rate,AV persistency,BB persistency,Lives persistency,\
        Lives,Pre-decrement AV,Mortality,Lapse,PWD,Rider charges,Surrender charges,\
        Interest credits,EOP AV,Expenses,Agent Commission,IMO Override,Wholesaler Override,\
        Chargebacks,Bonus comp,Total net cashflow,Net index credit reimbursement,Hedge gains").unwrap();

    for row in cashflows {
        writeln!(file, "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            row.projection_month,
            row.policy_year,
            row.month_in_policy_year,
            row.attained_age,
            row.baseline_mortality,
            row.mortality_improvement,
            row.final_mortality,
            row.surrender_charge,
            row.fpw_pct,
            if row.glwb_activated { 1 } else { 0 },
            row.non_systematic_pwd_rate,
            row.lapse_skew,
            row.premium,
            row.bop_av,
            row.bop_benefit_base,
            row.base_lapse_component,
            row.dynamic_lapse_component,
            row.final_lapse_rate,
            row.rider_charge_rate,
            row.credited_rate,
            row.systematic_withdrawal,
            row.rollup_rate,
            row.av_persistency,
            row.bb_persistency,
            row.lives_persistency,
            row.lives,
            row.pre_decrement_av,
            row.mortality_dec,
            row.lapse_dec,
            row.pwd_dec,
            row.rider_charges_dec,
            row.surrender_charges_dec,
            row.interest_credits_dec,
            row.eop_av,
            row.expenses,
            row.agent_commission,
            row.imo_override,
            row.wholesaler_override,
            row.chargebacks,
            row.bonus_comp,
            row.total_net_cashflow,
            row.net_index_credit_reimbursement,
            row.hedge_gains,
        ).unwrap();
    }
}

fn compare_outputs(excel_path: &str, rust_cashflows: &[CashflowRow], _policy_id: u32) {
    let file = match File::open(excel_path) {
        Ok(f) => f,
        Err(_) => {
            println!("  Excel file not found: {}", excel_path);
            return;
        }
    };

    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    // Skip header
    let _header = lines.next();

    println!("\n  Comparison (first divergence highlighted):");
    println!("  {:<5} {:<12} {:<12} {:<12} | {:<12} {:<12} {:<12}",
             "Month", "Excel_BOP", "Rust_BOP", "Diff_BOP", "Excel_EOP", "Rust_EOP", "Diff_EOP");
    println!("  {:-<89}", "");

    let mut first_divergence_found = false;

    for (i, line_result) in lines.enumerate() {
        if i >= rust_cashflows.len() {
            break;
        }

        let line = line_result.expect("Failed to read line");
        let fields: Vec<&str> = line.split(',').collect();

        // Column indices from Excel format:
        // 13 = BOP AV, 33 = EOP AV, 20 = Credited rate, 21 = Systematic withdrawal
        // 32 = Interest credits
        let excel_bop_av: f64 = fields.get(13).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let excel_eop_av: f64 = fields.get(33).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let excel_credited_rate: f64 = fields.get(19).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let excel_sys_wd: f64 = fields.get(20).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let excel_interest: f64 = fields.get(32).and_then(|s| s.parse().ok()).unwrap_or(0.0);

        let rust_row = &rust_cashflows[i];
        let month = rust_row.projection_month;

        let bop_diff = rust_row.bop_av - excel_bop_av;
        let eop_diff = rust_row.eop_av - excel_eop_av;

        // Check for significant divergence (> 0.01)
        let has_divergence = bop_diff.abs() > 0.01 || eop_diff.abs() > 0.01;

        if has_divergence || month <= 15 {
            let marker = if has_divergence && !first_divergence_found { ">>>" } else { "   " };

            println!("{} {:<5} {:>12.4} {:>12.4} {:>12.6} | {:>12.4} {:>12.4} {:>12.6}",
                     marker, month, excel_bop_av, rust_row.bop_av, bop_diff,
                     excel_eop_av, rust_row.eop_av, eop_diff);

            if has_divergence && !first_divergence_found {
                first_divergence_found = true;

                // Print detailed breakdown for the divergent month
                println!("\n  === DETAILED BREAKDOWN FOR MONTH {} ===", month);
                println!("  {:30} {:>15} {:>15} {:>15}", "Field", "Excel", "Rust", "Diff");
                println!("  {:-<75}", "");
                println!("  {:30} {:>15.6} {:>15.6} {:>15.6}", "BOP AV",
                         excel_bop_av, rust_row.bop_av, bop_diff);
                println!("  {:30} {:>15.6} {:>15.6} {:>15.6}", "Credited Rate",
                         excel_credited_rate, rust_row.credited_rate,
                         rust_row.credited_rate - excel_credited_rate);
                println!("  {:30} {:>15.6} {:>15.6} {:>15.6}", "Systematic WD",
                         excel_sys_wd, rust_row.systematic_withdrawal,
                         rust_row.systematic_withdrawal - excel_sys_wd);
                println!("  {:30} {:>15.6} {:>15.6} {:>15.6}", "Interest Credits",
                         excel_interest, rust_row.interest_credits_dec,
                         rust_row.interest_credits_dec - excel_interest);
                println!("  {:30} {:>15.6} {:>15.6} {:>15.6}", "EOP AV",
                         excel_eop_av, rust_row.eop_av, eop_diff);

                // Also show pre-decrement AV
                let excel_pre_dec: f64 = fields.get(26).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                println!("  {:30} {:>15.6} {:>15.6} {:>15.6}", "Pre-decrement AV",
                         excel_pre_dec, rust_row.pre_decrement_av,
                         rust_row.pre_decrement_av - excel_pre_dec);

                // Show BOP BB
                let excel_bop_bb: f64 = fields.get(14).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                println!("  {:30} {:>15.6} {:>15.6} {:>15.6}", "BOP Benefit Base",
                         excel_bop_bb, rust_row.bop_benefit_base,
                         rust_row.bop_benefit_base - excel_bop_bb);

                println!();
            }
        }
    }

    if !first_divergence_found {
        println!("\n  ✓ No significant divergence found in first {} months!", rust_cashflows.len());
    }
}
