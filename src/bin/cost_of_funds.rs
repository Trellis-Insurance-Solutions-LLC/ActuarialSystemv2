//! Calculate Cost of Funds for the block projection
//!
//! This binary runs the block projection and calculates the IRR (Cost of Funds)
//! Supports JSON output for API integration via --json flag
//! Accepts config via environment variables:
//!   PROJECTION_MONTHS, FIXED_ANNUAL_RATE, INDEXED_ANNUAL_RATE, TREASURY_CHANGE
//!   INFORCE_FIXED_PCT, INFORCE_MALE_MULT, INFORCE_FEMALE_MULT,
//!   INFORCE_QUAL_MULT, INFORCE_NONQUAL_MULT, INFORCE_BONUS
//! Set USE_DYNAMIC_INFORCE=1 to generate policies dynamically instead of loading CSV

use actuarial_system::{
    Assumptions,
    projection::{
        ProjectionEngine, ProjectionConfig, CreditingApproach, HedgeParams,
        calculate_cost_of_funds, DEFAULT_FIXED_ANNUAL_RATE, DEFAULT_INDEXED_ANNUAL_RATE,
    },
};
use actuarial_system::policy::{load_default_inforce, AdjustmentParams, load_adjusted_inforce};
use rayon::prelude::*;
use serde::Serialize;
use std::env;
use std::time::Instant;

#[derive(Serialize)]
struct ProjectionResponse {
    cost_of_funds_pct: Option<f64>,
    ceding_commission: Option<CedingCommission>,
    inforce_params: Option<InforceParamsOutput>,
    policy_count: usize,
    projection_months: u32,
    summary: ProjectionSummary,
    cashflows: Vec<DetailedCashflowRow>,
    execution_time_ms: u64,
}

#[derive(Serialize, Clone, Default)]
struct DetailedCashflowRow {
    month: u32,
    bop_av: f64,
    bop_bb: f64,
    lives: f64,
    mortality: f64,
    lapse: f64,
    pwd: f64,
    rider_charges: f64,
    surrender_charges: f64,
    interest: f64,
    eop_av: f64,
    expenses: f64,
    agent_commission: f64,
    imo_override: f64,
    wholesaler_override: f64,
    bonus_comp: f64,
    chargebacks: f64,
    hedge_gains: f64,
    net_cashflow: f64,
}

#[derive(Serialize)]
struct InforceParamsOutput {
    fixed_pct: f64,
    male_mult: f64,
    female_mult: f64,
    qual_mult: f64,
    nonqual_mult: f64,
    bonus: f64,
}

#[derive(Serialize)]
struct CedingCommission {
    npv: f64,
    bbb_rate_pct: f64,
    spread_pct: f64,
    total_rate_pct: f64,
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

/// Calculate ceding commission as NPV of cashflows at BBB rate + spread
/// Formula: NPV((1+annual_rate)^(1/12)-1, cashflows) * (1+annual_rate)^(1/12)
/// The multiplication adjusts Excel's end-of-period NPV to beginning-of-period
fn calculate_ceding_commission(cashflows: &[f64], bbb_rate: f64, spread: f64) -> f64 {
    let annual_rate = bbb_rate + spread;
    let monthly_factor = (1.0 + annual_rate).powf(1.0 / 12.0);
    let monthly_rate = monthly_factor - 1.0;

    // Excel NPV: sum of cashflow[i] / (1 + rate)^(i+1)
    let mut npv = 0.0;
    for (i, cf) in cashflows.iter().enumerate() {
        npv += cf / (1.0 + monthly_rate).powi((i + 1) as i32);
    }

    // Multiply by monthly factor to adjust to beginning of period
    npv * monthly_factor
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

    // BBB rate and spread for ceding commission calculation (as decimals, e.g., 0.05 for 5%)
    let bbb_rate: Option<f64> = env::var("BBB_RATE")
        .ok()
        .and_then(|s| s.parse().ok());

    let spread: f64 = env::var("SPREAD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    // Inforce adjustment parameters
    let use_adjusted = env::var("USE_DYNAMIC_INFORCE").is_ok();

    let adjustment_params = AdjustmentParams {
        fixed_pct: env::var("INFORCE_FIXED_PCT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.25),
        male_mult: env::var("INFORCE_MALE_MULT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0),
        female_mult: env::var("INFORCE_FEMALE_MULT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0),
        qual_mult: env::var("INFORCE_QUAL_MULT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0),
        nonqual_mult: env::var("INFORCE_NONQUAL_MULT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0),
        bb_bonus: env::var("INFORCE_BB_BONUS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.30),
        target_premium: 100_000_000.0,
    };

    // Rollup rate override (default 10%)
    let rollup_rate: f64 = env::var("ROLLUP_RATE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.10);

    // Policy filters
    let min_glwb_start_year: Option<u32> = env::var("MIN_GLWB_START_YEAR")
        .ok()
        .and_then(|s| s.parse().ok());
    let min_issue_age: Option<u8> = env::var("MIN_ISSUE_AGE")
        .ok()
        .and_then(|s| s.parse().ok());
    let max_issue_age: Option<u8> = env::var("MAX_ISSUE_AGE")
        .ok()
        .and_then(|s| s.parse().ok());
    let filter_genders: Option<Vec<String>> = env::var("FILTER_GENDERS")
        .ok()
        .map(|s| s.split(',').map(|g| g.to_string()).collect());
    let filter_qual_statuses: Option<Vec<String>> = env::var("FILTER_QUAL_STATUSES")
        .ok()
        .map(|s| s.split(',').map(|q| q.to_string()).collect());
    let filter_crediting_strategies: Option<Vec<String>> = env::var("FILTER_CREDITING_STRATEGIES")
        .ok()
        .map(|s| s.split(',').map(|c| c.to_string()).collect());
    let filter_bb_buckets: Option<Vec<String>> = env::var("FILTER_BB_BUCKETS")
        .ok()
        .map(|s| s.split('|').map(|b| b.to_string()).collect());

    // Load policies (with optional adjustments)
    let mut policies = if use_adjusted {
        if !json_output {
            println!("Loading adjusted inforce (fixed_pct={:.0}%, bb_bonus={:.0}%, rollup={:.0}%)...",
                     adjustment_params.fixed_pct * 100.0,
                     adjustment_params.bb_bonus * 100.0,
                     rollup_rate * 100.0);
        }
        load_adjusted_inforce(&adjustment_params).expect("Failed to load adjusted policies")
    } else {
        if !json_output {
            println!("Loading policies from pricing_inforce.csv...");
        }
        load_default_inforce().expect("Failed to load policies")
    };

    // Apply policy filters
    let initial_count = policies.len();

    if let Some(min_year) = min_glwb_start_year {
        policies.retain(|p| p.glwb_start_year >= min_year);
    }

    if let Some(min_age) = min_issue_age {
        policies.retain(|p| p.issue_age >= min_age);
    }

    if let Some(max_age) = max_issue_age {
        policies.retain(|p| p.issue_age <= max_age);
    }

    if let Some(ref genders) = filter_genders {
        policies.retain(|p| {
            let gender_str = match p.gender {
                actuarial_system::policy::Gender::Male => "Male",
                actuarial_system::policy::Gender::Female => "Female",
            };
            genders.contains(&gender_str.to_string())
        });
    }

    if let Some(ref qual_statuses) = filter_qual_statuses {
        policies.retain(|p| {
            let qual_str = match p.qual_status {
                actuarial_system::policy::QualStatus::Q => "Q",
                actuarial_system::policy::QualStatus::N => "N",
            };
            qual_statuses.contains(&qual_str.to_string())
        });
    }

    if let Some(ref crediting) = filter_crediting_strategies {
        policies.retain(|p| {
            let cred_str = match p.crediting_strategy {
                actuarial_system::policy::CreditingStrategy::Fixed => "Fixed",
                actuarial_system::policy::CreditingStrategy::Indexed => "Indexed",
            };
            crediting.contains(&cred_str.to_string())
        });
    }

    if let Some(ref buckets) = filter_bb_buckets {
        policies.retain(|p| {
            let bucket_str = match p.benefit_base_bucket {
                actuarial_system::policy::BenefitBaseBucket::Under50k => "[0, 50000)",
                actuarial_system::policy::BenefitBaseBucket::From50kTo100k => "[50000, 100000)",
                actuarial_system::policy::BenefitBaseBucket::From100kTo200k => "[100000, 200000)",
                actuarial_system::policy::BenefitBaseBucket::From200kTo500k => "[200000, 500000)",
                actuarial_system::policy::BenefitBaseBucket::Over500k => "[500000, Inf)",
            };
            buckets.contains(&bucket_str.to_string())
        });
    }

    if !json_output && policies.len() < initial_count {
        println!("Filtered to {} policies (from {})", policies.len(), initial_count);
    }

    if !json_output {
        println!("Loaded {} policies in {:?}", policies.len(), start.elapsed());
    }

    let policy_count = policies.len();

    // Load assumptions and apply rollup rate override
    let mut assumptions = Assumptions::default_pricing();
    assumptions.product.glwb.rollup_rate = rollup_rate;
    assumptions.product.glwb.bonus_rate = adjustment_params.bb_bonus;

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
        reserve_config: None, // Reserves off for cost of funds calculation
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

    // Aggregate results with all detailed columns
    let num_months = config.projection_months as usize;
    let mut detailed_cashflows: Vec<DetailedCashflowRow> = (1..=num_months as u32)
        .map(|m| DetailedCashflowRow { month: m, ..Default::default() })
        .collect();
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
                let agg = &mut detailed_cashflows[idx];
                agg.bop_av += row.bop_av;
                agg.bop_bb += row.bop_benefit_base;
                agg.lives += row.lives;
                agg.mortality += row.mortality_dec;
                agg.lapse += row.lapse_dec;
                agg.pwd += row.pwd_dec;
                agg.rider_charges += row.rider_charges_dec;
                agg.surrender_charges += row.surrender_charges_dec;
                agg.interest += row.interest_credits_dec;
                agg.eop_av += row.eop_av;
                agg.expenses += row.expenses;
                agg.agent_commission += row.agent_commission;
                agg.imo_override += row.imo_override;
                agg.wholesaler_override += row.wholesaler_override;
                agg.bonus_comp += row.bonus_comp;
                agg.chargebacks += row.chargebacks;
                agg.hedge_gains += row.hedge_gains;
                agg.net_cashflow += row.total_net_cashflow;
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

    // Extract just net cashflows for IRR calculation
    let aggregated_cashflows: Vec<f64> = detailed_cashflows.iter().map(|r| r.net_cashflow).collect();
    let total_net_cashflows: f64 = aggregated_cashflows.iter().sum();
    let month_1_cashflow = aggregated_cashflows.first().copied().unwrap_or(0.0);

    // Calculate Cost of Funds (IRR)
    let cost_of_funds = calculate_cost_of_funds(&aggregated_cashflows);
    let cost_of_funds_pct = cost_of_funds.map(|r| r * 100.0);

    // Calculate ceding commission if BBB rate is provided
    let ceding_commission = bbb_rate.map(|bbb| {
        let npv = calculate_ceding_commission(&aggregated_cashflows, bbb, spread);
        CedingCommission {
            npv,
            bbb_rate_pct: bbb * 100.0,
            spread_pct: spread * 100.0,
            total_rate_pct: (bbb + spread) * 100.0,
        }
    });

    let execution_time_ms = start.elapsed().as_millis() as u64;

    if json_output {
        // Output JSON for API consumption
        let inforce_params_output = if use_adjusted {
            Some(InforceParamsOutput {
                fixed_pct: adjustment_params.fixed_pct,
                male_mult: adjustment_params.male_mult,
                female_mult: adjustment_params.female_mult,
                qual_mult: adjustment_params.qual_mult,
                nonqual_mult: adjustment_params.nonqual_mult,
                bonus: adjustment_params.bb_bonus,
            })
        } else {
            None
        };

        let response = ProjectionResponse {
            cost_of_funds_pct,
            ceding_commission,
            inforce_params: inforce_params_output,
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
            cashflows: detailed_cashflows.clone(),
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

        if let Some(cc) = &ceding_commission {
            println!("\n========================================");
            println!("  CEDING COMMISSION");
            println!("  BBB Rate:    {:.2}%", cc.bbb_rate_pct);
            println!("  Spread:      {:.2}%", cc.spread_pct);
            println!("  Total Rate:  {:.2}%", cc.total_rate_pct);
            println!("  NPV:         ${:.2}", cc.npv);
            println!("========================================");
        }

        println!("\nTotal time: {:?}", start.elapsed());
    }
}
