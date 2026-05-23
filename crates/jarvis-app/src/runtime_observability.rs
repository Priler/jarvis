//! Production runtime observability: snapshot + structured event log.
//!
//! Produces a single `RuntimeSnapshot` JSON document that captures the full
//! production state at a point in time.  Written to disk periodically and on
//! demand.  All fields come from real runtime atomics — nothing is synthesised.

use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

use crate::adaptive_threshold;
use crate::adaptive_drift_detector;
use crate::environment_profile;
use crate::runtime_health;
use crate::runtime_modes;
use crate::runtime_recovery;
use crate::stt_worker::{
    ACTIVE_COMMAND_SESSION, ACTIVE_WAKE_SESSION, FORCED_GATE_RESETS,
    LAST_IPC_ACTIVITY_MS, LAST_STT_FRAME_MS, RECOGNIZER_REBUILDS,
    RECOVERY_FAILED, RECOVERY_TOTAL,
};
use crate::watchdog::{DEGRADED_MODE, WATCHDOG_INCIDENTS};

// ── Snapshot ──────────────────────────────────────────────────────────────────

/// Complete production state snapshot for offline observability.
#[derive(serde::Serialize)]
pub struct RuntimeSnapshot {
    pub ts_ms: u64,
    pub uptime_s: u64,

    // ── Health ─────────────────────────────────────────────────────────────────
    pub health_overall: u8,
    pub health_audio: u8,
    pub health_stt: u8,
    pub health_ipc: u8,
    pub health_wake_reliability: u8,
    pub health_fp: u8,
    pub health_fn: u8,
    pub health_adaptive: u8,
    pub degraded_mode: bool,

    // ── Thresholds ─────────────────────────────────────────────────────────────
    pub threshold_current: f32,
    pub threshold_enter: f32,
    pub threshold_exit: f32,
    pub threshold_base: f32,

    // ── Adaptive drift ─────────────────────────────────────────────────────────
    pub drift_kind: String,
    pub drift_magnitude: f32,
    pub drift_events_total: u64,
    pub rollback_count: u64,

    // ── Environment profile ────────────────────────────────────────────────────
    pub runtime_mode: String,
    pub ambient_rms: f32,
    pub recent_fp_count: usize,
    pub recent_fn_count: usize,
    pub mic_quality: f32,

    // ── Session state ──────────────────────────────────────────────────────────
    pub active_wake_session: u64,
    pub active_command_session: u64,

    // ── Production mode ────────────────────────────────────────────────────────
    pub production_mode: String,
    pub production_mode_age_s: u64,
    pub mode_transitions: u64,

    // ── Failure counters ───────────────────────────────────────────────────────
    pub watchdog_incidents: u64,
    pub recovery_total: u64,
    pub recovery_failed: u64,
    pub recognizer_rebuilds: u64,
    pub forced_gate_resets: u64,
    pub autonomous_recoveries: u64,
    pub adaptive_rollbacks: u64,

    // ── IPC heartbeat ──────────────────────────────────────────────────────────
    pub last_ipc_activity_ms: u64,
    pub last_stt_frame_ms: u64,
}

static START_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Record startup timestamp (call once at program start).
pub fn record_startup() {
    let ms = now_ms();
    START_MS.store(ms, Ordering::Relaxed);
}

fn uptime_s() -> u64 {
    let start = START_MS.load(Ordering::Relaxed);
    if start == 0 { return 0; }
    now_ms().saturating_sub(start) / 1000
}

/// Collect the current runtime snapshot.
pub fn collect_snapshot() -> RuntimeSnapshot {
    let health = runtime_health::ExtendedHealth::compute();
    let drift = adaptive_drift_detector::sample_and_detect();
    let snap = environment_profile::snapshot();

    RuntimeSnapshot {
        ts_ms: now_ms(),
        uptime_s: uptime_s(),

        health_overall: health.overall,
        health_audio: health.audio_score,
        health_stt: health.stt_score,
        health_ipc: health.ipc_score,
        health_wake_reliability: health.wake_reliability,
        health_fp: health.fp_health,
        health_fn: health.fn_health,
        health_adaptive: health.adaptive_stability,
        degraded_mode: DEGRADED_MODE.load(Ordering::Relaxed),

        threshold_current: adaptive_threshold::current(),
        threshold_enter: adaptive_threshold::enter_threshold(),
        threshold_exit: adaptive_threshold::exit_threshold(),
        threshold_base: jarvis_core::config::RUSPOTTER_MIN_SCORE,

        drift_kind: format!("{:?}", drift.kind),
        drift_magnitude: drift.drift_magnitude,
        drift_events_total: adaptive_drift_detector::DRIFT_EVENTS.load(Ordering::Relaxed),
        rollback_count: adaptive_drift_detector::ROLLBACK_COUNT.load(Ordering::Relaxed),

        runtime_mode: snap.mode.as_str().to_string(),
        ambient_rms: snap.ambient_rms,
        recent_fp_count: snap.recent_fp_count,
        recent_fn_count: snap.recent_fn_count,
        mic_quality: snap.mic_quality,

        active_wake_session: ACTIVE_WAKE_SESSION.load(Ordering::Acquire),
        active_command_session: ACTIVE_COMMAND_SESSION.load(Ordering::Acquire),

        production_mode: runtime_modes::current().as_str().to_string(),
        production_mode_age_s: runtime_modes::mode_age_s(),
        mode_transitions: runtime_modes::transition_count(),

        watchdog_incidents: WATCHDOG_INCIDENTS.load(Ordering::Relaxed),
        recovery_total: RECOVERY_TOTAL.load(Ordering::Relaxed),
        recovery_failed: RECOVERY_FAILED.load(Ordering::Relaxed),
        recognizer_rebuilds: RECOGNIZER_REBUILDS.load(Ordering::Relaxed),
        forced_gate_resets: FORCED_GATE_RESETS.load(Ordering::Relaxed),
        autonomous_recoveries: runtime_recovery::AUTONOMOUS_RECOVERIES.load(Ordering::Relaxed),
        adaptive_rollbacks: runtime_recovery::ADAPTIVE_ROLLBACKS.load(Ordering::Relaxed),

        last_ipc_activity_ms: LAST_IPC_ACTIVITY_MS.load(Ordering::Relaxed),
        last_stt_frame_ms: LAST_STT_FRAME_MS.load(Ordering::Relaxed),
    }
}

/// Write the current snapshot to `<log_dir>/runtime_snapshot.json`.
/// Overwrites on each call (always the latest state).
pub fn write_runtime_snapshot() {
    let snap = collect_snapshot();
    let json = match serde_json::to_string_pretty(&snap) {
        Ok(j) => j,
        Err(e) => { warn!("[OBS] Failed to serialize snapshot: {}", e); return; }
    };
    if let Some(dir) = jarvis_core::APP_LOG_DIR.get() {
        let path = dir.join("runtime_snapshot.json");
        if let Err(e) = std::fs::write(&path, &json) {
            warn!("[OBS] Failed to write snapshot: {}", e);
        } else {
            debug!("[OBS] Snapshot written: {:?}", path);
        }
    }
}

/// Append a structured production event to `production_events.jsonl`.
pub fn log_production_event(kind: &str, detail: &str) {
    let ts = now_ms();
    let detail_esc = detail.replace('"', "\\\"");
    let line = format!(
        "{{\"ts\":{},\"kind\":\"{}\",\"detail\":\"{}\"}}",
        ts, kind, detail_esc
    );
    if let Some(dir) = jarvis_core::APP_LOG_DIR.get() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(dir.join("production_events.jsonl"))
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
