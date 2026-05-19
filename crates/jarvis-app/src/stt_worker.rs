use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, Duration, SystemTime};

use jarvis_core::{
    audio_buffer::AudioRingBuffer,
    audio, audio_processing, config, i18n, stt,
};

// ── Pipeline timing constants ─────────────────────────────────────────────────

/// Ignore any new wake event within this window after a confirmed wake.
const WAKE_DEBOUNCE_MS: u64 = 2500;

/// After Cooldown clears, keep discarding frames for this long before listening.
const QUIET_WINDOW_MS: u64 = 1000;

/// Require this many consecutive voice frames before triggering Voice onset.
/// Filters brief transients, claps, and reverb tails (3 × ~32 ms = ~96 ms).
const MIN_VOICE_FRAMES_FOR_ONSET: u32 = 3;

/// Timestamp (ms since epoch) of the most recent confirmed wake event.
static LAST_WAKE_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Message types ─────────────────────────────────────────────────────────────

/// One audio frame from the capture thread to the STT worker.
pub struct AudioFrame {
    pub pcm: Vec<i16>,
    pub is_voice: bool,
    /// True if Rustpotter fired a wake-word detection on this frame.
    pub rustpotter_wake: bool,
}

/// Events sent from the STT worker back to the main thread.
pub enum SttEvent {
    /// Vosk or Rustpotter confirmed the wake word.
    WakeDetected,
    /// Final transcript of a spoken command, ready for dispatch.
    SpeechRecognized(String),
    /// Silence or wall-clock timeout in CommandMode; worker reset to idle.
    CommandTimeout,
}

/// Instructions sent from the main thread to the STT worker after command execution.
pub enum SttCmd {
    /// Command supports chaining — stay in CommandMode.
    Chain,
    /// No chaining — return to WaitingForVoice.
    Idle,
}

// ── Internal state machine ────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum State {
    WaitingForVoice,
    VoiceActive,
    /// Collecting a spoken command.
    /// `first` — strip likely mis-recognised wake word from first transcript.
    CommandMode { first: bool },
    /// Recognition done; waiting for Chain/Idle from the main thread.
    AwaitingChain,
    /// SttCmd::Idle received; waiting for audio playback + reverb to clear
    /// before accepting microphone input again.
    Cooldown,
    /// Short dead-zone immediately after Cooldown to block residual noise from
    /// triggering a new Voice onset.
    QuietWindow,
}

// ── Worker entry point ────────────────────────────────────────────────────────

pub fn run(
    frame_rx: Receiver<AudioFrame>,
    cmd_rx: Receiver<SttCmd>,
    event_tx: SyncSender<SttEvent>,
    frame_length: usize,
    sample_rate: usize,
) {
    let mut pre_roll = AudioRingBuffer::new(5.0, frame_length, sample_rate);
    let mut state = State::WaitingForVoice;
    let mut silence_frames: u32 = 0;
    let mut cmd_start = SystemTime::now();
    let mut speaking_drop_count: u32 = 0;
    // Set to Instant::now() when entering QuietWindow; expires after QUIET_WINDOW_MS.
    let mut quiet_until = Instant::now();
    // Consecutive voice frames seen in WaitingForVoice; Voice onset requires MIN_VOICE_FRAMES_FOR_ONSET.
    let mut consecutive_voice_frames: u32 = 0;

    let wake_silence_threshold =
        ((1.5 * sample_rate as f32) / frame_length as f32) as u32;
    let cmd_silence_threshold =
        ((5.0 * sample_rate as f32) / frame_length as f32) as u32;

    for frame in &frame_rx {
        // Handle chain/idle decision from the main thread (non-blocking).
        if let Ok(cmd) = cmd_rx.try_recv() {
            state = match (cmd, state) {
                (SttCmd::Chain, State::AwaitingChain) => {
                    info!("[STT] Chaining next command");
                    stt::reset_speech_recognizer();
                    silence_frames = 0;
                    cmd_start = SystemTime::now();
                    State::CommandMode { first: false }
                }
                (SttCmd::Idle, State::AwaitingChain) => {
                    // Do NOT transition directly to WaitingForVoice while audio may still
                    // be playing. Enter Cooldown and let the speaking gate drain; only
                    // after is_speaking() clears will we flush buffers and start listening.
                    info!("[STATE] Executing → Cooldown (waiting for audio to clear)");
                    stt::reset_speech_recognizer();
                    silence_frames = 0;
                    State::Cooldown
                }
                (_, s) => s,
            };
        }

        // Speaking gate: discard frames from all states except CommandMode while the
        // assistant is playing audio.  CommandMode must keep receiving frames so Vosk
        // can capture the user's command even while the wake-confirmation ding plays.
        if audio::is_speaking() && !matches!(state, State::CommandMode { .. }) {
            speaking_drop_count += 1;
            if speaking_drop_count == 1 {
                info!("[STT] Speaking gate active — discarding mic frames");
            }
            continue;
        }
        // Log when the gate clears.
        if speaking_drop_count > 0 {
            info!("[STT] Speaking gate cleared after {} dropped frames", speaking_drop_count);
            speaking_drop_count = 0;
        }

        // QuietWindow gate: discard frames during the post-cooldown dead zone.
        if matches!(state, State::QuietWindow) {
            if Instant::now() < quiet_until {
                debug!("[VAD] ignored: quiet window active");
                consecutive_voice_frames = 0;
                continue;
            }
            // Window expired: transition to WaitingForVoice.
            info!("[STATE] QuietWindow → Idle");
            state = State::WaitingForVoice;
            consecutive_voice_frames = 0;
            continue;
        }

        match state {
            // ── 1. Pre-wake: buffer pre-roll audio ────────────────────────────
            State::WaitingForVoice => {
                pre_roll.push(&frame.pcm);
                if frame.is_voice {
                    consecutive_voice_frames += 1;
                    if consecutive_voice_frames < MIN_VOICE_FRAMES_FOR_ONSET {
                        debug!(
                            "[VAD] ignored: too short speech ({}/{} consecutive frames)",
                            consecutive_voice_frames, MIN_VOICE_FRAMES_FOR_ONSET
                        );
                    } else {
                        info!(
                            "[STATE] Idle → Listening (voice onset after {} frames, flushing {})",
                            consecutive_voice_frames, pre_roll.len()
                        );
                        for buffered in pre_roll.drain_all() {
                            let _ = stt::recognize_wake_word(&buffered);
                        }
                        state = State::VoiceActive;
                        silence_frames = 0;
                        consecutive_voice_frames = 0;
                    }
                } else {
                    consecutive_voice_frames = 0;
                }
            }

            // ── 2. Wake word detection (Vosk grammar + Rustpotter flag) ───────
            State::VoiceActive => {
                // Dual-feed: speech recognizer accumulates audio so it captures
                // the "wake word + command" in one utterance if the user speaks both.
                let _ = stt::recognize(&frame.pcm, false);

                // Grammar-constrained wake detector (Vosk).  Expensive (~10-12 ms
                // per 512-sample frame) — this is why the worker runs off the audio thread.
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
                    // Wake debounce: ignore spurious re-triggers within WAKE_DEBOUNCE_MS.
                    let now = now_ms();
                    let last = LAST_WAKE_MS.load(Ordering::Acquire);
                    if last > 0 && now.saturating_sub(last) < WAKE_DEBOUNCE_MS {
                        info!(
                            "[WAKE] ignored: debounce active ({} ms since last wake, need {} ms)",
                            now.saturating_sub(last), WAKE_DEBOUNCE_MS
                        );
                        // Fall back to idle; the user can try again after the debounce window.
                        state = State::WaitingForVoice;
                        silence_frames = 0;
                        consecutive_voice_frames = 0;
                        stt::reset_wake_recognizer();
                        stt::reset_speech_recognizer();
                        continue;
                    }
                    LAST_WAKE_MS.store(now, Ordering::Release);

                    info!(
                        "[STT] Wake confirmed (vosk={} rustpotter={})",
                        vosk_wake, frame.rustpotter_wake
                    );
                    stt::reset_wake_recognizer();
                    // Keep speech recognizer running — it already has wake+command audio
                    // accumulated via dual-feed.  CommandMode { first: true } will strip
                    // the garbled wake-word artefact from the first transcript.
                    audio_processing::reset();
                    info!("[STATE] Listening → Recognizing");
                    state = State::CommandMode { first: true };
                    silence_frames = 0;
                    cmd_start = SystemTime::now();
                    let _ = event_tx.send(SttEvent::WakeDetected);
                } else if !frame.is_voice {
                    silence_frames += 1;
                    if silence_frames > wake_silence_threshold {
                        debug!("[STT] Wake silence timeout → WaitingForVoice");
                        state = State::WaitingForVoice;
                        silence_frames = 0;
                        stt::reset_wake_recognizer();
                        stt::reset_speech_recognizer();
                    }
                } else {
                    silence_frames = 0;
                }
            }

            // ── 3. Collect spoken command ─────────────────────────────────────
            State::CommandMode { first } => {
                if let Some(mut text) = stt::recognize(&frame.pcm, false) {
                    info!("[STT] transcript: '{}'", text);
                    text = text.to_lowercase();

                    let wake_phrases = config::get_wake_phrases(&i18n::get_language());
                    let contains_wake =
                        wake_phrases.iter().any(|wp| text.contains(*wp));

                    // First recognition after wake: the speech recognizer holds audio
                    // that includes the wake word (dual-feed from VoiceActive).
                    if first && !contains_wake {
                        let words: Vec<&str> = text.split_whitespace().collect();
                        if words.len() == 1 {
                            // Single-word transcript = garbled wake word alone (user paused
                            // after "джарвис" waiting for the beep).  Reset and listen for
                            // the actual command.
                            info!("[STT] First transcript '{}' is wake-word only — resetting for command", text);
                            stt::reset_speech_recognizer();
                            state = State::CommandMode { first: false };
                            silence_frames = 0;
                            continue;
                        } else if words.len() > 1 {
                            // Multi-word transcript: first word is garbled wake word,
                            // rest is the command (one-breath style: "джарвис открой…").
                            info!(
                                "[STT] Stripping wake-word artefact '{}' → '{}'",
                                words[0],
                                words[1..].join(" ")
                            );
                            text = words[1..].join(" ");
                        }
                    }

                    // Wake word + command in a single phrase.
                    if contains_wake {
                        for wp in wake_phrases {
                            text = text.replace(*wp, "");
                        }
                        text = text.trim().to_string();
                        if text.is_empty() {
                            // Bare wake in CommandMode: the user said the wake word but
                            // gave no command.  Do NOT reactivate — that would play a second
                            // reply sound and cause a double-response loop.  Return silently.
                            info!("[STT] bare wake ignored — no command captured, returning to Cooldown");
                            stt::reset_speech_recognizer();
                            silence_frames = 0;
                            state = State::Cooldown;
                            // Inform the main thread so IPC goes to Idle.
                            let _ = event_tx.send(SttEvent::CommandTimeout);
                            continue;
                        }
                    }

                    // Strip filler phrases ("пожалуйста", "please", …).
                    for tbr in config::get_phrases_to_remove(&i18n::get_language()) {
                        text = text.replace(*tbr, "");
                    }
                    text = text.trim().to_string();

                    if text.len() < 5 {
                        debug!("[STT] Too-short transcript '{}' — staying in CommandMode", text);
                        state = State::CommandMode { first: false };
                        continue;
                    }

                    // Reject obvious garbage transcripts before hitting the matcher.
                    // A valid command requires at least one word that looks like an action
                    // verb in the active language.  This filters common STT artefacts like
                    // "стрит тупой" that survive wake-word stripping but have no command intent.
                    if !has_command_intent(&text, &i18n::get_language()) {
                        info!("[STT] low-quality command ignored: '{}'", text);
                        stt::reset_speech_recognizer();
                        silence_frames = 0;
                        state = State::Cooldown;
                        let _ = event_tx.send(SttEvent::CommandTimeout);
                        continue;
                    }

                    // Good command: send to main thread and wait for chain decision.
                    info!("[STATE] Recognizing → Executing");
                    let _ = event_tx.send(SttEvent::SpeechRecognized(text));
                    state = State::AwaitingChain;
                    silence_frames = 0;
                    continue;
                }

                // Silence tracking.
                if frame.is_voice {
                    silence_frames = 0;
                } else {
                    silence_frames += 1;
                    if silence_frames > cmd_silence_threshold {
                        info!("[STT] Command silence timeout → WaitingForVoice");
                        stt::reset_speech_recognizer();
                        state = State::WaitingForVoice;
                        silence_frames = 0;
                        let _ = event_tx.send(SttEvent::CommandTimeout);
                        continue;
                    }
                }

                // Wall-clock timeout.
                if cmd_start.elapsed().map_or(false, |e| e > config::CMS_WAIT_DELAY) {
                    info!("[STT] Command wall-clock timeout → WaitingForVoice");
                    stt::reset_speech_recognizer();
                    state = State::WaitingForVoice;
                    silence_frames = 0;
                    let _ = event_tx.send(SttEvent::CommandTimeout);
                }
            }

            // ── 4. Post-command cooldown ──────────────────────────────────────
            // SttCmd::Idle was received. We stay here (speaking gate discards all
            // frames) until is_speaking() returns false, then flush every buffer
            // and enter QuietWindow before resuming listening.
            State::Cooldown => {
                // is_speaking() must be false here — the gate above continued otherwise.
                stt::reset_speech_recognizer();
                stt::reset_wake_recognizer();
                audio_processing::reset();
                pre_roll = AudioRingBuffer::new(5.0, frame_length, sample_rate);
                silence_frames = 0;
                consecutive_voice_frames = 0;
                info!("[STT] buffers cleared after cooldown");
                info!("[STATE] Cooldown → QuietWindow ({}ms dead-zone)", QUIET_WINDOW_MS);
                quiet_until = Instant::now() + Duration::from_millis(QUIET_WINDOW_MS);
                state = State::QuietWindow;
                // QuietWindow is handled by the gate at the top of the loop.
            }

            // ── 5. Waiting for chain/idle from main thread ────────────────────
            State::AwaitingChain => {
                // Discard incoming frames while main thread processes the command.
                // The SttCmd decision is handled at the top of the loop.
            }

            // ── 6. QuietWindow ────────────────────────────────────────────────
            // Handled fully by the inline gate above (uses `continue`).
            // This arm is unreachable at runtime but required for exhaustiveness.
            State::QuietWindow => {}
        }
    }

    info!("[STT] Worker thread exiting");
}

// ── Command quality filter ────────────────────────────────────────────────────

/// Returns true if the stripped transcript is likely a real command rather than
/// a noise artefact or garbled wake-word tail.
///
/// For Russian: requires at least one word from a known set of action verbs.
/// For other languages: passes everything (threshold-based rejection in the
/// matcher provides sufficient protection via CMD_RATIO_THRESHOLD).
fn has_command_intent(text: &str, lang: &str) -> bool {
    if lang != "ru" {
        return true;
    }

    // Words/prefixes that strongly signal a command in Russian.
    // Short but decisive: stop words and standalone command words are included.
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
