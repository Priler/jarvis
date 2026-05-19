use std::sync::mpsc::{Receiver, SyncSender};
use std::time::SystemTime;

use jarvis_core::{
    audio_buffer::AudioRingBuffer,
    audio_processing, config, i18n, stt,
};

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
                    info!("[STT] Returning to idle after command");
                    stt::reset_speech_recognizer();
                    silence_frames = 0;
                    State::WaitingForVoice
                }
                (_, s) => s,
            };
        }

        match state {
            // ── 1. Pre-wake: buffer pre-roll audio ────────────────────────────
            State::WaitingForVoice => {
                pre_roll.push(&frame.pcm);
                if frame.is_voice {
                    info!("[STT] Voice onset — flushing {} pre-roll frames", pre_roll.len());
                    for buffered in pre_roll.drain_all() {
                        let _ = stt::recognize_wake_word(&buffered);
                    }
                    state = State::VoiceActive;
                    silence_frames = 0;
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
                    info!(
                        "[STT] Wake confirmed (vosk={} rustpotter={})",
                        vosk_wake, frame.rustpotter_wake
                    );
                    stt::reset_wake_recognizer();
                    audio_processing::reset();
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

                    // First recognition after wake often includes the wake word
                    // itself (mis-transcribed or from pre-roll audio).  Strip it.
                    if first && !contains_wake {
                        let words: Vec<&str> = text.split_whitespace().collect();
                        if words.len() > 1 {
                            info!(
                                "[STT] Stripping likely wake-word artefact '{}' → '{}'",
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
                            // Bare wake-word repeat — reactivate command mode.
                            info!("[STT] Bare wake word in CommandMode — reactivating");
                            stt::reset_speech_recognizer();
                            state = State::CommandMode { first: false };
                            silence_frames = 0;
                            cmd_start = SystemTime::now();
                            let _ = event_tx.send(SttEvent::WakeDetected);
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

                    // Good command: send to main thread and wait for chain decision.
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

            // ── 4. Waiting for chain/idle from main thread ────────────────────
            State::AwaitingChain => {
                // Discard incoming frames while main thread processes the command.
                // The SttCmd decision is handled at the top of the loop.
            }
        }
    }

    info!("[STT] Worker thread exiting");
}
