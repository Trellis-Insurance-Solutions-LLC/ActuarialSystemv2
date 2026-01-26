# Seriatim Reserve Implementation Plans

## Executive Summary

This document outlines the implementation approach for seriatim (policy-by-policy) reserve calculations under multiple regulatory frameworks:
1. **CARVM/AG33/AG35** - Current statutory regime (deterministic, prescribed assumptions)
2. **VM-22** - Principles-based reserves (stochastic, company assumptions)

**Chosen Approach**: Plan A (Sequential Build) with Dynamic Programming + Roll-Forward Caching

---

## Understanding the Problem

### What Makes Reserve Calculation Hard

For a GLWB-equipped FIA, the reserve at any point is NOT simply "PV of future benefits." It's the answer to:

> "What is the maximum liability we could face if the policyholder acts optimally?"

This creates a **maximization problem** where we must:
1. Consider all possible policyholder decision paths (when to start income, when to surrender)
2. For each path, calculate PV of guaranteed benefits
3. Take the maximum across all paths

### Key Dimensions of Complexity

| Dimension | CARVM/AG33/AG35 | VM-22 |
|-----------|-----------------|-------|
| Policyholder behavior | Prescribed (optimal) | Company assumption |
| Discount rates | Prescribed (valuation rate) | Scenario-dependent |
| Economic scenarios | Single deterministic | Multiple stochastic |
| Asset modeling | Not required | Required |

### The Death Benefit / Discount Rate Challenge

AG33/AG35 require different discount rates for different benefit types:
- **Non-elective benefits** (death): Mortality-weighted, may use different rate
- **Elective benefits** (income, surrender): Valuation interest rate

This complicates dynamic programming because terms aren't in the same "units." Our solution:
**Separation of Concerns** - Calculate elective and non-elective benefit streams separately, then combine.

---

## Chosen Implementation: Plan A with Optimizations

### Philosophy
Build CARVM first as a foundation with:
1. **Dynamic programming** for the core optimal path algorithm
2. **Roll-forward caching** for efficient multi-timestep reserve calculations
3. **Separation of death benefit and elective benefit calculations**

### GLWB Optimal Behavior Pattern

For GLWB products, optimal behavior follows a predictable structure:

```
Time: 0 -------- T* -------- ∞
      |   Accumulate   |   Take Income   |
      └── CSV check ───┴── CSV check ────┘
```

Where T* = optimal income activation time. Key insight:
- Before T*: Continue accumulating (unless CSV > reserve)
- At T*: Activate income
- After T*: If still accumulating, activate immediately
- Always: CSV is a floor (cheap to calculate)

This allows **memoization**: solve T* once at time 0, then roll forward subsequent reserves.

---

## Implementation Phases

### Phase 1: Reserve Module Foundation

#### 1.1 New Module Structure
```
src/reserves/
├── mod.rs              # Module exports
├── types.rs            # Core types: ReserveResult, PolicyState, BenefitStream
├── discount.rs         # Discount curve handling
├── carvm.rs            # CARVM calculator (brute force + DP)
├── cache.rs            # Roll-forward caching for efficiency
└── benefits.rs         # Benefit stream calculations (death, income, surrender)
```

#### 1.2 Core Types

```rust
/// State of a policy for reserve calculation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyState {
    Accumulation,           // Pre-income, can elect income or surrender
    IncomeActive,           // Taking GLWB withdrawals
    Surrendered,            // Contract terminated
    Matured,                // Death or contract maturity
}

/// Projection state for reserve calculation
/// Extends existing ProjectionState with reserve-specific fields
#[derive(Debug, Clone)]
pub struct ReserveProjectionState {
    pub month: u32,
    pub policy_state: PolicyState,
    pub account_value: f64,
    pub benefit_base: f64,
    pub cumulative_withdrawals: f64,
    pub remaining_free_amount: f64,  // For products where free PWD may be optimal
    pub survival_probability: f64,   // Cumulative survival from t=0
}

/// Result of a reserve calculation
#[derive(Debug, Clone)]
pub struct ReserveResult {
    pub policy_id: u32,
    pub valuation_date: u32,         // Month of valuation
    pub gross_reserve: f64,          // Before any adjustments
    pub net_reserve: f64,            // After reinsurance, etc.
    pub optimal_activation_month: u32, // T* from optimization
    pub reserve_components: ReserveComponents,
    pub method: ReserveMethod,
}

/// Breakdown of reserve by benefit type
#[derive(Debug, Clone)]
pub struct ReserveComponents {
    pub death_benefit_pv: f64,       // PV of guaranteed death benefits
    pub income_benefit_pv: f64,      // PV of GLWB income stream
    pub surrender_value_pv: f64,     // CSV component (if binding)
    pub elective_benefit_pv: f64,    // Combined elective
}

/// Method used for calculation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveMethod {
    CARVM,
    AG33,
    AG35Type1,
    AG35Type2,
    VM22 { scenario_id: u32 },
}
```

#### 1.3 Cached Reserve Path (for Roll-Forward Optimization)

```rust
/// Cached optimal path information for efficient roll-forward
#[derive(Debug, Clone)]
pub struct CachedReservePath {
    pub policy_id: u64,
    pub solve_month: u32,               // When we last did full solve
    pub optimal_activation_month: u32,  // T* from full solve
    pub reserve_at_solve: f64,          // R(solve_month)

    // State at solve time (for validation)
    pub av_at_solve: f64,
    pub bb_at_solve: f64,
    pub itm_at_solve: f64,              // BB/AV ratio

    // Pre-computed values for the optimal path
    pub monthly_income_amount: f64,     // Once activated
    pub death_benefit_pv_remaining: f64,

    // For products with free PWD optimization
    pub optimal_pwd_schedule: Option<Vec<f64>>,
    pub remaining_free_amount_at_solve: f64,
}

/// Result of attempting to roll forward vs needing full re-solve
pub enum RollForwardResult {
    Success {
        reserve: f64,
        still_valid: bool,  // Should we re-validate soon?
    },
    NeedsResolve {
        reason: String,
    },
}
```

### Phase 2: CARVM Calculator

#### 2.1 Main Calculator Structure

```rust
/// Configuration for CARVM reserve calculation
#[derive(Debug, Clone)]
pub struct CARVMConfig {
    pub method: CARVMMethod,
    pub max_projection_months: u32,
    pub use_caching: bool,
    pub revalidation_frequency: u32,  // Months between full re-solves
}

#[derive(Debug, Clone, Copy)]
pub enum CARVMMethod {
    BruteForce,              // O(T × N) - guaranteed correct
    DynamicProgramming,      // O(N) - faster but more complex
    Hybrid,                  // DP with brute-force validation
}

/// Main CARVM calculator
pub struct CARVMCalculator {
    assumptions: Assumptions,
    config: CARVMConfig,
    projection_engine: ProjectionEngine,
    cache: HashMap<u64, CachedReservePath>,
}

impl CARVMCalculator {
    /// Calculate reserve for a policy at a given valuation month
    pub fn calculate_reserve(
        &mut self,
        policy: &Policy,
        valuation_month: u32,
    ) -> ReserveResult {
        // Step 1: Check cache
        if self.config.use_caching {
            if let Some(cached) = self.cache.get(&(policy.policy_id as u64)) {
                match self.try_roll_forward(policy, valuation_month, cached) {
                    RollForwardResult::Success { reserve, still_valid } => {
                        let csv = self.cash_surrender_value(policy, valuation_month);
                        let final_reserve = reserve.max(csv);

                        // Periodic re-validation
                        if !still_valid || self.needs_revalidation(valuation_month, cached) {
                            return self.full_solve_and_cache(policy, valuation_month);
                        }

                        return ReserveResult {
                            policy_id: policy.policy_id,
                            valuation_date: valuation_month,
                            gross_reserve: final_reserve,
                            net_reserve: final_reserve,
                            optimal_activation_month: cached.optimal_activation_month,
                            reserve_components: self.estimate_components(final_reserve, cached),
                            method: ReserveMethod::CARVM,
                        };
                    }
                    RollForwardResult::NeedsResolve { .. } => {
                        // Fall through to full solve
                    }
                }
            }
        }

        // Step 2: Full solve
        self.full_solve_and_cache(policy, valuation_month)
    }

    /// Full CARVM optimization - find optimal activation time
    fn full_solve(&self, policy: &Policy, valuation_month: u32) -> (u32, f64, ReserveComponents) {
        match self.config.method {
            CARVMMethod::BruteForce => self.brute_force_solve(policy, valuation_month),
            CARVMMethod::DynamicProgramming => self.dp_solve(policy, valuation_month),
            CARVMMethod::Hybrid => {
                let dp_result = self.dp_solve(policy, valuation_month);
                // Validate with brute force for first N policies or periodically
                dp_result
            }
        }
    }
}
```

#### 2.2 Brute Force Implementation

```rust
impl CARVMCalculator {
    /// Brute force: try all possible activation times
    fn brute_force_solve(
        &self,
        policy: &Policy,
        valuation_month: u32,
    ) -> (u32, f64, ReserveComponents) {
        let mut best_reserve = 0.0;
        let mut best_activation = 0;
        let mut best_components = ReserveComponents::default();

        let max_deferral = self.max_deferral_months(policy);

        // Try each possible activation month
        for activation_month in valuation_month..=max_deferral {
            let (reserve, components) = self.calculate_reserve_for_path(
                policy,
                valuation_month,
                Some(activation_month),
            );

            if reserve > best_reserve {
                best_reserve = reserve;
                best_activation = activation_month;
                best_components = components;
            }
        }

        // Also try "never activate" (pure surrender path)
        let (surrender_reserve, surrender_components) = self.calculate_reserve_for_path(
            policy,
            valuation_month,
            None,  // Never activate
        );

        if surrender_reserve > best_reserve {
            best_reserve = surrender_reserve;
            best_activation = u32::MAX;  // Sentinel for "never"
            best_components = surrender_components;
        }

        // CSV is always a floor
        let csv = self.cash_surrender_value(policy, valuation_month);
        if csv > best_reserve {
            best_reserve = csv;
            best_components = ReserveComponents {
                surrender_value_pv: csv,
                ..Default::default()
            };
        }

        (best_activation, best_reserve, best_components)
    }

    /// Calculate reserve for a specific activation path
    fn calculate_reserve_for_path(
        &self,
        policy: &Policy,
        valuation_month: u32,
        activation_month: Option<u32>,
    ) -> (f64, ReserveComponents) {
        // Project policy forward from valuation_month
        // with income activating at activation_month (or never if None)

        // Separate calculations for death benefits and elective benefits
        // to handle different discount rates properly

        let death_pv = self.calculate_death_benefit_pv(policy, valuation_month, activation_month);
        let elective_pv = self.calculate_elective_benefit_pv(policy, valuation_month, activation_month);

        let total = death_pv + elective_pv;

        (total, ReserveComponents {
            death_benefit_pv: death_pv,
            income_benefit_pv: elective_pv, // Simplification - would break out further
            surrender_value_pv: 0.0,
            elective_benefit_pv: elective_pv,
        })
    }
}
```

#### 2.3 Death Benefit Calculation (Separate from Elective)

```rust
impl CARVMCalculator {
    /// Calculate PV of death benefits along a path
    /// This is NON-ELECTIVE, so we use mortality-weighted discounting
    fn calculate_death_benefit_pv(
        &self,
        policy: &Policy,
        valuation_month: u32,
        activation_month: Option<u32>,
    ) -> f64 {
        let mut death_pv = 0.0;
        let mut survival_prob = 1.0;
        let v = 1.0 / (1.0 + policy.val_rate / 12.0);

        for t in valuation_month..self.config.max_projection_months {
            let state = if activation_month.map_or(false, |am| t >= am) {
                PolicyState::IncomeActive
            } else {
                PolicyState::Accumulation
            };

            let attained_age = policy.attained_age(t);
            let q = self.assumptions.mortality.monthly_rate(attained_age, policy.gender, t);

            // Death benefit depends on state
            let db = self.death_benefit_amount(policy, t, state);

            // PV contribution: survival to t × probability of death at t × DB × discount
            death_pv += survival_prob * q * db * v.powi((t - valuation_month) as i32);

            survival_prob *= 1.0 - q;

            if survival_prob < 1e-10 {
                break;  // Everyone has died
            }
        }

        death_pv
    }

    /// Death benefit amount depends on policy state
    fn death_benefit_amount(&self, policy: &Policy, month: u32, state: PolicyState) -> f64 {
        // For this GLWB product: DB = max(0, BB - AV) typically
        // Actual formula depends on product design
        // TODO: Get actual formula from product spec
        0.0  // Placeholder
    }
}
```

#### 2.4 Roll-Forward Logic

```rust
impl CARVMCalculator {
    /// Try to roll forward from cached reserve
    fn try_roll_forward(
        &self,
        policy: &Policy,
        valuation_month: u32,
        cached: &CachedReservePath,
    ) -> RollForwardResult {
        let t_star = cached.optimal_activation_month;

        // Case A: Still in accumulation, before optimal activation
        if valuation_month < t_star {
            let rolled_reserve = self.roll_accumulation_reserve(
                cached.reserve_at_solve,
                policy,
                cached.solve_month,
                valuation_month,
            );

            // Quick validation: has ITM changed dramatically?
            let current_itm = self.calculate_itm(policy, valuation_month);
            let still_valid = (current_itm - cached.itm_at_solve).abs() < 0.10;

            return RollForwardResult::Success {
                reserve: rolled_reserve,
                still_valid,
            };
        }

        // Case B: At or past optimal activation time
        if valuation_month >= t_star {
            // Should activate now - simpler calculation
            let income_pv = self.income_pv_if_activate_now(policy, valuation_month);
            let death_pv = self.calculate_death_benefit_pv(
                policy,
                valuation_month,
                Some(valuation_month),
            );

            return RollForwardResult::Success {
                reserve: income_pv + death_pv,
                still_valid: true,
            };
        }

        RollForwardResult::NeedsResolve {
            reason: "Unexpected state".into(),
        }
    }

    /// Roll reserve forward through accumulation period
    fn roll_accumulation_reserve(
        &self,
        r_prev: f64,
        policy: &Policy,
        t_prev: u32,
        t_now: u32,
    ) -> f64 {
        let mut reserve = r_prev;
        let v = 1.0 / (1.0 + policy.val_rate / 12.0);

        for t in t_prev..t_now {
            let attained_age = policy.attained_age(t);
            let q = self.assumptions.mortality.monthly_rate(attained_age, policy.gender, t);
            let p = 1.0 - q;

            let db_cost = q * self.death_benefit_amount(policy, t, PolicyState::Accumulation);

            // Roll forward: R(t+1) = [R(t) - DB_cost(t)] / (p × v)
            reserve = (reserve - db_cost) / (p * v);
        }

        reserve
    }

    /// Check if we should do a full re-solve
    fn needs_revalidation(&self, current_month: u32, cached: &CachedReservePath) -> bool {
        // 1. Periodic re-validation
        if current_month - cached.solve_month >= self.config.revalidation_frequency {
            return true;
        }

        // 2. Close to optimal activation time (higher sensitivity)
        let months_to_activation = cached.optimal_activation_month.saturating_sub(current_month);
        if months_to_activation <= 6 {
            return true;
        }

        false
    }
}
```

### Phase 3: Integration with Existing Projection Engine

#### 3.1 Reserve-Aware Projection Config

```rust
/// Extended projection config for reserve calculations
#[derive(Debug, Clone)]
pub struct ReserveProjectionConfig {
    /// Base projection config
    pub base: ProjectionConfig,

    /// Reserve calculation mode
    pub reserve_mode: ReserveMode,

    /// Discount curve for reserve calculations
    pub discount_curve: DiscountCurve,

    /// Valuation date (month)
    pub valuation_month: u32,

    /// Force income activation at specific month (for path testing)
    pub forced_activation_month: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum ReserveMode {
    /// Standard pricing projection (no reserve calculation)
    Pricing,

    /// CARVM reserve calculation
    CARVM {
        method: CARVMMethod,
        use_caching: bool,
    },

    /// VM-22 reserve calculation
    VM22 {
        scenario_set: ScenarioSet,
        confidence_level: f64,
    },
}

#[derive(Debug, Clone)]
pub struct DiscountCurve {
    /// Valuation interest rate (from policy or prescribed)
    pub valuation_rate: f64,

    /// Optional: separate rate for death benefits
    pub death_benefit_rate: Option<f64>,

    /// Optional: spot rate curve for more precise discounting
    pub spot_rates: Option<Vec<f64>>,
}
```

#### 3.2 Using Existing Projection Engine

The CARVM calculator wraps the existing `ProjectionEngine` rather than duplicating logic:

```rust
impl CARVMCalculator {
    /// Project policy forward for reserve calculation
    fn project_for_reserve(
        &self,
        policy: &Policy,
        activation_month: Option<u32>,
    ) -> ProjectionResult {
        // Create a modified policy with forced activation
        let modified_policy = if let Some(am) = activation_month {
            let mut p = policy.clone();
            p.glwb_start_year = policy.policy_year(am);
            p
        } else {
            let mut p = policy.clone();
            p.glwb_start_year = 999;  // Never activate
            p
        };

        // Use existing projection engine
        self.projection_engine.project_policy(&modified_policy)
    }
}
```

---

## Performance Characteristics

### Expected Performance with Caching

| Phase | Full Solve Cost | Roll Forward Cost | Savings |
|-------|-----------------|-------------------|---------|
| Initial (t=0) | O(T_max × projection) | N/A | Baseline |
| Accumulation (t < T*) | O(T_max × projection) | O(1) per month | ~99% |
| Near T* | O(T_max × projection) | Re-solve | 0% |
| Income (t > T*) | O(projection) | O(1) | ~95% |

For a 30-year projection with monthly reserves on 10,000 policies:
- **Without caching**: 10,000 × 360 × full_solve = massive
- **With caching**: 10,000 × (1 full_solve + ~12 re-solves + 347 roll_forwards) = ~30x faster

### Validation Strategy

1. **Brute force as reference**: Implement brute force first, validate DP against it
2. **Cache validation**: Periodically re-solve and compare to rolled values
3. **Edge case testing**: Test policies with:
   - Very high ITM (income likely optimal)
   - Very low ITM (surrender likely optimal)
   - Edge ages (near RMD start, very old)
   - Different surrender charge periods

---

## File Structure

```
src/reserves/
├── mod.rs              # Module exports and ReserveCalculator trait
├── types.rs            # ReserveResult, PolicyState, ReserveComponents, etc.
├── discount.rs         # DiscountCurve, present value helpers
├── carvm.rs            # CARVMCalculator implementation
├── cache.rs            # CachedReservePath, roll-forward logic
├── benefits.rs         # Death benefit and elective benefit stream calculations
└── tests/
    ├── brute_force_tests.rs
    ├── dp_validation_tests.rs
    └── cache_tests.rs
```

---

## Implementation Status

### Completed ✅

#### Module Structure Created
All core files are implemented and compiling:
- `src/reserves/mod.rs` - Module exports and `ReserveCalculator` trait
- `src/reserves/types.rs` - Core types: `PolicyState`, `ReserveResult`, `ReserveComponents`, `ReserveMethod`
- `src/reserves/discount.rs` - `DiscountCurve` with separate elective/death benefit discount factors
- `src/reserves/cache.rs` - `CachedReservePath`, `RollForwardResult`, `RevalidationCriteria`, `ReserveCache`
- `src/reserves/benefits.rs` - `BenefitCalculator` with death/income/surrender calculations
- `src/reserves/carvm.rs` - `CARVMCalculator` with brute force solver and caching

#### Brute Force CARVM Solver
Fully implemented and tested:
- Tests all possible activation times from valuation month to max deferral
- Tests "never activate" path
- CSV always applied as floor
- Separate death benefit and elective benefit calculations
- Components tracking (death PV, income PV, surrender PV)

#### Roll-Forward Caching
Implemented with:
- `CachedReservePath` storing optimal activation time and state
- `RevalidationCriteria` with configurable thresholds:
  - Periodic revalidation (default: 12 months)
  - ITM change threshold (default: 10%)
  - Activation proximity (default: 6 months)
  - AV deviation threshold (default: 15%)
- Roll-forward logic for accumulation and income phases

#### Reserve Toggle in ProjectionConfig
Added `reserve_config: Option<ReserveCalcConfig>` to `ProjectionConfig`:
- `None` (default): Reserves OFF - fast cost-of-funds projections
- `Some(ReserveCalcConfig::quick())`: Brute force, limited projection
- `Some(ReserveCalcConfig::full())`: Hybrid method with caching

Result stored in `ProjectionResult.reserve_result: Option<ReserveResult>`.

```rust
// Fast path (default)
let config = ProjectionConfig { reserve_config: None, .. };

// With reserves
let config = ProjectionConfig {
    reserve_config: Some(ReserveCalcConfig::quick()),
    ..
};

// Access result
if let Some(reserve) = &result.reserve_result {
    println!("Reserve: {:.2}", reserve.gross_reserve);
}
```

#### Test Coverage (22 tests passing)
```
reserves::benefits::tests::test_csv_calculation
reserves::benefits::tests::test_death_benefit_amount
reserves::cache::tests::test_approaching_activation
reserves::cache::tests::test_cached_reserve_path_creation
reserves::cache::tests::test_reserve_cache
reserves::cache::tests::test_revalidation_criteria
reserves::carvm::tests::test_cache_behavior
reserves::carvm::tests::test_carvm_calculator_creation
reserves::carvm::tests::test_carvm_reserve_calculation
reserves::carvm::tests::test_csv_is_floor
reserves::carvm::tests::test_different_ages
reserves::carvm::tests::test_high_itm_vs_low_itm
reserves::carvm::tests::test_optimal_activation_within_bounds
reserves::carvm::tests::test_reserve_at_later_months
reserves::carvm::tests::test_reserve_components_sum
reserves::discount::tests::test_discount_factors
reserves::discount::tests::test_pv_annuity
reserves::discount::tests::test_separate_death_rate
reserves::discount::tests::test_single_rate_curve
reserves::types::tests::test_policy_state_default
reserves::types::tests::test_reserve_components_total
reserves::types::tests::test_reserve_projection_state_itm
```

### In Progress 🔄

#### Dynamic Programming Solver
Currently a placeholder that falls back to brute force. Full implementation would:
- Use backward recursion from max projection month
- Maintain separate death benefit and elective benefit tracks
- Apply Bellman equation for optimal activation decision
- Achieve O(N) complexity vs O(T × N) for brute force

### Pending 📋

#### Short Term
1. Complete DP solver implementation
2. Implement actual AV/BB state tracking through projection
3. Validate against reference reserve calculations
4. Add debug/tracing output for reserve optimization

#### Medium Term
1. AG33/AG35 specific logic (rider charge deductions, specific formulas)
2. Performance profiling and optimization
3. Batch processing optimization

#### Future
1. VM-22 scenario infrastructure
2. Asset-liability integration
3. Production deployment

---

## Open Questions

1. **Death benefit formula**: ✅ Confirmed - Death benefit = AV (no surrender charges). Benefit base is only used for GLWB income calculation.
2. **AG35 Type 1 vs Type 2**: Which method will be used? Type 2 requires hedging certification.
3. **Validation data**: Need reference reserve values to validate against (e.g., from existing Excel model).
4. **Performance targets**: How many policies, what runtime target per policy?
5. **State tracking**: Current implementation uses simplified state projection. Need to integrate with full projection engine for actual AV/BB values at each month.
