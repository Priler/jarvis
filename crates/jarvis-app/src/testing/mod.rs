#![allow(dead_code)]

pub mod assertions;
pub mod corpus_metadata;
pub mod corpus_runner;
pub mod failure_classifier;
pub mod harness;
pub mod replay;
pub mod report;
pub mod scenario;
pub mod session_log;
pub mod statistical;

use once_cell::sync::OnceCell;
use std::sync::mpsc::SyncSender;
use std::time::SystemTime;

// ── ValidationEvent ───────────────────────────────────────────────────────────
//
// A structured record of every runtime observable the pipeline can emit.
// All fields are cheap to clone (statics, small integers).

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidationEvent {
    /// stt_worker state machine changed state.
    StateTransition {
        from: &'static str,
        to: &'static str,
        legal: bool,
        reason: &'static str,
        wake_sid: u64,
        ts: u64,
    },
    /// A new wake session was opened.
    WakeSessionOpen { session_id: u64, ts: u64 },
    /// A wake session was finalized (Cooldown → QuietWindow).
    WakeSessionClose {
        session_id: u64,
        commands: u32,
        timeouts: u32,
        clean: bool,
        ts: u64,
    },
    /// A command session was opened (text recognized, handed to app.rs).
    CommandSessionOpen {
        session_id: u64,
        wake_sid: u64,
        text: String,
        ts: u64,
    },
    /// A command session was closed.
    CommandSessionClose {
        session_id: u64,
        reason: &'static str,
        ts: u64,
    },
    /// A Vosk recognizer was fed a frame (tracks pre-wake contamination).
    RecognizerFed {
        /// "wake" or "speech"
        recognizer: &'static str,
        /// Current STT-worker state name at the time of the feed.
        in_state: &'static str,
        ts: u64,
    },
    /// A Vosk recognizer was reset.
    RecognizerReset {
        recognizer: &'static str,
        reason: &'static str,
        ts: u64,
    },
    /// audio::extend_speaking was called.
    SpeakingGateSet { duration_ms: u64, until_ms: u64, ts: u64 },
    /// audio::force_clear_speaking was called.
    SpeakingGateCleared { forced: bool, ts: u64 },
    /// An IPC event was sent to GUI clients.
    IpcEvent { tag: &'static str, ts: u64 },
    /// Rustpotter detection score for a wake event that passed the threshold.
    /// Published once per confirmed wake (after debounce + session guard pass).
    WakeScore { score: f32, threshold: f32, ts: u64 },
}

impl ValidationEvent {
    pub fn ts(&self) -> u64 {
        match self {
            Self::StateTransition { ts, .. }
            | Self::WakeSessionOpen { ts, .. }
            | Self::WakeSessionClose { ts, .. }
            | Self::CommandSessionOpen { ts, .. }
            | Self::CommandSessionClose { ts, .. }
            | Self::RecognizerFed { ts, .. }
            | Self::RecognizerReset { ts, .. }
            | Self::SpeakingGateSet { ts, .. }
            | Self::SpeakingGateCleared { ts, .. }
            | Self::IpcEvent { ts, .. }
            | Self::WakeScore { ts, .. } => *ts,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::StateTransition { .. } => "state_transition",
            Self::WakeSessionOpen { .. } => "wake_open",
            Self::WakeSessionClose { .. } => "wake_close",
            Self::CommandSessionOpen { .. } => "cmd_open",
            Self::CommandSessionClose { .. } => "cmd_close",
            Self::RecognizerFed { .. } => "recognizer_fed",
            Self::RecognizerReset { .. } => "recognizer_reset",
            Self::SpeakingGateSet { .. } => "gate_set",
            Self::SpeakingGateCleared { .. } => "gate_clear",
            Self::IpcEvent { .. } => "ipc_event",
            Self::WakeScore { .. } => "wake_score",
        }
    }
}

// ── Global validation bus ─────────────────────────────────────────────────────
//
// A single sync_channel sender registered at harness init.
// All hot-path hooks call `publish()` which resolves to a single atomic
// pointer load + try_send.  Zero-cost when no harness is registered.

static VALIDATION_TX: OnceCell<SyncSender<ValidationEvent>> = OnceCell::new();

/// Register the harness sender. Must be called before the pipeline starts.
pub fn register(tx: SyncSender<ValidationEvent>) {
    VALIDATION_TX.set(tx).ok();
}

/// Returns true if a validation harness is registered (testing mode active).
#[inline]
pub fn is_active() -> bool {
    VALIDATION_TX.get().is_some()
}

/// Publish a validation event.  Non-blocking: drops the event if the harness
/// channel is full (capacity 8192) rather than blocking the hot path.
#[inline]
pub fn publish(event: ValidationEvent) {
    if let Some(tx) = VALIDATION_TX.get() {
        let _ = tx.try_send(event);
    }
}

/// Monotonic millisecond timestamp (UNIX epoch).
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
