"use client";

import { useState } from "react";
import FredChart from "@/components/FredChart";
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  ReferenceLine,
} from "recharts";

// Custom number input with +/- buttons for mobile-friendly interaction
interface NumberInputProps {
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  decimals?: number;
}

function NumberInput({ value, onChange, min, max, step = 1, decimals = 2 }: NumberInputProps) {
  const handleDecrement = () => {
    const newValue = value - step;
    if (min === undefined || newValue >= min) {
      onChange(Number(newValue.toFixed(decimals)));
    }
  };

  const handleIncrement = () => {
    const newValue = value + step;
    if (max === undefined || newValue <= max) {
      onChange(Number(newValue.toFixed(decimals)));
    }
  };

  return (
    <div className="number-input-group">
      <button
        type="button"
        onClick={handleDecrement}
        className="number-input-btn number-input-btn-minus"
        aria-label="Decrease"
      >
        −
      </button>
      <input
        type="number"
        value={value}
        onChange={(e) => {
          const parsed = parseFloat(e.target.value);
          if (!isNaN(parsed)) {
            let newValue = parsed;
            if (min !== undefined) newValue = Math.max(min, newValue);
            if (max !== undefined) newValue = Math.min(max, newValue);
            onChange(newValue);
          }
        }}
        min={min}
        max={max}
        step={step}
      />
      <button
        type="button"
        onClick={handleIncrement}
        className="number-input-btn number-input-btn-plus"
        aria-label="Increase"
      >
        +
      </button>
    </div>
  );
}

// Types for the projection
interface ProjectionConfig {
  fixedAnnualRate: number;
  optionBudget: number;
  equityKicker: number;
  bbbRate: number;
  spread: number;
  // Dynamic inforce parameters
  useDynamicInforce: boolean;
  inforceFixedPct: number;
  inforceBBBonus: number;
  rollupRate: number;
}

interface CedingCommission {
  npv: number;
  bbbRatePct: number;
  spreadPct: number;
  totalRatePct: number;
}

interface InforceParamsOutput {
  fixedPct: number;
  bonus: number;
}

interface DetailedCashflowRow {
  month: number;
  bopAv: number;
  bopBb: number;
  lives: number;
  mortality: number;
  lapse: number;
  pwd: number;
  riderCharges: number;
  surrenderCharges: number;
  interest: number;
  eopAv: number;
  expenses: number;
  agentCommission: number;
  imoOverride: number;
  wholesalerOverride: number;
  bonusComp: number;
  chargebacks: number;
  hedgeGains: number;
  netCashflow: number;
}

interface ProjectionResult {
  costOfFundsPct: number | null;
  cedingCommission: CedingCommission | null;
  inforceParams: InforceParamsOutput | null;
  policyCount: number;
  projectionMonths: number;
  summary: {
    totalPremium: number;
    totalInitialAv: number;
    totalInitialBb: number;
    totalInitialLives: number;
    totalNetCashflows: number;
    month1Cashflow: number;
    finalLives: number;
    finalAv: number;
  };
  cashflows: DetailedCashflowRow[];
  executionTimeMs: number;
  error?: string;
}

// Navigation items
const navItems = [
  { name: "Dashboard" },
  { name: "Scenarios" },
  { name: "Results" },
  { name: "Explorer" },
];

// Format currency
const formatCurrency = (value: number): string => {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 0,
    maximumFractionDigits: 0,
  }).format(value);
};

// Format percentage
const formatPercent = (value: number): string => {
  return `${value.toFixed(2)}%`;
};

export default function Home() {
  const [activeTab, setActiveTab] = useState("Dashboard");
  const [config, setConfig] = useState<ProjectionConfig>({
    fixedAnnualRate: 2.75,
    optionBudget: 3.15,       // Option budget percentage
    equityKicker: 20,         // Gross equity kicker percentage (indexed_rate = option_budget * (1 + equity_kicker))
    bbbRate: 5.01,  // Current BBB rate
    spread: 0.6,    // Default spread of 0.6%
    // Dynamic inforce parameters
    useDynamicInforce: true,  // Use dynamic generation by default
    inforceFixedPct: 25,      // 25% fixed, 75% indexed
    inforceBBBonus: 30,       // 30% BB bonus (BB = Premium × 1.3)
    rollupRate: 10,           // 10% annual rollup
  });

  const [result, setResult] = useState<ProjectionResult | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Policy Explorer state
  const [explorerMinGlwbYear, setExplorerMinGlwbYear] = useState(1);
  const [explorerMinIssueAge, setExplorerMinIssueAge] = useState(55);
  const [explorerMaxIssueAge, setExplorerMaxIssueAge] = useState(80);
  const [explorerGenders, setExplorerGenders] = useState<string[]>(["Male", "Female"]);
  const [explorerQualStatuses, setExplorerQualStatuses] = useState<string[]>(["Q", "N"]);
  const [explorerCreditingStrategies, setExplorerCreditingStrategies] = useState<string[]>(["Fixed", "Indexed"]);
  const [explorerBBBuckets, setExplorerBBBuckets] = useState<string[]>([
    "[0, 50000)",
    "[50000, 100000)",
    "[100000, 200000)",
    "[200000, 500000)",
    "[500000, Inf)",
  ]);
  const [explorerResult, setExplorerResult] = useState<ProjectionResult | null>(null);
  const [explorerLoading, setExplorerLoading] = useState(false);
  const [explorerError, setExplorerError] = useState<string | null>(null);

  // Toggle helper for multi-select filters
  const toggleFilter = (
    current: string[],
    value: string,
    setter: (v: string[]) => void
  ) => {
    if (current.includes(value)) {
      // Don't allow deselecting all
      if (current.length > 1) {
        setter(current.filter((v) => v !== value));
      }
    } else {
      setter([...current, value]);
    }
  };

  const runProjection = async () => {
    setIsLoading(true);
    setError(null);

    try {
      // Calculate indexed_annual_rate from option_budget and equity_kicker
      const indexedAnnualRate = (config.optionBudget / 100) * (1 + config.equityKicker / 100);

      const response = await fetch("/api/projection", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          projection_months: 768,
          fixed_annual_rate: config.fixedAnnualRate / 100,
          indexed_annual_rate: indexedAnnualRate,
          option_budget: config.optionBudget / 100,
          equity_kicker: config.equityKicker / 100,
          bbb_rate: config.bbbRate / 100,
          spread: config.spread / 100,
          // Dynamic inforce parameters
          use_dynamic_inforce: config.useDynamicInforce,
          inforce_fixed_pct: config.inforceFixedPct / 100,
          inforce_bb_bonus: config.inforceBBBonus / 100,
          rollup_rate: config.rollupRate / 100,
        }),
      });

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      const data = await response.json();
      setResult({
        costOfFundsPct: data.cost_of_funds_pct,
        cedingCommission: data.ceding_commission ? {
          npv: data.ceding_commission.npv,
          bbbRatePct: data.ceding_commission.bbb_rate_pct,
          spreadPct: data.ceding_commission.spread_pct,
          totalRatePct: data.ceding_commission.total_rate_pct,
        } : null,
        inforceParams: data.inforce_params ? {
          fixedPct: data.inforce_params.fixed_pct,
          bonus: data.inforce_params.bonus,
        } : null,
        policyCount: data.policy_count,
        projectionMonths: data.projection_months,
        summary: {
          totalPremium: data.summary.total_premium,
          totalInitialAv: data.summary.total_initial_av,
          totalInitialBb: data.summary.total_initial_bb,
          totalInitialLives: data.summary.total_initial_lives,
          totalNetCashflows: data.summary.total_net_cashflows,
          month1Cashflow: data.summary.month_1_cashflow,
          finalLives: data.summary.final_lives,
          finalAv: data.summary.final_av,
        },
        cashflows: (data.cashflows || []).map((cf: Record<string, number>) => ({
          month: cf.month,
          bopAv: cf.bop_av,
          bopBb: cf.bop_bb,
          lives: cf.lives,
          mortality: cf.mortality,
          lapse: cf.lapse,
          pwd: cf.pwd,
          riderCharges: cf.rider_charges,
          surrenderCharges: cf.surrender_charges,
          interest: cf.interest,
          eopAv: cf.eop_av,
          expenses: cf.expenses,
          agentCommission: cf.agent_commission,
          imoOverride: cf.imo_override,
          wholesalerOverride: cf.wholesaler_override,
          bonusComp: cf.bonus_comp,
          chargebacks: cf.chargebacks,
          hedgeGains: cf.hedge_gains,
          netCashflow: cf.net_cashflow,
        })),
        executionTimeMs: data.execution_time_ms,
        error: data.error,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : "An error occurred");
    } finally {
      setIsLoading(false);
    }
  };

  const runExplorerProjection = async () => {
    setExplorerLoading(true);
    setExplorerError(null);

    try {
      // Calculate indexed_annual_rate from option_budget and equity_kicker
      const indexedAnnualRate = (config.optionBudget / 100) * (1 + config.equityKicker / 100);

      const response = await fetch("/api/projection", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          projection_months: 768,
          fixed_annual_rate: config.fixedAnnualRate / 100,
          indexed_annual_rate: indexedAnnualRate,
          option_budget: config.optionBudget / 100,
          equity_kicker: config.equityKicker / 100,
          bbb_rate: config.bbbRate / 100,
          spread: config.spread / 100,
          use_dynamic_inforce: config.useDynamicInforce,
          inforce_fixed_pct: config.inforceFixedPct / 100,
          inforce_bb_bonus: config.inforceBBBonus / 100,
          rollup_rate: config.rollupRate / 100,
          // Policy filters
          min_glwb_start_year: explorerMinGlwbYear,
          min_issue_age: explorerMinIssueAge,
          max_issue_age: explorerMaxIssueAge,
          genders: explorerGenders,
          qual_statuses: explorerQualStatuses,
          crediting_strategies: explorerCreditingStrategies,
          bb_buckets: explorerBBBuckets,
        }),
      });

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      const data = await response.json();
      setExplorerResult({
        costOfFundsPct: data.cost_of_funds_pct,
        cedingCommission: data.ceding_commission ? {
          npv: data.ceding_commission.npv,
          bbbRatePct: data.ceding_commission.bbb_rate_pct,
          spreadPct: data.ceding_commission.spread_pct,
          totalRatePct: data.ceding_commission.total_rate_pct,
        } : null,
        inforceParams: data.inforce_params ? {
          fixedPct: data.inforce_params.fixed_pct,
          bonus: data.inforce_params.bonus,
        } : null,
        policyCount: data.policy_count,
        projectionMonths: data.projection_months,
        summary: {
          totalPremium: data.summary.total_premium,
          totalInitialAv: data.summary.total_initial_av,
          totalInitialBb: data.summary.total_initial_bb,
          totalInitialLives: data.summary.total_initial_lives,
          totalNetCashflows: data.summary.total_net_cashflows,
          month1Cashflow: data.summary.month_1_cashflow,
          finalLives: data.summary.final_lives,
          finalAv: data.summary.final_av,
        },
        cashflows: (data.cashflows || []).map((cf: Record<string, number>) => ({
          month: cf.month,
          bopAv: cf.bop_av,
          bopBb: cf.bop_bb,
          lives: cf.lives,
          mortality: cf.mortality,
          lapse: cf.lapse,
          pwd: cf.pwd,
          riderCharges: cf.rider_charges,
          surrenderCharges: cf.surrender_charges,
          interest: cf.interest,
          eopAv: cf.eop_av,
          expenses: cf.expenses,
          agentCommission: cf.agent_commission,
          imoOverride: cf.imo_override,
          wholesalerOverride: cf.wholesaler_override,
          bonusComp: cf.bonus_comp,
          chargebacks: cf.chargebacks,
          hedgeGains: cf.hedge_gains,
          netCashflow: cf.net_cashflow,
        })),
        executionTimeMs: data.execution_time_ms,
        error: data.error,
      });
    } catch (err) {
      setExplorerError(err instanceof Error ? err.message : "An error occurred");
    } finally {
      setExplorerLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-trellis pb-24">
      {/* Main Content */}
      <main className="min-h-screen">
        {/* Header */}
        <header className="glass-dark border-b border-[--border-color] p-4 sm:p-6">
          <div className="flex justify-between items-center max-w-6xl mx-auto">
            <div>
              <p className="text-[--text-muted] text-xs sm:text-sm drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">Trellis Cost of Funds Calculator</p>
              <h2 className="text-xl sm:text-2xl font-bold drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">{activeTab}</h2>
            </div>
          </div>
        </header>

        {/* Sticky Run Projection Button */}
        {activeTab === "Dashboard" && (
          <button
            onClick={runProjection}
            disabled={isLoading}
            className="fixed top-4 right-4 sm:top-6 sm:right-6 z-50 flex items-center gap-2 text-white px-4 sm:px-6 py-2 sm:py-3 rounded-xl font-semibold hover:scale-105 transition-all disabled:opacity-50 disabled:cursor-not-allowed border-2 border-white/30 shadow-[0_8px_32px_rgba(0,0,0,0.3),inset_0_1px_0_0_rgba(255,255,255,0.4)] hover:shadow-[0_8px_48px_rgba(0,0,0,0.4)] glass-button"
          >
            {isLoading ? "Running..." : "Run Projection"}
          </button>
        )}

        <div className="p-4 sm:p-6 space-y-6 max-w-6xl mx-auto">
          {/* Error Display */}
          {error && (
            <div className="bg-red-900/30 border border-red-500 rounded-lg p-4 text-red-200">
              <p className="font-semibold">Error</p>
              <p>{error}</p>
            </div>
          )}

          {/* Dashboard Tab */}
          {activeTab === "Dashboard" && (
            <>
              {/* Cost of Funds Card - Hero with wet glass accent */}
              {result && result.costOfFundsPct !== null && (
                <div className="relative overflow-hidden hero-gradient rounded-3xl p-8 shadow-2xl">
                  {/* Wet glass decorative elements */}
                  <div className="absolute top-0 right-0 w-32 h-32 bg-white/30 rounded-full -translate-y-1/2 translate-x-1/2 blur-2xl"></div>
                  <div className="absolute bottom-0 left-0 w-24 h-24 bg-white/20 rounded-full translate-y-1/2 -translate-x-1/2 blur-xl"></div>
                  <div className="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-white/60 to-transparent"></div>
                  <div className="relative">
                    <p className="text-lg font-medium text-white drop-shadow-[0_2px_8px_rgba(0,0,0,0.3)]">Cost of Funds (IRR)</p>
                    <p className="text-6xl font-bold mt-2 tracking-tight text-white drop-shadow-[0_2px_12px_rgba(0,0,0,0.4)]">
                      {formatPercent(result.costOfFundsPct)}
                    </p>
                    <p className="mt-4 text-sm text-white/90 drop-shadow-[0_1px_4px_rgba(0,0,0,0.3)]">
                      Calculated in {result.executionTimeMs}ms for{" "}
                      {result.policyCount.toLocaleString()} policies
                    </p>
                  </div>
                </div>
              )}

              {/* Summary Cards */}
              <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 sm:gap-4">
                <div className="glass-card rounded-3xl p-4 sm:p-6 transition-all">
                  <p className="text-[--text-muted] text-xs sm:text-sm drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">Total Premium</p>
                  <p className="text-lg sm:text-2xl font-bold tracking-tight text-[--text-primary] drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)] mt-1">
                    {result ? formatCurrency(result.summary.totalPremium) : "—"}
                  </p>
                  <p className="text-[--highlight] text-xs font-medium mt-2 drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                    {result ? result.policyCount.toLocaleString() : "—"} policies
                  </p>
                </div>

                <div className="glass-card rounded-3xl p-4 sm:p-6 transition-all">
                  <p className="text-[--text-muted] text-xs sm:text-sm drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">Initial AV</p>
                  <p className="text-lg sm:text-2xl font-bold tracking-tight text-[--text-primary] drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)] mt-1">
                    {result ? formatCurrency(result.summary.totalInitialAv) : "—"}
                  </p>
                </div>

                <div className="glass-card rounded-3xl p-4 sm:p-6 transition-all">
                  <p className="text-[--text-muted] text-xs sm:text-sm drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">Initial Lives</p>
                  <p className="text-lg sm:text-2xl font-bold tracking-tight text-[--text-primary] drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)] mt-1">
                    {result ? result.summary.totalInitialLives.toFixed(2) : "—"}
                  </p>
                </div>

                <div className="glass-card rounded-3xl p-4 sm:p-6 transition-all">
                  <p className="text-[--text-muted] text-xs sm:text-sm drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">Benefit Base</p>
                  <p className="text-lg sm:text-2xl font-bold tracking-tight text-[--text-primary] drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)] mt-1">
                    {result ? formatCurrency(result.summary.totalInitialBb) : "—"}
                  </p>
                </div>
              </div>

              {/* Inforce Configuration Panel */}
              <div className="glass-card rounded-3xl p-4 sm:p-6">
                <h3 className="text-base sm:text-lg font-semibold mb-4 drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                  Inforce Configuration
                </h3>
                <div className="space-y-4">
                  <div className="flex items-center justify-between">
                    <label className="text-sm text-[--text-muted]">
                      Use Dynamic Generation
                    </label>
                    <button
                      onClick={() =>
                        setConfig({ ...config, useDynamicInforce: !config.useDynamicInforce })
                      }
                      className={`w-12 h-6 rounded-full transition-all shadow-inner ${
                        config.useDynamicInforce ? "bg-[--accent]" : "bg-[--bg-primary]"
                      }`}
                    >
                      <div
                        className={`w-5 h-5 bg-[--text-primary] rounded-full shadow-md transition-all ${
                          config.useDynamicInforce ? "translate-x-6 shadow-lg" : "translate-x-0.5"
                        }`}
                      />
                    </button>
                  </div>

                  {config.useDynamicInforce && (
                    <div className="space-y-4">
                      <div>
                        <label className="block text-sm text-[--text-muted] mb-1">
                          Fixed Allocation (%)
                        </label>
                        <NumberInput
                          value={config.inforceFixedPct}
                          onChange={(v) => setConfig({ ...config, inforceFixedPct: v })}
                          min={0}
                          max={100}
                          step={5}
                          decimals={0}
                        />
                        <p className="text-xs text-[--text-muted] mt-1">
                          Indexed: {100 - config.inforceFixedPct}%
                        </p>
                      </div>
                      <div>
                        <label className="block text-sm text-[--text-muted] mb-1">
                          Benefit Base Bonus (%)
                        </label>
                        <NumberInput
                          value={config.inforceBBBonus}
                          onChange={(v) => setConfig({ ...config, inforceBBBonus: v })}
                          min={0}
                          max={100}
                          step={1}
                          decimals={0}
                        />
                        <p className="text-xs text-[--text-muted] mt-1">
                          BB = Premium × (1 + {config.inforceBBBonus}%) = ×{(1 + config.inforceBBBonus / 100).toFixed(2)}
                        </p>
                      </div>
                      <div>
                        <label className="block text-sm text-[--text-muted] mb-1">
                          Rollup Rate (%)
                        </label>
                        <NumberInput
                          value={config.rollupRate}
                          onChange={(v) => setConfig({ ...config, rollupRate: v })}
                          min={0}
                          max={20}
                          step={1}
                          decimals={0}
                        />
                        <p className="text-xs text-[--text-muted] mt-1">
                          Annual BB rollup during deferral
                        </p>
                      </div>
                    </div>
                  )}

                  {!config.useDynamicInforce && (
                    <div className="bg-white/5 rounded-lg p-3 backdrop-blur-sm text-sm text-[--text-muted]">
                      Using static pricing_inforce.csv (806 policies)
                    </div>
                  )}
                </div>
              </div>

              {/* Configuration Panel */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 sm:gap-6">
                <div className="glass-card rounded-3xl p-4 sm:p-6">
                  <h3 className="text-base sm:text-lg font-semibold mb-4 drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                    Projection Configuration
                  </h3>
                  <div className="space-y-4">
                    <div>
                      <label className="block text-sm text-[--text-muted] mb-1">
                        Fixed Annual Rate (%)
                      </label>
                      <NumberInput
                        value={config.fixedAnnualRate}
                        onChange={(v) => setConfig({ ...config, fixedAnnualRate: v })}
                        min={0}
                        max={20}
                        step={0.05}
                        decimals={2}
                      />
                    </div>

                    <div>
                      <label className="block text-sm text-[--text-muted] mb-1">
                        Option Budget (%)
                      </label>
                      <NumberInput
                        value={config.optionBudget}
                        onChange={(v) => setConfig({ ...config, optionBudget: v })}
                        min={0}
                        max={10}
                        step={0.05}
                        decimals={2}
                      />
                      <p className="text-xs text-[--text-muted] mt-1">
                        Indexed crediting = {(config.optionBudget * (1 + config.equityKicker / 100)).toFixed(2)}%
                      </p>
                    </div>

                    <div>
                      <label className="block text-sm text-[--text-muted] mb-1">
                        Equity Kicker (%)
                      </label>
                      <NumberInput
                        value={config.equityKicker}
                        onChange={(v) => setConfig({ ...config, equityKicker: v })}
                        min={0}
                        max={100}
                        step={1}
                        decimals={0}
                      />
                      <p className="text-xs text-[--text-muted] mt-1">
                        Gross {config.equityKicker}% − 5% financing = {config.equityKicker - 5}% net
                      </p>
                    </div>

                    <div className="border-t border-[--border-color] pt-4 mt-4">
                      <p className="text-sm font-semibold text-[--text-muted] mb-3">Ceding Commission</p>

                      <div className="space-y-3">
                        <div>
                          <label className="block text-sm text-[--text-muted] mb-1">
                            BBB Rate (%)
                          </label>
                          <NumberInput
                            value={config.bbbRate}
                            onChange={(v) => setConfig({ ...config, bbbRate: v })}
                            min={0}
                            max={20}
                            step={0.05}
                            decimals={2}
                          />
                          <p className="text-xs text-[--text-muted] mt-1">
                            ICE BofA BBB Corporate Index
                          </p>
                        </div>

                        <div>
                          <label className="block text-sm text-[--text-muted] mb-1">
                            Spread (%)
                          </label>
                          <NumberInput
                            value={config.spread}
                            onChange={(v) => setConfig({ ...config, spread: v })}
                            min={0}
                            max={5}
                            step={0.05}
                            decimals={2}
                          />
                        </div>

                        <div className="bg-gradient-to-r from-[--accent]/10 to-transparent rounded-lg p-3 border border-[--accent]/20">
                          <div className="flex justify-between text-sm items-center">
                            <span className="text-[--text-muted]">Total Discount Rate</span>
                            <span className="font-bold text-[--accent] text-lg">
                              {(config.bbbRate + config.spread).toFixed(2)}%
                            </span>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>

                {/* Results Summary */}
                <div className="glass-card rounded-3xl p-6">
                  <h3 className="text-lg font-semibold mb-4 drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                    Projection Results
                  </h3>
                  {result ? (
                    <div className="space-y-4">
                      {/* Ceding Commission - Prominent Display */}
                      {result.cedingCommission && (
                        <div className="bg-gradient-to-r from-[--highlight]/20 via-[--highlight]/10 to-transparent rounded-lg p-4 border border-[--highlight]/40 shadow-md">
                          <div className="flex justify-between items-center">
                            <div>
                              <p className="text-sm font-semibold text-[--highlight]">Ceding Commission</p>
                              <p className="text-xs text-[--text-muted]">
                                NPV @ {result.cedingCommission.totalRatePct.toFixed(2)}% (BBB + spread)
                              </p>
                            </div>
                            <p className="text-2xl font-bold text-[--highlight] tracking-tight">
                              {formatCurrency(result.cedingCommission.npv)}
                            </p>
                          </div>
                        </div>
                      )}

                      {/* Key Metrics */}
                      <div className="grid grid-cols-2 gap-3 text-sm">
                        <div className="flex justify-between py-1">
                          <span className="text-[--text-muted]">Total Net Cashflows</span>
                          <span className="font-semibold">{formatCurrency(result.summary.totalNetCashflows)}</span>
                        </div>
                        <div className="flex justify-between py-1">
                          <span className="text-[--text-muted]">Execution Time</span>
                          <span className="font-semibold">{result.executionTimeMs}ms</span>
                        </div>
                      </div>

                      {/* CSV Download */}
                      <button
                        onClick={() => {
                          const headers = "Month,BOP_AV,BOP_BB,Lives,Mortality,Lapse,PWD,RiderCharges,SurrCharges,Interest,EOP_AV,Expenses,AgentComm,IMOOverride,WholesalerOverride,BonusComp,Chargebacks,HedgeGains,NetCashflow";
                          const rows = result.cashflows.map(cf =>
                            `${cf.month},${cf.bopAv.toFixed(2)},${cf.bopBb.toFixed(2)},${cf.lives.toFixed(8)},${cf.mortality.toFixed(2)},${cf.lapse.toFixed(2)},${cf.pwd.toFixed(2)},${cf.riderCharges.toFixed(2)},${cf.surrenderCharges.toFixed(2)},${cf.interest.toFixed(2)},${cf.eopAv.toFixed(2)},${cf.expenses.toFixed(2)},${cf.agentCommission.toFixed(2)},${cf.imoOverride.toFixed(2)},${cf.wholesalerOverride.toFixed(2)},${cf.bonusComp.toFixed(2)},${cf.chargebacks.toFixed(2)},${cf.hedgeGains.toFixed(2)},${cf.netCashflow.toFixed(2)}`
                          );
                          const csv = [headers, ...rows].join("\n");
                          const blob = new Blob([csv], { type: "text/csv" });
                          const url = URL.createObjectURL(blob);
                          const a = document.createElement("a");
                          a.href = url;
                          a.download = "block_projection_output.csv";
                          a.click();
                          URL.revokeObjectURL(url);
                        }}
                        className="download-btn w-full py-3 px-4 bg-white/10 border-2 border-white/30 rounded-xl text-sm font-medium flex items-center justify-center"
                      >
                        Download Cashflows CSV
                      </button>
                    </div>
                  ) : (
                    <div className="text-center py-12 text-[--text-muted]">
                      <p>Run a projection to see results</p>
                    </div>
                  )}
                </div>
              </div>

              {/* Cashflow Chart - Full Width Below */}
              {result && result.cashflows.length > 0 && (
                <div className="glass-card rounded-3xl p-6">
                  <h3 className="text-lg font-semibold mb-4 drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                    Net Cashflows (Months 2+)
                  </h3>
                  <p className="text-xs text-[--text-muted] mb-4 drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                    Month 1 ({formatCurrency(Math.abs(result.cashflows[0]?.netCashflow || 0))}) excluded for scale
                  </p>
                  <div className="h-64">
                    <ResponsiveContainer width="100%" height="100%">
                      <LineChart
                        data={result.cashflows.slice(1).map((cf) => ({
                          month: cf.month,
                          cashflow: -cf.netCashflow,
                        }))}
                        margin={{ top: 5, right: 20, left: 10, bottom: 5 }}
                      >
                        <CartesianGrid strokeDasharray="3 3" stroke="var(--border-color)" opacity={0.5} />
                        <XAxis
                          dataKey="month"
                          stroke="var(--text-muted)"
                          fontSize={12}
                          tickFormatter={(v) => v % 120 === 0 ? `${v}` : ""}
                        />
                        <YAxis
                          stroke="var(--text-muted)"
                          fontSize={12}
                          tickFormatter={(v) => `$${(v / 1000000).toFixed(1)}M`}
                        />
                        <Tooltip
                          contentStyle={{
                            backgroundColor: "var(--bg-card)",
                            border: "1px solid var(--border-color)",
                            borderRadius: "8px",
                          }}
                          formatter={(value) => [formatCurrency(value as number), "Net Cashflow"]}
                          labelFormatter={(label) => `Month ${label}`}
                        />
                        <ReferenceLine y={0} stroke="var(--text-muted)" strokeDasharray="3 3" />
                        <Line
                          type="monotone"
                          dataKey="cashflow"
                          stroke="var(--accent-bright)"
                          strokeWidth={2}
                          dot={false}
                        />
                      </LineChart>
                    </ResponsiveContainer>
                  </div>
                </div>
              )}

              {/* BBB Rate Chart - FRED Data */}
              <div className="glass-card rounded-3xl p-6">
                <FredChart
                  onRateSelect={(rate) => {
                    setConfig({ ...config, bbbRate: rate });
                  }}
                />
                <div className="mt-4 p-3 bg-white/5 rounded-lg flex justify-between items-center backdrop-blur-sm">
                  <span className="text-sm text-[--text-muted] drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                    Hover to preview • Click to select BBB rate
                  </span>
                  <span className="font-semibold drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                    Selected: {config.bbbRate.toFixed(2)}% + {config.spread.toFixed(2)}% = {(config.bbbRate + config.spread).toFixed(2)}%
                  </span>
                </div>
              </div>
            </>
          )}

          {/* Scenarios Tab */}
          {activeTab === "Scenarios" && (
            <div className="glass-card rounded-3xl p-6">
              <h3 className="text-lg font-semibold mb-4 drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                Scenario Management
              </h3>
              <div className="text-center py-12 text-[--text-muted]">
                <p>Scenario manager coming soon</p>
                <p className="text-sm mt-2">Create and compare multiple projection scenarios</p>
              </div>
            </div>
          )}

          {/* Results Tab */}
          {activeTab === "Results" && (
            <div className="glass-card rounded-3xl p-6">
              <h3 className="text-lg font-semibold mb-4 drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                Detailed Results
              </h3>
              {result ? (
                <div className="space-y-6">
                  <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                    <div className="bg-white/5 rounded-lg p-4 backdrop-blur-sm">
                      <p className="text-sm text-[--text-muted]">Cost of Funds</p>
                      <p className="text-2xl font-bold text-[--accent]">
                        {result.costOfFundsPct ? formatPercent(result.costOfFundsPct) : "N/A"}
                      </p>
                    </div>
                    <div className="bg-white/5 rounded-lg p-4 backdrop-blur-sm">
                      <p className="text-sm text-[--text-muted]">Policy Count</p>
                      <p className="text-2xl font-bold">{result.policyCount.toLocaleString()}</p>
                    </div>
                    <div className="bg-white/5 rounded-lg p-4 backdrop-blur-sm">
                      <p className="text-sm text-[--text-muted]">Projection Months</p>
                      <p className="text-2xl font-bold">{result.projectionMonths}</p>
                    </div>
                  </div>
                  <div className="border-t border-[--border-color] pt-4">
                    <h4 className="font-semibold mb-3">Summary Statistics</h4>
                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <p className="text-sm text-[--text-muted]">Total Premium</p>
                        <p className="font-semibold">{formatCurrency(result.summary.totalPremium)}</p>
                      </div>
                      <div>
                        <p className="text-sm text-[--text-muted]">Initial AV</p>
                        <p className="font-semibold">{formatCurrency(result.summary.totalInitialAv)}</p>
                      </div>
                      <div>
                        <p className="text-sm text-[--text-muted]">Initial Benefit Base</p>
                        <p className="font-semibold">{formatCurrency(result.summary.totalInitialBb)}</p>
                      </div>
                      <div>
                        <p className="text-sm text-[--text-muted]">Initial Lives</p>
                        <p className="font-semibold">{result.summary.totalInitialLives.toFixed(2)}</p>
                      </div>
                      <div>
                        <p className="text-sm text-[--text-muted]">Month 1 Cashflow</p>
                        <p className="font-semibold">{formatCurrency(result.summary.month1Cashflow)}</p>
                      </div>
                      <div>
                        <p className="text-sm text-[--text-muted]">Total Net Cashflows</p>
                        <p className="font-semibold">{formatCurrency(result.summary.totalNetCashflows)}</p>
                      </div>
                      <div>
                        <p className="text-sm text-[--text-muted]">Final Lives</p>
                        <p className="font-semibold">{result.summary.finalLives.toFixed(6)}</p>
                      </div>
                      <div>
                        <p className="text-sm text-[--text-muted]">Final AV</p>
                        <p className="font-semibold">{formatCurrency(result.summary.finalAv)}</p>
                      </div>
                    </div>
                  </div>
                </div>
              ) : (
                <div className="text-center py-12 text-[--text-muted]">
                  <p>No results yet</p>
                  <p className="text-sm mt-2">Run a projection from the Dashboard to see detailed results</p>
                </div>
              )}
            </div>
          )}

          {/* Policy Explorer Tab */}
          {activeTab === "Explorer" && (
            <div className="space-y-6">
              {/* Filter Controls */}
              <div className="glass-card rounded-3xl p-6">
                <h3 className="text-lg font-semibold mb-6 drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                  Policy Filters
                </h3>
                <div className="space-y-6">
                  {/* GLWB Start Year Slider */}
                  <div>
                    <label className="block text-sm text-[--text-muted] mb-2 drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                      Only policies who take income after year...
                    </label>
                    <div className="flex items-center gap-4">
                      <div className="flex-1 relative">
                        {/* Number line background */}
                        <div className="absolute inset-x-0 top-1/2 -translate-y-1/2 h-1 bg-white/10 rounded-full" />
                        <div className="absolute inset-x-0 top-1/2 translate-y-3 flex justify-between px-1">
                          {[1, 5, 10, 15, 20, 25].map((tick) => (
                            <span key={tick} className="text-[10px] text-[--text-muted]/60">{tick}</span>
                          ))}
                        </div>
                        <input
                          type="range"
                          min="1"
                          max="25"
                          value={explorerMinGlwbYear}
                          onChange={(e) => setExplorerMinGlwbYear(parseInt(e.target.value))}
                          className="relative z-10 w-full"
                        />
                      </div>
                      <div className="w-16 text-center">
                        <span className="text-2xl font-bold text-[--accent-bright] drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                          {explorerMinGlwbYear}
                        </span>
                      </div>
                    </div>
                    <p className="text-xs text-[--text-muted] mt-4 drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                      GLWBStartYear ≥ {explorerMinGlwbYear}
                    </p>
                  </div>

                  {/* Issue Age Range Slider */}
                  <div>
                    <label className="block text-sm text-[--text-muted] mb-2 drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                      Issue Age Range
                    </label>
                    <div className="flex items-center gap-4">
                      <span className="text-lg font-bold text-[--accent-bright] w-10 text-center drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">{explorerMinIssueAge}</span>
                      <div className="flex-1 relative">
                        {/* Number line background */}
                        <div className="absolute inset-x-0 top-1/2 -translate-y-1/2 h-1 bg-white/10 rounded-full" />
                        <div className="absolute inset-x-0 top-1/2 translate-y-3 flex justify-between px-1">
                          {[55, 60, 65, 70, 75, 80].map((tick) => (
                            <span key={tick} className="text-[10px] text-[--text-muted]/60">{tick}</span>
                          ))}
                        </div>
                        <input
                          type="range"
                          min="55"
                          max="80"
                          value={explorerMinIssueAge}
                          onChange={(e) => {
                            const val = parseInt(e.target.value);
                            setExplorerMinIssueAge(Math.min(val, explorerMaxIssueAge));
                          }}
                          className="relative z-10 w-full"
                        />
                      </div>
                      <span className="text-[--text-muted] font-medium">to</span>
                      <div className="flex-1 relative">
                        {/* Number line background */}
                        <div className="absolute inset-x-0 top-1/2 -translate-y-1/2 h-1 bg-white/10 rounded-full" />
                        <div className="absolute inset-x-0 top-1/2 translate-y-3 flex justify-between px-1">
                          {[55, 60, 65, 70, 75, 80].map((tick) => (
                            <span key={tick} className="text-[10px] text-[--text-muted]/60">{tick}</span>
                          ))}
                        </div>
                        <input
                          type="range"
                          min="55"
                          max="80"
                          value={explorerMaxIssueAge}
                          onChange={(e) => {
                            const val = parseInt(e.target.value);
                            setExplorerMaxIssueAge(Math.max(val, explorerMinIssueAge));
                          }}
                          className="relative z-10 w-full"
                        />
                      </div>
                      <span className="text-lg font-bold text-[--accent-bright] w-10 text-center drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">{explorerMaxIssueAge}</span>
                    </div>
                    <p className="text-xs text-[--text-muted] mt-4 drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                      Issue Age: {explorerMinIssueAge} - {explorerMaxIssueAge}
                    </p>
                  </div>

                  {/* Selection Cards Row */}
                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                    {/* Gender */}
                    <div>
                      <label className="block text-sm text-[--text-muted] mb-2 drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                        Gender
                      </label>
                      <div className="flex gap-2">
                        {[
                          { value: "Male", label: "M" },
                          { value: "Female", label: "F" },
                        ].map((g) => (
                          <button
                            key={g.value}
                            onClick={() => toggleFilter(explorerGenders, g.value, setExplorerGenders)}
                            className={`flex-1 py-2 px-3 rounded-xl text-sm font-medium transition-all ${
                              explorerGenders.includes(g.value)
                                ? "bg-[--accent] text-white border-2 border-white/40 shadow-[0_4px_12px_rgba(0,0,0,0.3),inset_0_1px_0_0_rgba(255,255,255,0.3)]"
                                : "bg-white/5 text-[--text-muted] border-2 border-white/10 hover:bg-white/10"
                            }`}
                          >
                            {g.label}
                          </button>
                        ))}
                      </div>
                    </div>

                    {/* Qualified Status */}
                    <div>
                      <label className="block text-sm text-[--text-muted] mb-2 drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                        Qualified Status
                      </label>
                      <div className="flex gap-2">
                        {[
                          { value: "Q", label: "Qualified" },
                          { value: "N", label: "Non-Qual" },
                        ].map((q) => (
                          <button
                            key={q.value}
                            onClick={() => toggleFilter(explorerQualStatuses, q.value, setExplorerQualStatuses)}
                            className={`flex-1 py-2 px-3 rounded-xl text-sm font-medium transition-all ${
                              explorerQualStatuses.includes(q.value)
                                ? "bg-[--accent] text-white border-2 border-white/40 shadow-[0_4px_12px_rgba(0,0,0,0.3),inset_0_1px_0_0_rgba(255,255,255,0.3)]"
                                : "bg-white/5 text-[--text-muted] border-2 border-white/10 hover:bg-white/10"
                            }`}
                          >
                            {q.label}
                          </button>
                        ))}
                      </div>
                    </div>

                    {/* Crediting Strategy */}
                    <div>
                      <label className="block text-sm text-[--text-muted] mb-2 drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                        Crediting Strategy
                      </label>
                      <div className="flex gap-2">
                        {[
                          { value: "Fixed", label: "Fixed" },
                          { value: "Indexed", label: "Indexed" },
                        ].map((c) => (
                          <button
                            key={c.value}
                            onClick={() => toggleFilter(explorerCreditingStrategies, c.value, setExplorerCreditingStrategies)}
                            className={`flex-1 py-2 px-3 rounded-xl text-sm font-medium transition-all ${
                              explorerCreditingStrategies.includes(c.value)
                                ? "bg-[--accent] text-white border-2 border-white/40 shadow-[0_4px_12px_rgba(0,0,0,0.3),inset_0_1px_0_0_rgba(255,255,255,0.3)]"
                                : "bg-white/5 text-[--text-muted] border-2 border-white/10 hover:bg-white/10"
                            }`}
                          >
                            {c.label}
                          </button>
                        ))}
                      </div>
                    </div>
                  </div>

                  {/* Benefit Base Buckets */}
                  <div>
                    <label className="block text-sm text-[--text-muted] mb-2 drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">
                      Benefit Base Size
                    </label>
                    <div className="flex flex-wrap gap-2">
                      {[
                        { value: "[0, 50000)", label: "<$50K" },
                        { value: "[50000, 100000)", label: "$50K-$100K" },
                        { value: "[100000, 200000)", label: "$100K-$200K" },
                        { value: "[200000, 500000)", label: "$200K-$500K" },
                        { value: "[500000, Inf)", label: "$500K+" },
                      ].map((b) => (
                        <button
                          key={b.value}
                          onClick={() => toggleFilter(explorerBBBuckets, b.value, setExplorerBBBuckets)}
                          className={`py-2 px-4 rounded-xl text-sm font-medium transition-all ${
                            explorerBBBuckets.includes(b.value)
                              ? "bg-[--accent] text-white border-2 border-white/40 shadow-[0_4px_12px_rgba(0,0,0,0.3),inset_0_1px_0_0_rgba(255,255,255,0.3)]"
                              : "bg-white/5 text-[--text-muted] border-2 border-white/10 hover:bg-white/10"
                          }`}
                        >
                          {b.label}
                        </button>
                      ))}
                    </div>
                  </div>

                  <button
                    onClick={runExplorerProjection}
                    disabled={explorerLoading}
                    className="w-full flex items-center justify-center bg-gradient-to-r from-[--accent] to-[--accent-bright] text-white px-6 py-3 rounded-xl font-semibold hover:from-[--accent-hover] hover:to-[--accent] hover:scale-[1.02] transition-all disabled:opacity-50 disabled:cursor-not-allowed border-2 border-white/30 shadow-[0_8px_32px_rgba(0,0,0,0.3),inset_0_1px_0_0_rgba(255,255,255,0.4)]"
                  >
                    {explorerLoading ? "Running Filtered Projection..." : "Run Filtered Projection"}
                  </button>
                </div>
              </div>

              {/* Explorer Error */}
              {explorerError && (
                <div className="bg-red-900/30 border border-red-500 rounded-lg p-4 text-red-200">
                  <p className="font-semibold">Error</p>
                  <p>{explorerError}</p>
                </div>
              )}

              {/* Explorer Results */}
              {explorerResult && (
                <>
                  {/* COF Hero Card */}
                  {explorerResult.costOfFundsPct !== null && (
                    <div className="relative overflow-hidden hero-gradient rounded-3xl p-8 shadow-2xl">
                      <div className="absolute top-0 right-0 w-32 h-32 bg-white/30 rounded-full -translate-y-1/2 translate-x-1/2 blur-2xl"></div>
                      <div className="absolute bottom-0 left-0 w-24 h-24 bg-white/20 rounded-full translate-y-1/2 -translate-x-1/2 blur-xl"></div>
                      <div className="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-white/60 to-transparent"></div>
                      <div className="relative">
                        <p className="text-lg font-medium text-white drop-shadow-[0_2px_8px_rgba(0,0,0,0.3)]">
                          Filtered Cost of Funds (IRR)
                        </p>
                        <p className="text-6xl font-bold mt-2 tracking-tight text-white drop-shadow-[0_2px_12px_rgba(0,0,0,0.4)]">
                          {formatPercent(explorerResult.costOfFundsPct)}
                        </p>
                        <p className="mt-4 text-sm text-white/90 drop-shadow-[0_1px_4px_rgba(0,0,0,0.3)]">
                          {explorerResult.policyCount.toLocaleString()} policies matching filters • {explorerResult.executionTimeMs}ms
                        </p>
                      </div>
                    </div>
                  )}

                  {/* Summary Cards */}
                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                    <div className="glass-card rounded-3xl p-6">
                      <p className="text-[--text-muted] text-sm drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">Policy Count</p>
                      <p className="text-2xl font-bold tracking-tight text-[--text-primary] drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                        {explorerResult.policyCount.toLocaleString()}
                      </p>
                    </div>
                    <div className="glass-card rounded-3xl p-6">
                      <p className="text-[--text-muted] text-sm drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">Total Premium</p>
                      <p className="text-2xl font-bold tracking-tight text-[--text-primary] drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                        {formatCurrency(explorerResult.summary.totalPremium)}
                      </p>
                    </div>
                    <div className="glass-card rounded-3xl p-6">
                      <p className="text-[--text-muted] text-sm drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">Initial AV</p>
                      <p className="text-2xl font-bold tracking-tight text-[--text-primary] drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                        {formatCurrency(explorerResult.summary.totalInitialAv)}
                      </p>
                    </div>
                    <div className="glass-card rounded-3xl p-6">
                      <p className="text-[--text-muted] text-sm drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">Net Cashflows</p>
                      <p className="text-2xl font-bold tracking-tight text-[--text-primary] drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                        {formatCurrency(explorerResult.summary.totalNetCashflows)}
                      </p>
                    </div>
                  </div>

                  {/* Cashflow Chart */}
                  {explorerResult.cashflows.length > 0 && (
                    <div className="glass-card rounded-3xl p-6">
                      <h3 className="text-lg font-semibold mb-4 drop-shadow-[0_2px_8px_rgba(0,0,0,0.5)]">
                        Filtered Cashflows (Months 2+)
                      </h3>
                      <div className="h-64">
                        <ResponsiveContainer width="100%" height="100%">
                          <LineChart
                            data={explorerResult.cashflows.slice(1).map((cf) => ({
                              month: cf.month,
                              cashflow: -cf.netCashflow,
                            }))}
                            margin={{ top: 5, right: 20, left: 10, bottom: 5 }}
                          >
                            <CartesianGrid strokeDasharray="3 3" stroke="var(--border-color)" opacity={0.5} />
                            <XAxis
                              dataKey="month"
                              stroke="var(--text-muted)"
                              fontSize={12}
                              tickFormatter={(v) => v % 120 === 0 ? `${v}` : ""}
                            />
                            <YAxis
                              stroke="var(--text-muted)"
                              fontSize={12}
                              tickFormatter={(v) => `$${(v / 1000000).toFixed(1)}M`}
                            />
                            <Tooltip
                              contentStyle={{
                                backgroundColor: "var(--bg-card)",
                                border: "1px solid var(--border-color)",
                                borderRadius: "8px",
                              }}
                              formatter={(value) => [formatCurrency(value as number), "Net Cashflow"]}
                              labelFormatter={(label) => `Month ${label}`}
                            />
                            <ReferenceLine y={0} stroke="var(--text-muted)" strokeDasharray="3 3" />
                            <Line
                              type="monotone"
                              dataKey="cashflow"
                              stroke="var(--accent-bright)"
                              strokeWidth={2}
                              dot={false}
                            />
                          </LineChart>
                        </ResponsiveContainer>
                      </div>

                      {/* CSV Download */}
                      <button
                        onClick={() => {
                          const headers = "Month,BOP_AV,BOP_BB,Lives,Mortality,Lapse,PWD,RiderCharges,SurrCharges,Interest,EOP_AV,Expenses,AgentComm,IMOOverride,WholesalerOverride,BonusComp,Chargebacks,HedgeGains,NetCashflow";
                          const rows = explorerResult.cashflows.map(cf =>
                            `${cf.month},${cf.bopAv.toFixed(2)},${cf.bopBb.toFixed(2)},${cf.lives.toFixed(8)},${cf.mortality.toFixed(2)},${cf.lapse.toFixed(2)},${cf.pwd.toFixed(2)},${cf.riderCharges.toFixed(2)},${cf.surrenderCharges.toFixed(2)},${cf.interest.toFixed(2)},${cf.eopAv.toFixed(2)},${cf.expenses.toFixed(2)},${cf.agentCommission.toFixed(2)},${cf.imoOverride.toFixed(2)},${cf.wholesalerOverride.toFixed(2)},${cf.bonusComp.toFixed(2)},${cf.chargebacks.toFixed(2)},${cf.hedgeGains.toFixed(2)},${cf.netCashflow.toFixed(2)}`
                          );
                          const csv = [headers, ...rows].join("\n");
                          const blob = new Blob([csv], { type: "text/csv" });
                          const url = URL.createObjectURL(blob);
                          const a = document.createElement("a");
                          a.href = url;
                          a.download = `filtered_projection_glwb_gte_${explorerMinGlwbYear}.csv`;
                          a.click();
                          URL.revokeObjectURL(url);
                        }}
                        className="download-btn mt-4 w-full py-3 px-4 bg-white/10 border-2 border-white/30 rounded-xl text-sm font-medium flex items-center justify-center"
                      >
                        Download Filtered Cashflows CSV
                      </button>
                    </div>
                  )}
                </>
              )}

              {/* Empty State */}
              {!explorerResult && !explorerLoading && (
                <div className="glass-card rounded-3xl p-6">
                  <div className="text-center py-12 text-[--text-muted]">
                    <p className="drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)]">Adjust filters and run a projection to explore policy subsets</p>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </main>

      {/* Bottom Navigation */}
      <nav className="fixed bottom-0 left-0 right-0 glass-nav border-t border-white/20 safe-area-bottom">
        <div className="flex justify-around items-center h-16 max-w-lg mx-auto px-2">
          {navItems.map((item) => (
            <button
              key={item.name}
              onClick={() => setActiveTab(item.name)}
              className={`flex-1 flex flex-col items-center justify-center py-2 px-1 rounded-xl mx-1 transition-all ${
                activeTab === item.name
                  ? "bg-white/20 text-white shadow-[0_4px_12px_rgba(0,0,0,0.2),inset_0_1px_0_0_rgba(255,255,255,0.3)]"
                  : "text-white/60 hover:text-white hover:bg-white/10"
              }`}
            >
              <span className={`text-xs font-medium drop-shadow-[0_1px_4px_rgba(0,0,0,0.5)] ${
                activeTab === item.name ? "text-white" : ""
              }`}>
                {item.name}
              </span>
            </button>
          ))}
        </div>
      </nav>
    </div>
  );
}
