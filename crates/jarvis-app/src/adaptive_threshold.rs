//! Adaptive threshold engine for wake-word detection.
//!
//! Adjusts Rustpotter's detection threshold in real-time based on
//! environment noise, FP/FN history, and runtime mode.
//! All adjustments are bounded and rate-limited; no uncontrolled drift.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use jarvis_core::config::RUSPOTTER_MIN_SCORE;

use crate::environment_profile::{snapshot, ProfileSnapshot};

// ── Safety bounds ─────────────────────────────────────────────────────────────

/// Absolute minimum threshold — never go below (hyper-sensitivity guard).
pub const MIN_THRESHOLD: f32 = 0.35;
/// Absolute maximum threshold — never go above (deaf guard).
pub const MAX_THRESHOLD: f32 = 0.85;
/// Maximum threshold change per `update_and_apply()` call.
const MAX_RATE_PER_TICK: f32 = 0.015;
/// EMA alpha for smoothing the adaptive target (prevents jitter).
const SMOOTH_ALPHA: f32 = 0.15;

// ── Hysteresis ────────────────────────────────────────────────────────────────

/// Hysteresis gap above/below the current threshold.
/// `enter_threshold = threshold + HYSTERESIS`
/// `exit_threshold  = threshold - HYSTERESIS`
///
/// A wake is confirmed only when score > enter_threshold,
/// and an in-progress session is maintained until score < exit_threshold.
const HYSTERESIS: f32 = 0.03;

// ── Global state ──────────────────────────────────────────────────────────────

/// Current smoothed adaptive threshold (stored as f32 bits).
static CURRENT_THRESHOLD: AtomicU32 = AtomicU32::new(0);
/// Whether adaptive engine is initialized.
static INITIALIZED: AtomicBool = AtomicBool::new(false);

fn load_threshold() -> f32 {
    let bits = CURRENT_THRESHOLD.load(Ordering::Relaxed);
    if bits == 0 { RUSPOTTER_MIN_SCORE } else { f32::from_bits(bits) }
}

fn store_threshold(v: f32) {
    CURRENT_THRESHOLD.store(v.to_bits(), Ordering::Relaxed);
}

// ── Adaptive computation ──────────────────────────────────────────────────────

/// Compute the adaptive target from the environment snapshot.
/// Returns a target threshold; caller smooths and clamps.
fn compute_target(base: f32, snap: &ProfileSnapshot) -> f32 {
    let mut delta: f32 = 0.0;

    // Mode-based adjustment.
    delta += snap.mode.threshold_delta();

    // Noise floor adjustment: higher ambient RMS → raise threshold.
    if snap.ambient_rms > 0.05 {
        delta += ((snap.ambient_rms - 0.05) * 0.40).min(0.10);
    }

    // FP spike adjustment: recent false positives → raise threshold.
    if snap.recent_fp_count > 0 {
        delta += (snap.recent_fp_count as f32 * 0.015).min(0.08);
    }

    // FN adjustment: recent missed wakes → lower threshold.
    if snap.recent_fn_count > 0 {
        delta -= (snap.recent_fn_count as f32 * 0.008).min(0.05);
    }

    // Mic quality degradation → raise threshold (bad mic = less reliable scores).
    if snap.mic_quality < 0.8 {
        delta += (0.8 - snap.mic_quality) * 0.15;
    }

    // Prefer stored threshold if available (session memory).
    let base_with_pref = if snap.preferred_threshold > 0.0 {
        snap.preferred_threshold
    } else {
        base
    };

    (base_with_pref + delta).clamp(MIN_THRESHOLD, MAX_THRESHOLD)
}

/// Called from stt_worker every `ADAPTIVE_UPDATE_EVERY_FRAMES` frames.
/// Recomputes the adaptive threshold and applies it to the Rustpotter engine.
pub fn update_and_apply() {
    let snap = snapshot();
    let base = RUSPOTTER_MIN_SCORE;
    let target = compute_target(base, &snap);

    let current = load_threshold();

    // Smooth towards target.
    let smoothed = current + SMOOTH_ALPHA * (target - current);

    // Rate-limit the change (no sudden jumps).
    let delta = (smoothed - current).clamp(-MAX_RATE_PER_TICK, MAX_RATE_PER_TICK);
    let new_threshold = (current + delta).clamp(MIN_THRESHOLD, MAX_THRESHOLD);

    if (new_threshold - current).abs() > 0.0001 {
        store_threshold(new_threshold);
        jarvis_core::listener::set_min_score(new_threshold);
    }

    if !INITIALIZED.load(Ordering::Relaxed) {
        INITIALIZED.store(true, Ordering::Relaxed);
    }
}

/// Wake enter threshold (with hysteresis).
/// A new wake is confirmed only when score > this value.
pub fn enter_threshold() -> f32 {
    (load_threshold() + HYSTERESIS).min(MAX_THRESHOLD)
}

/// Wake exit threshold (with hysteresis).
/// An active session is maintained while score > this value.
pub fn exit_threshold() -> f32 {
    (load_threshold() - HYSTERESIS).max(MIN_THRESHOLD)
}

/// Current adaptive threshold (without hysteresis).
pub fn current() -> f32 {
    load_threshold()
}

/// Record a wake session close for adaptive learning.
/// `clean = true` → successful session.
/// `clean = false` → dirty close (no command / timeout) = learning signal.
pub fn record_session_close(clean: bool) {
    crate::environment_profile::record_session_close(clean);

    if !clean {
        // Immediately bump threshold by a small amount to reduce FP streak.
        let current = load_threshold();
        let bumped = (current + 0.010).min(MAX_THRESHOLD);
        store_threshold(bumped);
        jarvis_core::listener::set_min_score(bumped);
    }
}

/// Force the adaptive threshold to a specific value (e.g., during Presentation mode).
/// Still respects bounds.
pub fn force_set(v: f32) {
    let clamped = v.clamp(MIN_THRESHOLD, MAX_THRESHOLD);
    store_threshold(clamped);
    jarvis_core::listener::set_min_score(clamped);
}

/// Snapshot of adaptive state for logging / observability.
#[derive(serde::Serialize)]
pub struct AdaptiveState {
    pub current_threshold: f32,
    pub enter_threshold: f32,
    pub exit_threshold: f32,
    pub base_threshold: f32,
    pub mode: &'static str,
    pub ambient_rms: f32,
    pub recent_fp_count: usize,
    pub recent_fn_count: usize,
    pub mic_quality: f32,
}

pub fn current_state() -> AdaptiveState {
    let snap = snapshot();
    AdaptiveState {
        current_threshold: current(),
        enter_threshold: enter_threshold(),
        exit_threshold: exit_threshold(),
        base_threshold: RUSPOTTER_MIN_SCORE,
        mode: snap.mode.as_str(),
        ambient_rms: snap.ambient_rms,
        recent_fp_count: snap.recent_fp_count,
        recent_fn_count: snap.recent_fn_count,
        mic_quality: snap.mic_quality,
    }
}
