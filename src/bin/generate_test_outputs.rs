//! Generate projection outputs for all test policies in cashflow_examples/
//!
//! This binary creates projection_output_*.csv files that can be compared
//! against the Excel reference outputs.

use actuarial_system::{
    Policy, Assumptions,
    projection::{ProjectionEngine, ProjectionConfig, CreditingApproach, HedgeParams, DEFAULT_INDEXED_ANNUAL_RATE},
};
use actuarial_system::policy::{QualStatus, Gender, CreditingStrategy, RollupType};
use std::fs::File;
use std::io::Write;

/// Test policy configuration
struct TestPolicy {
    policy_id: u32,
    qual_status: QualStatus,
    issue_age: u8,
    gender: Gender,
    initial_bb: f64,
    initial_pols: f64,
    initial_premium: f64,
    glwb_start_year: u32,
}

fn main() {
    env_logger::init();

    println!("Generating test policy outputs...\n");

    // Define all test policies from pricing_inforce.csv
    // Format: QualStatus,IssueAge,Gender,InitialBB,InitialPols,InitialPremium,...,GLWBStartYear,WaitPeriod
    let test_policies = vec![
        // Policy 2: N,57,Female,112.8484231,0.004042606,86.80647933,[0,50000),Indexed,2,SCPeriod=10,GLWBStartYear=1
        TestPolicy {
            policy_id: 2,
            qual_status: QualStatus::N,
            issue_age: 57,
            gender: Gender::Female,
            initial_bb: 112.8484231,
            initial_pols: 0.004042606,
            initial_premium: 86.80647933,
            glwb_start_year: 1,
        },
        // Policy 10: N,57,Female,463.2844779,0.016596393,356.3726753,[0,50000),0.003921188,Indexed,10,...,5,4
        TestPolicy {
            policy_id: 10,
            qual_status: QualStatus::N,
            issue_age: 57,
            gender: Gender::Female,
            initial_bb: 463.2844779,
            initial_pols: 0.016596393,
            initial_premium: 356.3726753,
            glwb_start_year: 5,
        },
        // Policy 22: N,57,Female,13208.30638,0.473165516,10160.23568,Indexed,GLWBStartYear=12,WaitPeriod=11
        TestPolicy {
            policy_id: 22,
            qual_status: QualStatus::N,
            issue_age: 57,
            gender: Gender::Female,
            initial_bb: 13208.30638,
            initial_pols: 0.473165516,
            initial_premium: 10160.23568,
            glwb_start_year: 12,
        },
        // Policy 1234: N,77,Female,45757.28471,0.072922627,35197.91132,Indexed,GLWBStartYear=1,WaitPeriod=0 (AV exhausts ~month 123)
        TestPolicy {
            policy_id: 1234,
            qual_status: QualStatus::N,
            issue_age: 77,
            gender: Gender::Female,
            initial_bb: 45757.28471,
            initial_pols: 0.072922627,
            initial_premium: 35197.91132,
            glwb_start_year: 1,
        },
        // Policy 1144: N,77,Female,11549.4487,0.387622541,8884.191306,Indexed,GLWBStartYear=15,WaitPeriod=14
        TestPolicy {
            policy_id: 1144,
            qual_status: QualStatus::N,
            issue_age: 77,
            gender: Gender::Female,
            initial_bb: 11549.4487,
            initial_pols: 0.387622541,
            initial_premium: 8884.191306,
            glwb_start_year: 15,
        },
        // Policy 2178: N,67,Male,1121266.128,Indexed,GLWBStartYear=12 (largest hedge diff)
        TestPolicy {
            policy_id: 2178,
            qual_status: QualStatus::N,
            issue_age: 67,
            gender: Gender::Male,
            initial_bb: 1121266.128,
            initial_pols: 3.941463896,
            initial_premium: 862512.4062,
            glwb_start_year: 12,
        },
        // Policy 100: N,57,Female,3225.474204,0.049357974,2481.134003,[50000,100000),0.014856668,Indexed,100,...,8,7
        TestPolicy {
            policy_id: 100,
            qual_status: QualStatus::N,
            issue_age: 57,
            gender: Gender::Female,
            initial_bb: 3225.474204,
            initial_pols: 0.049357974,
            initial_premium: 2481.134003,
            glwb_start_year: 8,
        },
        // Policy 200: N,57,Male,1044.918072,0.003811334,803.7831322,[200000,500000),0.001753843,Indexed,200,...,2,1
        TestPolicy {
            policy_id: 200,
            qual_status: QualStatus::N,
            issue_age: 57,
            gender: Gender::Male,
            initial_bb: 1044.918072,
            initial_pols: 0.003811334,
            initial_premium: 803.7831322,
            glwb_start_year: 2,
        },
        // Policy 1450: Q,57,Female,289382.9265,2.194709035,222602.2512,[100000,200000),0.161825759,Indexed,1450,...,12,11
        TestPolicy {
            policy_id: 1450,
            qual_status: QualStatus::Q,
            issue_age: 57,
            gender: Gender::Female,
            initial_bb: 289382.9265,
            initial_pols: 2.194709035,
            initial_premium: 222602.2512,
            glwb_start_year: 12,
        },
        // Policy 2000: Q,67,Female,105093.3338,0.784890245,80841.02598,[100000,200000),0.033229862,Indexed,2000,...,6,5
        TestPolicy {
            policy_id: 2000,
            qual_status: QualStatus::Q,
            issue_age: 67,
            gender: Gender::Female,
            initial_bb: 105093.3338,
            initial_pols: 0.784890245,
            initial_premium: 80841.02598,
            glwb_start_year: 6,
        },
        // Policy 2800: Q,77,Male,27178.1619,0.039019025,20906.27839,[500000,Inf),0.051961232,Indexed,2800,...,99,98
        TestPolicy {
            policy_id: 2800,
            qual_status: QualStatus::Q,
            issue_age: 77,
            gender: Gender::Male,
            initial_bb: 27178.1619,
            initial_pols: 0.039019025,
            initial_premium: 20906.27839,
            glwb_start_year: 99, // Never activates
        },
    ];

    // Load assumptions once
    let assumptions = Assumptions::default_pricing();

    // Standard projection config
    let config = ProjectionConfig {
        projection_months: 768, // Run to terminal age 121
        crediting: CreditingApproach::IndexedAnnual {
            annual_rate: DEFAULT_INDEXED_ANNUAL_RATE,
        },
        detailed_output: true,
        treasury_change: 0.0,
        fixed_lapse_rate: None, // Use predictive lapse model
        hedge_params: Some(HedgeParams::default()),
        reserve_config: None,
    };

    for tp in &test_policies {
        println!("Processing Policy {}...", tp.policy_id);

        // Create policy with Bonus=0.3 (30% bonus used in rollup formula)
        // InitialBB already includes the bonus (InitialBB = Premium * 1.3)
        // But the rollup formula still needs the bonus parameter
        let policy = Policy::with_glwb_start(
            tp.policy_id,
            tp.qual_status,
            tp.issue_age,
            tp.gender,
            tp.initial_bb,
            tp.initial_pols,
            tp.initial_premium,
            CreditingStrategy::Indexed,
            10,              // SC period
            0.0475,          // valRate
            0.01,            // MGIR
            0.3,             // Bonus (30% for rollup formula)
            RollupType::Simple,
            tp.glwb_start_year,
        );

        // Run projection
        let engine = ProjectionEngine::new(assumptions.clone(), config.clone());
        let result = engine.project_policy(&policy);

        // Write output to cashflow_examples/
        let csv_path = format!("cashflow_examples/rust_output_{}.csv", tp.policy_id);
        let mut file = File::create(&csv_path).expect("Unable to create CSV file");

        // Write header matching the Rust output format
        writeln!(file, "Month,PolicyYear,MonthInPY,Age,BOP_AV,BOP_BB,FinalMortality,FinalLapse,PWD_Rate,RiderChargeRate,CreditedRate,SurrChgPct,FPW_Pct,Lives,Mortality,Lapse,PWD,SurrChg,RiderChg,Interest,EOP_AV,BaseLapse,DynamicLapse,LapseSkew,NetIndexCreditReimb,HedgeGains").unwrap();

        // Write all rows
        for row in &result.cashflows {
            writeln!(file, "{},{},{},{},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.10},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.9},{:.9}",
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
                row.fpw_pct,
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
                row.net_index_credit_reimbursement,
                row.hedge_gains,
            ).unwrap();
        }

        println!("  -> Written to {}", csv_path);

        // Print summary stats
        let summary = result.summary();
        println!("     Final AV: ${:.2}, Final Lives: {:.8}", summary.final_av, summary.final_lives);
    }

    println!("\nDone! Generated {} output files.", test_policies.len());
}
