//! Production hardening watchdog — top-level orchestrator.
//!
//! Runs in its own background thread (distinct from `watchdog.rs` which handles
//! the hot-path L1–L4 incident response).  This watchdog operates on a slower
//! timescale (minutes, not seconds) and orchestrates:
//!   - Extended health sampling
//!   - Adaptive drift detection and rollback
//!   - Production mode escalation
//!   - Autonomous recovery decisions
//!   - Periodic runtime snapshot writes
//!   - Long-run stability tracking
//!
//! Does NOT duplicate checks already in `watchdog.rs`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use jarvis_core::recorder;

use crate::adaptive_drift_detector;
use crate::runtime_health;
use crate::runtime_modes::{self, ProductionMode};
use crate::runtime_observability;
use crate::runtime_recovery;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Health check interval.
const HEALTH_INTERVAL_S: u64 = 30;
/// Adaptive drift sample interval.
const DRIFT_INTERVAL_S: u64 = 60;
/// Snapshot write interval.
const SNAPSHOT_INTERVAL_S: u64 = 300;
/// Baseline save: system has been stable for this many seconds before we save.
const BASELINE_SAVE_AFTER_S: u64 = 120;
/// Score below which we escalate to Degraded.
const DEGRADED_THRESHOLD: u8 = 60;
/// Score below which we escalate to Safe.
const SAFE_THRESHOLD: u8 = 35;

// ── Startup guard ─────────────────────────────────────────────────────────────

const STARTUP_GRACE_S: u64 = 30;

// ── Counters ──────────────────────────────────────────────────────────────────

pub static PRODUCTION_WATCHDOG_TICKS: AtomicU64 = AtomicU64::new(0);

// ── Global stop flag ──────────────────────────────────────────────────────────

static STOP: AtomicBool = AtomicBool::new(false);

// ── Entry point ───────────────────────────────────────────────────────────────

/// Spawn the production hardening watchdog thread.
/// Must be called after the pipeline has fully initialised.
pub fn start() {
    runtime_observability::record_startup();
    adaptive_drift_detector::save_baseline();
    std::thread::Builder::new()
        .name("prod-watchdog".into())
        .spawn(production_watchdog_main)
        .expect("Failed to spawn production watchdog thread");
    info!("[PROD-WD] Production hardening watchdog started");
}

/// Signal the production watchdog to stop (e.g., during shutdown).
pub fn stop() {
    STOP.store(true, Ordering::Relaxed);
}

// ── Main loop ─────────────────────────────────────────────────────────────────

fn production_watchdog_main() {
    let startup = Instant::now();
    let mut last_health = Instant::now();
    let mut last_drift = Instant::now();
    let mut last_snapshot = Instant::now();
    let mut baseline_saved = false;

    loop {
        if STOP.load(Ordering::Relaxed) {
            info!("[PROD-WD] Stopping");
            break;
        }

        std::thread::sleep(Duration::from_secs(5));

        // Skip in WAV replay mode — deterministic, controlled.
        if recorder::is_wav_mode() {
            continue;
        }

        // Grace period: let the pipeline stabilise before first checks.
        if startup.elapsed().as_secs() < STARTUP_GRACE_S {
            continue;
        }

        PRODUCTION_WATCHDOG_TICKS.fetch_add(1, Ordering::Relaxed);

        // ── Save baseline once stable ─────────────────────────────────────────
        if !baseline_saved && startup.elapsed().as_secs() >= BASELINE_SAVE_AFTER_S {
            adaptive_drift_detector::save_baseline();
            baseline_saved = true;
            info!("[PROD-WD] Adaptive baseline saved after {}s stable uptime",
                startup.elapsed().as_secs());
        }

        // ── Health check ──────────────────────────────────────────────────────
        if last_health.elapsed().as_secs() >= HEALTH_INTERVAL_S {
            last_health = Instant::now();
            let health = runtime_health::ExtendedHealth::compute();
            health.log();

            // Mode escalation based on health score.
            if health.is_critical() {
                if runtime_modes::escalate(ProductionMode::Safe, "health_critical") {
                    runtime_observability::log_production_event(
                        "mode_escalation",
                        &format!("SAFE score={}", health.overall),
                    );
                }
            } else if health.is_unhealthy() {
                if runtime_modes::escalate(ProductionMode::Degraded, "health_degraded") {
                    runtime_observability::log_production_event(
                        "mode_escalation",
                        &format!("DEGRADED score={}", health.overall),
                    );
                }
            }
        }

        // ── Drift detection + autonomous recovery ─────────────────────────────
        if last_drift.elapsed().as_secs() >= DRIFT_INTERVAL_S {
            last_drift = Instant::now();
            let ev = runtime_recovery::run_autonomous_recovery();
            match ev.kind {
                crate::runtime_recovery::RecoveryActionKind::NoOp => {}
                _ => {
                    runtime_observability::log_production_event(
                        "autonomous_recovery",
                        &ev.reason,
                    );
                }
            }
        }

        // ── Periodic snapshot ─────────────────────────────────────────────────
        if last_snapshot.elapsed().as_secs() >= SNAPSHOT_INTERVAL_S {
            last_snapshot = Instant::now();
            runtime_observability::write_runtime_snapshot();
        }
    }
}

// ── Session integrity check (callable from stt_worker) ───────────────────────

/// Check for impossible session state.
/// Returns a list of integrity violations (empty = clean).
pub fn check_session_integrity() -> Vec<&'static str> {
    let mut violations = Vec::new();
    let wake = crate::stt_worker::ACTIVE_WAKE_SESSION.load(Ordering::Acquire);
    let cmd  = crate::stt_worker::ACTIVE_COMMAND_SESSION.load(Ordering::Acquire);

    // A command session cannot exist without an active wake session.
    if cmd != 0 && wake == 0 {
        violations.push("command_session_without_wake_session");
    }

    violations
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_integrity_clean_state() {
        // Default atomic state: both sessions are 0.
        // No command session without wake session in default state.
        let violations = check_session_integrity();
        // Both are 0, so cmd=0 and the violation check (cmd != 0 && wake == 0) is false.
        assert!(violations.is_empty());
    }
}
