#![allow(dead_code)]

//! Recovery engine — implements L1–L4 self-healing actions.
//!
//! Called by the watchdog thread.  All functions are safe to call from
//! any thread.  They operate only on well-established global state via
//! atomic operations and pre-verified APIs.
//!
//! Recovery levels:
//!   L1 — Soft reset  (recognizer state, speaking gate, buffer flush)
//!   L2 — Subsystem restart  (recorder stop+start, recognizer full recreate)
//!   L3 — Degraded mode  (voice suspended, IPC alive, no voice processing)
//!   L4 — Graceful shutdown  (set WATCHDOG_SHUTDOWN_REQUEST, let app clean up)

use std::sync::atomic::Ordering;

use jarvis_core::{audio, recorder, stt, APP_LOG_DIR};

use crate::stt_worker::FORCED_GATE_RESETS;
use crate::watchdog::{DEGRADED_MODE, RECORDER_RESTART_REQUEST, WATCHDOG_SHUTDOWN_REQUEST};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn write_timeline_entry(action: &str, level: u8, reason: &str) {
    let ts = now_ms();
    let reason_esc = reason.replace('\\', "\\\\").replace('"', "\\\"");
    let line = format!(
        "{{\"ts\":{},\"action\":\"{}\",\"level\":\"L{}\",\"reason\":\"{}\"}}",
        ts, action, level, reason_esc
    );
    if let Some(dir) = APP_LOG_DIR.get() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(dir.join("recovery_timeline.jsonl"))
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ── L1: Soft reset ────────────────────────────────────────────────────────────

/// Reset both Vosk recognizers to flush accumulated decoder state.
/// Safe to call from any thread; uses `parking_lot::Mutex` internally.
pub fn execute_l1_recognizer_reset() {
    write_timeline_entry("recognizer_reset", 1, "manual");
    info!("[RECOVERY][L1] Resetting recognizers");
    stt::reset_speech_recognizer();
    stt::reset_wake_recognizer();
    info!("[RECOVERY][L1] Recognizer reset complete");
}

/// Force-clear the speaking gate.
pub fn execute_l1_clear_gate() {
    info!("[RECOVERY][L1] Force-clearing speaking gate");
    audio::force_clear_speaking();
    FORCED_GATE_RESETS.fetch_add(1, Ordering::Relaxed);
}

/// Full L1 soft reset: recognizers + speaking gate.
pub fn execute_l1_full_soft_reset() {
    execute_l1_recognizer_reset();
    execute_l1_clear_gate();
    info!("[RECOVERY][L1] Full soft reset complete");
}

// ── L2: Subsystem restart ─────────────────────────────────────────────────────

/// Trigger a recorder stop+start cycle.
///
/// This function:
///  1. Calls `recorder::stop_recording()` to unblock any frozen `read()` in app.rs.
///  2. Sets `RECORDER_RESTART_REQUEST` so app.rs performs the `start_recording()`
///     call on its own thread at the top of the next capture loop iteration.
///
/// This two-phase approach is necessary because PvRecorder is not safe to start
/// from a different thread than the one that reads from it.
pub fn execute_l2_recorder_restart() {
    write_timeline_entry("recorder_restart", 2, "recorder_frozen");
    info!("[RECOVERY][L2] Recorder restart — stopping to unblock frozen read");
    if recorder::stop_recording().is_err() {
        warn!("[RECOVERY][L2] stop_recording failed (may already be stopped)");
    }
    RECORDER_RESTART_REQUEST.store(true, Ordering::Release);
    info!("[RECOVERY][L2] Recorder restart request set — app.rs will restart on next frame");
}

/// Fully recreate both Vosk recognizers from the loaded model.
///
/// Unlike `reset_*`, this discards and rebuilds the recognizer objects from
/// scratch.  Use when the recognizer is suspected to be in an irrecoverable
/// internal state (e.g., repeated SIGSEGV in the Vosk C library).
pub fn execute_l2_recreate_recognizers() {
    info!("[RECOVERY][L2] Recreating speech recognizer");
    match stt::reinit_speech_recognizer() {
        Ok(()) => info!("[RECOVERY][L2] Speech recognizer recreated"),
        Err(e) => error!("[RECOVERY][L2] Speech recognizer recreate failed: {}", e),
    }
    info!("[RECOVERY][L2] Recreating wake recognizer");
    match stt::reinit_wake_recognizer() {
        Ok(()) => info!("[RECOVERY][L2] Wake recognizer recreated"),
        Err(e) => error!("[RECOVERY][L2] Wake recognizer recreate failed: {}", e),
    }
}

// ── L3: Degraded mode ─────────────────────────────────────────────────────────

/// Enter degraded mode.
///
/// Sets `DEGRADED_MODE` so `stt_worker` drains frames without calling any
/// recognizer.  Clears stuck state, sends an IPC error to the GUI.
///
/// Degraded mode persists until the process is restarted.  It is a one-way
/// door: once entered, only a full process restart restores normal operation.
pub fn execute_l3_degraded_mode(reason: &str) {
    if DEGRADED_MODE.swap(true, Ordering::Release) {
        return; // already degraded
    }

    write_timeline_entry("degraded_mode", 3, reason);
    error!("[RECOVERY][L3] DEGRADED MODE ACTIVE — reason={}", reason);

    // Clear any stuck pipeline state before going silent.
    audio::force_clear_speaking();
    stt::reset_speech_recognizer();
    stt::reset_wake_recognizer();

    jarvis_core::ipc::send(jarvis_core::ipc::IpcEvent::Error {
        message: format!(
            "Voice runtime entered degraded mode ({}). \
             Voice commands are unavailable. Restart required.",
            reason
        ),
    });
}

// ── L4: Graceful shutdown ─────────────────────────────────────────────────────

/// Initiate a graceful runtime shutdown.
///
/// Sets `WATCHDOG_SHUTDOWN_REQUEST` which `app.rs` checks alongside `should_stop()`.
/// The app loop will break, play the goodbye sound, send `Stopping` to the GUI,
/// and exit normally.  Use only when continued operation is impossible.
pub fn execute_l4_graceful_restart(reason: &str) {
    write_timeline_entry("graceful_shutdown", 4, reason);
    error!("[RECOVERY][L4] GRACEFUL SHUTDOWN REQUESTED — reason={}", reason);

    jarvis_core::ipc::send(jarvis_core::ipc::IpcEvent::Error {
        message: format!("Runtime is restarting due to: {}. Reconnect shortly.", reason),
    });

    // Brief delay for IPC flush before shutting down.
    std::thread::sleep(std::time::Duration::from_millis(300));

    WATCHDOG_SHUTDOWN_REQUEST.store(true, Ordering::Release);
}

// ── Utility: incident snapshot ────────────────────────────────────────────────

/// Log a structured incident snapshot for post-mortem analysis.
pub fn log_incident_snapshot(subsystem: &str, level: u8, detail: &str) {
    let active_wake = crate::stt_worker::ACTIVE_WAKE_SESSION.load(Ordering::Relaxed);
    let active_cmd = crate::stt_worker::ACTIVE_COMMAND_SESSION.load(Ordering::Relaxed);
    let gate_resets = FORCED_GATE_RESETS.load(Ordering::Relaxed);
    let recovery_total = crate::stt_worker::RECOVERY_TOTAL.load(Ordering::Relaxed);
    let incidents = crate::watchdog::WATCHDOG_INCIDENTS.load(Ordering::Relaxed);

    error!(
        "[RECOVERY][SNAPSHOT] subsystem={} level=L{} detail={} \
         active_wake={} active_cmd={} gate_resets={} \
         recovery_total={} watchdog_incidents={}",
        subsystem, level, detail,
        active_wake, active_cmd, gate_resets,
        recovery_total, incidents,
    );
}
