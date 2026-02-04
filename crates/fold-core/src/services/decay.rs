//! Memory decay and retrieval strength calculation.
//!
//! Implements an ACT-R inspired decay model where memory strength decays
//! exponentially over time but is boosted by retrieval frequency.

use chrono::{DateTime, Utc};

/// Default half-life in days for memory decay.
pub const DEFAULT_HALF_LIFE_DAYS: f64 = 30.0;

/// Default weight for strength in score blending.
pub const DEFAULT_STRENGTH_WEIGHT: f64 = 0.3;

/// Minimum strength floor to prevent memories from becoming completely invisible.
pub const MIN_STRENGTH: f64 = 0.01;

/// Maximum strength cap.
pub const MAX_STRENGTH: f64 = 1.0;

/// Configuration for decay calculations.
#[derive(Debug, Clone)]
pub struct DecayConfig {
    /// Half-life in days for exponential decay.
    pub half_life_days: f64,
    /// Weight for blending strength with semantic score (0.0-1.0).
    pub strength_weight: f64,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            half_life_days: DEFAULT_HALF_LIFE_DAYS,
            strength_weight: DEFAULT_STRENGTH_WEIGHT,
        }
    }
}

impl DecayConfig {
    /// Create a new config with custom values.
    pub fn new(half_life_days: f64, strength_weight: f64) -> Self {
        Self {
            half_life_days: half_life_days.max(1.0),
            strength_weight: strength_weight.clamp(0.0, 1.0),
        }
    }

    /// Create config for pure semantic search (no decay weighting).
    pub fn pure_semantic() -> Self {
        Self {
            half_life_days: DEFAULT_HALF_LIFE_DAYS,
            strength_weight: 0.0,
        }
    }
}

/// Calculate the retrieval strength of a memory.
pub fn calculate_strength(
    updated_at: DateTime<Utc>,
    last_accessed: Option<DateTime<Utc>>,
    retrieval_count: i32,
    half_life_days: f64,
) -> f64 {
    let now = Utc::now();

    let base_time = match last_accessed {
        Some(accessed) if accessed > updated_at => accessed,
        _ => updated_at,
    };

    let duration = now.signed_duration_since(base_time);
    let days_elapsed = duration.num_seconds() as f64 / 86400.0;
    let days_elapsed = days_elapsed.max(0.0);

    let decay_factor = 0.5_f64.powf(days_elapsed / half_life_days);

    let access_boost = if retrieval_count > 0 {
        (1.0 + retrieval_count as f64).log2() * 0.1
    } else {
        0.0
    };

    let strength = decay_factor + access_boost;
    strength.clamp(MIN_STRENGTH, MAX_STRENGTH)
}

/// Blend semantic relevance score with retrieval strength.
pub fn blend_scores(relevance: f64, strength: f64, strength_weight: f64) -> f64 {
    let weight = strength_weight.clamp(0.0, 1.0);
    let relevance = relevance.clamp(0.0, 1.0);
    let strength = strength.clamp(0.0, 1.0);

    (1.0 - weight) * relevance + weight * strength
}

/// A search result with decay-adjusted scoring.
#[derive(Debug, Clone)]
pub struct ScoredResult<T> {
    pub item: T,
    pub relevance: f64,
    pub strength: f64,
    pub combined_score: f64,
}

impl<T> ScoredResult<T> {
    pub fn new(item: T, relevance: f64, strength: f64, strength_weight: f64) -> Self {
        let combined_score = blend_scores(relevance, strength, strength_weight);
        Self {
            item,
            relevance,
            strength,
            combined_score,
        }
    }
}

/// Re-rank a list of results by combined score.
pub fn rerank_by_combined_score<T>(results: &mut [ScoredResult<T>]) {
    results.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_fresh_memory_has_high_strength() {
        let now = Utc::now();
        let strength = calculate_strength(now, None, 0, 30.0);
        assert!(strength > 0.95);
    }

    #[test]
    fn test_old_memory_decays() {
        let now = Utc::now();
        let thirty_days_ago = now - Duration::days(30);
        let strength = calculate_strength(thirty_days_ago, None, 0, 30.0);
        assert!(strength > 0.45 && strength < 0.55);
    }

    #[test]
    fn test_access_boosts_strength() {
        let now = Utc::now();
        let thirty_days_ago = now - Duration::days(30);
        let strength_no_access = calculate_strength(thirty_days_ago, None, 0, 30.0);
        let strength_with_access = calculate_strength(thirty_days_ago, None, 10, 30.0);
        assert!(strength_with_access > strength_no_access);
    }

    #[test]
    fn test_recent_access_resets_decay() {
        let now = Utc::now();
        let thirty_days_ago = now - Duration::days(30);
        let yesterday = now - Duration::days(1);
        let strength_no_recent = calculate_strength(thirty_days_ago, None, 0, 30.0);
        let strength_recent = calculate_strength(thirty_days_ago, Some(yesterday), 0, 30.0);
        assert!(strength_recent > strength_no_recent);
    }

    #[test]
    fn test_blend_scores_pure_semantic() {
        let combined = blend_scores(0.9, 0.3, 0.0);
        assert!((combined - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_blend_scores_pure_strength() {
        let combined = blend_scores(0.9, 0.3, 1.0);
        assert!((combined - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_blend_scores_default_weight() {
        let combined = blend_scores(0.9, 0.5, 0.3);
        assert!((combined - 0.78).abs() < 0.001);
    }

    #[test]
    fn test_strength_clamped() {
        let now = Utc::now();
        let old = now - Duration::days(365);
        let strength = calculate_strength(old, None, 0, 30.0);
        assert!(strength >= MIN_STRENGTH);

        let strength = calculate_strength(now, Some(now), 1000, 30.0);
        assert!(strength <= MAX_STRENGTH);
    }
}
