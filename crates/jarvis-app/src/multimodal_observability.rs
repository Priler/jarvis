//! Unified observability for the multimodal runtime (Phase 14).
//!
//! Aggregates counters from all Phase 14 modules into a single
//! `MultimodalSnapshot` written to `multimodal_snapshot.json`.

use std::sync::atomic::Ordering;
use std::time::SystemTime;

pub use crate::screen_capture::{
    CAPTURES_REQUESTED, CAPTURES_SUCCEEDED, CAPTURES_FAILED, CAPTURE_TOTAL_LATENCY_MS,
};
pub use crate::ocr_runtime::{
    OCR_RUNS, OCR_SUCCESSES, OCR_FAILURES, OCR_TOTAL_LATENCY_MS,
};
pub use crate::visual_verifier::{
    VISUAL_CHECKS, VISUAL_CONFIRMED, VISUAL_FAILED, VISUAL_AMBIGUOUS,
};
pub use crate::dialog_detector::{
    DIALOG_CHECKS, DIALOGS_DETECTED, DIALOGS_NONE,
};
pub use crate::environment_reasoner::{
    REASONING_RUNS, REASONING_POSITIVE, REASONING_NEGATIVE,
};
pub use crate::multimodal_verification::{
    MM_VERIFICATIONS, MM_CONFIRMED, MM_FAILED, MM_PARTIAL,
};
pub use crate::ui_interaction_runtime::{
    INTERACTION_ATTEMPTS, INTERACTION_SUCCESSES, INTERACTION_CANCELLED, INTERACTION_BLOCKED,
};
pub use crate::multimodal_safety_runtime::{
    SAFETY_CHECKS, SAFETY_PASSED, SAFETY_BLOCKED,
};
pub use crate::window_graph::WINDOW_GRAPH_UPDATES;
pub use crate::world_state::{
    WORLD_STATE_SNAPSHOTS, WORLD_STATE_UPDATES, WORLD_STATE_STALE,
};
pub use crate::screen_state_journal::SCREEN_JOURNAL_ENTRIES;

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct MultimodalSnapshot {
    pub ts_ms: u64,

    // Screen capture
    pub captures_requested:       u64,
    pub captures_succeeded:       u64,
    pub captures_failed:          u64,
    pub capture_total_latency_ms: u64,

    // OCR
    pub ocr_runs:             u64,
    pub ocr_successes:        u64,
    pub ocr_failures:         u64,
    pub ocr_total_latency_ms: u64,

    // Visual verifier
    pub visual_checks:    u64,
    pub visual_confirmed: u64,
    pub visual_failed:    u64,
    pub visual_ambiguous: u64,

    // Dialog detector
    pub dialog_checks:    u64,
    pub dialogs_detected: u64,
    pub dialogs_none:     u64,

    // Environment reasoner
    pub reasoning_runs:     u64,
    pub reasoning_positive: u64,
    pub reasoning_negative: u64,

    // Multimodal verification
    pub mm_verifications: u64,
    pub mm_confirmed:     u64,
    pub mm_failed:        u64,
    pub mm_partial:       u64,

    // UI interaction
    pub interaction_attempts:  u64,
    pub interaction_successes: u64,
    pub interaction_cancelled: u64,
    pub interaction_blocked:   u64,

    // Safety
    pub safety_checks:  u64,
    pub safety_passed:  u64,
    pub safety_blocked: u64,

    // World state + journal
    pub world_state_snapshots: u64,
    pub world_state_updates:   u64,
    pub world_state_stale:     u64,
    pub window_graph_updates:  u64,
    pub screen_journal_entries: u64,
}

impl MultimodalSnapshot {
    pub fn collect() -> Self {
        Self {
            ts_ms: now_ms(),

            captures_requested:       CAPTURES_REQUESTED.load(Ordering::Relaxed),
            captures_succeeded:       CAPTURES_SUCCEEDED.load(Ordering::Relaxed),
            captures_failed:          CAPTURES_FAILED.load(Ordering::Relaxed),
            capture_total_latency_ms: CAPTURE_TOTAL_LATENCY_MS.load(Ordering::Relaxed),

            ocr_runs:             OCR_RUNS.load(Ordering::Relaxed),
            ocr_successes:        OCR_SUCCESSES.load(Ordering::Relaxed),
            ocr_failures:         OCR_FAILURES.load(Ordering::Relaxed),
            ocr_total_latency_ms: OCR_TOTAL_LATENCY_MS.load(Ordering::Relaxed),

            visual_checks:    VISUAL_CHECKS.load(Ordering::Relaxed),
            visual_confirmed: VISUAL_CONFIRMED.load(Ordering::Relaxed),
            visual_failed:    VISUAL_FAILED.load(Ordering::Relaxed),
            visual_ambiguous: VISUAL_AMBIGUOUS.load(Ordering::Relaxed),

            dialog_checks:    DIALOG_CHECKS.load(Ordering::Relaxed),
            dialogs_detected: DIALOGS_DETECTED.load(Ordering::Relaxed),
            dialogs_none:     DIALOGS_NONE.load(Ordering::Relaxed),

            reasoning_runs:     REASONING_RUNS.load(Ordering::Relaxed),
            reasoning_positive: REASONING_POSITIVE.load(Ordering::Relaxed),
            reasoning_negative: REASONING_NEGATIVE.load(Ordering::Relaxed),

            mm_verifications: MM_VERIFICATIONS.load(Ordering::Relaxed),
            mm_confirmed:     MM_CONFIRMED.load(Ordering::Relaxed),
            mm_failed:        MM_FAILED.load(Ordering::Relaxed),
            mm_partial:       MM_PARTIAL.load(Ordering::Relaxed),

            interaction_attempts:  INTERACTION_ATTEMPTS.load(Ordering::Relaxed),
            interaction_successes: INTERACTION_SUCCESSES.load(Ordering::Relaxed),
            interaction_cancelled: INTERACTION_CANCELLED.load(Ordering::Relaxed),
            interaction_blocked:   INTERACTION_BLOCKED.load(Ordering::Relaxed),

            safety_checks:  SAFETY_CHECKS.load(Ordering::Relaxed),
            safety_passed:  SAFETY_PASSED.load(Ordering::Relaxed),
            safety_blocked: SAFETY_BLOCKED.load(Ordering::Relaxed),

            world_state_snapshots:  WORLD_STATE_SNAPSHOTS.load(Ordering::Relaxed),
            world_state_updates:    WORLD_STATE_UPDATES.load(Ordering::Relaxed),
            world_state_stale:      WORLD_STATE_STALE.load(Ordering::Relaxed),
            window_graph_updates:   WINDOW_GRAPH_UPDATES.load(Ordering::Relaxed),
            screen_journal_entries: SCREEN_JOURNAL_ENTRIES.load(Ordering::Relaxed),
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "captures={}/{} ocr={}/{} visual={}/{} dialogs={} env_ok={}/{} mm={}/{} safety={}/{}",
            self.captures_succeeded, self.captures_requested,
            self.ocr_successes, self.ocr_runs,
            self.visual_confirmed, self.visual_checks,
            self.dialogs_detected,
            self.reasoning_positive, self.reasoning_runs,
            self.mm_confirmed, self.mm_verifications,
            self.safety_passed, self.safety_checks,
        )
    }

    pub fn ocr_success_rate(&self) -> f64 {
        if self.ocr_runs == 0 { 1.0 }
        else { self.ocr_successes as f64 / self.ocr_runs as f64 }
    }

    pub fn capture_success_rate(&self) -> f64 {
        if self.captures_requested == 0 { 1.0 }
        else { self.captures_succeeded as f64 / self.captures_requested as f64 }
    }
}

pub fn write_snapshot() {
    let snap = MultimodalSnapshot::collect();
    if let Ok(json) = serde_json::to_string_pretty(&snap) {
        let _ = std::fs::write("multimodal_snapshot.json", json);
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
        let snap = MultimodalSnapshot::collect();
        assert!(snap.ts_ms > 0);
    }

    #[test]
    fn summary_not_empty() {
        let snap = MultimodalSnapshot::collect();
        assert!(!snap.summary().is_empty());
        assert!(snap.summary().contains("captures="));
    }

    #[test]
    fn ocr_success_rate_is_1_when_no_runs() {
        let snap = MultimodalSnapshot {
            ts_ms: 0, captures_requested: 0, captures_succeeded: 0, captures_failed: 0,
            capture_total_latency_ms: 0, ocr_runs: 0, ocr_successes: 0, ocr_failures: 0,
            ocr_total_latency_ms: 0, visual_checks: 0, visual_confirmed: 0,
            visual_failed: 0, visual_ambiguous: 0, dialog_checks: 0, dialogs_detected: 0,
            dialogs_none: 0, reasoning_runs: 0, reasoning_positive: 0, reasoning_negative: 0,
            mm_verifications: 0, mm_confirmed: 0, mm_failed: 0, mm_partial: 0,
            interaction_attempts: 0, interaction_successes: 0, interaction_cancelled: 0,
            interaction_blocked: 0, safety_checks: 0, safety_passed: 0, safety_blocked: 0,
            world_state_snapshots: 0, world_state_updates: 0, world_state_stale: 0,
            window_graph_updates: 0, screen_journal_entries: 0,
        };
        assert!((snap.ocr_success_rate() - 1.0).abs() < f64::EPSILON);
        assert!((snap.capture_success_rate() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn summary_contains_ocr_field() {
        let snap = MultimodalSnapshot::collect();
        assert!(snap.summary().contains("ocr="));
    }
}
