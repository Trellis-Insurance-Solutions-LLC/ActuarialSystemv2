import { NextRequest, NextResponse } from "next/server";

// Force dynamic execution - no caching
export const dynamic = "force-dynamic";
export const fetchCache = "force-no-store";

interface ProjectionRequest {
  projection_months?: number;
  fixed_annual_rate?: number;
  indexed_annual_rate?: number;
  option_budget?: number;    // Option budget for hedge params (as decimal, e.g., 0.0315 for 3.15%)
  equity_kicker?: number;    // Equity kicker / appreciation rate (as decimal, e.g., 0.20 for 20%)
  treasury_change?: number;
  bbb_rate?: number;  // BBB rate for ceding commission (as decimal, e.g., 0.05 for 5%)
  spread?: number;    // Spread for ceding commission (as decimal)
  // Dynamic inforce parameters
  use_dynamic_inforce?: boolean;
  inforce_fixed_pct?: number;
  inforce_male_mult?: number;
  inforce_female_mult?: number;
  inforce_qual_mult?: number;
  inforce_nonqual_mult?: number;
  inforce_bb_bonus?: number;
  rollup_rate?: number;
  // Policy filters
  min_glwb_start_year?: number;
  min_issue_age?: number;
  max_issue_age?: number;
  genders?: string[];
  qual_statuses?: string[];
  crediting_strategies?: string[];
  bb_buckets?: string[];
}

interface ProjectionSummary {
  total_premium: number;
  total_initial_av: number;
  total_initial_bb: number;
  total_initial_lives: number;
  total_net_cashflows: number;
  month_1_cashflow: number;
  final_lives: number;
  final_av: number;
}

interface CedingCommission {
  npv: number;
  bbb_rate_pct: number;
  spread_pct: number;
  total_rate_pct: number;
}

interface InforceParamsOutput {
  fixed_pct: number;
  male_mult: number;
  female_mult: number;
  qual_mult: number;
  nonqual_mult: number;
  bonus: number;
}

interface DetailedCashflowRow {
  month: number;
  bop_av: number;
  bop_bb: number;
  lives: number;
  mortality: number;
  lapse: number;
  pwd: number;
  rider_charges: number;
  surrender_charges: number;
  interest: number;
  eop_av: number;
  expenses: number;
  agent_commission: number;
  imo_override: number;
  wholesaler_override: number;
  bonus_comp: number;
  chargebacks: number;
  hedge_gains: number;
  net_cashflow: number;
}

interface ProjectionResponse {
  cost_of_funds_pct: number | null;
  ceding_commission?: CedingCommission | null;
  inforce_params?: InforceParamsOutput | null;
  policy_count: number;
  projection_months: number;
  summary: ProjectionSummary;
  cashflows: DetailedCashflowRow[];
  execution_time_ms: number;
  error?: string;
}

// Default values matching the Rust code
const DEFAULT_PROJECTION_MONTHS = 768;
const DEFAULT_FIXED_ANNUAL_RATE = 0.0275;
const DEFAULT_INDEXED_ANNUAL_RATE = 0.0378;

export async function POST(request: NextRequest): Promise<NextResponse> {
  const start = Date.now();

  try {
    const body: ProjectionRequest = await request.json();

    const projectionMonths = body.projection_months ?? DEFAULT_PROJECTION_MONTHS;
    const fixedAnnualRate = body.fixed_annual_rate ?? DEFAULT_FIXED_ANNUAL_RATE;
    const indexedAnnualRate = body.indexed_annual_rate ?? DEFAULT_INDEXED_ANNUAL_RATE;
    const optionBudget = body.option_budget ?? 0.0315;  // Default 3.15%
    const equityKicker = body.equity_kicker ?? 0.20;    // Default 20%
    const treasuryChange = body.treasury_change ?? 0;
    const bbbRate = body.bbb_rate;  // Optional - only calculate ceding commission if provided
    const spread = body.spread ?? 0;

    // Dynamic inforce parameters
    const useDynamicInforce = body.use_dynamic_inforce ?? false;
    const inforceFixedPct = body.inforce_fixed_pct ?? 0.25;
    const inforceMaleMult = body.inforce_male_mult ?? 1.0;
    const inforceFemaleMult = body.inforce_female_mult ?? 1.0;
    const inforceQualMult = body.inforce_qual_mult ?? 1.0;
    const inforceNonqualMult = body.inforce_nonqual_mult ?? 1.0;
    const inforceBBBonus = body.inforce_bb_bonus ?? 0.30;
    const rollupRate = body.rollup_rate ?? 0.10;

    // Policy filters
    const minGlwbStartYear = body.min_glwb_start_year;
    const minIssueAge = body.min_issue_age;
    const maxIssueAge = body.max_issue_age;
    const genders = body.genders;
    const qualStatuses = body.qual_statuses;
    const creditingStrategies = body.crediting_strategies;
    const bbBuckets = body.bb_buckets;

    // Check if we should use Lambda
    const useLambda = process.env.USE_LAMBDA === "true";
    const lambdaUrl = process.env.LAMBDA_FUNCTION_URL;

    console.log("ENV CHECK - USE_LAMBDA:", process.env.USE_LAMBDA, "useLambda:", useLambda, "LAMBDA_URL exists:", !!lambdaUrl);

    // Always try Lambda first if configured
    if (lambdaUrl) {
      // Call Lambda function with all parameters
      const lambdaPayload: Record<string, unknown> = {
        projection_months: projectionMonths,
        fixed_annual_rate: fixedAnnualRate,
        indexed_annual_rate: indexedAnnualRate,
        option_budget: optionBudget,
        equity_kicker: equityKicker,
        treasury_change: treasuryChange,
        use_dynamic_inforce: useDynamicInforce,
        inforce_fixed_pct: inforceFixedPct,
        inforce_male_mult: inforceMaleMult,
        inforce_female_mult: inforceFemaleMult,
        inforce_qual_mult: inforceQualMult,
        inforce_nonqual_mult: inforceNonqualMult,
        inforce_bb_bonus: inforceBBBonus,
        rollup_rate: rollupRate,
      };

      // Add optional parameters
      if (bbbRate !== undefined) {
        lambdaPayload.bbb_rate = bbbRate;
      }
      if (spread !== undefined) {
        lambdaPayload.spread = spread;
      }
      if (minGlwbStartYear !== undefined) {
        lambdaPayload.min_glwb_start_year = minGlwbStartYear;
      }
      if (minIssueAge !== undefined) {
        lambdaPayload.min_issue_age = minIssueAge;
      }
      if (maxIssueAge !== undefined) {
        lambdaPayload.max_issue_age = maxIssueAge;
      }
      if (genders !== undefined) {
        lambdaPayload.genders = genders;
      }
      if (qualStatuses !== undefined) {
        lambdaPayload.qual_statuses = qualStatuses;
      }
      if (creditingStrategies !== undefined) {
        lambdaPayload.crediting_strategies = creditingStrategies;
      }
      if (bbBuckets !== undefined) {
        lambdaPayload.bb_buckets = bbBuckets;
      }

      console.log("Calling Lambda at:", lambdaUrl);
      console.log("Payload:", JSON.stringify(lambdaPayload));

      const lambdaResponse = await fetch(lambdaUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(lambdaPayload),
        cache: "no-store",
      });

      if (!lambdaResponse.ok) {
        throw new Error(`Lambda error: ${lambdaResponse.status}`);
      }

      const data = await lambdaResponse.json();
      console.log("Lambda response COF:", data.cost_of_funds_pct);

      const response = NextResponse.json({
        ...data,
        execution_time_ms: Date.now() - start,
      });

      // Prevent caching
      response.headers.set("Cache-Control", "no-store, no-cache, must-revalidate, proxy-revalidate");
      response.headers.set("Pragma", "no-cache");
      response.headers.set("Expires", "0");

      return response;
    }

    // No Lambda URL configured - return error
    throw new Error("LAMBDA_FUNCTION_URL not configured");
  } catch (error) {
    console.error("Projection error:", error);
    return NextResponse.json(
      {
        cost_of_funds_pct: null,
        policy_count: 0,
        projection_months: DEFAULT_PROJECTION_MONTHS,
        summary: {
          total_premium: 0,
          total_initial_av: 0,
          total_initial_bb: 0,
          total_initial_lives: 0,
          total_net_cashflows: 0,
          month_1_cashflow: 0,
          final_lives: 0,
          final_av: 0,
        },
        execution_time_ms: Date.now() - start,
        error: error instanceof Error ? error.message : "Unknown error",
      } as ProjectionResponse,
      { status: 500 }
    );
  }
}
