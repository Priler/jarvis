//! Self-evaluation — generates a runtime quality report that assesses:
//! planner quality, cognitive stability, workflow improvement, and strategy degradation.
//! Read-only: queries other modules, never modifies them.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static EVALUATIONS_RUN:   AtomicU64 = AtomicU64::new(0);
pub static REPORTS_GENERATED: AtomicU64 = AtomicU64::new(0);

const MAX_REPORT_HISTORY: usize = 20;

// ── Self-evaluation report ────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelfEvaluationReport {
    pub ts_ms:                 u64,
    pub generation:            u32,
    pub planner_quality:       f32,    // 0.0–1.0
    pub cognitive_stability:   f32,
    pub workflow_improvement:  f32,
    pub strategy_degradation:  f32,    // higher = more degraded
    pub drift_risk:            f32,
    pub adaptation_safety:     f32,
    pub overall_health:        f32,
    pub summary:               String,
}

impl SelfEvaluationReport {
    pub fn is_healthy(&self) -> bool     { self.overall_health >= 0.6 }
    pub fn needs_attention(&self) -> bool { self.overall_health < 0.5 || self.drift_risk > 0.6 }
}

static HISTORY: Lazy<Mutex<Vec<SelfEvaluationReport>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn evaluate() -> SelfEvaluationReport {
    EVALUATIONS_RUN.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let planner_quality = crate::cognitive_memory::recent_success_rate(20);

    let drift_events = crate::cognitive_drift_control::recent_events(10).len() as f32;
    let cognitive_stability = (1.0 - (drift_events / 10.0).min(1.0)).max(0.0);

    let patterns_learned = crate::workflow_learning::PATTERNS_LEARNED.load(Ordering::Relaxed) as f32;
    let sequences        = crate::workflow_learning::SEQUENCES_RECORDED.load(Ordering::Relaxed) as f32;
    let workflow_improvement = if sequences > 0.0 { (patterns_learned / sequences * 10.0).min(1.0) } else { 0.5 };

    let strategy_scores = crate::strategy_evaluator::all_latest();
    let strategy_degradation = if strategy_scores.is_empty() { 0.5 } else {
        let avg: f32 = strategy_scores.iter().map(|s| s.score).sum::<f32>() / strategy_scores.len() as f32;
        1.0 - avg
    };

    let drift_risk = drift_events / 10.0_f32.min(1.0);

    let blocked  = crate::safe_adaptation::ADAPTATION_BLOCKED.load(Ordering::Relaxed) as f32;
    let approved = crate::safe_adaptation::ADAPTATION_APPROVED.load(Ordering::Relaxed) as f32;
    let total    = (blocked + approved).max(1.0);
    let adaptation_safety = approved / total;

    let overall_health = (
        planner_quality      * 0.30 +
        cognitive_stability  * 0.25 +
        workflow_improvement * 0.15 +
        (1.0 - strategy_degradation) * 0.15 +
        adaptation_safety    * 0.15
    ).clamp(0.0, 1.0);

    let generation = crate::cognitive_evolution::generation();

    let summary = format!(
        "gen={generation} planner={:.2} stability={:.2} workflow={:.2} drift_risk={:.2} health={:.2}",
        planner_quality, cognitive_stability, workflow_improvement, drift_risk, overall_health
    );

    let report = SelfEvaluationReport {
        ts_ms: now, generation, planner_quality, cognitive_stability,
        workflow_improvement, strategy_degradation, drift_risk,
        adaptation_safety, overall_health, summary,
    };

    REPORTS_GENERATED.fetch_add(1, Ordering::Relaxed);

    if let Ok(mut h) = HISTORY.lock() {
        if h.len() >= MAX_REPORT_HISTORY { h.remove(0); }
        h.push(report.clone());
    }

    report
}

pub fn latest_report() -> Option<SelfEvaluationReport> {
    HISTORY.lock().ok().and_then(|h| h.last().cloned())
}

pub fn report_count() -> usize {
    HISTORY.lock().map(|h| h.len()).unwrap_or(0)
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

    #[test]
    fn evaluate_returns_report() {
        let r = evaluate();
        assert!(r.overall_health >= 0.0 && r.overall_health <= 1.0);
    }

    #[test]
    fn summary_is_non_empty() {
        let r = evaluate();
        assert!(!r.summary.is_empty());
    }

    #[test]
    fn report_count_grows() {
        let before = report_count();
        evaluate();
        assert!(report_count() > before);
    }

    #[test]
    fn all_scores_bounded() {
        let r = evaluate();
        assert!(r.planner_quality      >= 0.0 && r.planner_quality      <= 1.0);
        assert!(r.cognitive_stability  >= 0.0 && r.cognitive_stability  <= 1.0);
        assert!(r.workflow_improvement >= 0.0 && r.workflow_improvement <= 1.0);
        assert!(r.adaptation_safety    >= 0.0 && r.adaptation_safety    <= 1.0);
    }

    #[test]
    fn evaluations_run_increments() {
        let before = EVALUATIONS_RUN.load(Ordering::Relaxed);
        evaluate();
        assert!(EVALUATIONS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn latest_report_some_after_evaluate() {
        evaluate();
        assert!(latest_report().is_some());
    }
}
