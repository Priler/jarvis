use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, Duration, SystemTime};

use jarvis_core::{
    audio_buffer::AudioRingBuffer,
    audio, audio_processing, config, i18n, listener, stt,
};

use crate::voice_intelligence as vi;

// ── Pipeline timing constants ─────────────────────────────────────────────────

const WAKE_DEBOUNCE_MS: u64 = 2500;
const QUIET_WINDOW_MS: u64 = 1000;
const MIN_VOICE_FRAMES_FOR_ONSET: u32 = 3;

// ── Watchdog thresholds ───────────────────────────────────────────────────────

const WATCHDOG_SPEAKING_MAX_MS: u64 = 30_000;
const WATCHDOG_COOLDOWN_MAX_MS: u64 = 12_000;
const WATCHDOG_AWAITING_CHAIN_MAX_MS: u64 = 15_000;
const WATCHDOG_ZOMBIE_WAKE_MAX_S: u64 = 120;
const WATCHDOG_CHECK_INTERVAL_FRAMES: u32 = 50;
const WATCHDOG_HEALTH_INTERVAL_TICKS: u32 = 10;

// ── Global session locks ──────────────────────────────────────────────────────

static LAST_WAKE_MS: AtomicU64 = AtomicU64::new(0);
static WAKE_SESSION_ID: AtomicU64 = AtomicU64::new(0);
static COMMAND_SESSION_ID: AtomicU64 = AtomicU64::new(0);

pub static ACTIVE_WAKE_SESSION: AtomicU64 = AtomicU64::new(0);
pub static ACTIVE_COMMAND_SESSION: AtomicU64 = AtomicU64::new(0);

// ── Heartbeat timestamps ──────────────────────────────────────────────────────

pub static LAST_STT_FRAME_MS: AtomicU64 = AtomicU64::new(0);
pub static LAST_WAKE_ACTIVITY_MS: AtomicU64 = AtomicU64::new(0);
pub static LAST_COMMAND_ACTIVITY_MS: AtomicU64 = AtomicU64::new(0);
pub static LAST_IPC_ACTIVITY_MS: AtomicU64 = AtomicU64::new(0);

// ── Recovery metrics ──────────────────────────────────────────────────────────

pub static RECOVERY_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static RECOVERY_SUCCESS: AtomicU64 = AtomicU64::new(0);
pub static RECOVERY_FAILED: AtomicU64 = AtomicU64::new(0);
pub static RECOGNIZER_REBUILDS: AtomicU64 = AtomicU64::new(0);
pub static FORCED_GATE_RESETS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Message types ─────────────────────────────────────────────────────────────

pub struct AudioFrame {
    pub pcm: Vec<i16>,
    pub is_voice: bool,
    pub rustpotter_wake: bool,
    pub vad_rms: f32,
}

pub enum SttEvent {
    WakeDetected { session_id: u64 },
    SpeechRecognized {
        text: String,
        wake_session_id: u64,
        cmd_session_id: u64,
    },
    CommandTimeout {
        wake_session_id: u64,
        timeout_type: &'static str,
    },
}

pub enum SttCmd {
    Chain,
    Idle,
}

// ── Internal state machine ────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum State {
    WaitingForVoice,
    VoiceActive,
    CommandMode { first: bool },
    AwaitingChain,
    Cooldown,
    QuietWindow,
}

fn state_name(s: State) -> &'static str {
    match s {
        State::WaitingForVoice => "WaitingForVoice",
        State::VoiceActive => "VoiceActive",
        State::CommandMode { .. } => "CommandMode",
        State::AwaitingChain => "AwaitingChain",
        State::Cooldown => "Cooldown",
        State::QuietWindow => "QuietWindow",
    }
}

fn is_legal_transition(from: State, to: State) -> bool {
    matches!(
        (from, to),
        (State::WaitingForVoice, State::VoiceActive)
            | (State::VoiceActive, State::WaitingForVoice)
            | (State::VoiceActive, State::CommandMode { .. })
            | (State::CommandMode { .. }, State::CommandMode { .. })
            | (State::CommandMode { .. }, State::AwaitingChain)
            | (State::CommandMode { .. }, State::Cooldown)
            | (State::AwaitingChain, State::CommandMode { .. })
            | (State::AwaitingChain, State::Cooldown)
            | (State::Cooldown, State::QuietWindow)
            | (State::QuietWindow, State::WaitingForVoice)
    )
}

// ── Runtime context ───────────────────────────────────────────────────────────

struct RuntimeCtx {
    state: State,
    wake_sid: u64,
    session_start: Instant,
    session_commands: u32,
    session_timeouts: u32,
    session_resets: u32,
    cooldown_clean: bool,
    pre_roll_at_onset: usize,
    live_frames: u32,
    silence_frames: u32,
    consecutive_voice_frames: u32,
    cmd_start: SystemTime,
    quiet_until: Instant,
    speaking_drop_count: u32,
    // Watchdog & health.
    state_entered_at: Instant,
    watchdog_tick: u32,
    health_tick: u32,
    consecutive_recoveries: u32,
    // Audio buffer.
    pre_roll: AudioRingBuffer,
    frame_length: usize,
    sample_rate: usize,
}

impl RuntimeCtx {
    fn new(frame_length: usize, sample_rate: usize) -> Self {
        Self {
            state: State::WaitingForVoice,
            wake_sid: 0,
            session_start: Instant::now(),
            session_commands: 0,
            session_timeouts: 0,
            session_resets: 0,
            cooldown_clean: true,
            pre_roll_at_onset: 0,
            live_frames: 0,
            silence_frames: 0,
            consecutive_voice_frames: 0,
            cmd_start: SystemTime::now(),
            quiet_until: Instant::now(),
            speaking_drop_count: 0,
            state_entered_at: Instant::now(),
            watchdog_tick: 0,
            health_tick: 0,
            consecutive_recoveries: 0,
            pre_roll: AudioRingBuffer::new(5.0, frame_length, sample_rate),
            frame_length,
            sample_rate,
        }
    }

    // ── Transition ────────────────────────────────────────────────────────────

    fn transition_to(&mut self, next: State, reason: &'static str) {
        let from = self.state;
        let legal = is_legal_transition(from, next);
        if legal {
            info!(
                "[STATE][WAKE S:{}] {} → {} reason={}",
                self.wake_sid, state_name(from), state_name(next), reason
            );
        } else {
            warn!(
                "[ILLEGAL_TRANSITION][WAKE S:{}] {} → {} reason={}",
                self.wake_sid, state_name(from), state_name(next), reason
            );
        }
        crate::testing::publish(crate::testing::ValidationEvent::StateTransition {
            from: state_name(from),
            to: state_name(next),
            legal,
            reason,
            wake_sid: self.wake_sid,
            ts: now_ms(),
        });
        self.state = next;
        self.state_entered_at = Instant::now();
    }

    // ── Wake session lifecycle ─────────────────────────────────────────────────

    fn open_wake_session(&mut self, vosk_wake: bool, rustpotter_wake: bool) -> Option<u64> {
        let active = ACTIVE_WAKE_SESSION.load(Ordering::Acquire);
        if active != 0 {
            warn!("[WARN][WAKE] Rejected overlapping wake active_session={}", active);
            vi::FALSE_WAKE_REJECTIONS.fetch_add(1, Ordering::Relaxed);
            return None;
        }

        let now = now_ms();
        let last = LAST_WAKE_MS.load(Ordering::Acquire);
        if last > 0 && now.saturating_sub(last) < WAKE_DEBOUNCE_MS {
            info!(
                "[WAKE] ignored: debounce active ({} ms since last wake, need {} ms)",
                now.saturating_sub(last), WAKE_DEBOUNCE_MS
            );
            vi::FALSE_WAKE_REJECTIONS.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        LAST_WAKE_MS.store(now, Ordering::Release);
        LAST_WAKE_ACTIVITY_MS.store(now, Ordering::Relaxed);

        // Latency budget: stamp wake detection time.
        vi::WAKE_DETECTED_AT_MS.store(now, Ordering::Relaxed);
        vi::WAKE_DETECTIONS_TOTAL.fetch_add(1, Ordering::Relaxed);

        let sid = WAKE_SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1;
        ACTIVE_WAKE_SESSION.store(sid, Ordering::Release);

        crate::testing::publish(crate::testing::ValidationEvent::WakeSessionOpen {
            session_id: sid,
            ts: now,
        });

        self.wake_sid = sid;
        self.session_start = Instant::now();
        self.session_commands = 0;
        self.session_timeouts = 0;
        self.session_resets = 0;
        self.cooldown_clean = true;
        self.consecutive_recoveries = 0;

        info!(
            "[WAKE S:{}] OPEN vosk={} rustpotter={} pre_roll_frames={} live_frames={}",
            sid, vosk_wake, rustpotter_wake, self.pre_roll_at_onset, self.live_frames
        );

        Some(sid)
    }

    fn finalize_wake(&mut self) {
        if self.wake_sid == 0 {
            warn!("[WARN][RUNTIME] finalize_wake called without active wake session — possible lifecycle leak");
        }

        crate::testing::publish(crate::testing::ValidationEvent::WakeSessionClose {
            session_id: self.wake_sid,
            commands: self.session_commands,
            timeouts: self.session_timeouts,
            clean: self.cooldown_clean,
            ts: now_ms(),
        });

        info!("[RESET][WAKE S:{}] speech_recognizer reason=finalize_wake", self.wake_sid);
        stt::reset_speech_recognizer();
        crate::testing::publish(crate::testing::ValidationEvent::RecognizerReset {
            recognizer: "speech",
            reason: "finalize_wake",
            ts: now_ms(),
        });
        self.session_resets += 1;
        info!("[RESET][WAKE S:{}] wake_recognizer reason=finalize_wake", self.wake_sid);
        stt::reset_wake_recognizer();
        crate::testing::publish(crate::testing::ValidationEvent::RecognizerReset {
            recognizer: "wake",
            reason: "finalize_wake",
            ts: now_ms(),
        });
        self.session_resets += 1;
        info!("[RESET][WAKE S:{}] rustpotter_remainder reason=finalize_wake", self.wake_sid);
        listener::reset_state();
        audio_processing::reset();
        info!("[AUDIO][WAKE S:{}] pre_roll replaced reason=finalize_wake", self.wake_sid);
        self.pre_roll = AudioRingBuffer::new(5.0, self.frame_length, self.sample_rate);
        self.silence_frames = 0;
        self.consecutive_voice_frames = 0;

        info!(
            "[WAKE S:{} SUMMARY] duration={:.1}s commands={} timeouts={} resets={} clean_close={} recoveries={}",
            self.wake_sid,
            self.session_start.elapsed().as_secs_f32(),
            self.session_commands,
            self.session_timeouts,
            self.session_resets,
            self.cooldown_clean,
            self.consecutive_recoveries,
        );
        info!(
            "[WAKE S:{}] CLOSE {}",
            self.wake_sid,
            if self.cooldown_clean { "clean" } else { "timeout" }
        );

        // Reset voice context if the session was short (no commands).
        if self.session_commands == 0 {
            vi::VOICE_CTX.lock().reset();
        }

        self.quiet_until = Instant::now() + Duration::from_millis(QUIET_WINDOW_MS);
        self.transition_to(State::QuietWindow, "cooldown_complete");

        ACTIVE_WAKE_SESSION.store(0, Ordering::Release);

        self.wake_sid = 0;
        self.session_commands = 0;
        self.session_timeouts = 0;
        self.session_resets = 0;
        self.cooldown_clean = true;
        self.consecutive_recoveries = 0;
    }

    // ── Command session lifecycle ──────────────────────────────────────────────

    fn open_command_session(&mut self, text: &str) -> Option<u64> {
        let active = ACTIVE_COMMAND_SESSION.load(Ordering::Acquire);
        if active != 0 {
            warn!("[WARN][CMD] Rejected duplicate execute cmd_session={}", active);
            return None;
        }

        let cmd_sid = COMMAND_SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1;
        ACTIVE_COMMAND_SESSION.store(cmd_sid, Ordering::Release);

        let now = now_ms();
        crate::testing::publish(crate::testing::ValidationEvent::CommandSessionOpen {
            session_id: cmd_sid,
            wake_sid: self.wake_sid,
            text: text.to_string(),
            ts: now,
        });
        LAST_COMMAND_ACTIVITY_MS.store(now, Ordering::Relaxed);
        vi::COMMAND_RECOGNIZED_AT_MS.store(now, Ordering::Relaxed);

        // Latency: wake → command recognition.
        let wake_at = vi::WAKE_DETECTED_AT_MS.load(Ordering::Relaxed);
        if wake_at > 0 {
            let stt_latency = now.saturating_sub(wake_at);
            vi::update_running_avg(&vi::AVG_STT_LATENCY_MS, stt_latency);
            info!(
                "[LATENCY][CMD S:{}][WAKE S:{}] stt={}ms",
                cmd_sid, self.wake_sid, stt_latency
            );
        }

        self.session_commands += 1;

        info!(
            "[CMD S:{}][WAKE S:{}] OPEN text=\"{}\"",
            cmd_sid, self.wake_sid, text
        );

        Some(cmd_sid)
    }

    fn finalize_command(&mut self, cmd_sid: u64, reason: &'static str) {
        info!(
            "[CMD S:{}][WAKE S:{}] CLOSE reason={}",
            cmd_sid, self.wake_sid, reason
        );
        crate::testing::publish(crate::testing::ValidationEvent::CommandSessionClose {
            session_id: cmd_sid,
            reason,
            ts: now_ms(),
        });
        ACTIVE_COMMAND_SESSION.store(0, Ordering::Release);
    }

    // ── Invariant engine ──────────────────────────────────────────────────────

    fn check_invariants(&self) {
        let active_wake = ACTIVE_WAKE_SESSION.load(Ordering::Acquire);
        let active_cmd = ACTIVE_COMMAND_SESSION.load(Ordering::Acquire);

        if self.wake_sid != 0 && active_wake != self.wake_sid {
            warn!("[INVARIANT] ACTIVE_WAKE_SESSION={} != local wake_sid={}", active_wake, self.wake_sid);
        }
        if active_wake != 0 && self.wake_sid == 0 {
            warn!("[INVARIANT] ACTIVE_WAKE_SESSION={} but local wake_sid=0 — stale lock", active_wake);
        }
        if matches!(self.state, State::CommandMode { .. } | State::AwaitingChain) && self.wake_sid == 0 {
            warn!("[INVARIANT] state={} without active wake session", state_name(self.state));
        }
        if active_cmd != 0 && self.wake_sid == 0 {
            warn!("[INVARIANT] ACTIVE_COMMAND_SESSION={} but no active wake session", active_cmd);
        }
        if matches!(self.state, State::AwaitingChain) && active_cmd == 0 {
            warn!("[INVARIANT] AwaitingChain but ACTIVE_COMMAND_SESSION=0");
        }
        if matches!(self.state, State::Cooldown) && self.wake_sid == 0 {
            warn!("[INVARIANT] Cooldown entered without active wake session");
        }
    }

    // ── Interruption engine ───────────────────────────────────────────────────

    /// Feed one frame to the wake recognizer during playback to detect re-wake interrupts.
    /// Called from the speaking gate before the `continue`.
    /// Returns true if an interrupt was detected (gate should not continue).
    fn check_interrupt_during_playback(&mut self, frame: &AudioFrame) -> bool {
        // Only check during states where an interrupt makes sense.
        if !matches!(self.state, State::Cooldown | State::AwaitingChain) {
            return false;
        }
        if self.wake_sid == 0 {
            return false;
        }

        // Feed frame to wake recognizer (clean state since last reset).
        let wake_result = stt::recognize_wake_word(&frame.pcm);
        let is_wake = wake_result.map_or(false, |(text, _confidence)| {
            let phrases = config::get_wake_phrases(&i18n::get_language());
            let lower = text.to_lowercase();
            phrases.iter().any(|wp| lower.contains(*wp))
        });

        if !is_wake {
            return false;
        }

        vi::INTERRUPTIONS_DETECTED.fetch_add(1, Ordering::Relaxed);
        vi::INTERRUPT_REQUESTED.store(true, Ordering::Release);

        info!(
            "[INTERRUPT][WAKE S:{}] re-wake interrupt detected state={}",
            self.wake_sid, state_name(self.state)
        );

        // Force-clear playback gate immediately.
        audio::force_clear_speaking();
        FORCED_GATE_RESETS.fetch_add(1, Ordering::Relaxed);
        stt::reset_wake_recognizer();
        listener::reset_state();

        match self.state {
            State::Cooldown => {
                // Pipeline is in post-command cooldown. Finalize now and let the
                // QuietWindow absorb any residual physical audio.
                self.finalize_wake();
                // State is now QuietWindow — caller must not `continue`.
            }
            State::AwaitingChain => {
                // Execution is running in app.rs. INTERRUPT_REQUESTED is set;
                // app.rs will see it when SpeechRecognized arrives and suppress execution.
                // The command session is closed via SttCmd::Idle when execution finishes.
                // We stay in AwaitingChain — caller must not `continue` so the frame
                // flows through normally (AwaitingChain arm discards it anyway).
            }
            _ => {}
        }

        true
    }

    // ── Watchdog ──────────────────────────────────────────────────────────────

    fn run_watchdog(&mut self, event_tx: &SyncSender<SttEvent>) {
        let recovered = self.check_cooldown_stuck(event_tx)
            || self.check_awaiting_chain_stuck(event_tx)
            || self.check_zombie_wake(event_tx);

        if !recovered && self.consecutive_recoveries > 0 {
            self.consecutive_recoveries = self.consecutive_recoveries.saturating_sub(1);
        }

        self.health_tick += 1;
        if self.health_tick >= WATCHDOG_HEALTH_INTERVAL_TICKS {
            self.health_tick = 0;
            self.emit_health_report();
        }
    }

    fn check_speaking_gate_stuck(&mut self) -> bool {
        let remaining_ms = audio::speaking_remaining_ms();
        if remaining_ms <= WATCHDOG_SPEAKING_MAX_MS {
            return false;
        }
        warn!(
            "[RECOVERY] subsystem=speaking_gate action=force_clear stuck_ms={} wake_sid={}",
            remaining_ms, self.wake_sid
        );
        audio::force_clear_speaking();
        FORCED_GATE_RESETS.fetch_add(1, Ordering::Relaxed);
        RECOVERY_TOTAL.fetch_add(1, Ordering::Relaxed);
        RECOVERY_SUCCESS.fetch_add(1, Ordering::Relaxed);
        self.consecutive_recoveries += 1;
        info!("[RECOVERY] subsystem=speaking_gate result=success");
        true
    }

    fn check_cooldown_stuck(&mut self, _event_tx: &SyncSender<SttEvent>) -> bool {
        if !matches!(self.state, State::Cooldown) {
            return false;
        }
        let elapsed_ms = self.state_entered_at.elapsed().as_millis() as u64;
        if elapsed_ms <= WATCHDOG_COOLDOWN_MAX_MS {
            return false;
        }
        warn!(
            "[RECOVERY] subsystem=cooldown action=force_finalize stuck_ms={} wake_sid={}",
            elapsed_ms, self.wake_sid
        );
        RECOVERY_TOTAL.fetch_add(1, Ordering::Relaxed);
        self.consecutive_recoveries += 1;
        audio::force_clear_speaking();
        FORCED_GATE_RESETS.fetch_add(1, Ordering::Relaxed);
        self.cooldown_clean = false;
        self.finalize_wake();
        RECOVERY_SUCCESS.fetch_add(1, Ordering::Relaxed);
        info!("[RECOVERY] subsystem=cooldown result=success");
        true
    }

    fn check_awaiting_chain_stuck(&mut self, event_tx: &SyncSender<SttEvent>) -> bool {
        if !matches!(self.state, State::AwaitingChain) {
            return false;
        }
        let elapsed_ms = self.state_entered_at.elapsed().as_millis() as u64;
        if elapsed_ms <= WATCHDOG_AWAITING_CHAIN_MAX_MS {
            return false;
        }
        let cmd_sid = ACTIVE_COMMAND_SESSION.load(Ordering::Acquire);
        warn!(
            "[RECOVERY] subsystem=awaiting_chain action=force_idle stuck_ms={} cmd_sid={} wake_sid={}",
            elapsed_ms, cmd_sid, self.wake_sid
        );
        RECOVERY_TOTAL.fetch_add(1, Ordering::Relaxed);
        self.consecutive_recoveries += 1;
        if cmd_sid != 0 {
            self.finalize_command(cmd_sid, "watchdog_timeout");
        }
        info!("[RECOVERY] subsystem=awaiting_chain action=emergency_recognizer_reset");
        stt::reset_speech_recognizer();
        stt::reset_wake_recognizer();
        RECOGNIZER_REBUILDS.fetch_add(1, Ordering::Relaxed);
        self.session_resets += 1;
        self.cooldown_clean = false;
        self.session_timeouts += 1;
        self.transition_to(State::Cooldown, "watchdog_awaiting_chain_timeout");
        let _ = event_tx.send(SttEvent::CommandTimeout {
            wake_session_id: self.wake_sid,
            timeout_type: "watchdog_timeout",
        });
        RECOVERY_SUCCESS.fetch_add(1, Ordering::Relaxed);
        info!("[RECOVERY] subsystem=awaiting_chain result=success");
        true
    }

    fn check_zombie_wake(&mut self, event_tx: &SyncSender<SttEvent>) -> bool {
        if self.wake_sid == 0 {
            return false;
        }
        let age_s = self.session_start.elapsed().as_secs();
        if age_s <= WATCHDOG_ZOMBIE_WAKE_MAX_S {
            return false;
        }
        warn!(
            "[RECOVERY] subsystem=zombie_wake action=force_finalize wake_sid={} age_s={}",
            self.wake_sid, age_s
        );
        RECOVERY_TOTAL.fetch_add(1, Ordering::Relaxed);
        self.consecutive_recoveries += 1;
        let cmd_sid = ACTIVE_COMMAND_SESSION.load(Ordering::Acquire);
        if cmd_sid != 0 {
            warn!(
                "[RECOVERY] subsystem=zombie_command action=force_close cmd_sid={} wake_sid={}",
                cmd_sid, self.wake_sid
            );
            self.finalize_command(cmd_sid, "zombie_session");
            let _ = event_tx.send(SttEvent::CommandTimeout {
                wake_session_id: self.wake_sid,
                timeout_type: "watchdog_timeout",
            });
        }
        audio::force_clear_speaking();
        FORCED_GATE_RESETS.fetch_add(1, Ordering::Relaxed);
        self.cooldown_clean = false;
        if !matches!(self.state, State::Cooldown) {
            self.transition_to(State::Cooldown, "zombie_session_recovery");
        }
        self.finalize_wake();
        RECOVERY_SUCCESS.fetch_add(1, Ordering::Relaxed);
        info!("[RECOVERY] subsystem=zombie_wake result=success");
        true
    }

    // ── Health monitor ────────────────────────────────────────────────────────

    fn emit_health_report(&self) {
        let active_wake = ACTIVE_WAKE_SESSION.load(Ordering::Acquire);
        let active_cmd = ACTIVE_COMMAND_SESSION.load(Ordering::Acquire);
        let now = now_ms();
        let stt_age_ms = now.saturating_sub(LAST_STT_FRAME_MS.load(Ordering::Relaxed));
        let ipc_age_ms = now.saturating_sub(LAST_IPC_ACTIVITY_MS.load(Ordering::Relaxed));

        let wake_status = if active_wake != 0
            && self.session_start.elapsed().as_secs() > WATCHDOG_ZOMBIE_WAKE_MAX_S / 2
        {
            "degraded[slow_session]"
        } else if active_wake != 0 {
            "active"
        } else {
            "healthy"
        };
        let stt_status = if stt_age_ms > 5_000 { "degraded[stale_frames]" } else { "healthy" };
        let ipc_status = if LAST_IPC_ACTIVITY_MS.load(Ordering::Relaxed) == 0 {
            "no_activity_yet"
        } else if ipc_age_ms > 60_000 {
            "degraded[stale]"
        } else {
            "healthy"
        };
        let cooldown_status = if matches!(self.state, State::Cooldown)
            && self.state_entered_at.elapsed().as_millis() as u64 > WATCHDOG_COOLDOWN_MAX_MS / 2
        {
            "warning[slow_drain]"
        } else {
            "healthy"
        };

        let avg_roundtrip = vi::AVG_ROUNDTRIP_MS.load(Ordering::Relaxed);
        let avg_stt = vi::AVG_STT_LATENCY_MS.load(Ordering::Relaxed);
        let wake_total = vi::WAKE_DETECTIONS_TOTAL.load(Ordering::Relaxed);
        let interrupts = vi::INTERRUPTIONS_DETECTED.load(Ordering::Relaxed);
        let low_conf = vi::LOW_CONFIDENCE_DEFLECTIONS.load(Ordering::Relaxed);

        info!(
            "[HEALTH] state={} wake={} stt={} ipc={} cooldown={} \
             sessions=wake:{}/cmd:{} \
             latency=roundtrip:{}ms/stt:{}ms \
             vi=wakes:{}/interrupts:{}/low_conf:{} \
             recovery=total:{}/failed:{}/consecutive:{}",
            state_name(self.state),
            wake_status, stt_status, ipc_status, cooldown_status,
            active_wake, active_cmd,
            avg_roundtrip, avg_stt,
            wake_total, interrupts, low_conf,
            RECOVERY_TOTAL.load(Ordering::Relaxed),
            RECOVERY_FAILED.load(Ordering::Relaxed),
            self.consecutive_recoveries,
        );
    }
}

// ── Worker entry point ────────────────────────────────────────────────────────

/// Public entry point.  Wraps `run_inner` in a panic safety net: if any Rust
/// panic escapes frame processing the worker thread exits cleanly and the app
/// thread detects the channel disconnect, entering L3 degraded mode.
pub fn run(
    frame_rx: Receiver<AudioFrame>,
    cmd_rx: Receiver<SttCmd>,
    event_tx: SyncSender<SttEvent>,
    frame_length: usize,
    sample_rate: usize,
) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_inner(frame_rx, cmd_rx, event_tx, frame_length, sample_rate);
    }));
    if result.is_err() {
        error!("[STT] Worker panicked — entering degraded mode");
        RECOVERY_FAILED.fetch_add(1, Ordering::Relaxed);
        crate::watchdog::DEGRADED_MODE.store(true, Ordering::Release);
    }
}

fn run_inner(
    frame_rx: Receiver<AudioFrame>,
    cmd_rx: Receiver<SttCmd>,
    event_tx: SyncSender<SttEvent>,
    frame_length: usize,
    sample_rate: usize,
) {
    let mut ctx = RuntimeCtx::new(frame_length, sample_rate);

    let wake_silence_threshold =
        ((1.5 * sample_rate as f32) / frame_length as f32) as u32;

    for frame in &frame_rx {
        LAST_STT_FRAME_MS.store(now_ms(), Ordering::Relaxed);

        // ── Degraded mode gate ─────────────────────────────────────────────────
        // When the watchdog has entered L3 degraded mode, drain frames without
        // any recognizer work to keep the channel from filling up.
        if crate::watchdog::DEGRADED_MODE.load(Ordering::Relaxed) {
            debug!("[STT][DEGRADED] frame drained — voice processing suspended");
            continue;
        }

        // ── Chain/Idle from main thread (non-blocking) ────────────────────────
        if let Ok(cmd) = cmd_rx.try_recv() {
            let cmd_sid = ACTIVE_COMMAND_SESSION.load(Ordering::Acquire);
            match (cmd, ctx.state) {
                (SttCmd::Chain, State::AwaitingChain) => {
                    ctx.finalize_command(cmd_sid, "chain");
                    info!("[STT][WAKE S:{}] Chaining next command", ctx.wake_sid);
                    info!("[RESET][WAKE S:{}] speech_recognizer reason=chain", ctx.wake_sid);
                    stt::reset_speech_recognizer();
                    ctx.session_resets += 1;
                    ctx.silence_frames = 0;
                    ctx.cmd_start = SystemTime::now();
                    ctx.transition_to(State::CommandMode { first: false }, "chain_received");
                }
                (SttCmd::Idle, State::AwaitingChain) => {
                    // Consume interrupt flag if an interrupt drove this idle.
                    if vi::INTERRUPT_REQUESTED.load(Ordering::Acquire) {
                        info!(
                            "[INTERRUPT][WAKE S:{}] idle consumed interrupt flag cmd_sid={}",
                            ctx.wake_sid, cmd_sid
                        );
                        vi::INTERRUPT_REQUESTED.store(false, Ordering::Release);
                    }
                    ctx.finalize_command(cmd_sid, "idle");
                    info!("[RESET][WAKE S:{}] speech_recognizer reason=idle_before_cooldown", ctx.wake_sid);
                    stt::reset_speech_recognizer();
                    ctx.session_resets += 1;
                    ctx.silence_frames = 0;
                    ctx.transition_to(State::Cooldown, "idle_received");
                }
                (_, _) => {}
            }
        }

        // ── Speaking gate (with stuck recovery + interrupt detection) ─────────
        if audio::is_speaking() {
            if ctx.check_speaking_gate_stuck() {
                // Gate was stuck and is now cleared — fall through.
            } else if !matches!(ctx.state, State::CommandMode { .. }) {
                // Before dropping this frame, check for wake-word interrupt.
                let interrupted = ctx.check_interrupt_during_playback(&frame);
                if !interrupted {
                    // Normal gate: discard frame.
                    ctx.speaking_drop_count += 1;
                    if ctx.speaking_drop_count == 1 {
                        info!("[SPEAK][WAKE S:{}] BEGIN gate_active", ctx.wake_sid);
                    }
                    continue;
                }
                // Interrupt handled — gate was force-cleared; fall through.
            }
        }
        if ctx.speaking_drop_count > 0 {
            info!(
                "[SPEAK][WAKE S:{}] END dropped_frames={}",
                ctx.wake_sid, ctx.speaking_drop_count
            );
            ctx.speaking_drop_count = 0;
        }

        // ── QuietWindow gate ──────────────────────────────────────────────────
        if matches!(ctx.state, State::QuietWindow) {
            if Instant::now() < ctx.quiet_until {
                debug!("[VAD] ignored: quiet window active");
                ctx.consecutive_voice_frames = 0;
                continue;
            }
            ctx.transition_to(State::WaitingForVoice, "quiet_window_expired");
            ctx.consecutive_voice_frames = 0;
            continue;
        }

        // ── Watchdog tick ─────────────────────────────────────────────────────
        ctx.watchdog_tick += 1;
        if ctx.watchdog_tick >= WATCHDOG_CHECK_INTERVAL_FRAMES {
            ctx.watchdog_tick = 0;
            ctx.run_watchdog(&event_tx);
            if matches!(ctx.state, State::QuietWindow) {
                if Instant::now() < ctx.quiet_until {
                    ctx.consecutive_voice_frames = 0;
                    continue;
                }
                ctx.transition_to(State::WaitingForVoice, "quiet_window_expired");
                ctx.consecutive_voice_frames = 0;
                continue;
            }
        }

        ctx.check_invariants();

        // Compute adaptive silence threshold based on conversational depth.
        let conversation_depth = vi::VOICE_CTX.lock().conversation_depth;
        let cmd_silence_threshold = vi::adaptive_silence_threshold(
            ctx.sample_rate,
            ctx.frame_length,
            conversation_depth,
        );

        match ctx.state {
            // ── 1. Pre-wake: buffer pre-roll audio ────────────────────────────
            State::WaitingForVoice => {
                ctx.pre_roll.push(&frame.pcm);
                if frame.is_voice {
                    ctx.consecutive_voice_frames += 1;
                    if ctx.consecutive_voice_frames < MIN_VOICE_FRAMES_FOR_ONSET {
                        debug!(
                            "[VAD] ignored: too short speech ({}/{} consecutive frames)",
                            ctx.consecutive_voice_frames, MIN_VOICE_FRAMES_FOR_ONSET
                        );
                    } else {
                        ctx.pre_roll_at_onset = ctx.pre_roll.len();
                        ctx.live_frames = 0;
                        info!(
                            "[AUDIO] voice_onset onset_frames={} rms={:.0} pre_roll_frames={}",
                            ctx.consecutive_voice_frames, frame.vad_rms, ctx.pre_roll_at_onset
                        );
                        for buffered in ctx.pre_roll.drain_all() {
                            let _ = stt::recognize_wake_word(&buffered);
                        }
                        ctx.transition_to(State::VoiceActive, "voice_onset");
                        ctx.silence_frames = 0;
                        ctx.consecutive_voice_frames = 0;
                    }
                } else {
                    ctx.consecutive_voice_frames = 0;
                }
            }

            // ── 2. Wake word detection ────────────────────────────────────────
            State::VoiceActive => {
                ctx.live_frames += 1;

                let vosk_wake =
                    stt::recognize_wake_word(&frame.pcm).map_or(false, |(text, _)| {
                        let phrases = config::get_wake_phrases(&i18n::get_language());
                        let lower = text.to_lowercase();
                        let matched = phrases.iter().any(|wp| lower.contains(*wp));
                        if matched {
                            info!("[STT] Vosk wake: '{}'", text);
                        }
                        matched
                    });

                if vosk_wake || frame.rustpotter_wake {
                    if let Some(sid) = ctx.open_wake_session(vosk_wake, frame.rustpotter_wake) {
                        // Publish wake score for threshold calibration analysis.
                        if frame.rustpotter_wake {
                            crate::testing::publish(crate::testing::ValidationEvent::WakeScore {
                                score: listener::get_last_detect_score(),
                                threshold: jarvis_core::config::RUSPOTTER_MIN_SCORE,
                                ts: crate::testing::now_ms(),
                            });
                        }
                        info!("[RESET][WAKE S:{}] wake_recognizer reason=wake_complete", sid);
                        stt::reset_wake_recognizer();
                        ctx.session_resets += 1;
                        info!("[RESET][WAKE S:{}] rustpotter_remainder reason=wake_complete", sid);
                        listener::reset_state();
                        audio_processing::reset();
                        ctx.transition_to(State::CommandMode { first: true }, "wake_confirmed");
                        ctx.silence_frames = 0;
                        ctx.cmd_start = SystemTime::now();
                        let _ = event_tx.send(SttEvent::WakeDetected { session_id: sid });
                    } else {
                        info!("[RESET] wake_recognizer reason=wake_rejected");
                        stt::reset_wake_recognizer();
                        info!("[RESET] rustpotter_remainder reason=wake_rejected");
                        listener::reset_state();
                        info!("[RESET] speech_recognizer reason=wake_rejected");
                        stt::reset_speech_recognizer();
                        ctx.transition_to(State::WaitingForVoice, "wake_rejected");
                        ctx.silence_frames = 0;
                        ctx.consecutive_voice_frames = 0;
                    }
                } else if !frame.is_voice {
                    ctx.silence_frames += 1;
                    if ctx.silence_frames > wake_silence_threshold {
                        debug!("[STT] Wake silence timeout → WaitingForVoice");
                        ctx.transition_to(State::WaitingForVoice, "wake_silence_timeout");
                        ctx.silence_frames = 0;
                        info!("[RESET] wake_recognizer reason=wake_silence_timeout");
                        stt::reset_wake_recognizer();
                        info!("[RESET] rustpotter_remainder reason=wake_silence_timeout");
                        listener::reset_state();
                        info!("[RESET] speech_recognizer reason=wake_silence_timeout");
                        stt::reset_speech_recognizer();
                    }
                } else {
                    ctx.silence_frames = 0;
                }
            }

            // ── 3. Collect spoken command ─────────────────────────────────────
            State::CommandMode { first } => {
                // Use streaming recognition: surfaces partial transcripts as early signals
                // while still waiting for the final result to trigger execution.
                crate::testing::publish(crate::testing::ValidationEvent::RecognizerFed {
                    recognizer: "speech",
                    in_state: "CommandMode",
                    ts: now_ms(),
                });
                let (final_opt, partial_opt) = stt::recognize_with_partial(&frame.pcm);

                // Partial signal: gives app.rs latency-ahead visibility (no execution yet).
                if let Some(ref partial) = partial_opt {
                    if !partial.is_empty() {
                        vi::PARTIAL_SIGNALS_TOTAL.fetch_add(1, Ordering::Relaxed);
                        debug!(
                            "[STT][WAKE S:{}] partial: '{}'",
                            ctx.wake_sid, partial
                        );
                    }
                }

                if let Some(mut text) = final_opt {
                    info!("[STT][WAKE S:{}] transcript: '{}'", ctx.wake_sid, text);
                    text = text.to_lowercase();

                    let wake_phrases = config::get_wake_phrases(&i18n::get_language());
                    let contains_wake = wake_phrases.iter().any(|wp| text.contains(*wp));

                    if first && !contains_wake {
                        let words: Vec<&str> = text.split_whitespace().collect();
                        if words.len() == 1 {
                            info!(
                                "[STT][WAKE S:{}] first transcript '{}' is wake-word only — resetting for command",
                                ctx.wake_sid, text
                            );
                            info!("[RESET][WAKE S:{}] speech_recognizer reason=first_single_word_garbled", ctx.wake_sid);
                            stt::reset_speech_recognizer();
                            ctx.session_resets += 1;
                            ctx.transition_to(
                                State::CommandMode { first: false },
                                "garbled_first_word",
                            );
                            ctx.silence_frames = 0;
                            continue;
                        } else if has_command_intent(words[0], &i18n::get_language()) {
                            info!(
                                "[STT][WAKE S:{}] first transcript already command-like, preserving first word: '{}'",
                                ctx.wake_sid, text
                            );
                        } else {
                            let stripped = words[1..].join(" ");
                            if has_command_intent(&stripped, &i18n::get_language()) {
                                info!(
                                    "[STT][WAKE S:{}] stripping suspected wake artifact: '{}' -> '{}'",
                                    ctx.wake_sid, text, stripped
                                );
                                text = stripped;
                            } else {
                                info!(
                                    "[STT][WAKE S:{}] first transcript not command-like after stripping, ignoring: '{}'",
                                    ctx.wake_sid, text
                                );
                                info!("[RESET][WAKE S:{}] speech_recognizer reason=garbage_first_transcript", ctx.wake_sid);
                                stt::reset_speech_recognizer();
                                ctx.session_resets += 1;
                                ctx.session_timeouts += 1;
                                ctx.cooldown_clean = false;
                                info!("[TIMEOUT][WAKE S:{}] type=garbage_transcript", ctx.wake_sid);
                                ctx.transition_to(State::Cooldown, "garbage_first_transcript");
                                let _ = event_tx.send(SttEvent::CommandTimeout {
                                    wake_session_id: ctx.wake_sid,
                                    timeout_type: "garbage_transcript",
                                });
                                continue;
                            }
                        }
                    }

                    if contains_wake {
                        for wp in wake_phrases {
                            text = text.replace(*wp, "");
                        }
                        text = text.trim().to_string();
                        if text.is_empty() {
                            info!(
                                "[STT][WAKE S:{}] bare wake — no command captured → Cooldown",
                                ctx.wake_sid
                            );
                            info!("[RESET][WAKE S:{}] speech_recognizer reason=bare_wake", ctx.wake_sid);
                            stt::reset_speech_recognizer();
                            ctx.session_resets += 1;
                            ctx.session_timeouts += 1;
                            ctx.cooldown_clean = false;
                            info!("[TIMEOUT][WAKE S:{}] type=bare_wake", ctx.wake_sid);
                            ctx.transition_to(State::Cooldown, "bare_wake");
                            let _ = event_tx.send(SttEvent::CommandTimeout {
                                wake_session_id: ctx.wake_sid,
                                timeout_type: "bare_wake",
                            });
                            continue;
                        }
                    }

                    for tbr in config::get_phrases_to_remove(&i18n::get_language()) {
                        text = text.replace(*tbr, "");
                    }
                    text = text.trim().to_string();

                    if text.trim().is_empty() {
                        debug!(
                            "[STT][WAKE S:{}] empty transcript after cleanup — staying in CommandMode",
                            ctx.wake_sid
                        );
                        ctx.transition_to(
                            State::CommandMode { first: false },
                            "empty_after_cleanup",
                        );
                        continue;
                    }

                    if !has_command_intent(&text, &i18n::get_language()) {
                        info!(
                            "[STT][WAKE S:{}] low-quality command ignored: '{}'",
                            ctx.wake_sid, text
                        );
                        info!("[RESET][WAKE S:{}] speech_recognizer reason=low_quality_transcript", ctx.wake_sid);
                        stt::reset_speech_recognizer();
                        ctx.session_resets += 1;
                        ctx.session_timeouts += 1;
                        ctx.cooldown_clean = false;
                        info!("[TIMEOUT][WAKE S:{}] type=garbage_transcript", ctx.wake_sid);
                        ctx.transition_to(State::Cooldown, "low_quality_transcript");
                        let _ = event_tx.send(SttEvent::CommandTimeout {
                            wake_session_id: ctx.wake_sid,
                            timeout_type: "garbage_transcript",
                        });
                        continue;
                    }

                    if let Some(cmd_sid) = ctx.open_command_session(&text) {
                        ctx.transition_to(State::AwaitingChain, "command_recognized");
                        ctx.silence_frames = 0;
                        let _ = event_tx.send(SttEvent::SpeechRecognized {
                            text,
                            wake_session_id: ctx.wake_sid,
                            cmd_session_id: cmd_sid,
                        });
                    } else {
                        info!(
                            "[STT][WAKE S:{}] command session rejected (duplicate guard), ignoring transcript",
                            ctx.wake_sid
                        );
                    }
                    continue;
                }

                // Silence tracking (uses adaptive threshold).
                if frame.is_voice {
                    ctx.silence_frames = 0;
                } else {
                    ctx.silence_frames += 1;
                    if ctx.silence_frames > cmd_silence_threshold {
                        ctx.session_timeouts += 1;
                        ctx.cooldown_clean = false;
                        info!(
                            "[TIMEOUT][WAKE S:{}] type=silence_timeout elapsed_frames={} threshold={} conv_depth={}",
                            ctx.wake_sid, ctx.silence_frames, cmd_silence_threshold, conversation_depth
                        );
                        info!("[RESET][WAKE S:{}] speech_recognizer reason=silence_timeout", ctx.wake_sid);
                        stt::reset_speech_recognizer();
                        ctx.session_resets += 1;
                        ctx.transition_to(State::Cooldown, "silence_timeout");
                        ctx.silence_frames = 0;
                        let _ = event_tx.send(SttEvent::CommandTimeout {
                            wake_session_id: ctx.wake_sid,
                            timeout_type: "silence_timeout",
                        });
                        continue;
                    }
                }

                // Wall-clock timeout.
                if ctx.cmd_start.elapsed().map_or(false, |e| e > config::CMS_WAIT_DELAY) {
                    ctx.session_timeouts += 1;
                    ctx.cooldown_clean = false;
                    info!("[TIMEOUT][WAKE S:{}] type=wall_clock_timeout", ctx.wake_sid);
                    info!("[RESET][WAKE S:{}] speech_recognizer reason=wall_clock_timeout", ctx.wake_sid);
                    stt::reset_speech_recognizer();
                    ctx.session_resets += 1;
                    ctx.transition_to(State::Cooldown, "wall_clock_timeout");
                    ctx.silence_frames = 0;
                    let _ = event_tx.send(SttEvent::CommandTimeout {
                        wake_session_id: ctx.wake_sid,
                        timeout_type: "wall_clock_timeout",
                    });
                }
            }

            // ── 4. Post-command cooldown ──────────────────────────────────────
            State::Cooldown => {
                ctx.finalize_wake();
            }

            // ── 5. Waiting for chain/idle from main thread ────────────────────
            State::AwaitingChain => {
                // Frames discarded; SttCmd at top of loop. Watchdog covers hung execution.
            }

            // ── 6. QuietWindow — handled by gate above ────────────────────────
            State::QuietWindow => {}
        }
    }

    info!("[STT] Worker thread exiting");
}

// ── Command quality filter ────────────────────────────────────────────────────

fn has_command_intent(text: &str, lang: &str) -> bool {
    if lang != "ru" {
        return true;
    }

    const RU_VERBS: &[&str] = &[
        "открой", "открыть", "закрой", "закрыть",
        "включи", "включить", "выключи", "выключить",
        "запусти", "запустить", "останови", "остановить",
        "поставь", "поставить", "убавь", "прибавь",
        "воспроизведи", "воспроизвести",
        "переключи", "переключить",
        "найди", "найти", "покажи", "показать",
        "сыграй", "играй",
        "расскажи", "рассказать",
        "стоп", "пауза", "дальше", "назад",
        "следующий", "предыдущий",
        "создай", "создать", "удали", "удалить",
        "вычисли", "посчитай",
        "какая", "сколько", "который", "что",
    ];

    let lower = text.to_lowercase();
    RU_VERBS.iter().any(|v| lower.contains(v))
}
