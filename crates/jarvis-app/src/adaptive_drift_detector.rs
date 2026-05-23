//! Adaptive threshold drift detection and rollback.
//!
//! Monitors the adaptive threshold history for pathological patterns:
//!   - Monotonic drift:  threshold drifting in one direction over many ticks.
//!   - Oscillation:      threshold reversing direction rapidly (unstable adaptation).
//!   - Runaway:          threshold pinned at MIN or MAX for extended time.
//!
//! When a drift condition is detected, the detector can trigger a deterministic
//! rollback to a saved baseline value.  The rollback is always to a known-safe
//! value — never to a synthesised or hardcoded one.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::adaptive_threshold;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Number of threshold samples retained for analysis.
const HISTORY_LEN: usize = 30;
/// Threshold change considered "significant" (in either direction).
const SIGNIFICANT_DELTA: f32 = 0.005;
/// Number of direction reversals in the window that indicates oscillation.
const OSCILLATION_REVERSAL_LIMIT: u32 = 8;
/// Consecutive ticks at MIN/MAX boundary before "runaway" is flagged.
const RUNAWAY_TICKS: u32 = 20;
/// Maximum allowed total drift from baseline before forced rollback.
const MAX_DRIFT_FROM_BASELINE: f32 = 0.20;
/// Minimum seconds between rollbacks (prevents rollback storms).
const ROLLBACK_COOLDOWN_S: u64 = 120;

// ── Global state ──────────────────────────────────────────────────────────────

pub static DRIFT_EVENTS: AtomicU64 = AtomicU64::new(0);
pub static ROLLBACK_COUNT: AtomicU64 = AtomicU64::new(0);

struct DetectorState {
    history: VecDeque<f32>,
    reversal_count: u32,
    boundary_ticks: u32,
    baseline: f32,
    last_rollback: Option<Instant>,
}

impl DetectorState {
    fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(HISTORY_LEN),
            reversal_count: 0,
            boundary_ticks: 0,
            baseline: adaptive_threshold::current(),
            last_rollback: None,
        }
    }
}

static STATE: Lazy<Mutex<DetectorState>> = Lazy::new(|| Mutex::new(DetectorState::new()));

// ── Drift kind ────────────────────────────────────────────────────────────────

/// Classification of the detected drift condition.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum DriftKind {
    None,
    /// Threshold consistently increasing or decreasing over the history window.
    MonotonicDrift,
    /// Threshold reversing direction faster than the adaptation engine can stabilise.
    Oscillation,
    /// Threshold stuck at MIN or MAX boundary for extended time.
    Runaway,
}

impl DriftKind {
    pub fn is_problem(&self) -> bool {
        *self != DriftKind::None
    }
}

/// Report from a single drift-detection sample.
#[derive(Clone, Debug, serde::Serialize)]
pub struct DriftReport {
    pub kind: DriftKind,
    /// Net threshold change from baseline.
    pub drift_magnitude: f32,
    /// Current threshold.
    pub current_threshold: f32,
    /// Saved baseline threshold.
    pub baseline: f32,
    /// Number of direction reversals in the history window.
    pub reversals: u32,
    /// Consecutive ticks the threshold has been at a boundary.
    pub boundary_ticks: u32,
    /// Total drift events recorded since startup.
    pub drift_events_total: u64,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Sample the current adaptive threshold and run drift detection.
///
/// Should be called periodically (e.g., every 60 s from the production watchdog).
/// Returns the current drift report and increments `DRIFT_EVENTS` if a problem is found.
pub fn sample_and_detect() -> DriftReport {
    let current = adaptive_threshold::current();
    let mut state = STATE.lock();

    // Update history ring.
    if state.history.len() >= HISTORY_LEN {
        state.history.pop_front();
    }
    state.history.push_back(current);

    // Compute direction reversals.
    state.reversal_count = count_reversals(&state.history);

    // Boundary tracking.
    if current <= adaptive_threshold::MIN_THRESHOLD + SIGNIFICANT_DELTA
        || current >= adaptive_threshold::MAX_THRESHOLD - SIGNIFICANT_DELTA
    {
        state.boundary_ticks += 1;
    } else {
        state.boundary_ticks = 0;
    }

    let drift_magnitude = current - state.baseline;

    // Classify.
    let kind = if state.reversal_count >= OSCILLATION_REVERSAL_LIMIT {
        DriftKind::Oscillation
    } else if state.boundary_ticks >= RUNAWAY_TICKS {
        DriftKind::Runaway
    } else if drift_magnitude.abs() > MAX_DRIFT_FROM_BASELINE
        && state.history.len() >= HISTORY_LEN
    {
        DriftKind::MonotonicDrift
    } else {
        DriftKind::None
    };

    if kind.is_problem() {
        DRIFT_EVENTS.fetch_add(1, Ordering::Relaxed);
        warn!(
            "[DRIFT] Detected {:?} threshold={:.3} baseline={:.3} drift={:.3} reversals={} boundary_ticks={}",
            kind, current, state.baseline, drift_magnitude,
            state.reversal_count, state.boundary_ticks
        );
    }

    DriftReport {
        kind,
        drift_magnitude,
        current_threshold: current,
        baseline: state.baseline,
        reversals: state.reversal_count,
        boundary_ticks: state.boundary_ticks,
        drift_events_total: DRIFT_EVENTS.load(Ordering::Relaxed),
    }
}

/// Save the current threshold as the rollback baseline.
///
/// Should be called when the system is known to be in a stable state
/// (e.g., at startup after 60 s of normal operation).
pub fn save_baseline() {
    let current = adaptive_threshold::current();
    STATE.lock().baseline = current;
    info!("[DRIFT] Baseline saved: threshold={:.3}", current);
}

/// Roll back the adaptive threshold to the saved baseline.
///
/// **Deterministic:** always rolls back to the value that was explicitly saved
/// with `save_baseline()`.  Never hardcodes a value; uses the runtime-measured
/// stable operating point.
///
/// Rate-limited: no-op if a rollback occurred within `ROLLBACK_COOLDOWN_S`.
/// Returns `true` if the rollback was executed.
pub fn rollback_to_baseline(reason: &str) -> bool {
    let mut state = STATE.lock();

    if let Some(last) = state.last_rollback {
        if last.elapsed().as_secs() < ROLLBACK_COOLDOWN_S {
            warn!("[DRIFT] Rollback requested but on cooldown (reason={})", reason);
            return false;
        }
    }

    let target = state.baseline;
    state.last_rollback = Some(Instant::now());
    state.history.clear();
    state.reversal_count = 0;
    state.boundary_ticks = 0;

    adaptive_threshold::force_set(target);
    ROLLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
    warn!(
        "[DRIFT] ROLLBACK threshold → {:.3} reason={} rollbacks={}",
        target, reason, ROLLBACK_COUNT.load(Ordering::Relaxed)
    );

    write_rollback_event(target, reason);
    true
}

fn count_reversals(history: &VecDeque<f32>) -> u32 {
    let v: Vec<f32> = history.iter().copied().collect();
    if v.len() < 3 {
        return 0;
    }
    let mut reversals = 0u32;
    let mut prev_dir: Option<bool> = None; // true = up, false = down
    for w in v.windows(2) {
        let delta = w[1] - w[0];
        if delta.abs() < SIGNIFICANT_DELTA { continue; }
        let dir = delta > 0.0;
        if let Some(prev) = prev_dir {
            if prev != dir { reversals += 1; }
        }
        prev_dir = Some(dir);
    }
    reversals
}

fn write_rollback_event(target: f32, reason: &str) {
    use std::io::Write;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let reason_esc = reason.replace('"', "\\\"");
    let line = format!(
        "{{\"ts\":{},\"event\":\"adaptive_rollback\",\"target_threshold\":{:.4},\"reason\":\"{}\"}}",
        ts, target, reason_esc
    );
    if let Some(dir) = jarvis_core::APP_LOG_DIR.get() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(dir.join("adaptive_events.jsonl"))
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_reversals_monotone_is_zero() {
        let mut h = VecDeque::new();
        for i in 0..10 { h.push_back(0.40 + i as f32 * 0.01); }
        assert_eq!(count_reversals(&h), 0);
    }

    #[test]
    fn count_reversals_alternating_is_nonzero() {
        let mut h = VecDeque::new();
        for i in 0..10u32 {
            h.push_back(if i % 2 == 0 { 0.50 } else { 0.60 });
        }
        assert!(count_reversals(&h) > 0);
    }

    #[test]
    fn drift_kind_none_is_not_problem() {
        assert!(!DriftKind::None.is_problem());
    }

    #[test]
    fn drift_kinds_are_problems() {
        assert!(DriftKind::Oscillation.is_problem());
        assert!(DriftKind::Runaway.is_problem());
        assert!(DriftKind::MonotonicDrift.is_problem());
    }
}
