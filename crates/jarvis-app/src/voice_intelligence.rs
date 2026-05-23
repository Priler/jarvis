use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

use once_cell::sync::Lazy;
use parking_lot::Mutex;

// ── Production metrics ────────────────────────────────────────────────────────

pub static WAKE_DETECTIONS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Wakes rejected by debounce or overlap lock (false fire signal).
pub static FALSE_WAKE_REJECTIONS: AtomicU64 = AtomicU64::new(0);
pub static COMMANDS_MATCHED: AtomicU64 = AtomicU64::new(0);
pub static COMMANDS_UNMATCHED: AtomicU64 = AtomicU64::new(0);
pub static INTERRUPTIONS_DETECTED: AtomicU64 = AtomicU64::new(0);
pub static LOW_CONFIDENCE_DEFLECTIONS: AtomicU64 = AtomicU64::new(0);
pub static PARTIAL_SIGNALS_TOTAL: AtomicU64 = AtomicU64::new(0);

// ── Interrupt flag ────────────────────────────────────────────────────────────

/// Set by STT worker when a wake-word interrupt is detected during playback.
/// Consumed and cleared by app.rs at the start of SpeechRecognized handling.
pub static INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);

// ── Latency budget ────────────────────────────────────────────────────────────

pub static WAKE_DETECTED_AT_MS: AtomicU64 = AtomicU64::new(0);
pub static COMMAND_RECOGNIZED_AT_MS: AtomicU64 = AtomicU64::new(0);
pub static EXECUTION_STARTED_AT_MS: AtomicU64 = AtomicU64::new(0);
pub static RESPONSE_STARTED_AT_MS: AtomicU64 = AtomicU64::new(0);
pub static LAST_ROUNDTRIP_MS: AtomicU64 = AtomicU64::new(0);

/// Running exponential average of wake→reply latency (ms).
pub static AVG_WAKE_LATENCY_MS: AtomicU64 = AtomicU64::new(0);
/// Running exponential average of command→execution latency (ms).
pub static AVG_STT_LATENCY_MS: AtomicU64 = AtomicU64::new(0);
/// Running exponential average of full roundtrip latency (ms).
pub static AVG_ROUNDTRIP_MS: AtomicU64 = AtomicU64::new(0);

// ── Confidence thresholds ─────────────────────────────────────────────────────

/// Intent confidence below this → ask for clarification instead of executing.
pub const CONFIDENCE_THRESHOLD_WARN: f32 = 0.52;

/// Intent confidence below this → reject entirely (no execution, play not-found).
pub const CONFIDENCE_THRESHOLD_REJECT: f32 = 0.30;

// ── Voice context ─────────────────────────────────────────────────────────────

pub struct VoiceContext {
    /// Domain of the last successfully executed command (e.g. "music", "system").
    pub last_domain: Option<String>,
    /// Text of the last recognized command.
    pub last_command: Option<String>,
    /// Monotonically increasing depth of unbroken conversation turns.
    pub conversation_depth: u32,
    /// Epoch-ms of the last successful command execution.
    pub last_command_at_ms: u64,
}

impl VoiceContext {
    fn new() -> Self {
        Self {
            last_domain: None,
            last_command: None,
            conversation_depth: 0,
            last_command_at_ms: 0,
        }
    }

    /// Update context after a successful command execution.
    pub fn record_command(&mut self, command_text: &str, intent_id: &str) {
        let domain = intent_id.split('.').next().unwrap_or("unknown").to_string();
        self.last_domain = Some(domain);
        self.last_command = Some(command_text.to_string());
        self.conversation_depth = self.conversation_depth.saturating_add(1);
        self.last_command_at_ms = now_ms();
    }

    /// Reset context when the conversation session ends.
    pub fn reset(&mut self) {
        self.last_domain = None;
        self.last_command = None;
        self.conversation_depth = 0;
        self.last_command_at_ms = 0;
    }

    /// Returns true if the last command was recent enough to be considered in-session.
    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        let elapsed_ms = now_ms().saturating_sub(self.last_command_at_ms);
        self.last_command_at_ms > 0 && elapsed_ms < 120_000 // 2 minute session window
    }
}

pub static VOICE_CTX: Lazy<Mutex<VoiceContext>> = Lazy::new(|| Mutex::new(VoiceContext::new()));

// ── Adaptive silence window ───────────────────────────────────────────────────

/// Returns the silence-frame threshold adapted to the current conversational state.
/// Longer for initial commands, shorter in chained/active sessions.
pub fn adaptive_silence_threshold(
    sample_rate: usize,
    frame_length: usize,
    conversation_depth: u32,
) -> u32 {
    let base_secs: f32 = if conversation_depth == 0 {
        5.0 // first command in session: full window
    } else if conversation_depth < 3 {
        3.5 // early chain: moderate window
    } else {
        2.5 // deep conversation: tight window for quick back-and-forth
    };
    ((base_secs * sample_rate as f32) / frame_length as f32) as u32
}

// ── Running average helper ────────────────────────────────────────────────────

/// Update an exponential moving average atomic. Uses ~20% weight for new samples.
pub fn update_running_avg(avg: &AtomicU64, new_value: u64) {
    let current = avg.load(Ordering::Relaxed);
    let updated = if current == 0 {
        new_value
    } else {
        (current.saturating_mul(4) / 5).saturating_add(new_value / 5)
    };
    avg.store(updated, Ordering::Relaxed);
}

// ── Utility ───────────────────────────────────────────────────────────────────

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
