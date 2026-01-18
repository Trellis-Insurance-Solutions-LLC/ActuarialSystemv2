//! AWS Lambda handler for running block projections
//!
//! This Lambda function accepts projection configuration via JSON and returns the Cost of Funds (IRR)
//! along with detailed cashflows and ceding commission calculation.
//!
//! Supports Lambda Function URLs for direct HTTP access.

use actuarial_system::{
    Assumptions,
    projection::{
        ProjectionEngine, ProjectionConfig, CreditingApproach, HedgeParams,
        calculate_cost_of_funds, DEFAULT_FIXED_ANNUAL_RATE, DEFAULT_INDEXED_ANNUAL_RATE,
    },
    policy::{load_default_inforce, AdjustmentParams, load_adjusted_inforce, Gender, QualStatus, CreditingStrategy, BenefitBaseBucket},
};
use lambda_http::{run, service_fn, Body, Error, Request, Response};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

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

    /// Option budget for hedge calculations (default: 3.15%)
    #[serde(default = "default_option_budget")]
    pub option_budget: f64,

    /// Equity kicker / appreciation rate for hedge calculations (default: 20%)
    #[serde(default = "default_equity_kicker")]
    pub equity_kicker: f64,

    /// Treasury rate change for lapse sensitivity (default: 0)
    #[serde(default)]
    pub treasury_change: f64,

    /// BBB rate for ceding commission (as decimal, e.g., 0.05 for 5%)
    #[serde(default)]
    pub bbb_rate: Option<f64>,

    /// Spread for ceding commission (as decimal)
    #[serde(default)]
    pub spread: Option<f64>,

    /// Whether to use dynamic inforce generation
    #[serde(default)]
    pub use_dynamic_inforce: bool,

    /// Fixed allocation percentage (0-1)
    #[serde(default = "default_fixed_pct")]
    pub inforce_fixed_pct: f64,

    /// Male mortality multiplier
    #[serde(default = "default_one")]
    pub inforce_male_mult: f64,

    /// Female mortality multiplier
    #[serde(default = "default_one")]
    pub inforce_female_mult: f64,

    /// Qualified status multiplier
    #[serde(default = "default_one")]
    pub inforce_qual_mult: f64,

    /// Non-qualified status multiplier
    #[serde(default = "default_one")]
    pub inforce_nonqual_mult: f64,

    /// Benefit base bonus (0-1, e.g., 0.30 for 30%)
    #[serde(default = "default_bb_bonus")]
    pub inforce_bb_bonus: f64,

    /// Annual rollup rate (default: 10%)
    #[serde(default = "default_rollup_rate")]
    pub rollup_rate: f64,

    // Policy filters
    #[serde(default)]
    pub min_glwb_start_year: Option<u32>,

    #[serde(default)]
    pub min_issue_age: Option<u8>,

    #[serde(default)]
    pub max_issue_age: Option<u8>,

    /// Filter by gender (e.g., ["Male", "Female"])
    #[serde(default)]
    pub genders: Option<Vec<String>>,

    /// Filter by qualified status (e.g., ["Q", "N"])
    #[serde(default)]
    pub qual_statuses: Option<Vec<String>>,

    /// Filter by crediting strategy (e.g., ["Fixed", "Indexed"])
    #[serde(default)]
    pub crediting_strategies: Option<Vec<String>>,

    /// Filter by benefit base bucket
    #[serde(default)]
    pub bb_buckets: Option<Vec<String>>,
}

fn default_projection_months() -> u32 { 768 }
fn default_fixed_rate() -> f64 { DEFAULT_FIXED_ANNUAL_RATE }
fn default_indexed_rate() -> f64 { DEFAULT_INDEXED_ANNUAL_RATE }
fn default_option_budget() -> f64 { 0.0315 }  // 3.15%
fn default_equity_kicker() -> f64 { 0.20 }    // 20%
fn default_fixed_pct() -> f64 { 0.25 }
fn default_one() -> f64 { 1.0 }
fn default_bb_bonus() -> f64 { 0.30 }
fn default_rollup_rate() -> f64 { 0.10 }

/// Output from the projection
#[derive(Debug, Serialize)]
pub struct ProjectionResponse {
    pub cost_of_funds_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ceding_commission: Option<CedingCommission>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inforce_params: Option<InforceParamsOutput>,
    pub policy_count: usize,
    pub projection_months: u32,
    pub summary: ProjectionSummary,
    pub cashflows: Vec<DetailedCashflowRow>,
    pub execution_time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CedingCommission {
    pub npv: f64,
    pub bbb_rate_pct: f64,
    pub spread_pct: f64,
    pub total_rate_pct: f64,
}

#[derive(Debug, Serialize)]
pub struct InforceParamsOutput {
    pub fixed_pct: f64,
    pub male_mult: f64,
    pub female_mult: f64,
    pub qual_mult: f64,
    pub nonqual_mult: f64,
    pub bonus: f64,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct DetailedCashflowRow {
    pub month: u32,
    pub bop_av: f64,
    pub bop_bb: f64,
    pub lives: f64,
    pub mortality: f64,
    pub lapse: f64,
    pub pwd: f64,
    pub rider_charges: f64,
    pub surrender_charges: f64,
    pub interest: f64,
    pub eop_av: f64,
    pub expenses: f64,
    pub agent_commission: f64,
    pub imo_override: f64,
    pub wholesaler_override: f64,
    pub bonus_comp: f64,
    pub chargebacks: f64,
    pub hedge_gains: f64,
    pub net_cashflow: f64,
}

#[derive(Debug, Serialize)]
pub struct ProjectionSummary {
    pub total_premium: f64,
    pub total_initial_av: f64,
    pub total_initial_bb: f64,
    pub total_initial_lives: f64,
    pub total_net_cashflows: f64,
    pub month_1_cashflow: f64,
    pub final_lives: f64,
    pub final_av: f64,
}

/// Calculate ceding commission as NPV of cashflows at BBB rate + spread
fn calculate_ceding_commission(cashflows: &[f64], bbb_rate: f64, spread: f64) -> f64 {
    let annual_rate = bbb_rate + spread;
    let monthly_factor = (1.0 + annual_rate).powf(1.0 / 12.0);
    let monthly_rate = monthly_factor - 1.0;

    let mut npv = 0.0;
    for (i, cf) in cashflows.iter().enumerate() {
        npv += cf / (1.0 + monthly_rate).powi((i + 1) as i32);
    }

    npv * monthly_factor
}

fn error_response(status: u16, message: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Text(format!(r#"{{"error":"{}"}}"#, message)))
        .unwrap()
}

fn json_response(body: &ProjectionResponse) -> Response<Body> {
    Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "POST, OPTIONS")
        .header("Access-Control-Allow-Headers", "Content-Type")
        .body(Body::Text(serde_json::to_string(body).unwrap()))
        .unwrap()
}

/// Lambda handler function
async fn handler(event: Request) -> Result<Response<Body>, Error> {
    let start = std::time::Instant::now();

    // Handle CORS preflight
    if event.method().as_str() == "OPTIONS" {
        return Ok(Response::builder()
            .status(200)
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", "POST, OPTIONS")
            .header("Access-Control-Allow-Headers", "Content-Type")
            .body(Body::Empty)
            .unwrap());
    }

    // Parse request body
    let body = event.body();
    let body_str = match body {
        Body::Text(s) => s.clone(),
        Body::Binary(b) => String::from_utf8_lossy(b).to_string(),
        Body::Empty => "{}".to_string(),
    };

    let request: ProjectionRequest = match serde_json::from_str(&body_str) {
        Ok(r) => r,
        Err(e) => {
            return Ok(error_response(400, &format!("Invalid JSON: {}", e)));
        }
    };

    // Set up adjustment params for dynamic inforce
    let adjustment_params = AdjustmentParams {
        fixed_pct: request.inforce_fixed_pct,
        male_mult: request.inforce_male_mult,
        female_mult: request.inforce_female_mult,
        qual_mult: request.inforce_qual_mult,
        nonqual_mult: request.inforce_nonqual_mult,
        bb_bonus: request.inforce_bb_bonus,
        target_premium: 100_000_000.0,
    };

    // Load policies
    let mut policies = if request.use_dynamic_inforce {
        match load_adjusted_inforce(&adjustment_params) {
            Ok(p) => p,
            Err(e) => {
                return Ok(error_response(500, &format!("Failed to generate dynamic inforce: {}", e)));
            }
        }
    } else {
        match load_default_inforce() {
            Ok(p) => p,
            Err(e) => {
                return Ok(error_response(500, &format!("Failed to load default inforce: {}", e)));
            }
        }
    };

    // Apply policy filters
    if let Some(min_year) = request.min_glwb_start_year {
        policies.retain(|p| p.glwb_start_year >= min_year);
    }

    if let Some(min_age) = request.min_issue_age {
        policies.retain(|p| p.issue_age >= min_age);
    }

    if let Some(max_age) = request.max_issue_age {
        policies.retain(|p| p.issue_age <= max_age);
    }

    if let Some(ref genders) = request.genders {
        if genders.len() < 2 {
            policies.retain(|p| {
                let gender_str = match p.gender {
                    Gender::Male => "Male",
                    Gender::Female => "Female",
                };
                genders.contains(&gender_str.to_string())
            });
        }
    }

    if let Some(ref qual_statuses) = request.qual_statuses {
        if qual_statuses.len() < 2 {
            policies.retain(|p| {
                let qual_str = match p.qual_status {
                    QualStatus::Q => "Q",
                    QualStatus::N => "N",
                };
                qual_statuses.contains(&qual_str.to_string())
            });
        }
    }

    if let Some(ref crediting) = request.crediting_strategies {
        if crediting.len() < 2 {
            policies.retain(|p| {
                let cred_str = match p.crediting_strategy {
                    CreditingStrategy::Fixed => "Fixed",
                    CreditingStrategy::Indexed => "Indexed",
                };
                crediting.contains(&cred_str.to_string())
            });
        }
    }

    if let Some(ref buckets) = request.bb_buckets {
        if buckets.len() < 5 {
            policies.retain(|p| {
                let bucket_str = match p.benefit_base_bucket {
                    BenefitBaseBucket::Under50k => "[0, 50000)",
                    BenefitBaseBucket::From50kTo100k => "[50000, 100000)",
                    BenefitBaseBucket::From100kTo200k => "[100000, 200000)",
                    BenefitBaseBucket::From200kTo500k => "[200000, 500000)",
                    BenefitBaseBucket::Over500k => "[500000, Inf)",
                };
                buckets.contains(&bucket_str.to_string())
            });
        }
    }

    let policy_count = policies.len();

    // Load assumptions and apply rollup rate override
    let mut assumptions = Assumptions::default_pricing();
    assumptions.product.glwb.rollup_rate = request.rollup_rate;

    // Projection config with dynamic hedge params
    let config = ProjectionConfig {
        projection_months: request.projection_months,
        crediting: CreditingApproach::PolicyBased {
            fixed_annual_rate: request.fixed_annual_rate,
            indexed_annual_rate: request.indexed_annual_rate,
        },
        detailed_output: false,
        treasury_change: request.treasury_change,
        fixed_lapse_rate: None,
        hedge_params: Some(HedgeParams {
            option_budget: request.option_budget,
            appreciation_rate: request.equity_kicker,
            financing_fee: 0.05,  // Hardcoded at 5%
        }),
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

    // Extract net cashflows for IRR calculation
    let aggregated_cashflows: Vec<f64> = detailed_cashflows.iter().map(|r| r.net_cashflow).collect();
    let total_net_cashflows: f64 = aggregated_cashflows.iter().sum();
    let month_1_cashflow = aggregated_cashflows.first().copied().unwrap_or(0.0);

    // Calculate Cost of Funds (IRR)
    let cost_of_funds = calculate_cost_of_funds(&aggregated_cashflows);
    let cost_of_funds_pct = cost_of_funds.map(|r| r * 100.0);

    // Calculate ceding commission if BBB rate is provided
    let ceding_commission = request.bbb_rate.map(|bbb| {
        let spread = request.spread.unwrap_or(0.0);
        let npv = calculate_ceding_commission(&aggregated_cashflows, bbb, spread);
        CedingCommission {
            npv,
            bbb_rate_pct: bbb * 100.0,
            spread_pct: spread * 100.0,
            total_rate_pct: (bbb + spread) * 100.0,
        }
    });

    // Build inforce params output
    let inforce_params = if request.use_dynamic_inforce {
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

    let execution_time_ms = start.elapsed().as_millis() as u64;

    let response = ProjectionResponse {
        cost_of_funds_pct,
        ceding_commission,
        inforce_params,
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
        cashflows: detailed_cashflows,
        execution_time_ms,
        error: None,
    };

    Ok(json_response(&response))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    run(service_fn(handler)).await
}
