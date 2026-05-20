#![allow(dead_code)]

//! Central watchdog thread.
//!
//! Monitors all runtime subsystems via heartbeat atomics and dispatches
//! recovery actions when thresholds are exceeded.  The watchdog thread
//! is the only external authority over the pipeline lifecycle — it never
//! panics and never blocks.
//!
//! Checks performed every WATCHDOG_INTERVAL_MS:
//!  1. Recorder frozen   — LAST_STT_FRAME_MS stale while not in WAV mode
//!  2. Speaking gate     — FORCED_GATE_RESETS increment rate
//!  3. Recovery storm    — RECOVERY_TOTAL rapid escalation → L3 degraded mode
//!  4. IPC silent        — LAST_IPC_ACTIVITY_MS warning threshold

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use jarvis_core::recorder;

use crate::stt_worker::{
    FORCED_GATE_RESETS, LAST_IPC_ACTIVITY_MS, LAST_STT_FRAME_MS, RECOVERY_TOTAL,
};

// ── Configuration ─────────────────────────────────────────────────────────────

/// How often the watchdog wakes up.
const WATCHDOG_INTERVAL_MS: u64 = 1_000;
/// Startup grace period before first check.
const STARTUP_GRACE_S: u64 = 15;
/// Recorder frozen if no STT frame received for this long (live mode only).
const RECORDER_FROZEN_THRESHOLD_MS: u64 = 10_000;
/// Minimum interval between recorder restart attempts.
const RECORDER_RESTART_COOLDOWN_S: u64 = 30;
/// FORCED_GATE_RESETS increase per interval that triggers a warning.
const GATE_RAPID_RESET_RATE: u64 = 3;
/// RECOVERY_TOTAL increase per interval that triggers L3.
const RECOVERY_STORM_THRESHOLD: u64 = 8;
/// Minimum interval between L3 activations.
const L3_COOLDOWN_S: u64 = 300;
/// IPC silent warning threshold.
const IPC_IDLE_WARNING_MS: u64 = 300_000;

// ── Global flags ──────────────────────────────────────────────────────────────

/// Watchdog has issued a recorder stop and requests a start on the app thread.
/// `app.rs` checks this at the top of each capture loop iteration.
pub static RECORDER_RESTART_REQUEST: AtomicBool = AtomicBool::new(false);

/// Watchdog has degraded the runtime.
/// `stt_worker.rs` drains frames without recognizer calls when set.
pub static DEGRADED_MODE: AtomicBool = AtomicBool::new(false);

/// Watchdog has requested a graceful shutdown after unrecoverable failure.
/// `app.rs` checks this alongside `should_stop()`.
pub static WATCHDOG_SHUTDOWN_REQUEST: AtomicBool = AtomicBool::new(false);

/// Total incidents raised by the watchdog across all runs.
pub static WATCHDOG_INCIDENTS: AtomicU64 = AtomicU64::new(0);

// ── Watchdog state ────────────────────────────────────────────────────────────

struct WatchdogState {
    startup_ts: Instant,
    last_forced_gate_resets: u64,
    last_recovery_total: u64,
    last_recorder_restart: Option<Instant>,
    last_l3_activation: Option<Instant>,
    health_tick: u32,
}

impl WatchdogState {
    fn new() -> Self {
        Self {
            startup_ts: Instant::now(),
            last_forced_gate_resets: FORCED_GATE_RESETS.load(Ordering::Relaxed),
            last_recovery_total: RECOVERY_TOTAL.load(Ordering::Relaxed),
            last_recorder_restart: None,
            last_l3_activation: None,
            health_tick: 0,
        }
    }

    fn in_startup_grace(&self) -> bool {
        self.startup_ts.elapsed().as_secs() < STARTUP_GRACE_S
    }

    fn recorder_restart_on_cooldown(&self) -> bool {
        self.last_recorder_restart
            .map_or(false, |t| t.elapsed().as_secs() < RECORDER_RESTART_COOLDOWN_S)
    }

    fn l3_on_cooldown(&self) -> bool {
        self.last_l3_activation
            .map_or(false, |t| t.elapsed().as_secs() < L3_COOLDOWN_S)
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Spawn the watchdog background thread.
/// Must be called after the pipeline has been fully initialised.
pub fn start() {
    std::thread::Builder::new()
        .name("watchdog".into())
        .spawn(watchdog_main)
        .expect("Failed to spawn watchdog thread");
    info!("[WATCHDOG] Started (interval={}ms, frozen_threshold={}ms)",
        WATCHDOG_INTERVAL_MS, RECORDER_FROZEN_THRESHOLD_MS);
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn watchdog_main() {
    let mut state = WatchdogState::new();

    loop {
        std::thread::sleep(Duration::from_millis(WATCHDOG_INTERVAL_MS));

        // Skip all checks in WAV replay mode (deterministic, controlled).
        if recorder::is_wav_mode() {
            continue;
        }

        if state.in_startup_grace() {
            continue;
        }

        // If already degraded, just emit a periodic reminder and skip active checks.
        if DEGRADED_MODE.load(Ordering::Relaxed) {
            info!("[WATCHDOG] Runtime is in degraded mode — voice processing suspended");
            continue;
        }

        let now = now_ms();
        check_recorder_frozen(&mut state, now);
        check_gate_rapid_resets(&mut state);
        check_recovery_storm(&mut state);
        check_ipc_silent(now);

        // GC: expire stale pending confirmations (>30 s without user response).
        jarvis_core::commands::expire_pending_confirm(30);

        // Emit structured health score every 60 s.
        state.health_tick += 1;
        if state.health_tick >= 60 {
            state.health_tick = 0;
            crate::health::RuntimeHealth::compute().log();
        }
    }
}

// ── Incident file writer ──────────────────────────────────────────────────────

fn write_incident_line(kind: &str, detail: &str) {
    let ts = now_ms();
    let detail_esc = detail.replace('\\', "\\\\").replace('"', "\\\"");
    let line = format!("{{\"ts\":{},\"kind\":\"{}\",\"detail\":\"{}\"}}", ts, kind, detail_esc);
    if let Some(dir) = jarvis_core::APP_LOG_DIR.get() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(dir.join("incidents.jsonl"))
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ── Per-check functions ───────────────────────────────────────────────────────

fn check_recorder_frozen(state: &mut WatchdogState, now: u64) {
    let last_frame_ms = LAST_STT_FRAME_MS.load(Ordering::Relaxed);
    if last_frame_ms == 0 {
        return; // no frame received yet since startup
    }

    let age_ms = now.saturating_sub(last_frame_ms);
    if age_ms < RECORDER_FROZEN_THRESHOLD_MS {
        return;
    }

    if state.recorder_restart_on_cooldown() {
        return;
    }

    WATCHDOG_INCIDENTS.fetch_add(1, Ordering::Relaxed);
    warn!(
        "[WATCHDOG] INCIDENT recorder_frozen: last_frame_age={}ms threshold={}ms \
         — L2 recorder restart",
        age_ms, RECORDER_FROZEN_THRESHOLD_MS
    );
    write_incident_line("recorder_frozen", &format!("age_ms={}", age_ms));

    crate::recovery::execute_l2_recorder_restart();
    state.last_recorder_restart = Some(Instant::now());
}

fn check_gate_rapid_resets(state: &mut WatchdogState) {
    let current = FORCED_GATE_RESETS.load(Ordering::Relaxed);
    let delta = current.saturating_sub(state.last_forced_gate_resets);
    state.last_forced_gate_resets = current;

    if delta >= GATE_RAPID_RESET_RATE {
        WATCHDOG_INCIDENTS.fetch_add(1, Ordering::Relaxed);
        warn!(
            "[WATCHDOG] INCIDENT gate_rapid_resets: delta_per_interval={} total={} \
             — speaking gate is stuck repeatedly (Audit P0-1)",
            delta, current
        );
        write_incident_line("gate_rapid_resets", &format!("delta={} total={}", delta, current));
        // L1 gate clear is already handled per-frame in stt_worker's check_speaking_gate_stuck.
        // Cross-thread safety net: clear once more here.
        jarvis_core::audio::force_clear_speaking();
    }
}

fn check_recovery_storm(state: &mut WatchdogState) {
    let current = RECOVERY_TOTAL.load(Ordering::Relaxed);
    let delta = current.saturating_sub(state.last_recovery_total);
    state.last_recovery_total = current;

    if delta < RECOVERY_STORM_THRESHOLD {
        return;
    }

    if state.l3_on_cooldown() {
        warn!(
            "[WATCHDOG] Recovery storm detected (delta={}) but L3 is on cooldown",
            delta
        );
        return;
    }

    WATCHDOG_INCIDENTS.fetch_add(1, Ordering::Relaxed);
    error!(
        "[WATCHDOG] INCIDENT recovery_storm: delta_per_interval={} total={} \
         — entering L3 degraded mode",
        delta, current
    );
    write_incident_line("recovery_storm", &format!("delta={} total={}", delta, current));

    crate::recovery::execute_l3_degraded_mode("watchdog_recovery_storm");
    state.last_l3_activation = Some(Instant::now());
}

fn check_ipc_silent(now: u64) {
    let last_ipc = LAST_IPC_ACTIVITY_MS.load(Ordering::Relaxed);
    if last_ipc == 0 {
        return; // no IPC activity ever — GUI not connected
    }

    let age_ms = now.saturating_sub(last_ipc);
    if age_ms > IPC_IDLE_WARNING_MS {
        warn!(
            "[WATCHDOG] IPC silent for {}s — GUI may be disconnected",
            age_ms / 1000
        );
    }
}
