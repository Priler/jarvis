use std::sync::mpsc::Receiver;
use std::time::SystemTime;

use jarvis_core::{audio_buffer::AudioRingBuffer, audio_processing, commands, config, listener, recorder, stt, COMMANDS_LIST, intent, voices, ipc::{self, IpcEvent}, i18n, slots};

use crate::should_stop;

// VAD state machine
#[derive(Debug, Clone, Copy, PartialEq)]
enum VadState {
    WaitingForVoice,
    VoiceActive,
}

pub fn start(text_cmd_rx: Receiver<String>, rt: &tokio::runtime::Runtime) -> Result<(), ()> {
    main_loop(text_cmd_rx, rt)
}

fn main_loop(text_cmd_rx: Receiver<String>, rt: &tokio::runtime::Runtime) -> Result<(), ()> {
    let frame_length: usize = 512;
    let sample_rate: usize = 16000;
    let mut frame_buffer: Vec<i16> = vec![0; frame_length];

    // ring buffer: keeps last 5 seconds of audio (pre-roll)
    let mut audio_buffer = AudioRingBuffer::new(5.0, frame_length, sample_rate);

    // VAD state
    let mut vad_state = VadState::WaitingForVoice;
    let mut silence_frames: u32 = 0;

    // how many frames of silence before we consider speech ended
    // 1.5 seconds = 1.5 * (16000 / 512) ≈ 47 frames
    let silence_threshold: u32 = ((1.5 * sample_rate as f32) / frame_length as f32) as u32;

    // WAV test mode: count silence frames after WAV is exhausted for auto-terminate
    let mut wav_done_silence: u32 = 0;
    // ~1.5 seconds of post-WAV silence before terminating
    let wav_done_grace: u32 = ((1.5 * sample_rate as f32) / frame_length as f32) as u32;
    
    voices::play_greet();

    match recorder::start_recording() {
        Ok(_) => info!("Recording started. Microphone: {}", 
            recorder::get_audio_device_name(recorder::get_selected_microphone_index())),
        Err(_) => {
            error!("Cannot start recording.");
            return Err(());
        }
    }

    ipc::send(IpcEvent::Idle);

    // ### WAKE WORD DETECTION LOOP
    'wake_word: loop {
        if should_stop() {
            info!("Stop signal received, shutting down...");
            voices::play_goodbye();
            ipc::send(IpcEvent::Stopping);
            break;
        }

        if let Ok(text) = text_cmd_rx.try_recv() {
            process_text_command(&text, &rt);
            continue 'wake_word;
        }

        recorder::read_microphone(&mut frame_buffer);
        let processed = audio_processing::process(&frame_buffer);

        // WAV test mode: auto-terminate after WAV is exhausted + grace period
        if recorder::is_wav_mode() && recorder::is_wav_done() {
            if vad_state == VadState::WaitingForVoice {
                wav_done_silence += 1;
                if wav_done_silence >= wav_done_grace {
                    info!("[AUDIO_TEST] WAV replay complete — shutting down");
                    info!("[WAKE] Total frames sent to Rustpotter: {}", listener::get_frame_count());
                    info!("[WAKE] Max score seen: {:.3}", listener::get_max_score());
                    info!("[WAKE] Min score threshold: {:.3}", config::RUSPOTTER_MIN_SCORE);
                    info!("[WAKE] Expected pipeline sample rate: 16000 Hz");
                    break 'wake_word;
                }
            } else {
                wav_done_silence = 0;
            }
        }

        match vad_state {
            VadState::WaitingForVoice => {
                // always buffer audio
                audio_buffer.push(&frame_buffer);

                if processed.is_voice {
                    // voice started — flush pre-roll buffer to wake word detector
                    info!("[VAD] Voice started, flushing {} buffered frames to wake word engine", audio_buffer.len());
                    
                    for buffered_frame in audio_buffer.drain_all() {
                        listener::data_callback(&buffered_frame);
                        let _ = stt::recognize_wake_word(&buffered_frame);
                    }
                    
                    vad_state = VadState::VoiceActive;
                    silence_frames = 0;
                }
            }
            
            VadState::VoiceActive => {
                // dual-feed: speech recognizer accumulates audio (captures "wake + command" in one breath)
                let _ = stt::recognize(&frame_buffer, false);

                // Vosk wake recognizer: grammar-constrained, works with any mic
                let vosk_wake = stt::recognize_wake_word(&frame_buffer).map_or(false, |(text, _conf)| {
                    let phrases = config::get_wake_phrases(&i18n::get_language());
                    let text_lower = text.to_lowercase();
                    let matched = phrases.iter().any(|wp| text_lower.contains(wp));
                    if matched {
                        info!("[WAKE] Vosk detected: '{}'", text);
                    }
                    matched
                });

                // Rustpotter wake detector
                let rustpotter_wake = listener::data_callback(&frame_buffer).is_some();

                if vosk_wake || rustpotter_wake {
                    // WAKE WORD DETECTED!
                    if crate::is_muted() {
                        info!("[MUTED] Wake word detected but muted — ignoring");
                        stt::reset_wake_recognizer();
                        audio_processing::reset();
                        vad_state = VadState::WaitingForVoice;
                        silence_frames = 0;
                        audio_buffer.clear();
                        continue 'wake_word;
                    }

                    info!("Wake word activated!");
                    info!("[IPC] → WakeWordDetected");
                    ipc::send(IpcEvent::WakeWordDetected);
                    
                    stt::reset_wake_recognizer();
                    audio_processing::reset();

                    // brief sniff to keep feeding STT while transitioning
                    let sniff_frames = ((0.3 * sample_rate as f32) / frame_length as f32) as u32;
                    for _ in 0..sniff_frames {
                        recorder::read_microphone(&mut frame_buffer);
                        audio_processing::process(&frame_buffer);
                        stt::recognize(&frame_buffer, false);
                    }

                    info!("[IPC] → Listening");
                    ipc::send(IpcEvent::Listening);
                    recognize_command(&mut frame_buffer, &rt, frame_length, sample_rate, true);

                    // reset state after command
                    vad_state = VadState::WaitingForVoice;
                    silence_frames = 0;
                    audio_buffer.clear();
                    stt::reset_wake_recognizer();
                    stt::reset_speech_recognizer(); // NOW reset, after command is done
                    audio_processing::reset();
                    info!("[IPC] → Idle (after command)");
                    ipc::send(IpcEvent::Idle);

                    continue 'wake_word;
                }
                
                // track silence
                if processed.is_voice {
                    silence_frames = 0;
                } else {
                    silence_frames += 1;
                    
                    if silence_frames > silence_threshold {
                        debug!("[VAD] Silence timeout — returning to WaitingForVoice");
                        vad_state = VadState::WaitingForVoice;
                        silence_frames = 0;
                        stt::reset_wake_recognizer();
                        stt::reset_speech_recognizer(); // reset since we were dual-feeding
                    }
                }
            }
        }
    }

    recorder::stop_recording().ok();
    ipc::send(IpcEvent::Stopping);

    Ok(())
}


// Voice recognition for command after wake word
fn recognize_command(
    frame_buffer: &mut [i16],
    rt: &tokio::runtime::Runtime,
    frame_length: usize,
    sample_rate: usize,
    prefed_audio: bool
) {
    let mut audio_buffer = AudioRingBuffer::new(2.0, frame_length, sample_rate);
    let mut vad_state = if prefed_audio {
        VadState::VoiceActive
    } else {
        VadState::WaitingForVoice
    };
    let mut silence_frames: u32 = 0;
    let mut start = SystemTime::now();
    let mut first_recognition = prefed_audio;
    
    // longer silence threshold for commands (user might pause to think)
    // 5 seconds
    let silence_threshold: u32 = ((5.0 * sample_rate as f32) / frame_length as f32) as u32;
    
    loop {
        if crate::should_stop() {
            return;
        }
        
        recorder::read_microphone(frame_buffer);
        let processed = audio_processing::process(frame_buffer);
        
        match vad_state {
            VadState::WaitingForVoice => {
                audio_buffer.push(frame_buffer);
                
                if processed.is_voice {
                    // flush buffer to STT
                    for buffered_frame in audio_buffer.drain_all() {
                        stt::recognize(&buffered_frame, false);
                    }
                    vad_state = VadState::VoiceActive;
                    silence_frames = 0;
                } else {
                    silence_frames += 1;
                    
                    if silence_frames > silence_threshold {
                        info!("Long silence detected, returning to wake word mode.");
                        return;
                    }
                }
            }
            
            VadState::VoiceActive => {
                // feed to STT
                if let Some(mut recognized_voice) = stt::recognize(frame_buffer, false) {
                    info!("[STT] final transcript: '{}'", recognized_voice);
                    
                    ipc::send(IpcEvent::SpeechRecognized {
                        text: recognized_voice.clone(),
                    });
                    
                    recognized_voice = recognized_voice.to_lowercase();
                    
                    // check if wake word repeated (reactivate)
                    let wake_phrases = config::get_wake_phrases(&i18n::get_language());
                    let contains_wake = wake_phrases.iter().any(|wp| recognized_voice.contains(wp));

                    // On the first recognition after wake-word detection, the STT has accumulated
                    // audio from the dual-feed buffer which includes the wake word pronunciation.
                    // If the wake word wasn't recognized exactly (e.g. STT model misread it),
                    // strip the leading word as the likely misrecognized wake word.
                    if first_recognition && !contains_wake {
                        let words: Vec<&str> = recognized_voice.split_whitespace().collect();
                        if words.len() > 1 {
                            info!("[STT] Stripping likely misrecognized wake word: '{}' → '{}'",
                                words[0], words[1..].join(" "));
                            recognized_voice = words[1..].join(" ");
                        }
                    }

                    if contains_wake {
                        // strip the wake word
                        let mut remaining = recognized_voice.clone();
                        for wp in wake_phrases {
                            remaining = remaining.replace(wp, "");
                        }
                        let remaining = remaining.trim();

                        if remaining.is_empty() {
                            if first_recognition {
                                // leftover wake word from dual-feed, just discard it
                                info!("Discarding initial wake word from prefed audio");
                                first_recognition = false;
                                stt::reset_speech_recognizer();
                                voices::play_reply();
                                vad_state = VadState::WaitingForVoice;
                                silence_frames = 0;
                                start = SystemTime::now();
                                audio_buffer.clear();
                                continue;
                            }

                            // just wake word, no command - reactivate
                            info!("Wake word repeated during chaining, reactivating...");
                            voices::play_reply();
                            stt::reset_speech_recognizer();
                            ipc::send(IpcEvent::Listening);
                            
                            vad_state = VadState::WaitingForVoice;
                            silence_frames = 0;
                            start = SystemTime::now();
                            audio_buffer.clear();
                            continue;
                        } else {
                            // wake word + command in one phrase - execute the command part
                            info!("Wake word + command during chaining: '{}'", remaining);
                            recognized_voice = remaining.to_string();
                            // fall through to command execution below
                        }
                    }

                    first_recognition = false;
                    
                    // filter activation phrases
                    // for tbr in config::ASSISTANT_PHRASES_TBR {
                    //     recognized_voice = recognized_voice.replace(tbr, "");
                    // }
                    for tbr in config::get_phrases_to_remove(&i18n::get_language()) {
                        recognized_voice = recognized_voice.replace(tbr, "");
                    }

                    recognized_voice = recognized_voice.trim().to_string();
                    
                    if recognized_voice.len() < 5 {
                        debug!("Ignoring too short recognition: '{}'", recognized_voice);
                        continue;
                    }

                    if recognized_voice.is_empty() {
                        continue;
                    }
                    
                    // execute command and check if we should chain
                    let should_chain = execute_command(&recognized_voice, rt);
                    
                    if should_chain {
                        // chain: reset and continue listening
                        info!("Chaining enabled, continuing to listen...");
                        stt::reset_speech_recognizer();
                        vad_state = VadState::WaitingForVoice;
                        silence_frames = 0;
                        start = SystemTime::now();
                        audio_buffer.clear();
                        ipc::send(IpcEvent::Listening);
                        continue;
                    } else {
                        // no chain: return to wake word
                        info!("No chain, returning to wake word mode.");
                        return;
                    }
                }
                
                // track silence
                if processed.is_voice {
                    silence_frames = 0;
                } else {
                    silence_frames += 1;
                    
                    if silence_frames > silence_threshold {
                        info!("Long silence detected, returning to wake word mode.");
                        return;
                    }
                }
            }
        }
        
        // timeout
        if let Ok(elapsed) = start.elapsed() {
            if elapsed > config::CMS_WAIT_DELAY {
                info!("Command timeout, returning to wake word mode.");
                return;
            }
        }
    }
}


fn process_text_command(text: &str, rt: &tokio::runtime::Runtime) {
    info!("Processing text command: {}", text);
    
    ipc::send(IpcEvent::SpeechRecognized { text: text.to_string() });
    
    let mut filtered = text.to_lowercase();
    // for tbr in config::ASSISTANT_PHRASES_TBR {
    //     filtered = filtered.replace(tbr, "");
    // }
    for tbr in config::get_phrases_to_remove(&i18n::get_language()) {
        filtered = filtered.replace(tbr, "");
    }

    let filtered = filtered.trim();
    
    if filtered.is_empty() {
        ipc::send(IpcEvent::Idle);
        return;
    }
    
    // text commands never chain
    execute_command(filtered, rt);
}


// Execute command, returns true if chaining should continue
fn execute_command(text: &str, rt: &tokio::runtime::Runtime) -> bool {
    let commands_lock = match COMMANDS_LIST.get() {
        Some(lock) => lock,
        None => {
            ipc::send(IpcEvent::Error { message: "Commands not loaded".to_string() });
            ipc::send(IpcEvent::Idle);
            return false;
        }
    };
    let commands_list = commands_lock.read();
    
    let cmd_result = if let Some((intent_id, confidence)) =
        rt.block_on(intent::classify(text))
    {
        info!("Intent recognized: {} (confidence: {:.2})", intent_id, confidence);
        intent::get_command_by_intent(&*commands_list, &intent_id)
    } else {
        info!("Intent not recognized, trying levenshtein fallback...");
        commands::fetch_command(text, &*commands_list)
    };
    
    if let Some((cmd_path, cmd_config)) = cmd_result {
        info!("[COMMAND] Matched: {:?}", cmd_path);
        
        // extract slots if needed
        let extracted_slots = if !cmd_config.slots.is_empty() {
            let s = slots::extract(text, &cmd_config.slots);
            if !s.is_empty() {
                info!("Extracted slots: {:?}", s);
            }
            Some(s)
        } else {
            None
        };

        // Require GUI confirmation for dangerous CLI commands
        if commands::requires_confirmation(&cmd_config) {
            let cmd_str = if cmd_config.cli_args.is_empty() {
                cmd_config.cli_cmd.clone()
            } else {
                format!("{} {}", cmd_config.cli_cmd, cmd_config.cli_args.join(" "))
            };
            commands::store_pending_command(cmd_path, &cmd_config);
            ipc::send(IpcEvent::ConfirmationRequired {
                id: cmd_config.id.clone(),
                description: cmd_config.description.clone(),
                cmd: cmd_str,
            });
            ipc::send(IpcEvent::Idle);
            return false;
        }

        match commands::execute_command(&cmd_path, &cmd_config, Some(&text), extracted_slots.as_ref()) {
            Ok(chain) => {
                info!("[COMMAND] Executed successfully: {}", cmd_config.id);
                // voices::play_ok();
                voices::play_random_from(cmd_config.get_sounds(&i18n::get_language()).as_slice());
                ipc::send(IpcEvent::CommandExecuted {
                    id: cmd_config.id.clone(),
                    success: true,
                });
                ipc::send(IpcEvent::Idle);
                return chain; // return chain status from command
            }
            Err(msg) => {
                error!("Error executing command: {}", msg);
                voices::play_error();
                ipc::send(IpcEvent::CommandExecuted {
                    id: cmd_config.id.clone(),
                    success: false,
                });
                ipc::send(IpcEvent::Error { message: msg.to_string() });
            }
        }
    } else {
        info!("No command found for: {}", text);
        voices::play_not_found();
        ipc::send(IpcEvent::Error { 
            message: format!("Command not found: {}", text) 
        });
    }
    
    ipc::send(IpcEvent::Idle);
    false // no chain on error or not found
}


pub fn close(code: i32) {
    info!("Closing application.");
    voices::play_goodbye();
    ipc::send(IpcEvent::Stopping);
    std::process::exit(code);
}