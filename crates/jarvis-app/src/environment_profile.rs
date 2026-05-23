//! Online environment profiling for adaptive wake intelligence.
//!
//! Tracks ambient noise, mic stability, FP/FN density, and session quality.
//! Updated from the audio frame loop; read by the adaptive threshold engine.

use std::collections::VecDeque;
use std::time::SystemTime;

use once_cell::sync::Lazy;
use parking_lot::Mutex;

// ── Runtime mode ─────────────────────────────────────────────────────────────

/// Operational mode selection for the adaptive wake engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RuntimeMode {
    /// Very low ambient noise — maximise sensitivity, minimum threshold bump.
    Quiet = 0,
    /// Balanced default.
    Normal = 1,
    /// High ambient noise — conservative threshold, reduced sensitivity.
    Noisy = 2,
    /// TTS/speaker output playing — ultra-suppressed, maximum threshold.
    Presentation = 3,
}

impl RuntimeMode {
    /// Threshold delta (added to base) for this mode.
    pub fn threshold_delta(self) -> f32 {
        match self {
            Self::Quiet => -0.03,
            Self::Normal => 0.00,
            Self::Noisy => 0.06,
            Self::Presentation => 0.18,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quiet => "quiet",
            Self::Normal => "normal",
            Self::Noisy => "noisy",
            Self::Presentation => "presentation",
        }
    }
}

// ── Environment profile state ─────────────────────────────────────────────────

const AMBIENT_EMA_ALPHA: f32 = 0.02;        // slow update for noise floor
const STABILITY_EMA_ALPHA: f32 = 0.05;      // mic stability
const EVENT_WINDOW_SECS: u64 = 300;         // sliding 5-minute window
const MAX_EVENTS_IN_WINDOW: usize = 64;

struct EnvProfileState {
    /// Exponential moving average of non-speech RMS (0.0 – 1.0).
    ambient_rms: f32,
    /// EMA of RMS variance (mic stability proxy).
    noise_variance: f32,
    /// Ring buffer of FP event timestamps (ms since epoch).
    fp_events: VecDeque<u64>,
    /// Ring buffer of FN event timestamps.
    fn_events: VecDeque<u64>,
    /// Total session closes since start.
    total_closes: u32,
    /// Total dirty closes (FP proxy).
    total_dirty_closes: u32,
    /// Currently selected runtime mode.
    mode: RuntimeMode,
    /// True while TTS/playback is active.
    in_playback: bool,
    /// Count of frames observed.
    frame_count: u64,
    /// Preferred threshold loaded from persistence (0.0 = not set).
    pub preferred_threshold: f32,
    /// FP rate estimate (FP events / total closes).
    pub fp_rate: f32,
    /// FN rate estimate.
    pub fn_rate: f32,
    /// Mic quality score 0.0 (bad) – 1.0 (perfect).
    pub mic_quality: f32,
}

impl Default for EnvProfileState {
    fn default() -> Self {
        Self {
            ambient_rms: 0.0,
            noise_variance: 0.0,
            fp_events: VecDeque::new(),
            fn_events: VecDeque::new(),
            total_closes: 0,
            total_dirty_closes: 0,
            mode: RuntimeMode::Normal,
            in_playback: false,
            frame_count: 0,
            preferred_threshold: 0.0,
            fp_rate: 0.0,
            fn_rate: 0.0,
            mic_quality: 1.0,
        }
    }
}

impl EnvProfileState {
    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn purge_old_events(&mut self) {
        let cutoff = Self::now_ms().saturating_sub(EVENT_WINDOW_SECS * 1000);
        while self.fp_events.front().map_or(false, |&t| t < cutoff) {
            self.fp_events.pop_front();
        }
        while self.fn_events.front().map_or(false, |&t| t < cutoff) {
            self.fn_events.pop_front();
        }
    }

    fn update_mode(&mut self) {
        if self.in_playback {
            self.mode = RuntimeMode::Presentation;
            return;
        }
        self.mode = if self.ambient_rms > 0.12 {
            RuntimeMode::Noisy
        } else if self.ambient_rms < 0.02 {
            RuntimeMode::Quiet
        } else {
            RuntimeMode::Normal
        };
    }

    fn recompute_rates(&mut self) {
        if self.total_closes > 0 {
            self.fp_rate = self.total_dirty_closes as f32 / self.total_closes as f32;
        }
    }

    fn update_mic_quality(&mut self) {
        // Mic quality degrades with high noise variance (unstable readings)
        // and improves over time with stable readings.
        self.mic_quality = (1.0 - self.noise_variance * 5.0).clamp(0.1, 1.0);
    }
}

// ── Global profile ────────────────────────────────────────────────────────────

static PROFILE: Lazy<Mutex<EnvProfileState>> = Lazy::new(|| Mutex::new(EnvProfileState::default()));

/// Called from the stt_worker frame loop on every audio frame.
pub fn observe_frame(vad_rms: f32, is_voice: bool) {
    let mut p = PROFILE.lock();
    p.frame_count += 1;

    // Update ambient RMS only on non-speech frames.
    if !is_voice {
        let old = p.ambient_rms;
        p.ambient_rms = old + AMBIENT_EMA_ALPHA * (vad_rms - old);
        let var_update = (vad_rms - old).abs();
        p.noise_variance = p.noise_variance + STABILITY_EMA_ALPHA * (var_update - p.noise_variance);
        p.update_mic_quality();
    }

    // Update mode every 200 frames (~6.4 s).
    if p.frame_count % 200 == 0 {
        p.update_mode();
    }
}

/// Record a wake session close outcome.
/// `clean = true` → successful session (command dispatched).
/// `clean = false` → dirty close (timeout / no command) = FP/FN candidate.
pub fn record_session_close(clean: bool) {
    let mut p = PROFILE.lock();
    p.total_closes += 1;
    if !clean {
        p.total_dirty_closes += 1;
        let ts = EnvProfileState::now_ms();
        p.fp_events.push_back(ts);
        if p.fp_events.len() > MAX_EVENTS_IN_WINDOW {
            p.fp_events.pop_front();
        }
    }
    p.purge_old_events();
    p.recompute_rates();
}

/// Record a command-less wake (silence / wall-clock timeout) as FN signal.
pub fn record_command_timeout() {
    let mut p = PROFILE.lock();
    let ts = EnvProfileState::now_ms();
    p.fn_events.push_back(ts);
    if p.fn_events.len() > MAX_EVENTS_IN_WINDOW {
        p.fn_events.pop_front();
    }
}

/// Signal that TTS/audio playback has started (switches to Presentation mode).
pub fn set_playback(active: bool) {
    let mut p = PROFILE.lock();
    p.in_playback = active;
    p.update_mode();
}

/// Current runtime mode.
pub fn current_mode() -> RuntimeMode {
    PROFILE.lock().mode
}

/// Snapshot of the current profile for use by the threshold engine.
pub struct ProfileSnapshot {
    pub ambient_rms: f32,
    pub noise_variance: f32,
    pub recent_fp_count: usize,
    pub recent_fn_count: usize,
    pub mode: RuntimeMode,
    pub fp_rate: f32,
    pub mic_quality: f32,
    pub preferred_threshold: f32,
}

/// Take a read-only snapshot (cheap clone).
pub fn snapshot() -> ProfileSnapshot {
    let p = PROFILE.lock();
    ProfileSnapshot {
        ambient_rms: p.ambient_rms,
        noise_variance: p.noise_variance,
        recent_fp_count: p.fp_events.len(),
        recent_fn_count: p.fn_events.len(),
        mode: p.mode,
        fp_rate: p.fp_rate,
        mic_quality: p.mic_quality,
        preferred_threshold: p.preferred_threshold,
    }
}

/// Load preferred threshold from persistence into the profile.
pub fn load_preferred_threshold(v: f32) {
    PROFILE.lock().preferred_threshold = v;
}
