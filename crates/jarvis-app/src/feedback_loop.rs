//! Cognitive feedback loop — drives the pipeline:
//! execution → verification → reflection → evaluation → optimization → adaptation.
//! Each tick runs all 6 stages in order.

use std::sync::atomic::{AtomicU64, Ordering};

pub static FEEDBACK_TICKS:    AtomicU64 = AtomicU64::new(0);
pub static FEEDBACK_IMPROVED: AtomicU64 = AtomicU64::new(0);

// ── Feedback tick result ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FeedbackTickResult {
    pub tick_id:           u64,
    pub quality_score:     f32,
    pub drift_events:      usize,
    pub insights_generated: usize,
    pub adaptation_applied: bool,
    pub strategy_improved:  bool,
    pub ts_ms:              u64,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Run one complete feedback cycle. Safe to call from tests.
pub fn run_tick() -> FeedbackTickResult {
    let tick_id = FEEDBACK_TICKS.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    // Stage 1 — Measure execution quality
    let quality_snap = crate::execution_quality::measure();
    let quality_score = quality_snap.overall;

    // Stage 2 — Drift control check
    let drift_events = crate::cognitive_drift_control::check().len();

    // Stage 3 — Reflect on recent ticks
    let insights = crate::reflection_runtime::ReflectionRuntime::reflect();
    let insights_count = insights.len();

    // Stage 4 — Evaluate strategies
    crate::strategy_evaluator::evaluate();

    // Stage 5 — Optimize workflows
    let _ = crate::workflow_optimizer::optimize();

    // Stage 6 — Adapt (gated by safe_adaptation)
    let adaptation_applied = if !crate::cognitive_drift_control::is_frozen() {
        crate::behavior_adaptation::run_cycle();
        crate::behavior_adaptation::ADAPTATIONS_APPLIED.load(Ordering::Relaxed) > 0
    } else {
        false
    };

    // Strategy comparison
    let candidates = vec![
        crate::strategy_optimizer::safe_candidate(),
        crate::strategy_optimizer::aggressive_candidate(),
    ];
    let strategy_improved = crate::strategy_optimizer::select_best(&candidates)
        .map(|r| r.best_score < 0.5)
        .unwrap_or(false);

    if strategy_improved || adaptation_applied {
        FEEDBACK_IMPROVED.fetch_add(1, Ordering::Relaxed);
    }

    FeedbackTickResult {
        tick_id,
        quality_score,
        drift_events,
        insights_generated: insights_count,
        adaptation_applied,
        strategy_improved,
        ts_ms: now,
    }
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
    fn run_tick_returns_result() {
        crate::cognitive_drift_control::unfreeze_for_test();
        let r = run_tick();
        assert!(r.quality_score >= 0.0 && r.quality_score <= 1.0);
    }

    #[test]
    fn feedback_ticks_increments() {
        let before = FEEDBACK_TICKS.load(Ordering::Relaxed);
        run_tick();
        assert!(FEEDBACK_TICKS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn tick_id_monotonically_increases() {
        crate::cognitive_drift_control::unfreeze_for_test();
        let r1 = run_tick();
        let r2 = run_tick();
        assert!(r2.tick_id > r1.tick_id);
    }

    #[test]
    fn run_tick_multiple_times_no_panic() {
        crate::cognitive_drift_control::unfreeze_for_test();
        for _ in 0..5 { run_tick(); }
    }

    #[test]
    fn ts_is_non_zero() {
        let r = run_tick();
        assert!(r.ts_ms > 0);
    }

    #[test]
    fn drift_events_non_negative() {
        let r = run_tick();
        let _ = r.drift_events;
    }
}
