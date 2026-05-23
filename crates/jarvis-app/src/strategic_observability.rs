//! Strategic observability — aggregates all Phase 16 counters and quality
//! metrics into a single snapshot.  Writes `strategic_snapshot.json` on demand.

use std::sync::atomic::{AtomicU64, Ordering};

pub static STRATEGIC_SNAPSHOTS: AtomicU64 = AtomicU64::new(0);

// ── Strategic snapshot ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategicSnapshot {
    pub ts_ms:                    u64,

    // Strategy evaluator
    pub evaluations_run:          u64,
    pub evaluations_low:          u64,

    // Execution quality
    pub quality_snapshots:        u64,
    pub quality_degraded:         u64,
    pub current_overall_quality:  f32,

    // Failure patterns
    pub analyses_run:             u64,
    pub patterns_detected:        u64,
    pub has_critical_pattern:     bool,

    // Cognitive drift
    pub drift_checks:             u64,
    pub drift_detected:           u64,
    pub drift_frozen:             u64,
    pub is_frozen:                bool,

    // Cognitive evolution
    pub evolutions_run:           u64,
    pub heuristics_updated:       u64,
    pub current_generation:       u32,

    // Safe adaptation
    pub adaptation_checks:        u64,
    pub adaptation_approved:      u64,
    pub adaptation_blocked:       u64,

    // Behavior adaptation
    pub adaptations_applied:      u64,
    pub adaptations_skipped:      u64,

    // Strategy optimizer
    pub optimizations_run:        u64,
    pub strategies_compared:      u64,

    // Workflow optimizer
    pub workflow_optimizations:   u64,
    pub workflows_improved:       u64,

    // Feedback loop
    pub feedback_ticks:           u64,
    pub feedback_improved:        u64,

    // Long horizon goals
    pub lh_goals_created:         u64,
    pub lh_goals_completed:       u64,
    pub active_horizon_goals:     usize,

    // Self evaluation
    pub self_evaluations_run:     u64,
    pub latest_overall_health:    f32,

    // Autonomous learning safety
    pub safety_verifications:     u64,
    pub safety_violations:        u64,
    pub safety_certified:         u64,

    pub planner_quality_score:    f32,
    pub cognitive_stability_score:f32,
}

impl StrategicSnapshot {
    pub fn summary(&self) -> String {
        format!(
            "gen={} health={:.2} quality={:.2} drift_frozen={} adaptations={}/{} patterns={} safety_cert={}",
            self.current_generation,
            self.latest_overall_health,
            self.current_overall_quality,
            self.is_frozen,
            self.adaptation_approved,
            self.adaptation_checks,
            self.patterns_detected,
            self.safety_certified,
        )
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn snapshot() -> StrategicSnapshot {
    STRATEGIC_SNAPSHOTS.fetch_add(1, Ordering::Relaxed);

    let quality = crate::execution_quality::latest()
        .map(|q| q.overall)
        .unwrap_or(0.5);

    let health = crate::self_evaluation::latest_report()
        .map(|r| r.overall_health)
        .unwrap_or(0.5);

    let planner_q = crate::strategy_evaluator::latest(crate::strategy_evaluator::DIM_PLANNER)
        .map(|s| s.score).unwrap_or(0.5);
    let stability = crate::cognitive_memory::recent_success_rate(20);

    StrategicSnapshot {
        ts_ms: ts_now(),

        evaluations_run:          crate::strategy_evaluator::EVALUATIONS_RUN.load(Ordering::Relaxed),
        evaluations_low:          crate::strategy_evaluator::EVALUATIONS_LOW.load(Ordering::Relaxed),

        quality_snapshots:        crate::execution_quality::QUALITY_SNAPSHOTS.load(Ordering::Relaxed),
        quality_degraded:         crate::execution_quality::QUALITY_DEGRADED.load(Ordering::Relaxed),
        current_overall_quality:  quality,

        analyses_run:             crate::failure_pattern_analyzer::ANALYSES_RUN.load(Ordering::Relaxed),
        patterns_detected:        crate::failure_pattern_analyzer::PATTERNS_DETECTED.load(Ordering::Relaxed),
        has_critical_pattern:     crate::failure_pattern_analyzer::has_critical_pattern(),

        drift_checks:             crate::cognitive_drift_control::DRIFT_CHECKS.load(Ordering::Relaxed),
        drift_detected:           crate::cognitive_drift_control::DRIFT_DETECTED.load(Ordering::Relaxed),
        drift_frozen:             crate::cognitive_drift_control::DRIFT_FROZEN.load(Ordering::Relaxed),
        is_frozen:                crate::cognitive_drift_control::is_frozen(),

        evolutions_run:           crate::cognitive_evolution::EVOLUTIONS_RUN.load(Ordering::Relaxed),
        heuristics_updated:       crate::cognitive_evolution::HEURISTICS_UPDATED.load(Ordering::Relaxed),
        current_generation:       crate::cognitive_evolution::generation(),

        adaptation_checks:        crate::safe_adaptation::ADAPTATION_CHECKS.load(Ordering::Relaxed),
        adaptation_approved:      crate::safe_adaptation::ADAPTATION_APPROVED.load(Ordering::Relaxed),
        adaptation_blocked:       crate::safe_adaptation::ADAPTATION_BLOCKED.load(Ordering::Relaxed),

        adaptations_applied:      crate::behavior_adaptation::ADAPTATIONS_APPLIED.load(Ordering::Relaxed),
        adaptations_skipped:      crate::behavior_adaptation::ADAPTATIONS_SKIPPED.load(Ordering::Relaxed),

        optimizations_run:        crate::strategy_optimizer::OPTIMIZATIONS_RUN.load(Ordering::Relaxed),
        strategies_compared:      crate::strategy_optimizer::STRATEGIES_COMPARED.load(Ordering::Relaxed),

        workflow_optimizations:   crate::workflow_optimizer::OPTIMIZATIONS_APPLIED.load(Ordering::Relaxed),
        workflows_improved:       crate::workflow_optimizer::WORKFLOWS_IMPROVED.load(Ordering::Relaxed),

        feedback_ticks:           crate::feedback_loop::FEEDBACK_TICKS.load(Ordering::Relaxed),
        feedback_improved:        crate::feedback_loop::FEEDBACK_IMPROVED.load(Ordering::Relaxed),

        lh_goals_created:         crate::long_horizon_goals::LH_GOALS_CREATED.load(Ordering::Relaxed),
        lh_goals_completed:       crate::long_horizon_goals::LH_GOALS_COMPLETED.load(Ordering::Relaxed),
        active_horizon_goals:     crate::long_horizon_goals::active_goals().len(),

        self_evaluations_run:     crate::self_evaluation::EVALUATIONS_RUN.load(Ordering::Relaxed),
        latest_overall_health:    health,

        safety_verifications:     crate::autonomous_learning_safety::SAFETY_VERIFICATIONS.load(Ordering::Relaxed),
        safety_violations:        crate::autonomous_learning_safety::SAFETY_VIOLATIONS.load(Ordering::Relaxed),
        safety_certified:         crate::autonomous_learning_safety::SAFETY_CERTIFIED.load(Ordering::Relaxed),

        planner_quality_score:    planner_q,
        cognitive_stability_score:stability,
    }
}

pub fn write_snapshot() -> Result<std::path::PathBuf, String> {
    let snap = snapshot();
    let path = crate::execution_journal::journal_dir().join("strategic_snapshot.json");
    let json = serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
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
    fn snapshot_collects_without_panic() {
        let _ = snapshot();
    }

    #[test]
    fn strategic_snapshots_increments() {
        let before = STRATEGIC_SNAPSHOTS.load(Ordering::Relaxed);
        snapshot();
        assert!(STRATEGIC_SNAPSHOTS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn scores_bounded() {
        let s = snapshot();
        assert!(s.current_overall_quality >= 0.0 && s.current_overall_quality <= 1.0);
        assert!(s.latest_overall_health   >= 0.0 && s.latest_overall_health   <= 1.0);
        assert!(s.planner_quality_score   >= 0.0 && s.planner_quality_score   <= 1.0);
    }

    #[test]
    fn summary_non_empty() {
        let s = snapshot();
        assert!(!s.summary().is_empty());
    }

    #[test]
    fn snapshot_serializes_to_json() {
        let s = snapshot();
        let json = serde_json::to_string(&s);
        assert!(json.is_ok());
    }

    #[test]
    fn ts_is_nonzero() {
        let s = snapshot();
        assert!(s.ts_ms > 0);
    }
}
