"use client";

import { useState } from "react";

// Types for the projection
interface ProjectionConfig {
  projectionMonths: number;
  fixedAnnualRate: number;
  indexedAnnualRate: number;
  treasuryChange: number;
}

interface ProjectionResult {
  costOfFundsPct: number | null;
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
  executionTimeMs: number;
  error?: string;
}

// Navigation items
const navItems = [
  { name: "Dashboard", icon: "📊" },
  { name: "Assumptions", icon: "⚙️" },
  { name: "Scenarios", icon: "📋" },
  { name: "Results", icon: "📈" },
  { name: "Policy Explorer", icon: "🔍" },
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
    projectionMonths: 768,
    fixedAnnualRate: 2.75,
    indexedAnnualRate: 3.78,
    treasuryChange: 0,
  });

  const [result, setResult] = useState<ProjectionResult | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const runProjection = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const response = await fetch("/api/projection", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          projection_months: config.projectionMonths,
          fixed_annual_rate: config.fixedAnnualRate / 100,
          indexed_annual_rate: config.indexedAnnualRate / 100,
          treasury_change: config.treasuryChange / 100,
        }),
      });

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      const data = await response.json();
      setResult({
        costOfFundsPct: data.cost_of_funds_pct,
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
        executionTimeMs: data.execution_time_ms,
        error: data.error,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : "An error occurred");
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="flex min-h-screen">
      {/* Sidebar */}
      <aside className="w-64 bg-[--bg-secondary] border-r border-[--border-color] flex flex-col">
        {/* Logo */}
        <div className="p-6 border-b border-[--border-color]">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 bg-[--accent] rounded-lg flex items-center justify-center">
              <span className="text-[--bg-primary] font-bold text-xl">A</span>
            </div>
            <div>
              <h1 className="font-bold text-lg">Actuarial</h1>
              <p className="text-xs text-[--text-muted]">Projection System</p>
            </div>
          </div>
        </div>

        {/* Navigation */}
        <nav className="flex-1 p-4">
          <ul className="space-y-2">
            {navItems.map((item) => (
              <li key={item.name}>
                <button
                  onClick={() => setActiveTab(item.name)}
                  className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg transition-colors ${
                    activeTab === item.name
                      ? "bg-[--bg-card] text-[--accent]"
                      : "text-[--text-muted] hover:bg-[--bg-card] hover:text-[--text-primary]"
                  }`}
                >
                  <span>{item.icon}</span>
                  <span>{item.name}</span>
                </button>
              </li>
            ))}
          </ul>
        </nav>

        {/* Version */}
        <div className="p-4 border-t border-[--border-color]">
          <p className="text-xs text-[--text-muted]">Version 1.0.0</p>
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 overflow-auto">
        {/* Header */}
        <header className="bg-[--bg-secondary] border-b border-[--border-color] p-6">
          <div className="flex justify-between items-center">
            <div>
              <p className="text-[--text-muted] text-sm">Welcome back</p>
              <h2 className="text-2xl font-bold">{activeTab}</h2>
            </div>
            {activeTab === "Dashboard" && (
              <button
                onClick={runProjection}
                disabled={isLoading}
                className="flex items-center gap-2 bg-[--accent] text-[--bg-primary] px-6 py-3 rounded-lg font-semibold hover:bg-[--accent-muted] transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {isLoading ? (
                  <>
                    <span className="animate-spin">⏳</span>
                    Running...
                  </>
                ) : (
                  <>
                    <span>▶️</span>
                    Run Projection
                  </>
                )}
              </button>
            )}
          </div>
        </header>

        <div className="p-6 space-y-6">
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
              {/* Cost of Funds Card - Hero */}
              {result && result.costOfFundsPct !== null && (
                <div className="bg-gradient-to-r from-[--accent-muted] to-[--accent] rounded-xl p-8 text-[--bg-primary]">
                  <p className="text-lg opacity-80">Cost of Funds (IRR)</p>
                  <p className="text-5xl font-bold mt-2">
                    {formatPercent(result.costOfFundsPct)}
                  </p>
                  <p className="mt-4 opacity-70">
                    Calculated in {result.executionTimeMs}ms for{" "}
                    {result.policyCount.toLocaleString()} policies
                  </p>
                </div>
              )}

              {/* Summary Cards */}
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <div className="bg-[--bg-card] rounded-xl p-6 border border-[--border-color]">
                  <div className="flex items-center gap-3 mb-4">
                    <div className="w-12 h-12 bg-[--bg-secondary] rounded-lg flex items-center justify-center">
                      <span className="text-2xl">💰</span>
                    </div>
                    <span className="text-[--text-muted] text-sm">
                      {result ? result.policyCount.toLocaleString() : "—"} policies
                    </span>
                  </div>
                  <p className="text-[--text-muted] text-sm">Total Premium</p>
                  <p className="text-2xl font-bold">
                    {result
                      ? formatCurrency(result.summary.totalPremium)
                      : "$100,000,000"}
                  </p>
                </div>

                <div className="bg-[--bg-card] rounded-xl p-6 border border-[--border-color]">
                  <div className="flex items-center gap-3 mb-4">
                    <div className="w-12 h-12 bg-[--bg-secondary] rounded-lg flex items-center justify-center">
                      <span className="text-2xl">📊</span>
                    </div>
                    <span className="text-[--accent] text-sm">Active</span>
                  </div>
                  <p className="text-[--text-muted] text-sm">Initial AV</p>
                  <p className="text-2xl font-bold">
                    {result
                      ? formatCurrency(result.summary.totalInitialAv)
                      : "$100,000,000"}
                  </p>
                </div>

                <div className="bg-[--bg-card] rounded-xl p-6 border border-[--border-color]">
                  <div className="flex items-center gap-3 mb-4">
                    <div className="w-12 h-12 bg-[--bg-secondary] rounded-lg flex items-center justify-center">
                      <span className="text-2xl">👥</span>
                    </div>
                  </div>
                  <p className="text-[--text-muted] text-sm">Initial Lives</p>
                  <p className="text-2xl font-bold">
                    {result
                      ? result.summary.totalInitialLives.toFixed(2)
                      : "806.57"}
                  </p>
                </div>

                <div className="bg-[--bg-card] rounded-xl p-6 border border-[--border-color]">
                  <div className="flex items-center gap-3 mb-4">
                    <div className="w-12 h-12 bg-[--bg-secondary] rounded-lg flex items-center justify-center">
                      <span className="text-2xl">📈</span>
                    </div>
                  </div>
                  <p className="text-[--text-muted] text-sm">Benefit Base</p>
                  <p className="text-2xl font-bold">
                    {result
                      ? formatCurrency(result.summary.totalInitialBb)
                      : "$130,000,000"}
                  </p>
                </div>
              </div>

              {/* Configuration Panel */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                <div className="bg-[--bg-card] rounded-xl p-6 border border-[--border-color]">
                  <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                    <span>⚙️</span>
                    Projection Configuration
                  </h3>
                  <div className="space-y-4">
                    <div>
                      <label className="block text-sm text-[--text-muted] mb-1">
                        Projection Months
                      </label>
                      <input
                        type="number"
                        value={config.projectionMonths}
                        onChange={(e) =>
                          setConfig({
                            ...config,
                            projectionMonths: parseInt(e.target.value) || 768,
                          })
                        }
                        className="w-full bg-[--bg-secondary] border border-[--border-color] rounded-lg px-4 py-2 text-[--text-primary] focus:outline-none focus:border-[--accent]"
                      />
                      <p className="text-xs text-[--text-muted] mt-1">
                        768 months = terminal age 121 for issue age 57
                      </p>
                    </div>

                    <div>
                      <label className="block text-sm text-[--text-muted] mb-1">
                        Fixed Annual Rate (%)
                      </label>
                      <input
                        type="number"
                        step="0.01"
                        value={config.fixedAnnualRate}
                        onChange={(e) =>
                          setConfig({
                            ...config,
                            fixedAnnualRate: parseFloat(e.target.value) || 2.75,
                          })
                        }
                        className="w-full bg-[--bg-secondary] border border-[--border-color] rounded-lg px-4 py-2 text-[--text-primary] focus:outline-none focus:border-[--accent]"
                      />
                    </div>

                    <div>
                      <label className="block text-sm text-[--text-muted] mb-1">
                        Indexed Annual Rate (%)
                      </label>
                      <input
                        type="number"
                        step="0.01"
                        value={config.indexedAnnualRate}
                        onChange={(e) =>
                          setConfig({
                            ...config,
                            indexedAnnualRate: parseFloat(e.target.value) || 3.78,
                          })
                        }
                        className="w-full bg-[--bg-secondary] border border-[--border-color] rounded-lg px-4 py-2 text-[--text-primary] focus:outline-none focus:border-[--accent]"
                      />
                    </div>

                    <div>
                      <label className="block text-sm text-[--text-muted] mb-1">
                        Treasury Rate Change (%)
                      </label>
                      <input
                        type="number"
                        step="0.1"
                        value={config.treasuryChange}
                        onChange={(e) =>
                          setConfig({
                            ...config,
                            treasuryChange: parseFloat(e.target.value) || 0,
                          })
                        }
                        className="w-full bg-[--bg-secondary] border border-[--border-color] rounded-lg px-4 py-2 text-[--text-primary] focus:outline-none focus:border-[--accent]"
                      />
                      <p className="text-xs text-[--text-muted] mt-1">
                        Affects dynamic lapse rates
                      </p>
                    </div>
                  </div>
                </div>

                {/* Results Summary */}
                <div className="bg-[--bg-card] rounded-xl p-6 border border-[--border-color]">
                  <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                    <span>📋</span>
                    Projection Results
                  </h3>
                  {result ? (
                    <div className="space-y-3">
                      <div className="flex justify-between py-2 border-b border-[--border-color]">
                        <span className="text-[--text-muted]">Month 1 Cashflow</span>
                        <span className="font-semibold">
                          {formatCurrency(result.summary.month1Cashflow)}
                        </span>
                      </div>
                      <div className="flex justify-between py-2 border-b border-[--border-color]">
                        <span className="text-[--text-muted]">
                          Total Net Cashflows
                        </span>
                        <span className="font-semibold">
                          {formatCurrency(result.summary.totalNetCashflows)}
                        </span>
                      </div>
                      <div className="flex justify-between py-2 border-b border-[--border-color]">
                        <span className="text-[--text-muted]">Final Lives</span>
                        <span className="font-semibold">
                          {result.summary.finalLives.toFixed(4)}
                        </span>
                      </div>
                      <div className="flex justify-between py-2 border-b border-[--border-color]">
                        <span className="text-[--text-muted]">Final AV</span>
                        <span className="font-semibold">
                          {formatCurrency(result.summary.finalAv)}
                        </span>
                      </div>
                      <div className="flex justify-between py-2">
                        <span className="text-[--text-muted]">Execution Time</span>
                        <span className="font-semibold">
                          {result.executionTimeMs}ms
                        </span>
                      </div>
                    </div>
                  ) : (
                    <div className="text-center py-12 text-[--text-muted]">
                      <p className="text-4xl mb-4">📊</p>
                      <p>Run a projection to see results</p>
                    </div>
                  )}
                </div>
              </div>
            </>
          )}

          {/* Assumptions Tab */}
          {activeTab === "Assumptions" && (
            <div className="bg-[--bg-card] rounded-xl p-6 border border-[--border-color]">
              <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                <span>⚙️</span>
                Assumptions Management
              </h3>
              <div className="text-center py-12 text-[--text-muted]">
                <p className="text-4xl mb-4">⚙️</p>
                <p>Assumptions editor coming soon</p>
                <p className="text-sm mt-2">Manage mortality, lapse, and expense assumptions</p>
              </div>
            </div>
          )}

          {/* Scenarios Tab */}
          {activeTab === "Scenarios" && (
            <div className="bg-[--bg-card] rounded-xl p-6 border border-[--border-color]">
              <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                <span>📋</span>
                Scenario Management
              </h3>
              <div className="text-center py-12 text-[--text-muted]">
                <p className="text-4xl mb-4">📋</p>
                <p>Scenario manager coming soon</p>
                <p className="text-sm mt-2">Create and compare multiple projection scenarios</p>
              </div>
            </div>
          )}

          {/* Results Tab */}
          {activeTab === "Results" && (
            <div className="bg-[--bg-card] rounded-xl p-6 border border-[--border-color]">
              <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                <span>📈</span>
                Detailed Results
              </h3>
              {result ? (
                <div className="space-y-6">
                  <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                    <div className="bg-[--bg-secondary] rounded-lg p-4">
                      <p className="text-sm text-[--text-muted]">Cost of Funds</p>
                      <p className="text-2xl font-bold text-[--accent]">
                        {result.costOfFundsPct ? formatPercent(result.costOfFundsPct) : "N/A"}
                      </p>
                    </div>
                    <div className="bg-[--bg-secondary] rounded-lg p-4">
                      <p className="text-sm text-[--text-muted]">Policy Count</p>
                      <p className="text-2xl font-bold">{result.policyCount.toLocaleString()}</p>
                    </div>
                    <div className="bg-[--bg-secondary] rounded-lg p-4">
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
                  <p className="text-4xl mb-4">📈</p>
                  <p>No results yet</p>
                  <p className="text-sm mt-2">Run a projection from the Dashboard to see detailed results</p>
                </div>
              )}
            </div>
          )}

          {/* Policy Explorer Tab */}
          {activeTab === "Policy Explorer" && (
            <div className="bg-[--bg-card] rounded-xl p-6 border border-[--border-color]">
              <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                <span>🔍</span>
                Policy Explorer
              </h3>
              <div className="text-center py-12 text-[--text-muted]">
                <p className="text-4xl mb-4">🔍</p>
                <p>Policy explorer coming soon</p>
                <p className="text-sm mt-2">Search and analyze individual policy projections</p>
              </div>
            </div>
          )}
        </div>
      </main>
    </div>
  );
}
