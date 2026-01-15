//! Actuarial System CLI
//!
//! Command-line interface for running actuarial projections

use actuarial_system::{
    Policy, Assumptions,
    projection::{ProjectionEngine, ProjectionConfig},
};
use actuarial_system::policy::{QualStatus, Gender, CreditingStrategy, RollupType};
use actuarial_system::projection::CreditingApproach;
use std::fs::File;
use std::io::Write;

fn main() {
    env_logger::init();

    println!("Actuarial System v0.1.0");
    println!("======================\n");

    // Create test policy - Policy 2000 (Q, Female, Age 67, GLWB starts year 6)
    let policy = Policy::with_glwb_start(
        2000,
        QualStatus::Q,
        67,
        Gender::Female,
        105093.33,            // InitialBB
        0.784890,             // InitialPols
        80841.03,             // InitialPremium
        CreditingStrategy::Indexed,
        10,                    // SC period
        0.0475,               // valRate
        0.01,                 // MGIR
        0.3,                  // Bonus (BB bonus, not premium bonus)
        RollupType::Simple,
        6,                    // GLWBStartYear
    );

    println!("Policy: {}", policy.policy_id);
    println!("  Issue Age: {}", policy.issue_age);
    println!("  Gender: {:?}", policy.gender);
    println!("  Initial AV: ${:.2}", policy.initial_premium);
    println!("  Initial BB: ${:.2}", policy.initial_benefit_base);
    println!("  Initial Pols: {:.6}", policy.initial_pols);
    println!();

    // Set up assumptions
    let assumptions = Assumptions::default_pricing();

    // Configure projection - full 30 years
    // Using dynamic predictive lapse model and 3.78% indexed annual credit
    let config = ProjectionConfig {
        projection_months: 360,
        crediting: CreditingApproach::IndexedAnnual {
            annual_rate: 0.0378, // 3.78% annual indexed credit
        },
        detailed_output: true,
        treasury_change: 0.0,
        fixed_lapse_rate: None, // Use predictive lapse model
    };

    // Run projection
    let engine = ProjectionEngine::new(assumptions, config);
    let result = engine.project_policy(&policy);

    // Print header
    println!("Projection Results ({} months):", result.cashflows.len());
    println!("{:>5} {:>4} {:>4} {:>3} {:>14} {:>14} {:>10} {:>10} {:>14}",
        "Month", "PY", "MiPY", "Age", "BOP AV", "EOP AV", "CreditRt", "Interest", "Lives");
    println!("{}", "-".repeat(110));

    // Print first 24 months to console
    for row in result.cashflows.iter().take(24) {
        println!("{:>5} {:>4} {:>4} {:>3} {:>14.2} {:>14.2} {:>10.6} {:>10.2} {:>14.10}",
            row.projection_month,
            row.policy_year,
            row.month_in_policy_year,
            row.attained_age,
            row.bop_av,
            row.eop_av,
            row.credited_rate,
            row.interest_credits_dec,
            row.lives,
        );
    }

    if result.cashflows.len() > 24 {
        println!("... ({} more months)", result.cashflows.len() - 24);
    }

    // Write full results to CSV
    let csv_path = "projection_output.csv";
    let mut file = File::create(csv_path).expect("Unable to create CSV file");

    // Write header - includes per-policy decrement amounts for AV roll-forward
    // Lapse shown as net-of-SC (matching Excel), SurrChg shown separately
    // FPW_Pct added to show the free partial withdrawal % (incorporates RMD for qualified)
    writeln!(file, "Month,PolicyYear,MonthInPY,Age,BOP_AV,BOP_BB,FinalMortality,FinalLapse,PWD_Rate,RiderChargeRate,CreditedRate,SurrChgPct,FPW_Pct,Lives,Mortality,Lapse,PWD,SurrChg,RiderChg,Interest,EOP_AV,BaseLapse,DynamicLapse,LapseSkew").unwrap();

    // Write all rows with per-policy decrement amounts
    // Engine now calculates these using Excel's proportional allocation approach
    for row in &result.cashflows {
        writeln!(file, "{},{},{},{},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.10},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8}",
            row.projection_month,
            row.policy_year,
            row.month_in_policy_year,
            row.attained_age,
            row.bop_av,
            row.bop_benefit_base,
            row.final_mortality,
            row.final_lapse_rate,
            row.non_systematic_pwd_rate,
            row.rider_charge_rate,
            row.credited_rate,
            row.surrender_charge,
            row.fpw_pct,  // FPW% - free partial withdrawal percentage (incorporating RMD)
            row.lives,
            row.mortality_dec,
            row.lapse_dec,
            row.pwd_dec,
            row.surrender_charges_dec,
            row.rider_charges_dec,
            row.interest_credits_dec,
            row.eop_av,
            row.base_lapse_component,
            row.dynamic_lapse_component,
            row.lapse_skew,
        ).unwrap();
    }

    println!("\nFull results written to: {}", csv_path);

    // Print summary
    let summary = result.summary();
    println!("\nSummary:");
    println!("  Total Months: {}", summary.total_months);
    println!("  Total Premium: ${:.2}", summary.total_premium);
    println!("  Total Mortality CF: ${:.2}", summary.total_mortality);
    println!("  Total Lapse CF: ${:.2}", summary.total_lapse);
    println!("  Final AV: ${:.2}", summary.final_av);
    println!("  Final Lives: {:.10}", summary.final_lives);

    // Print some key milestone months for validation
    println!("\nKey Milestones (for Excel comparison):");
    let milestones = [1, 2, 12, 13, 24, 36, 48, 60, 120, 132];
    for &m in &milestones {
        if let Some(row) = result.cashflows.get(m - 1) {
            println!("  Month {:>3}: Mort={:.8} Lapse={:.8} Lives={:.10} BOP_AV={:.2}",
                m, row.final_mortality, row.final_lapse_rate, row.lives, row.bop_av);
        }
    }
}
