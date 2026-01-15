# Actuarial System Implementation Plan

## Project Overview

Build a high-performance actuarial projection system in Rust for fixed indexed annuity (FIA) products with GLWB riders. The system will support pricing, valuation, and multi-scenario projections, ultimately deployed to AWS.

---

## Phase 1: Foundation & Single Policy Projection

### 1.1 Project Setup
- Initialize Rust project with Cargo
- Set up directory structure (src/lib, src/bin, tests, data)
- Configure dependencies (serde, csv, chrono, rayon for parallelism)
- Create build configuration for release optimization

### 1.2 Core Data Structures
- **Policy struct**: PolicyID, QualStatus, IssueAge, Gender, InitialBB, InitialPols, InitialPremium, BenefitBaseBucket, Percentage, CreditingStrategy, SCPeriod, valRate, MGIR, Bonus, RollupType
- **Assumption tables**: Mortality (IAM 2012 with improvement), surrender charges, payout factors, RMD rates
- **Projection state**: BOP/EOP account value, benefit base, lives, persistency factors

### 1.3 Decrement Models
- **Mortality**: IAM 2012 Basic table with multiplicative factors by age and mortality improvement (1.5% annual improvement)
- **Lapse model**: Implement the surrender predictive model with:
  - Base/dynamic components
  - Polynomial terms for duration vs SC period
  - Benefit base bucket adjustments
  - Income activation impacts
  - ITM-ness sensitivity
- **Non-systematic PWDs**: Free withdrawal utilization by policy year, RMD rates by attained age

### 1.4 Single Policy Projection Engine
- Monthly timestep projection loop
- Calculate for each month:
  - Attained age, policy year, month in policy year
  - Baseline mortality → mortality improvement → final mortality rate
  - Surrender charge lookup
  - Free partial withdrawal percentage
  - GLWB activation status
  - Non-systematic PWD rate
  - Base lapse component + dynamic component → final lapse rate
  - Rider charge calculation (0.5% pre-activation, 1.5% post-activation)
  - Credited rate application (Note that this should handle either an "Option Budget + Equity kicker" approach like the spreadsheet has or actual calculations based on scenarios, subject to floors, caps, and participation rates)
  - Systematic withdrawal (GLWB payments)
  - Rollup rate (10% simple for 10 years)
- Track persistency: AV persistency, BB persistency, Lives persistency
- Calculate cashflows: Premium, Mortality, Lapse, PWD, Rider charges, Surrender charges, Interest credits, Expenses, Commission

### 1.5 Validation
- Create test harness comparing to Excel "Calculation example" sheet
- Validate row-by-row for Policy 2800 across all 40+ output columns
- Ensure numerical precision to 6+ decimal places

---

## Phase 2: Multi-Policy Projections

### 2.1 Inforce Data Loading
- Parse pricing inforce file (~2,800 policies)
- Handle grouped data format (benefit base buckets × age × wait periods × qual status)
- Create efficient in-memory representation

### 2.2 Batch Projection Engine
- Implement parallel projection using rayon
- Aggregate cashflows across all policies
- Memory-efficient streaming for large inforce files

### 2.3 Output Generation
- Summary cashflows by projection month
- Detailed policy-level outputs (optional)
- CSV/Parquet export capabilities

---

## Phase 3: Reserve Calculations

### 3.1 Discounting Framework
- Net present value calculations (Note: the reserve framework should generally be set up as "what quantum of assets do I need to support these liabilities under this set of policyholder behavior, mortality, and interest rate scenarios". This maps well to VM-22, which is a scenario-based, stochastic economic reserve framework, as well as static economic reserve scenarios. We should also be able to provide CARVM reserves, which assume that all elective benefits are utilized optimally. This will require testing across many sets of policyholder behavior (deterministic, not stochastic))
- Configurable discount rates (valuation rate from policy data)
- Spot rate curve support

### 3.2 Reserve Metrics
- Statutory reserves
- GAAP reserves
- Best estimate liabilities
- Risk margins

### 3.3 API Design
- Standalone reserve calculation functions
- Integration with projection engine
- Flexible output formats

---

## Phase 4: Simple Asset Modeling

### 4.1 Asset Data Structures
- Asset struct: ID, type, par value, coupon, maturity, credit rating
- Portfolio struct: collection of assets with metadata
- Transaction types: Buy, Sell, Income, Maturity

### 4.2 Asset Projection
- Coupon/income generation
- Principal amortization
- Maturity processing
- Book value tracking

### 4.3 Asset-Liability Integration
- Match asset cashflows to liability cashflows
- Track surplus/deficit by period
- Simple reinvestment assumptions

---

## Phase 5: Fixed & Floating Rate Securities

### 5.1 Fixed Rate Bonds
- Bullet and amortizing structures
- Accrued interest calculations
- Yield calculations (YTM, current yield)

### 5.2 Floating Rate Notes
- Reference rate lookups (e.g., SOFR)
- Spread calculations
- Reset mechanics
- Caps/floors

### 5.3 Market Value Calculations
- Price from yield
- Duration and convexity
- Key rate durations

---

## Phase 6: Portfolio Analytics

### 6.1 Portfolio Metrics
- Total market value
- Weighted average life
- Portfolio duration
- Portfolio yield
- Credit quality distribution

### 6.2 Risk Metrics
- Interest rate sensitivity
- Credit spread sensitivity
- Liquidity analysis

### 6.3 Reporting
- Portfolio summary reports
- Holdings detail
- Transaction history

---

## Phase 7: Multi-Scenario Runs

### 7.1 Scenario Framework
- Scenario definition format (YAML/JSON)
- Parameter overrides (mortality, lapse, interest rates)
- Deterministic scenarios

### 7.2 Stochastic Scenarios
- Economic scenario generator integration
- Interest rate models
- Equity return models

### 7.3 Parallel Execution
- Run multiple scenarios concurrently
- Result aggregation
- Percentile calculations

---

## Phase 8: Embedded Decision Making

### 8.1 Rebalancing Logic
- Target allocation definitions
- Rebalancing triggers (threshold-based, periodic)
- Transaction generation

### 8.2 Strategic Asset Allocation (SAA)
- ALM-aware allocation updates
- Liability-driven investing rules
- Duration matching

### 8.3 Dynamic Hedging
- Hedge ratio calculations
- Hedge rebalancing rules
- Greeks tracking

---

## Phase 9: Optimization

### 9.1 Performance Optimization
- Profile and benchmark
- SIMD vectorization where applicable
- Memory layout optimization
- Cache-friendly data structures

### 9.2 Portfolio Optimization
- Mean-variance optimization
- Liability-relative optimization
- Constraint handling (duration, credit, liquidity)

### 9.3 AWS Deployment
- Lambda function packaging
- API Gateway integration
- S3 for data storage
- Batch processing with AWS Batch
- Infrastructure as code (CloudFormation/CDK)

---

## File Structure

```
ActuarialSystemv2/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Library root
│   ├── main.rs                   # CLI entrypoint
│   ├── policy/
│   │   ├── mod.rs
│   │   ├── data.rs               # Policy data structures
│   │   └── inforce.rs            # Inforce loading
│   ├── assumptions/
│   │   ├── mod.rs
│   │   ├── mortality.rs          # Mortality tables & improvement
│   │   ├── lapse.rs              # Surrender predictive model
│   │   ├── pwd.rs                # Partial withdrawal rates
│   │   └── product.rs            # Product features (SC, payout factors)
│   ├── projection/
│   │   ├── mod.rs
│   │   ├── engine.rs             # Core projection loop
│   │   ├── state.rs              # Projection state tracking
│   │   └── cashflows.rs          # Cashflow calculations
│   ├── reserves/
│   │   ├── mod.rs
│   │   ├── discount.rs           # Discounting utilities
│   │   └── metrics.rs            # Reserve calculations
│   ├── assets/
│   │   ├── mod.rs
│   │   ├── data.rs               # Asset data structures
│   │   ├── fixed.rs              # Fixed rate securities
│   │   ├── floating.rs           # Floating rate securities
│   │   └── portfolio.rs          # Portfolio analytics
│   ├── scenarios/
│   │   ├── mod.rs
│   │   ├── config.rs             # Scenario configuration
│   │   └── runner.rs             # Multi-scenario execution
│   └── optimization/
│       ├── mod.rs
│       └── portfolio.rs          # Portfolio optimization
├── tests/
│   ├── single_policy_test.rs     # Validation vs Excel
│   ├── batch_projection_test.rs
│   └── integration_tests.rs
├── data/
│   ├── mortality_tables/
│   ├── inforce/
│   └── scenarios/
└── docs/
    └── api.md
```

---

## Success Criteria by Phase

| Phase | Key Deliverable | Validation |
|-------|----------------|------------|
| 1 | Single policy projection | Match Excel "Calculation example" within 6 decimal places |
| 2 | Multi-policy batch | Process 2,800 policies, match aggregate cashflows |
| 3 | Reserve calculations | Validated PV calculations, API documentation |
| 4 | Asset modeling | Track buys/sells/income, balance sheet reconciliation |
| 5 | Fixed/floating securities | Price bonds correctly, handle floating resets |
| 6 | Portfolio analytics | Duration, yield, credit metrics validated |
| 7 | Multi-scenario | Run 100+ scenarios, correct aggregation |
| 8 | Decision making | Automated rebalancing triggers correctly |
| 9 | Production ready | AWS deployed, <1s single policy, <10s full inforce |

---

## Immediate Next Steps (Phase 1)

1. **Initialize Cargo project** with appropriate dependencies
2. **Create Policy struct** matching Excel column headers
3. **Implement mortality lookup** with IAM 2012 table and improvement
4. **Build surrender charge lookup** by policy year
5. **Implement lapse model** with all polynomial and interaction terms
6. **Create projection loop** for single policy
7. **Validate against Excel** row by row
