//! Unified observability for the autonomous planning runtime (Phase 13).
//!
//! Aggregates counters from all Phase 13 modules into a single
//! `AutonomousSnapshot` that is periodically written to
//! `autonomous_snapshot.json`.

use std::sync::atomic::Ordering;
use std::time::SystemTime;

pub use crate::planner_v2::{PLANS_CREATED, PLANS_FAILED, PLANS_ROLLED_BACK};
pub use crate::execution_verifier::{VERIFICATIONS_RUN, VERIFICATIONS_PASSED, VERIFICATIONS_FAILED};
pub use crate::execution_recovery::{
    RECOVERY_ATTEMPTS, RECOVERY_SUCCESSES, RECOVERY_FAILURES, ROLLBACK_ACTIONS, PARTIAL_FAILURES,
};
pub use crate::hallucination_guard_v2::{GUARD_V2_CHECKS, GUARD_V2_BLOCKED, GUARD_V2_PASSED};
pub use crate::execution_sandbox::{SANDBOX_VALIDATIONS, SANDBOX_VIOLATIONS, SANDBOX_PASSED};
pub use crate::execution_journal::JOURNAL_ENTRIES_WRITTEN;
pub use crate::contextual_task_memory::{TASKS_CREATED, TASKS_COMPLETED, TASKS_FAILED, TASKS_RECOVERED};

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct AutonomousSnapshot {
    pub ts_ms:                    u64,
    // Planner
    pub plans_created:            u64,
    pub plans_failed:             u64,
    pub plans_rolled_back:        u64,
    // Verifier
    pub verifications_run:        u64,
    pub verifications_passed:     u64,
    pub verifications_failed:     u64,
    // Recovery
    pub recovery_attempts:        u64,
    pub recovery_successes:       u64,
    pub recovery_failures:        u64,
    pub rollback_actions:         u64,
    pub partial_failures:         u64,
    // Hallucination guard V2
    pub guard_v2_checks:          u64,
    pub guard_v2_blocked:         u64,
    pub guard_v2_passed:          u64,
    // Sandbox
    pub sandbox_validations:      u64,
    pub sandbox_violations:       u64,
    pub sandbox_passed:           u64,
    // Journal
    pub journal_entries:          u64,
    // Task memory
    pub tasks_created:            u64,
    pub tasks_completed:          u64,
    pub tasks_failed:             u64,
    pub tasks_recovered:          u64,
}

impl AutonomousSnapshot {
    pub fn collect() -> Self {
        Self {
            ts_ms:                now_ms(),
            plans_created:        PLANS_CREATED.load(Ordering::Relaxed),
            plans_failed:         PLANS_FAILED.load(Ordering::Relaxed),
            plans_rolled_back:    PLANS_ROLLED_BACK.load(Ordering::Relaxed),
            verifications_run:    VERIFICATIONS_RUN.load(Ordering::Relaxed),
            verifications_passed: VERIFICATIONS_PASSED.load(Ordering::Relaxed),
            verifications_failed: VERIFICATIONS_FAILED.load(Ordering::Relaxed),
            recovery_attempts:    RECOVERY_ATTEMPTS.load(Ordering::Relaxed),
            recovery_successes:   RECOVERY_SUCCESSES.load(Ordering::Relaxed),
            recovery_failures:    RECOVERY_FAILURES.load(Ordering::Relaxed),
            rollback_actions:     ROLLBACK_ACTIONS.load(Ordering::Relaxed),
            partial_failures:     PARTIAL_FAILURES.load(Ordering::Relaxed),
            guard_v2_checks:      GUARD_V2_CHECKS.load(Ordering::Relaxed),
            guard_v2_blocked:     GUARD_V2_BLOCKED.load(Ordering::Relaxed),
            guard_v2_passed:      GUARD_V2_PASSED.load(Ordering::Relaxed),
            sandbox_validations:  SANDBOX_VALIDATIONS.load(Ordering::Relaxed),
            sandbox_violations:   SANDBOX_VIOLATIONS.load(Ordering::Relaxed),
            sandbox_passed:       SANDBOX_PASSED.load(Ordering::Relaxed),
            journal_entries:      JOURNAL_ENTRIES_WRITTEN.load(Ordering::Relaxed),
            tasks_created:        TASKS_CREATED.load(Ordering::Relaxed),
            tasks_completed:      TASKS_COMPLETED.load(Ordering::Relaxed),
            tasks_failed:         TASKS_FAILED.load(Ordering::Relaxed),
            tasks_recovered:      TASKS_RECOVERED.load(Ordering::Relaxed),
        }
    }

    /// Single-line log summary.
    pub fn summary(&self) -> String {
        format!(
            "plans={}/{} verify={}/{} recovery={}/{} guard_blocked={} sandbox_violations={} tasks={}/{}",
            self.plans_created - self.plans_failed, self.plans_created,
            self.verifications_passed, self.verifications_run,
            self.recovery_successes, self.recovery_attempts,
            self.guard_v2_blocked,
            self.sandbox_violations,
            self.tasks_completed, self.tasks_created,
        )
    }

    pub fn plan_success_rate(&self) -> f64 {
        if self.plans_created == 0 { 1.0 }
        else { 1.0 - (self.plans_failed as f64 / self.plans_created as f64) }
    }
}

/// Write snapshot to `autonomous_snapshot.json` (overwrite).
pub fn write_snapshot() {
    let snap = AutonomousSnapshot::collect();
    if let Ok(json) = serde_json::to_string_pretty(&snap) {
        let _ = std::fs::write("autonomous_snapshot.json", json);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_collects_without_panic() {
        let snap = AutonomousSnapshot::collect();
        assert!(snap.ts_ms > 0);
    }

    #[test]
    fn summary_not_empty() {
        let snap = AutonomousSnapshot::collect();
        assert!(!snap.summary().is_empty());
    }

    #[test]
    fn plan_success_rate_is_1_when_no_plans() {
        // This tests the formula — not global state.
        let snap = AutonomousSnapshot {
            ts_ms: 0, plans_created: 0, plans_failed: 0, plans_rolled_back: 0,
            verifications_run: 0, verifications_passed: 0, verifications_failed: 0,
            recovery_attempts: 0, recovery_successes: 0, recovery_failures: 0,
            rollback_actions: 0, partial_failures: 0,
            guard_v2_checks: 0, guard_v2_blocked: 0, guard_v2_passed: 0,
            sandbox_validations: 0, sandbox_violations: 0, sandbox_passed: 0,
            journal_entries: 0, tasks_created: 0, tasks_completed: 0,
            tasks_failed: 0, tasks_recovered: 0,
        };
        assert!((snap.plan_success_rate() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn summary_contains_plans_field() {
        let snap = AutonomousSnapshot::collect();
        assert!(snap.summary().contains("plans="));
    }
}
