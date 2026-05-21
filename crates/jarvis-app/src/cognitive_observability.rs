//! Cognitive observability — aggregates counters from all Phase 15 modules into
//! a single snapshot and writes it to `cognitive_snapshot.json`.

use std::sync::atomic::Ordering;

// ── Cognitive snapshot ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct CognitiveSnapshot {
    pub ts_ms: u64,

    // Ticks
    pub ticks_created:         u64,
    pub ticks_completed:       u64,
    pub ticks_failed:          u64,

    // Loop
    pub loop_ticks_total:      u64,
    pub loop_ticks_skipped:    u64,
    pub loop_errors:           u64,

    // Memory
    pub memory_writes:         u64,
    pub memory_reads:          u64,
    pub memory_evicted:        u64,
    pub memory_count:          usize,

    // Scheduler
    pub tasks_enqueued:        u64,
    pub tasks_dispatched:      u64,
    pub tasks_dropped:         u64,

    // World model
    pub model_entries:         u64,
    pub model_evictions:       u64,
    pub model_queries:         u64,
    pub model_history_len:     usize,

    // Observer
    pub observations:          u64,
    pub changes_detected:      u64,
    pub no_changes_detected:   u64,

    // Goal runtime
    pub goals_created:         u64,
    pub goals_completed:       u64,
    pub goals_abandoned:       u64,
    pub active_goals:          usize,

    // Workflow learning
    pub sequences_recorded:    u64,
    pub patterns_learned:      u64,
    pub pattern_matches:       u64,
    pub strong_patterns:       usize,

    // Anomaly detector
    pub anomaly_checks:        u64,
    pub anomalies_found:       u64,
    pub anomalies_clear:       u64,

    // Reflection
    pub reflections_run:       u64,
    pub insights_generated:    u64,
    pub insights_applied:      u64,

    // Predictions
    pub predictions_made:      u64,
    pub predictions_verified:  u64,
    pub predictions_correct:   u64,
    pub prediction_accuracy:   f32,

    // Attention
    pub attention_evaluations: u64,
    pub attention_shifts:      u64,

    // Persistent reasoner
    pub reasoner_updates:      u64,
    pub reasoner_inferences:   u64,
    pub reasoner_tick_count:   u64,

    // Task continuity
    pub continuity_saves:      u64,
    pub continuity_restores:   u64,
    pub continuity_cleared:    u64,
    pub continuity_pending:    usize,

    // Safety
    pub cognitive_safety_checks:  u64,
    pub cognitive_safety_allowed: u64,
    pub cognitive_safety_blocked: u64,
    pub cognitive_safety_rate_limited: u64,

    // World journal
    pub world_journal_entries:  u64,

    // Success rate (last 20 ticks)
    pub recent_success_rate: f32,
}

// ── Snapshot builder ──────────────────────────────────────────────────────────

pub fn snapshot() -> CognitiveSnapshot {
    use crate::cognitive_tick::{TICKS_CREATED, TICKS_COMPLETED, TICKS_FAILED};
    use crate::cognition_loop::{LOOP_TICKS_TOTAL, LOOP_TICKS_SKIPPED, LOOP_ERRORS};
    use crate::cognitive_memory::{self, MEMORY_WRITES, MEMORY_READS, MEMORY_EVICTED};
    use crate::cognition_scheduler::{TASKS_ENQUEUED, TASKS_DISPATCHED, TASKS_DROPPED};
    use crate::persistent_world_model::{MODEL_ENTRIES, MODEL_EVICTIONS, MODEL_QUERIES};
    use crate::active_observer::{OBSERVATIONS, CHANGES_DETECTED, NO_CHANGES_DETECTED};
    use crate::goal_runtime::{GOALS_CREATED, GOALS_COMPLETED, GOALS_ABANDONED, GoalRuntime};
    use crate::workflow_learning::{SEQUENCES_RECORDED, PATTERNS_LEARNED, PATTERN_MATCHES};
    use crate::anomaly_detector::{ANOMALY_CHECKS, ANOMALIES_FOUND, ANOMALIES_CLEAR};
    use crate::reflection_runtime::{REFLECTIONS_RUN, INSIGHTS_GENERATED, INSIGHTS_APPLIED};
    use crate::predictive_reasoner::{PREDICTIONS_MADE, PREDICTIONS_VERIFIED, PREDICTIONS_CORRECT, PredictiveReasoner};
    use crate::attention_runtime::{ATTENTION_EVALUATIONS, ATTENTION_SHIFTS};
    use crate::persistent_reasoner::{REASONER_UPDATES, REASONER_INFERENCES, PersistentReasoner};
    use crate::task_continuity::{CONTINUITY_SAVES, CONTINUITY_RESTORES, CONTINUITY_CLEARED, pending_count};
    use crate::cognitive_safety::{SAFETY_CHECKS, SAFETY_ALLOWED, SAFETY_BLOCKED, SAFETY_RATE_LIMITED};
    use crate::world_state_journal::WORLD_JOURNAL_ENTRIES;

    CognitiveSnapshot {
        ts_ms: ts_now(),

        ticks_created:         TICKS_CREATED.load(Ordering::Relaxed),
        ticks_completed:       TICKS_COMPLETED.load(Ordering::Relaxed),
        ticks_failed:          TICKS_FAILED.load(Ordering::Relaxed),

        loop_ticks_total:      LOOP_TICKS_TOTAL.load(Ordering::Relaxed),
        loop_ticks_skipped:    LOOP_TICKS_SKIPPED.load(Ordering::Relaxed),
        loop_errors:           LOOP_ERRORS.load(Ordering::Relaxed),

        memory_writes:         MEMORY_WRITES.load(Ordering::Relaxed),
        memory_reads:          MEMORY_READS.load(Ordering::Relaxed),
        memory_evicted:        MEMORY_EVICTED.load(Ordering::Relaxed),
        memory_count:          cognitive_memory::count(),

        tasks_enqueued:        TASKS_ENQUEUED.load(Ordering::Relaxed),
        tasks_dispatched:      TASKS_DISPATCHED.load(Ordering::Relaxed),
        tasks_dropped:         TASKS_DROPPED.load(Ordering::Relaxed),

        model_entries:         MODEL_ENTRIES.load(Ordering::Relaxed),
        model_evictions:       MODEL_EVICTIONS.load(Ordering::Relaxed),
        model_queries:         MODEL_QUERIES.load(Ordering::Relaxed),
        model_history_len:     crate::persistent_world_model::history_len(),

        observations:          OBSERVATIONS.load(Ordering::Relaxed),
        changes_detected:      CHANGES_DETECTED.load(Ordering::Relaxed),
        no_changes_detected:   NO_CHANGES_DETECTED.load(Ordering::Relaxed),

        goals_created:         GOALS_CREATED.load(Ordering::Relaxed),
        goals_completed:       GOALS_COMPLETED.load(Ordering::Relaxed),
        goals_abandoned:       GOALS_ABANDONED.load(Ordering::Relaxed),
        active_goals:          GoalRuntime::active_goals().len(),

        sequences_recorded:    SEQUENCES_RECORDED.load(Ordering::Relaxed),
        patterns_learned:      PATTERNS_LEARNED.load(Ordering::Relaxed),
        pattern_matches:       PATTERN_MATCHES.load(Ordering::Relaxed),
        strong_patterns:       crate::workflow_learning::strong_patterns().len(),

        anomaly_checks:        ANOMALY_CHECKS.load(Ordering::Relaxed),
        anomalies_found:       ANOMALIES_FOUND.load(Ordering::Relaxed),
        anomalies_clear:       ANOMALIES_CLEAR.load(Ordering::Relaxed),

        reflections_run:       REFLECTIONS_RUN.load(Ordering::Relaxed),
        insights_generated:    INSIGHTS_GENERATED.load(Ordering::Relaxed),
        insights_applied:      INSIGHTS_APPLIED.load(Ordering::Relaxed),

        predictions_made:      PREDICTIONS_MADE.load(Ordering::Relaxed),
        predictions_verified:  PREDICTIONS_VERIFIED.load(Ordering::Relaxed),
        predictions_correct:   PREDICTIONS_CORRECT.load(Ordering::Relaxed),
        prediction_accuracy:   PredictiveReasoner::accuracy(),

        attention_evaluations: ATTENTION_EVALUATIONS.load(Ordering::Relaxed),
        attention_shifts:      ATTENTION_SHIFTS.load(Ordering::Relaxed),

        reasoner_updates:      REASONER_UPDATES.load(Ordering::Relaxed),
        reasoner_inferences:   REASONER_INFERENCES.load(Ordering::Relaxed),
        reasoner_tick_count:   PersistentReasoner::tick_count(),

        continuity_saves:      CONTINUITY_SAVES.load(Ordering::Relaxed),
        continuity_restores:   CONTINUITY_RESTORES.load(Ordering::Relaxed),
        continuity_cleared:    CONTINUITY_CLEARED.load(Ordering::Relaxed),
        continuity_pending:    pending_count(),

        cognitive_safety_checks:      SAFETY_CHECKS.load(Ordering::Relaxed),
        cognitive_safety_allowed:     SAFETY_ALLOWED.load(Ordering::Relaxed),
        cognitive_safety_blocked:     SAFETY_BLOCKED.load(Ordering::Relaxed),
        cognitive_safety_rate_limited: SAFETY_RATE_LIMITED.load(Ordering::Relaxed),

        world_journal_entries: WORLD_JOURNAL_ENTRIES.load(Ordering::Relaxed),

        recent_success_rate:   cognitive_memory::recent_success_rate(20),
    }
}

pub fn write_snapshot() -> Result<std::path::PathBuf, String> {
    let snap = snapshot();
    let json = serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?;
    let path = crate::execution_journal::journal_dir().join("cognitive_snapshot.json");
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn summary(snap: &CognitiveSnapshot) -> String {
    format!(
        "ticks={} success_rate={:.0}% anomalies_found={} predictions_made={} active_goals={} patterns_strong={}",
        snap.loop_ticks_total,
        snap.recent_success_rate * 100.0,
        snap.anomalies_found,
        snap.predictions_made,
        snap.active_goals,
        snap.strong_patterns,
    )
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
    fn snapshot_has_valid_timestamp() {
        let snap = snapshot();
        assert!(snap.ts_ms > 0);
    }

    #[test]
    fn snapshot_prediction_accuracy_bounded() {
        let snap = snapshot();
        assert!(snap.prediction_accuracy >= 0.0 && snap.prediction_accuracy <= 1.0);
    }

    #[test]
    fn snapshot_serializes_to_json() {
        let snap = snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("ticks_created"));
        assert!(json.contains("active_goals"));
    }

    #[test]
    fn summary_string_non_empty() {
        let snap = snapshot();
        let s = summary(&snap);
        assert!(!s.is_empty());
        assert!(s.contains("ticks="));
    }

    #[test]
    fn recent_success_rate_bounded() {
        let snap = snapshot();
        assert!(snap.recent_success_rate >= 0.0 && snap.recent_success_rate <= 1.0);
    }
}
