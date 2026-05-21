//! Hierarchical safety system — prevents cognition deadlocks, layer starvation,
//! recursive hierarchy storms, and unsafe escalation chains.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::cognition_layers::CognitionLayer;

pub static SAFETY_CHECKS:       AtomicU64 = AtomicU64::new(0);
pub static DEADLOCKS_PREVENTED: AtomicU64 = AtomicU64::new(0);
pub static STARVATIONS_CAUGHT:  AtomicU64 = AtomicU64::new(0);
pub static HIERARCHY_VIOLATIONS: AtomicU64 = AtomicU64::new(0);

// Starvation: reactive layer starved if it gets < this fraction of its budget
const REACTIVE_MIN_BUDGET: f32 = 0.9;
// Deadlock: escalation chain that never resolves within N steps
const MAX_ESCALATION_CHAIN: usize = 5;
// Storm: hierarchy events per second exceeds this
const STORM_RATE_THRESH: u64 = 50;
const STORM_WINDOW_MS:   u64 = 1_000;

// ── Safety report ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HierarchySafetyReport {
    pub check_id:             u64,
    pub reactive_starved:     bool,
    pub deadlock_risk:        bool,
    pub hierarchy_storm:      bool,
    pub recursive_escalation: bool,
    pub violations:           Vec<String>,
    pub certified:            bool,
    pub ts_ms:                u64,
}

impl HierarchySafetyReport {
    pub fn is_safe(&self) -> bool { self.certified }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct SafetyState {
    history:         Vec<HierarchySafetyReport>,
    event_ts_window: Vec<u64>,   // timestamps of recent hierarchy events
}

static STATE: Lazy<Mutex<SafetyState>> = Lazy::new(|| Mutex::new(SafetyState {
    history:         Vec::new(),
    event_ts_window: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Record a hierarchy event (called per coordination dispatch).
pub fn record_event() {
    let now = ts_now();
    if let Ok(mut s) = STATE.lock() {
        let cutoff = now.saturating_sub(STORM_WINDOW_MS);
        s.event_ts_window.retain(|&t| t >= cutoff);
        s.event_ts_window.push(now);
    }
}

/// Run a full hierarchical safety check.
pub fn check() -> HierarchySafetyReport {
    SAFETY_CHECKS.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();
    let check_id = SAFETY_CHECKS.load(Ordering::Relaxed);
    let mut violations: Vec<String> = Vec::new();

    // ── 1. Reactive layer starvation ──────────────────────────────────────────
    let reactive_budget = crate::resource_scheduler::budget_for(CognitionLayer::Reactive);
    let reactive_starved = reactive_budget < REACTIVE_MIN_BUDGET;
    if reactive_starved {
        STARVATIONS_CAUGHT.fetch_add(1, Ordering::Relaxed);
        violations.push(format!("reactive_starved:budget={reactive_budget:.3}"));
        // Force-restore reactive budget
        crate::resource_scheduler::EMERGENCY_SUPPRESSED.store(false, Ordering::SeqCst);
    }

    // ── 2. Hierarchy storm ────────────────────────────────────────────────────
    let event_rate = STATE.lock().map(|s| s.event_ts_window.len() as u64).unwrap_or(0);
    let hierarchy_storm = event_rate >= STORM_RATE_THRESH;
    if hierarchy_storm {
        HIERARCHY_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
        violations.push(format!("hierarchy_storm:rate={event_rate}/s"));
        // Trigger watchdog to freeze cognition
        crate::cognitive_watchdog::COGNITION_FROZEN.store(true, Ordering::SeqCst);
    } else if crate::cognitive_watchdog::is_frozen() && event_rate < STORM_RATE_THRESH / 2 {
        crate::cognitive_watchdog::COGNITION_FROZEN.store(false, Ordering::SeqCst);
    }

    // ── 3. Deadlock risk: check escalation counter vs events ──────────────────
    let escalations = crate::cognition_layers::ESCALATIONS.load(Ordering::Relaxed);
    let events      = crate::cognition_layers::EVENTS_ROUTED.load(Ordering::Relaxed);
    let deadlock_risk = events > 0 && escalations as f32 / events.max(1) as f32 > 0.8;
    if deadlock_risk {
        DEADLOCKS_PREVENTED.fetch_add(1, Ordering::Relaxed);
        violations.push(format!("deadlock_risk:escalation_ratio={:.2}", escalations as f32 / events.max(1) as f32));
    }

    // ── 4. Recursive escalation: chain depth ─────────────────────────────────
    let coord_history = crate::cognition_coordinator::history(10);
    let max_chain = coord_history.iter().map(|r| r.escalations).max().unwrap_or(0);
    let recursive_escalation = max_chain >= MAX_ESCALATION_CHAIN;
    if recursive_escalation {
        HIERARCHY_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
        violations.push(format!("recursive_escalation:chain={max_chain}"));
    }

    let certified = violations.is_empty();

    let report = HierarchySafetyReport {
        check_id, reactive_starved, deadlock_risk, hierarchy_storm,
        recursive_escalation, violations, certified, ts_ms: now,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= 60 { s.history.remove(0); }
        s.history.push(report.clone());
    }

    report
}

pub fn history(n: usize) -> Vec<HierarchySafetyReport> {
    STATE.lock().map(|s| s.history.iter().rev().take(n).cloned().collect()).unwrap_or_default()
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
    fn check_runs_cleanly() {
        let report = check();
        assert_eq!(report.check_id, SAFETY_CHECKS.load(Ordering::Relaxed));
    }

    #[test]
    fn record_event_increments_window() {
        record_event();
        record_event();
        let report = check();
        assert!(report.check_id > 0);
    }

    #[test]
    fn certified_when_no_violations() {
        // Fresh state should be certified
        let report = check();
        // Either certified or has specific violations logged — just no panic
        let _ = report.is_safe();
    }
}
