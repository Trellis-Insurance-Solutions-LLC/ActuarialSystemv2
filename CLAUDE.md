# Actuarial System v2 - Project Context

## Overview
High-performance Rust-based actuarial projection system for Fixed Indexed Annuity (FIA) products with Guaranteed Lifetime Withdrawal Benefit (GLWB) riders. Supports pricing, valuation, and multi-scenario projections.

## Current Status
- **Phase 1**: Complete - Single policy monthly projection with validated decrements
- **Phase 2**: Complete - Multi-policy batch projections with parallel processing
- **Phase 3**: In progress - Reserve calculations (CARVM foundation complete, DP solver pending)

## Key Documentation
Read these files for detailed context:
- `IMPLEMENTATION_PLAN.md` - Full project roadmap and architecture
- `PROGRESS.md` - Current development status and validated features
- `Reserve implementation.md` - Reserve calculation requirements and approach

## Reference Materials (`/reference_material/`)
**IMPORTANT**: These contain regulatory requirements that MUST be followed precisely.

| Document | Purpose |
|----------|---------|
| `AG33 and AG35 text.pdf` | CARVM for annuity contracts with elective benefits (AG33) and equity indexed annuities (AG35) |
| `VM22 full text.pdf` | Principles-based reserves for fixed annuities - stochastic framework |
| `2026_NAIC_Valuation_Manual_full_text.pdf` | Complete NAIC valuation manual, reference if there are questions on VM22 |

## Reserve Calculation Regimes

### CARVM (Commissioners Annuity Reserve Valuation Method)
- **Core concept**: Reserves = MAX over all future time points of PV(guaranteed benefits)
- **Key requirement**: Assumes "perfectly rational" policyholder behavior (optimal exercise)
- **Discount rates**: Prescribed by regulation, vary by decrement type
- **AG33 specifics**: Integrated Benefit Stream approach for contracts with elective benefits
- **AG35 specifics**: Type 1 (basic) vs Type 2 (requires "Hedged as Required" certification) computational methods

### VM-22 (Principles-Based Reserves)
- **Core concept**: Scenario-based, stochastic economic reserve framework
- **Key difference from CARVM**: Behavior and discount rates are company assumptions, not prescribed
- **Requires**: Multiple economic scenarios, asset modeling integration
- **Complexity**: Higher than CARVM due to scenario generation and asset-liability interaction

## Product Features (FIA with GLWB)

### Account Value Mechanics
- Monthly timesteps
- Interest credits: Annual indexed crediting (option budget + equity kicker approach OR actual scenario-based)
- Rider charges: 0.5% pre-activation, 1.5% post-activation (applied to benefit base)

### GLWB Rider
- Benefit base rollup: 10% simple interest on premium for 10 years
- Payout factors: Age and gender dependent
- Income activation: Policyholder elective
- Systematic withdrawals: Based on payout factor × benefit base

### Decrements
- **Mortality**: IAM 2012 with age/gender-varying improvement rates
- **Lapse**: Predictive model with ITM-ness, shock year skew, surrender charge period effects
- **Partial withdrawals**: Free withdrawal % (incorporates RMD for qualified contracts)

## Code Architecture

```
src/
├── policy/          # Policy data structures, loading, generation
├── assumptions/     # Mortality, lapse, PWD, product features
├── projection/      # Core projection engine, state tracking, cashflows
├── reserves/        # Reserve calculations (CARVM, AG33, AG35)
│   ├── mod.rs       # Module exports, ReserveCalculator trait, ReserveCalcConfig
│   ├── types.rs     # ReserveResult, PolicyState, ReserveComponents
│   ├── carvm.rs     # CARVMCalculator with brute force + caching
│   ├── cache.rs     # Roll-forward caching for efficiency
│   ├── benefits.rs  # Death benefit and income benefit PV calculations
│   └── discount.rs  # DiscountCurve handling
└── scenario.rs      # ScenarioRunner for batch projections

data/
├── assumptions/     # CSV assumption files (mortality, surrender charges, etc.)
└── sample_inforce.csv
```

## Key Formulas

### Attained Age
```
attained_age = issue_age + policy_year - 1
```
(Increments at policy year boundaries: months 13, 25, etc.)

### Benefit Base Rollup
```
BB(t+1) = BB(t) × (1 + Bonus + 0.1×PY) / (1 + Bonus + 0.1×(PY-1))
```

### CARVM Reserve (simplified)
```
Reserve(t) = MAX over all s >= t of: PV(guaranteed_benefits from t to s, discounted at valuation rate)
```

## Validation Approach
1. Compare against Excel reference outputs in `cashflow_examples/`
2. Use "implied rates" analysis to isolate rate issues
3. Target: Match to 6+ decimal places

## When Working on Reserves
1. **Always reference the regulatory text** in `/reference_material/` for compliance
2. **CARVM is a maximization problem** - must test all possible exercise timings
3. **Discount rates vary by decrement** - mortality uses different rate than voluntary termination
4. **AG35 computational methods** have specific requirements for "Hedged as Required" certification
5. **VM-22 requires scenario infrastructure** - don't attempt without economic scenario generator

## Reserve Calculation Usage

### Toggle Reserves On/Off
Reserve calculations are **off by default** for fast cost-of-funds projections:

```rust
// Default: reserves OFF (fast)
let config = ProjectionConfig {
    projection_months: 768,
    reserve_config: None,  // No reserve calculation
    // ...
};

// Enable reserves with quick mode (brute force, limited projection)
let config = ProjectionConfig {
    reserve_config: Some(ReserveCalcConfig::quick()),
    // ...
};

// Enable reserves with full mode (hybrid method, caching)
let config = ProjectionConfig {
    reserve_config: Some(ReserveCalcConfig::full()),
    // ...
};

// Custom valuation month
let config = ProjectionConfig {
    reserve_config: Some(ReserveCalcConfig::quick().at_month(12)),
    // ...
};
```

### Accessing Reserve Results
```rust
let result = engine.project_policy(&policy);

// Reserve result is None when reserve_config is None
if let Some(reserve) = &result.reserve_result {
    println!("Gross reserve: {:.2}", reserve.gross_reserve);
    println!("Optimal activation month: {}", reserve.optimal_activation_month);
    println!("CSV at valuation: {:.2}", reserve.csv_at_valuation);
    println!("Death benefit PV: {:.2}", reserve.reserve_components.death_benefit_pv);
}
```

### Product-Specific Notes
- **Death benefit** = Account Value (no surrender charges, benefit base not used)
- **GLWB income** = Benefit Base × Payout Rate (elective benefit)
- **CSV floor** = Always checked as alternative to income path

## Testing Commands
```bash
cargo test                           # Run all tests
cargo test reserves::                # Run reserve module tests only
cargo run --bin compare_excel        # Compare against Excel reference
cargo run --bin cost_of_funds        # Run cost of funds calculation (reserves off)
```
