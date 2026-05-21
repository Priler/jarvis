//! Strategy optimizer — compares alternative execution plans, estimates
//! execution risk, recovery cost, and workflow stability to select the
//! optimal strategy.  Pure heuristic; no ML, no LLM.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static OPTIMIZATIONS_RUN:   AtomicU64 = AtomicU64::new(0);
pub static STRATEGIES_COMPARED: AtomicU64 = AtomicU64::new(0);
pub static STRATEGIES_ADOPTED:  AtomicU64 = AtomicU64::new(0);

// ── Strategy candidate ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategyCandidate {
    pub id:               String,
    pub description:      String,
    pub risk_estimate:    f32,   // 0.0 = safe, 1.0 = risky
    pub latency_estimate: f32,   // 0.0 = fast, 1.0 = slow
    pub recovery_cost:    f32,   // cost if strategy fails
    pub stability:        f32,   // 0.0 = unstable, 1.0 = stable
}

impl StrategyCandidate {
    /// Lower score = better strategy. Weighted composite.
    pub fn cost_score(&self) -> f32 {
        self.risk_estimate    * 0.35
        + self.latency_estimate * 0.20
        + self.recovery_cost    * 0.30
        + (1.0 - self.stability) * 0.15
    }
}

// ── Optimization result ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OptimizationResult {
    pub best_id:      String,
    pub best_score:   f32,
    pub alternatives: usize,
    pub ts_ms:        u64,
}

const MAX_RESULTS: usize = 30;

static RESULTS: Lazy<Mutex<Vec<OptimizationResult>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Public API ────────────────────────────────────────────────────────────────

/// Select the best strategy from the given candidates.
/// Returns `None` if the list is empty.
pub fn select_best(candidates: &[StrategyCandidate]) -> Option<OptimizationResult> {
    if candidates.is_empty() { return None; }

    OPTIMIZATIONS_RUN.fetch_add(1, Ordering::Relaxed);
    STRATEGIES_COMPARED.fetch_add(candidates.len() as u64, Ordering::Relaxed);

    let best = candidates.iter()
        .min_by(|a, b| a.cost_score().partial_cmp(&b.cost_score()).unwrap_or(std::cmp::Ordering::Equal))?;

    let result = OptimizationResult {
        best_id:      best.id.clone(),
        best_score:   best.cost_score(),
        alternatives: candidates.len() - 1,
        ts_ms:        ts_now(),
    };

    STRATEGIES_ADOPTED.fetch_add(1, Ordering::Relaxed);

    if let Ok(mut r) = RESULTS.lock() {
        if r.len() >= MAX_RESULTS { r.remove(0); }
        r.push(result.clone());
    }

    Some(result)
}

/// Build a default "safe" candidate from current runtime quality.
pub fn safe_candidate() -> StrategyCandidate {
    let quality = crate::execution_quality::average_overall(5);
    StrategyCandidate {
        id:               "safe_default".into(),
        description:      "conservative execution with full verification".into(),
        risk_estimate:    1.0 - quality,
        latency_estimate: 0.6,
        recovery_cost:    0.3,
        stability:        quality,
    }
}

/// Build an "aggressive" candidate (faster but riskier).
pub fn aggressive_candidate() -> StrategyCandidate {
    let quality = crate::execution_quality::average_overall(5);
    StrategyCandidate {
        id:               "aggressive_default".into(),
        description:      "fast execution with minimal verification".into(),
        risk_estimate:    (1.0 - quality).min(0.8),
        latency_estimate: 0.2,
        recovery_cost:    0.7,
        stability:        quality * 0.7,
    }
}

pub fn recent_results(n: usize) -> Vec<OptimizationResult> {
    RESULTS.lock().map(|r| {
        let len = r.len();
        r[len.saturating_sub(n)..].to_vec()
    }).unwrap_or_default()
}

pub fn result_count() -> usize {
    RESULTS.lock().map(|r| r.len()).unwrap_or(0)
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidate(id: &str, risk: f32, latency: f32) -> StrategyCandidate {
        StrategyCandidate {
            id: id.into(), description: id.into(),
            risk_estimate: risk, latency_estimate: latency,
            recovery_cost: 0.3, stability: 0.7,
        }
    }

    #[test]
    fn select_best_returns_none_for_empty() {
        assert!(select_best(&[]).is_none());
    }

    #[test]
    fn select_best_prefers_lower_risk() {
        let a = make_candidate("risky",  0.9, 0.1);
        let b = make_candidate("safe",   0.1, 0.5);
        let result = select_best(&[a, b]).unwrap();
        assert_eq!(result.best_id, "safe");
    }

    #[test]
    fn cost_score_bounded() {
        let c = make_candidate("x", 0.5, 0.5);
        assert!(c.cost_score() >= 0.0 && c.cost_score() <= 1.0);
    }

    #[test]
    fn optimizations_run_increments() {
        let before = OPTIMIZATIONS_RUN.load(Ordering::Relaxed);
        select_best(&[make_candidate("a", 0.3, 0.3)]);
        assert!(OPTIMIZATIONS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn safe_and_aggressive_candidates_valid() {
        let s = safe_candidate();
        let a = aggressive_candidate();
        assert!(s.risk_estimate >= 0.0 && s.risk_estimate <= 1.0);
        assert!(a.latency_estimate < s.latency_estimate);
    }

    #[test]
    fn result_count_grows() {
        let before = result_count();
        select_best(&[make_candidate("grow", 0.2, 0.2)]);
        assert!(result_count() > before || result_count() == MAX_RESULTS);
    }
}
