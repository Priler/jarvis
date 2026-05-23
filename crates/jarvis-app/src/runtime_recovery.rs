//! Autonomous production recovery coordinator.
//!
//! Orchestrates multi-step recovery sequences for the production runtime.
//! Operates at a higher level than the L1–L4 recovery actions in `recovery.rs`:
//! it decides *which* recovery action to apply based on the current mode and
//! the drift/health state.
//!
//! Recovery principles:
//!   1. Never bypass lifecycle contracts.
//!   2. Never duplicate commands via unsafe resets during active sessions.
//!   3. Rollback is always deterministic (to a saved baseline).
//!   4. Each recovery step is logged and rate-limited.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::adaptive_drift_detector;
use crate::runtime_health;
use crate::runtime_modes::{self, ProductionMode};

// ── Counters ──────────────────────────────────────────────────────────────────

pub static AUTONOMOUS_RECOVERIES: AtomicU64 = AtomicU64::new(0);
pub static ADAPTIVE_ROLLBACKS: AtomicU64 = AtomicU64::new(0);

// ── Recovery action kinds ─────────────────────────────────────────────────────

/// The kind of autonomous recovery action taken.
#[derive(Clone, Debug, serde::Serialize)]
pub enum RecoveryActionKind {
    /// Soft recognizer + gate reset (delegates to existing L1).
    SoftReset,
    /// Adaptive threshold rolled back to saved baseline.
    AdaptiveRollback,
    /// Threshold adaptation frozen for a cooldown period.
    AdaptiveFreeze,
    /// Production mode de-escalated to Normal after health recovery.
    ModeDeescalation,
    /// No action — health is acceptable.
    NoOp,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct RecoveryEvent {
    pub kind: RecoveryActionKind,
    pub reason: String,
    pub ts_ms: u64,
    pub health_before: u8,
}

// ── Recovery cooldown state ───────────────────────────────────────────────────

struct RecoveryState {
    last_soft_reset: Option<Instant>,
    last_adaptive_action: Option<Instant>,
    last_deescalation: Option<Instant>,
    freeze_until: Option<Instant>,
}

impl RecoveryState {
    fn new() -> Self {
        Self {
            last_soft_reset: None,
            last_adaptive_action: None,
            last_deescalation: None,
            freeze_until: None,
        }
    }

    fn soft_reset_on_cooldown(&self) -> bool {
        self.last_soft_reset
            .map_or(false, |t| t.elapsed() < Duration::from_secs(30))
    }

    fn adaptive_action_on_cooldown(&self) -> bool {
        self.last_adaptive_action
            .map_or(false, |t| t.elapsed() < Duration::from_secs(120))
    }

    fn deescalation_on_cooldown(&self) -> bool {
        self.last_deescalation
            .map_or(false, |t| t.elapsed() < Duration::from_secs(60))
    }

    fn is_frozen(&self) -> bool {
        self.freeze_until.map_or(false, |t| Instant::now() < t)
    }
}

static STATE: Lazy<Mutex<RecoveryState>> = Lazy::new(|| Mutex::new(RecoveryState::new()));

// ── Public API ────────────────────────────────────────────────────────────────

/// Main autonomous recovery decision function.
///
/// Called periodically by the production watchdog.  Evaluates current health
/// and drift state and takes the appropriate recovery action.
///
/// Returns the action taken (for logging / observability).
pub fn run_autonomous_recovery() -> RecoveryEvent {
    let health = runtime_health::ExtendedHealth::compute();
    let health_before = health.overall;

    // ── Drift-driven adaptive rollback ────────────────────────────────────────
    let drift = adaptive_drift_detector::sample_and_detect();
    if drift.kind.is_problem() && !STATE.lock().adaptive_action_on_cooldown() {
        let reason = format!("drift_kind={:?} magnitude={:.3}", drift.kind, drift.drift_magnitude);
        runtime_modes::escalate(ProductionMode::Degraded, "adaptive_drift");
        let rolled_back = adaptive_drift_detector::rollback_to_baseline(&reason);
        STATE.lock().last_adaptive_action = Some(Instant::now());
        ADAPTIVE_ROLLBACKS.fetch_add(1, Ordering::Relaxed);
        AUTONOMOUS_RECOVERIES.fetch_add(1, Ordering::Relaxed);
        let kind = if rolled_back {
            RecoveryActionKind::AdaptiveRollback
        } else {
            RecoveryActionKind::AdaptiveFreeze
        };
        let ev = RecoveryEvent { kind, reason, ts_ms: now_ms(), health_before };
        write_recovery_event(&ev);
        return ev;
    }

    // ── Health-driven soft reset ──────────────────────────────────────────────
    if health.is_critical() && !STATE.lock().soft_reset_on_cooldown() {
        let reason = format!("health_critical score={}", health.overall);
        runtime_modes::escalate(ProductionMode::Recovery, "health_critical");
        crate::recovery::execute_l1_full_soft_reset();
        STATE.lock().last_soft_reset = Some(Instant::now());
        AUTONOMOUS_RECOVERIES.fetch_add(1, Ordering::Relaxed);
        let ev = RecoveryEvent {
            kind: RecoveryActionKind::SoftReset,
            reason, ts_ms: now_ms(), health_before,
        };
        write_recovery_event(&ev);
        return ev;
    }

    // ── De-escalation when health recovers ────────────────────────────────────
    let mode = runtime_modes::current();
    if mode != ProductionMode::Normal
        && mode != ProductionMode::Recovery
        && health.overall >= 75
        && !drift.kind.is_problem()
        && !STATE.lock().deescalation_on_cooldown()
    {
        let reason = format!("health_recovered score={}", health.overall);
        runtime_modes::try_recover(&reason);
        STATE.lock().last_deescalation = Some(Instant::now());
        let ev = RecoveryEvent {
            kind: RecoveryActionKind::ModeDeescalation,
            reason, ts_ms: now_ms(), health_before,
        };
        write_recovery_event(&ev);
        return ev;
    }

    RecoveryEvent {
        kind: RecoveryActionKind::NoOp,
        reason: format!("health_ok score={} mode={}", health.overall, runtime_modes::current()),
        ts_ms: now_ms(),
        health_before,
    }
}

/// Explicitly freeze adaptation for a duration (e.g., after repeated drift).
pub fn freeze_adaptation(duration: Duration) {
    let mut state = STATE.lock();
    state.freeze_until = Some(Instant::now() + duration);
    state.last_adaptive_action = Some(Instant::now());
    info!("[RECOVERY] Adaptation frozen for {}s", duration.as_secs());
}

/// Returns true if adaptation is currently frozen by the recovery engine.
pub fn is_adaptation_frozen() -> bool {
    STATE.lock().is_frozen()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn write_recovery_event(ev: &RecoveryEvent) {
    info!(
        "[RECOVERY] {:?} health_before={} reason={}",
        ev.kind, ev.health_before, ev.reason
    );
    let json = serde_json::to_string(ev).unwrap_or_else(|_| "{}".into());
    if let Some(dir) = jarvis_core::APP_LOG_DIR.get() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(dir.join("autonomous_recovery.jsonl"))
        {
            let _ = writeln!(f, "{}", json);
        }
    }
}
