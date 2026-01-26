# Actuarial System v2 - Development Progress

## Overview

Building a Rust-based actuarial projection system for Fixed Indexed Annuity (FIA) products with GLWB riders. The goal is to match outputs from an Excel reference model ("Trellis - Reference Rate Calculator").

## Current Status: Phase 1 - Single Policy Projection ✓

Successfully implemented single-policy monthly projection with all major decrements matching the Excel reference.

### Validated Decrements

| Decrement | Status | Notes |
|-----------|--------|-------|
| Mortality | ✓ | IAM 2012 with age/gender-varying improvement rates |
| Lapse | ✓ | Predictive model with ITM-ness, shock year skew |
| PWD (Partial Withdrawals) | ✓ | FPW% incorporates RMD for qualified contracts |
| Rider Charges | ✓ | Applied to benefit base, monthly rate |
| Surrender Charges | ✓ | 10-year schedule, applied to lapse decrements |
| Interest Credits | ✓ | Annual indexed crediting |
| Benefit Base Rollup | ✓ | 10% simple interest, multiplicative formula |

### Key Fixes Applied

1. **Attained Age Calculation**: Fixed to increment at policy year boundaries (month 13, 25, etc.)
   - Formula: `issue_age + policy_year - 1`

2. **FPW% with RMD**: For qualified contracts, FPW% = MAX(base_free_pct, RMD_rate)
   - RMD rates start at age 73

3. **Benefit Base Rollup**: 10% simple interest on premium, applied multiplicatively
   - Formula: `BB * (1 + Bonus + 0.1*PY) / (1 + Bonus + 0.1*(PY-1))`
   - Applied to persisted benefit base (after decrements)

4. **Mortality Improvement**: Rates vary by age AND gender (not constant 1.5%)
   - Peak ~1.5% for males age 60-80, declining at older ages

## File Structure

```
src/
├── lib.rs              # Library exports
├── main.rs             # CLI entry point
├── scenario.rs         # ScenarioRunner for batch projections
├── policy/
│   ├── mod.rs          # Policy struct and enums
│   └── data.rs         # Policy year/age calculations
├── assumptions/
│   ├── mod.rs          # Assumptions container
│   ├── loader.rs       # CSV file loading
│   ├── mortality.rs    # Mortality tables and improvement
│   ├── lapse.rs        # Predictive lapse model
│   ├── product.rs      # Product features, payout factors
│   └── pwd.rs          # PWD, RMD, free withdrawal
└── projection/
    ├── mod.rs          # Engine exports
    ├── engine.rs       # Main projection loop
    └── state.rs        # Projection state tracking

data/
├── assumptions/        # CSV assumption files
│   ├── mortality_base_rates.csv
│   ├── mortality_improvement.csv
│   ├── mortality_age_factors.csv
│   ├── surrender_charges.csv
│   ├── rmd_rates.csv
│   ├── free_withdrawal_util.csv
│   ├── payout_factors.csv
│   └── surrender_predictive_model.csv
└── sample_inforce.csv  # Test policies

cashflow_examples/      # Excel reference outputs
├── output_10.csv
├── output_100.csv
├── output_200.csv
├── output_1450.csv
├── output_2000.csv
└── output_2800.csv
```

## Test Policies

| ID | Qual | Age | Gender | Initial AV | GLWB Start | Notes |
|----|------|-----|--------|------------|------------|-------|
| 10 | N | 57 | Female | 356.37 | Year 5 | Young, NQ |
| 100 | N | 57 | Female | 2,481.13 | Year 8 | Young, NQ |
| 200 | N | 57 | Male | 803.78 | Year 2 | Early GLWB |
| 1450 | Q | 57 | Female | 222,602.25 | Year 12 | Large policy |
| 2000 | Q | 67 | Female | 80,841.03 | Year 6 | Mid-age, Q |
| 2800 | Q | 77 | Male | 20,906.28 | Never | Reference policy |

## Usage

### Basic Projection

```rust
use actuarial_system::{Policy, Assumptions, ProjectionEngine, ProjectionConfig};

let policy = Policy::new(/* ... */);
let assumptions = Assumptions::default_pricing();
let config = ProjectionConfig {
    projection_months: 360,
    crediting: CreditingApproach::IndexedAnnual { annual_rate: 0.0378 },
    detailed_output: true,
    treasury_change: 0.0,
    fixed_lapse_rate: None,  // Use predictive model
};

let engine = ProjectionEngine::new(assumptions, config);
let result = engine.project_policy(&policy);
```

### Batch Scenario Testing with ScenarioRunner

For efficient iteration over multiple scenarios, use `ScenarioRunner` to pre-load assumptions once:

```rust
use actuarial_system::ScenarioRunner;

// Load assumptions once from CSV (or use ::new() for in-memory defaults)
let runner = ScenarioRunner::from_csv()?;

// Run many scenarios efficiently (clone is ~1000x faster than CSV reload)
let configs = vec![
    ProjectionConfig { /* scenario 1 */ },
    ProjectionConfig { /* scenario 2 */ },
    ProjectionConfig { /* scenario 3 */ },
];

// Single policy, multiple configs
let results = runner.run_scenarios(&policy, &configs);

// Multiple policies, same config
let results = runner.run_batch(&policies, config);

// Modify assumptions for sensitivity testing
let mut runner = ScenarioRunner::from_csv()?;
runner.assumptions_mut().mortality.set_improvement_rate(0.02);
```

### Performance Characteristics

| Operation | Time |
|-----------|------|
| CSV load | ~0.37ms |
| In-memory default | ~0.001ms |
| Clone assumptions | ~0.0003ms |
| Full 360-month projection | ~1ms (release) |

## Next Steps

### Phase 2: Multi-Policy Projection ✓
- [x] Load policies from CSV
- [x] Aggregate cashflows across cohort
- [x] Parallel projection support (via rayon)

### Phase 3: Reserve Calculations (In Progress)
- [x] Reserve module structure (`src/reserves/`)
- [x] Core types: `ReserveResult`, `PolicyState`, `ReserveComponents`
- [x] Brute force CARVM solver (tests all activation times)
- [x] Roll-forward caching for efficient multi-timestep calculations
- [x] Separate death benefit and elective benefit calculations
- [x] `ReserveCalcConfig` toggle in `ProjectionConfig`
- [x] 22 reserve-specific tests passing
- [ ] Dynamic programming solver (currently falls back to brute force)
- [ ] AG33/AG35 specific logic
- [ ] Integration with full projection state tracking
- [ ] Validation against reference reserves

### Phase 4: Asset Modeling
- [ ] Portfolio representation
- [ ] Hedge strategy modeling
- [ ] ALM analytics

### Phase 5: VM-22 (Future)
- [ ] Economic scenario generation
- [ ] Stochastic scenario infrastructure
- [ ] Company assumption framework

## Validation Approach

1. Run Rust projection for a policy
2. Compare against Excel reference output (cashflow_examples/)
3. Use "implied rates" analysis: `1 - (1 - decrement/BOPAV)^12` to isolate rate issues
4. Check key months: 1, 12, 13 (year boundary), 24, 60, 120, 132 (shock year)

## Known Differences

- Lives column may show <1% difference due to floating-point accumulation
- All other decrements match to 6+ decimal places
