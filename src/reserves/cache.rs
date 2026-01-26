//! Roll-forward caching for efficient multi-timestep reserve calculations
//!
//! The key insight is that once we solve for the optimal activation time T* at time 0,
//! subsequent reserves can often be derived without re-solving the full optimization:
//!
//! - Before T*: Roll forward the reserve (adjust for time value and mortality)
//! - At/After T*: Simple PV of remaining income stream
//! - Always: CSV is a floor
//!
//! This can provide ~30x speedup for monthly reserve calculations.

use serde::{Deserialize, Serialize};

/// Cached optimal path information for efficient roll-forward
///
/// Stores the results of a full CARVM solve so that subsequent valuations
/// can use roll-forward logic instead of re-solving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedReservePath {
    /// Policy identifier
    pub policy_id: u64,

    /// Projection month when we last did full solve
    pub solve_month: u32,

    /// Optimal income activation month from full solve
    /// u32::MAX indicates "never activate" is optimal
    pub optimal_activation_month: u32,

    /// Reserve value at solve time
    pub reserve_at_solve: f64,

    // ---- State at solve time (for validation) ----

    /// Account value at solve time
    pub av_at_solve: f64,

    /// Benefit base at solve time
    pub bb_at_solve: f64,

    /// ITM-ness (BB/AV) at solve time
    pub itm_at_solve: f64,

    /// Surrender charge rate at solve time
    pub sc_rate_at_solve: f64,

    // ---- Pre-computed values for the optimal path ----

    /// Monthly income amount once activated
    pub monthly_income_amount: f64,

    /// PV of remaining death benefits from solve time (along optimal path)
    pub death_benefit_pv_remaining: f64,

    // ---- For products with free PWD optimization ----

    /// Optimal PWD schedule (if free PWD is part of optimal path)
    pub optimal_pwd_schedule: Option<Vec<f64>>,

    /// Remaining free withdrawal amount at solve time
    pub remaining_free_amount_at_solve: f64,
}

impl CachedReservePath {
    /// Create a new cache entry from a full solve result
    pub fn new(
        policy_id: u64,
        solve_month: u32,
        optimal_activation_month: u32,
        reserve: f64,
        av: f64,
        bb: f64,
        monthly_income: f64,
        death_pv: f64,
        sc_rate: f64,
    ) -> Self {
        Self {
            policy_id,
            solve_month,
            optimal_activation_month,
            reserve_at_solve: reserve,
            av_at_solve: av,
            bb_at_solve: bb,
            itm_at_solve: if av > 0.0 { bb / av } else { f64::MAX },
            sc_rate_at_solve: sc_rate,
            monthly_income_amount: monthly_income,
            death_benefit_pv_remaining: death_pv,
            optimal_pwd_schedule: None,
            remaining_free_amount_at_solve: av * 0.10, // Typical 10% free
        }
    }

    /// Check if the cache is still potentially valid for the given month
    pub fn is_potentially_valid(&self, current_month: u32) -> bool {
        // Cache is only valid for months after the solve month
        current_month >= self.solve_month
    }

    /// Calculate months elapsed since full solve
    pub fn months_since_solve(&self, current_month: u32) -> u32 {
        current_month.saturating_sub(self.solve_month)
    }

    /// Check if we're past the optimal activation time
    pub fn past_optimal_activation(&self, current_month: u32) -> bool {
        current_month >= self.optimal_activation_month
    }

    /// Check if we're approaching the optimal activation time
    pub fn approaching_activation(&self, current_month: u32, threshold_months: u32) -> bool {
        let months_to_activation = self.optimal_activation_month.saturating_sub(current_month);
        months_to_activation <= threshold_months
    }
}

/// Result of attempting to roll forward a cached reserve
#[derive(Debug, Clone)]
pub enum RollForwardResult {
    /// Successfully rolled forward
    Success {
        /// Rolled forward reserve value
        reserve: f64,

        /// Whether the cache is still considered valid
        /// If false, a full re-solve is recommended soon
        still_valid: bool,

        /// Reason for any validation concerns
        validation_notes: Option<String>,
    },

    /// Could not roll forward, need full re-solve
    NeedsResolve {
        /// Reason why roll-forward failed
        reason: String,
    },
}

/// Criteria for determining when to re-solve vs roll forward
#[derive(Debug, Clone)]
pub struct RevalidationCriteria {
    /// Re-solve every N months regardless
    pub periodic_revalidation_months: u32,

    /// Re-solve if ITM changes by more than this fraction
    pub itm_change_threshold: f64,

    /// Re-solve if within N months of optimal activation
    pub activation_proximity_months: u32,

    /// Re-solve if AV changed by more than this fraction from expected
    pub av_deviation_threshold: f64,

    /// Re-solve if surrender charge period boundary crossed
    pub check_sc_boundaries: bool,
}

impl Default for RevalidationCriteria {
    fn default() -> Self {
        Self {
            periodic_revalidation_months: 12,
            itm_change_threshold: 0.10, // 10% change in ITM
            activation_proximity_months: 6,
            av_deviation_threshold: 0.15, // 15% deviation from expected AV
            check_sc_boundaries: true,
        }
    }
}

impl RevalidationCriteria {
    /// Check if revalidation (full re-solve) is needed
    pub fn needs_revalidation(
        &self,
        cached: &CachedReservePath,
        current_month: u32,
        current_av: f64,
        current_bb: f64,
        _current_sc_period: u32,
    ) -> Option<String> {
        // 1. Periodic revalidation
        let months_elapsed = current_month.saturating_sub(cached.solve_month);
        if months_elapsed >= self.periodic_revalidation_months {
            return Some(format!(
                "Periodic revalidation: {} months since last solve",
                months_elapsed
            ));
        }

        // 2. ITM change
        let current_itm = if current_av > 0.0 {
            current_bb / current_av
        } else {
            f64::MAX
        };
        let itm_change = (current_itm - cached.itm_at_solve).abs() / cached.itm_at_solve.max(0.01);
        if itm_change > self.itm_change_threshold {
            return Some(format!(
                "ITM changed by {:.1}% (threshold: {:.1}%)",
                itm_change * 100.0,
                self.itm_change_threshold * 100.0
            ));
        }

        // 3. Approaching optimal activation
        if cached.approaching_activation(current_month, self.activation_proximity_months) {
            return Some(format!(
                "Within {} months of optimal activation",
                self.activation_proximity_months
            ));
        }

        // 4. AV deviation from expected
        // (This would require projecting expected AV, simplified here)
        let av_change = (current_av - cached.av_at_solve).abs() / cached.av_at_solve.max(1.0);
        if av_change > self.av_deviation_threshold {
            return Some(format!(
                "AV changed by {:.1}% from solve time",
                av_change * 100.0
            ));
        }

        // 5. Surrender charge boundary (would need to track policy year)
        // Simplified: check if SC rate changed significantly
        // This would be implemented with actual SC lookup

        None // No revalidation needed
    }
}

/// Cache manager for multiple policies
#[derive(Debug, Default)]
pub struct ReserveCache {
    /// Cached paths by policy ID
    entries: std::collections::HashMap<u64, CachedReservePath>,

    /// Revalidation criteria
    criteria: RevalidationCriteria,

    /// Statistics
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub revalidations: u64,
}

impl ReserveCache {
    /// Create a new cache with default criteria
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new cache with custom criteria
    pub fn with_criteria(criteria: RevalidationCriteria) -> Self {
        Self {
            criteria,
            ..Default::default()
        }
    }

    /// Get cached path for a policy
    pub fn get(&self, policy_id: u64) -> Option<&CachedReservePath> {
        self.entries.get(&policy_id)
    }

    /// Store a cached path for a policy
    pub fn insert(&mut self, path: CachedReservePath) {
        self.entries.insert(path.policy_id, path);
    }

    /// Remove cached path for a policy
    pub fn remove(&mut self, policy_id: u64) -> Option<CachedReservePath> {
        self.entries.remove(&policy_id)
    }

    /// Clear all cached data
    pub fn clear(&mut self) {
        self.entries.clear();
        self.cache_hits = 0;
        self.cache_misses = 0;
        self.revalidations = 0;
    }

    /// Get number of cached entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Record a cache hit
    pub fn record_hit(&mut self) {
        self.cache_hits += 1;
    }

    /// Record a cache miss
    pub fn record_miss(&mut self) {
        self.cache_misses += 1;
    }

    /// Record a revalidation
    pub fn record_revalidation(&mut self) {
        self.revalidations += 1;
    }

    /// Get cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cached_reserve_path_creation() {
        let cache = CachedReservePath::new(
            12345,      // policy_id
            0,          // solve_month
            96,         // optimal_activation_month (year 8)
            50_000.0,   // reserve
            100_000.0,  // av
            130_000.0,  // bb
            1_000.0,    // monthly_income
            5_000.0,    // death_pv
            0.08,       // sc_rate
        );

        assert_eq!(cache.policy_id, 12345);
        assert_eq!(cache.optimal_activation_month, 96);
        assert!((cache.itm_at_solve - 1.3).abs() < 0.001);
    }

    #[test]
    fn test_approaching_activation() {
        let cache = CachedReservePath::new(1, 0, 96, 50000.0, 100000.0, 130000.0, 1000.0, 5000.0, 0.08);

        // Month 90: 6 months away, should be approaching
        assert!(cache.approaching_activation(90, 6));

        // Month 80: 16 months away, not approaching
        assert!(!cache.approaching_activation(80, 6));

        // Month 100: past activation
        assert!(cache.past_optimal_activation(100));
    }

    #[test]
    fn test_revalidation_criteria() {
        let criteria = RevalidationCriteria::default();
        let cache = CachedReservePath::new(1, 0, 96, 50000.0, 100000.0, 130000.0, 1000.0, 5000.0, 0.08);

        // Should trigger periodic revalidation
        assert!(criteria
            .needs_revalidation(&cache, 13, 100000.0, 130000.0, 10)
            .is_some());

        // Should not trigger at month 6
        assert!(criteria
            .needs_revalidation(&cache, 6, 100000.0, 130000.0, 10)
            .is_none());
    }

    #[test]
    fn test_reserve_cache() {
        let mut cache = ReserveCache::new();

        let path = CachedReservePath::new(1, 0, 96, 50000.0, 100000.0, 130000.0, 1000.0, 5000.0, 0.08);
        cache.insert(path);

        assert_eq!(cache.len(), 1);
        assert!(cache.get(1).is_some());
        assert!(cache.get(999).is_none());

        cache.clear();
        assert!(cache.is_empty());
    }
}
