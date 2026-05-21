//! Multi-signal confidence fusion for wake-word confirmation.
//!
//! Fuses Rustpotter score, VAD confidence, and runtime state signals
//! into a single `FusedConfidence` decision.
//!
//! The fusion gate is **conservative**: it only suppresses wakes when
//! multiple strong negative signals converge.  A wake that passes the
//! adaptive threshold is let through unless the gate explicitly blocks.

use crate::adaptive_threshold;
use crate::environment_profile::RuntimeMode;

// ── Evidence input ────────────────────────────────────────────────────────────

/// Input signals for wake confidence fusion.
pub struct WakeEvidence {
    /// Raw Rustpotter detection score (the score that already passed the
    /// adaptive threshold — so this is always > threshold).
    pub wake_score: f32,
    /// VAD RMS energy at the moment of detection (0.0 – ~32768.0).
    pub vad_rms: f32,
    /// True if the session has had > 2 consecutive timeouts (FP/FN history).
    pub in_problematic_session: bool,
    /// Number of consecutive timeouts in the current session window.
    pub consecutive_timeouts: u32,
}

// ── Output ────────────────────────────────────────────────────────────────────

/// Result of confidence fusion.
#[derive(Debug)]
pub struct FusedConfidence {
    /// Composite score in [0.0, 1.0].
    pub score: f32,
    /// Whether this wake event should be confirmed.
    /// `false` means the wake is suppressed by the fusion gate.
    pub confirmed: bool,
    /// Human-readable reason for suppression (empty if confirmed).
    pub suppression_reason: &'static str,
}

// ── Fusion logic ──────────────────────────────────────────────────────────────

/// Fuse multi-signal evidence into a wake confirmation decision.
///
/// **Safety guarantee:** The fusion gate only blocks a wake when it has
/// strong evidence of a false positive.  When uncertain it passes through.
/// This preserves recall at the cost of marginally higher FPR.
pub fn fuse(ev: &WakeEvidence) -> FusedConfidence {
    let threshold = adaptive_threshold::current();

    // ── Wake score contribution (60%) ─────────────────────────────────────────
    // Normalise so that threshold maps to 0.60 and 1.0 maps to ~1.0.
    let score_norm = if threshold > 0.0 {
        ((ev.wake_score / threshold) * 0.60).min(1.0)
    } else {
        0.60
    };

    // ── VAD confidence contribution (25%) ─────────────────────────────────────
    // Expect at least 500 RMS for real speech; scale proportionally.
    let vad_norm = (ev.vad_rms / 2000.0).clamp(0.0, 1.0) * 0.25;

    // ── Runtime state penalties ────────────────────────────────────────────────
    // Penalise if we're in a problematic session window.
    let state_penalty: f32 = if ev.in_problematic_session {
        0.08 + (ev.consecutive_timeouts as f32 * 0.02).min(0.10)
    } else {
        0.0
    };

    // ── Presentation mode suppression ─────────────────────────────────────────
    // During TTS playback, apply a strong penalty.
    let playback_penalty: f32 = match crate::environment_profile::current_mode() {
        RuntimeMode::Presentation => 0.20,
        RuntimeMode::Noisy => 0.05,
        _ => 0.0,
    };

    // ── Compute fused score ────────────────────────────────────────────────────
    let raw = score_norm + vad_norm;
    let penalised = (raw - state_penalty - playback_penalty).clamp(0.0, 1.0);

    // ── Decision: confirm unless strong negative convergence ──────────────────
    // The gate requires the fused score to drop below 0.35 AND the raw
    // Rustpotter score margin to be thin (within 0.05 of threshold).
    // This is deliberately conservative: a strong Rustpotter score (well
    // above threshold) is always confirmed regardless of other signals.
    let score_margin = ev.wake_score - threshold;
    let is_marginal_score = score_margin < 0.05;

    let (confirmed, suppression_reason) = if penalised < 0.35 && is_marginal_score {
        (false, "fusion_score_below_gate")
    } else if crate::environment_profile::current_mode() == RuntimeMode::Presentation
        && score_margin < 0.08
    {
        (false, "presentation_mode_playback_suppression")
    } else {
        (true, "")
    };

    FusedConfidence { score: penalised, confirmed, suppression_reason }
}

// ── VAD confidence helper ─────────────────────────────────────────────────────

/// Convert raw VAD RMS to a 0.0–1.0 confidence score.
pub fn vad_confidence(rms: f32) -> f32 {
    (rms / 3000.0).clamp(0.0, 1.0)
}
