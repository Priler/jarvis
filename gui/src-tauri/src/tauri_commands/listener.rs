use porcupine::{Porcupine, PorcupineBuilder};
use std::ops::Sub;
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::Path;
use log::{info, warn, error};
use rustpotter::{Rustpotter, RustpotterConfig, WavFmt, DetectorConfig, FiltersConfig, ScoreMode, GainNormalizationConfig, BandPassConfig};
// use dasp::{sample::ToSample, Sample};

// use crate::events::Payload;
use tauri::Manager;

use rand::seq::SliceRandom;
use std::time::SystemTime;
use once_cell::sync::OnceCell;
use std::sync::Mutex;

use crate::assistant_commands;
use crate::events;

use crate::config;
use crate::vosk;
use crate::recorder::{self, FRAME_LENGTH};

use crate::COMMANDS;
use crate::DB;

// track listening state
static LISTENING: AtomicBool = AtomicBool::new(false);

// stop listening with Atomic flag (to make it work between different threads)
static STOP_LISTENING: AtomicBool = AtomicBool::new(false);

// store tauri app_handle
static TAURI_APP_HANDLE: OnceCell<tauri::AppHandle> = OnceCell::new();

// store porcupine instance
static PORCUPINE: OnceCell<Porcupine> = OnceCell::new();

// store rustpotter instance
static RUSTPOTTER: OnceCell<Mutex<Rustpotter>> = OnceCell::new();

#[tauri::command]
pub fn is_listening() -> bool {
    LISTENING.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn stop_listening() {
    if is_listening() {
        STOP_LISTENING.store(true, Ordering::SeqCst);
        stop_recording();
    }

    // wait until listening stops
    while is_listening() {}
}

fn get_wake_word_engine() -> config::WakeWordEngine {
    let selected_wake_word_engine;
    if let Some(wwengine) = DB.lock().unwrap().get::<String>("selected_wake_word_engine") {
        // from db
        match wwengine.trim().to_lowercase().as_str() {
            "rustpotter" => selected_wake_word_engine = config::WakeWordEngine::Rustpotter,
            "vosk" => selected_wake_word_engine = config::WakeWordEngine::Vosk,
            "picovoice" => selected_wake_word_engine = config::WakeWordEngine::Porcupine,
            _ => selected_wake_word_engine = config::DEFAULT_WAKE_WORD_ENGINE
        }
    } else {
        // default
        selected_wake_word_engine = config::DEFAULT_WAKE_WORD_ENGINE; // set default wake_word engine
    }

    selected_wake_word_engine
}

#[tauri::command(async)]
pub fn start_listening(app_handle: tauri::AppHandle) -> Result<bool, String> {
    // only one listener thread is allowed
    if is_listening() {
        return Err("Already listening.".into());
    }

    // keep app handle
    if TAURI_APP_HANDLE.get().is_none() {
        TAURI_APP_HANDLE.set(app_handle);
    }

    // call selected wake-word engine listener command
    match get_wake_word_engine() {
        config::WakeWordEngine::Rustpotter => {
            info!("Starting RUSTPOTTER wake-word engine ...");
            return rustpotter_init();
        },
        config::WakeWordEngine::Vosk => {
            info!("Starting VOSK wake-word engine ...");
            return vosk_init();
        },
        config::WakeWordEngine::Porcupine => {
            info!("Starting PICOVOICE PORCUPINE wake-word engine ...");
            return picovoice_init();
        }
    }
}

fn keyword_callback(_keyword_index: i32) {
    // vars
    let mut start: SystemTime = SystemTime::now();
    let mut frame_buffer = vec![0; recorder::FRAME_LENGTH.load(Ordering::SeqCst) as usize];

    // play greet phrase
    events::play(
        config::ASSISTANT_GREET_PHRASES
            .choose(&mut rand::thread_rng())
            .unwrap(),
        TAURI_APP_HANDLE.get().unwrap(),
    );

    // emit assistant greet event
    TAURI_APP_HANDLE.get().unwrap()
        .emit_all(events::EventTypes::AssistantGreet.get(), ())
        .unwrap();

    // the loop
    while !STOP_LISTENING.load(Ordering::SeqCst) {
        recorder::read_microphone(&mut frame_buffer);

        // vosk part (partials included)
        if let Some(mut test) = vosk::recognize(&frame_buffer, false) {
            if !test.is_empty() {
                println!("Recognized: {}", test);

                // some filtration
                test = test.to_lowercase();
                for tbr in config::ASSISTANT_PHRASES_TBR {
                    test = test.replace(tbr, "");
                }
                test = test.trim().into();

                // infer command
                if let Some((cmd_path, cmd_config)) =
                    assistant_commands::fetch_command(&test, &COMMANDS)
                {
                    println!("Recognized (filtered): {}", test);
                    println!("Command found: {:?}", cmd_path);
                    println!("Executing ...");

                    let cmd_result = assistant_commands::execute_command(
                        &cmd_path,
                        &cmd_config,
                        TAURI_APP_HANDLE.get().unwrap(),
                    );

                    match cmd_result {
                        Ok(chain) => {
                            println!("Command executed successfully!");

                            if chain {
                                // continue chaining commands
                                start = SystemTime::now(); // listen for more commands
                            } else {
                                // skip forward if chaining is not required
                                start = start.checked_sub(core::time::Duration::from_secs(1000)).unwrap();
                            }

                            continue;
                        }
                        Err(error_message) => {
                            println!("Error executing command: {}", error_message);
                        }
                    }

                    TAURI_APP_HANDLE.get().unwrap()
                        .emit_all(events::EventTypes::AssistantWaiting.get(), ())
                        .unwrap();
                    break; // return to picovoice after command execution (no matter successfull or not)
                }
            }
        }

        match start.elapsed() {
            Ok(elapsed) if elapsed > config::CMS_WAIT_DELAY => {
                // return to picovoice after N seconds
                TAURI_APP_HANDLE.get().unwrap()
                    .emit_all(events::EventTypes::AssistantWaiting.get(), ())
                    .unwrap();
                break;
            }
            _ => (),
        }
    }
}

pub fn data_callback(frame_buffer: &[i16]) {
    // println!("DATA CALLBACK {}", frame_buffer.len());
    match get_wake_word_engine() {
        config::WakeWordEngine::Rustpotter => {
            let mut lock = RUSTPOTTER.get().unwrap().lock();
            let rustpotter = lock.as_mut().unwrap();
            let detection = rustpotter.process_i16(&frame_buffer);

            if let Some(detection) = detection {
                if detection.score > config::RUSPOTTER_MIN_SCORE {
                    info!("Rustpotter detection info:\n{:?}", detection);
                    keyword_callback(0);
                } else {
                    info!("Rustpotter detection info:\n{:?}", detection);
                }
            }
        },
        config::WakeWordEngine::Vosk => {
            // recognize & convert to sequence
            let recognized_phrase = vosk::recognize(&frame_buffer, true).unwrap_or("".into());

            if !recognized_phrase.trim().is_empty() {
                info!("Rec: {}", recognized_phrase);
                let recognized_phrases = recognized_phrase.split_whitespace();
                for phrase in recognized_phrases {
                    let recognized_phrase_chars = phrase.trim().to_lowercase().chars().collect::<Vec<_>>();
            
                    // compare
                    let compare_ratio = seqdiff::ratio(&config::VOSK_FETCH_PHRASE.chars().collect::<Vec<_>>(), &recognized_phrase_chars);
                    info!("OG phrase: {:?}", &config::VOSK_FETCH_PHRASE);
                    info!("Recognized phrase: {:?}", &recognized_phrase_chars);
                    info!("Compare ratio: {}", compare_ratio);

                    if compare_ratio >= config::VOSK_MIN_RATIO {
                        info!("Phrase activated.");
                        keyword_callback(0);
                        break;
                    }
                }
            }
        },
        config::WakeWordEngine::Porcupine => {
            if let Ok(keyword_index) = PORCUPINE.get().unwrap().process(&frame_buffer) {
                if keyword_index >= 0 {
                    // println!("Yes, sir! {}", keyword_index);
                    keyword_callback(keyword_index);
                }
            }
        }
    }
}

fn start_recording() -> Result<bool, String> {
    // vars
    let frame_length: usize;

    // idenfity frame length
    match get_wake_word_engine() {
        config::WakeWordEngine::Rustpotter => {
            // start recording for Rustpotter
            // You need a buffer of size `rustpotter.get_samples_per_frame()` when using samples.
            // You need a buffer of size `rustpotter.get_bytes_per_frame()` when using bytes.
            frame_length = RUSTPOTTER.get().unwrap().lock().unwrap().get_samples_per_frame();
            recorder::FRAME_LENGTH.store(frame_length as u32, Ordering::SeqCst);
        },
        config::WakeWordEngine::Vosk => {
            // start recording for Vosk
            frame_length = 128;
            recorder::FRAME_LENGTH.store(frame_length as u32, Ordering::SeqCst);
        },
        config::WakeWordEngine::Porcupine => {
            // start recording for Porcupine
            frame_length = PORCUPINE.get().unwrap().frame_length() as usize;
            recorder::FRAME_LENGTH.store(PORCUPINE.get().unwrap().frame_length(), Ordering::SeqCst);
        }
    }

    // define frame buffer
    let mut frame_buffer: Vec<i16> = vec![0; frame_length];

    // init stuff
    recorder::init(); // init
    recorder::start_recording(); // start
    LISTENING.store(true, Ordering::SeqCst);
    info!("START listening ...");

    // greet user
    events::play("run", TAURI_APP_HANDLE.get().unwrap());

    // record
    match recorder::RECORDER_TYPE.load(Ordering::SeqCst) {
        recorder::RecorderType::PvRecorder => {
            while !STOP_LISTENING.load(Ordering::SeqCst) {
                recorder::read_microphone(&mut frame_buffer);
                data_callback(&frame_buffer);
            }

            // stop
            stop_recording();

            Ok(true)
        },
        recorder::RecorderType::PortAudio => {
            while !STOP_LISTENING.load(Ordering::SeqCst) {
                recorder::read_microphone(&mut frame_buffer);
                data_callback(&frame_buffer);
            }

            // stop
            stop_recording();

            Ok(true)
        }
        recorder::RecorderType::Cpal => {
            todo!()
        }
    }
}

fn stop_recording() {
    // Stop listening
    recorder::stop_recording();

    LISTENING.store(false, Ordering::SeqCst);
    STOP_LISTENING.store(false, Ordering::SeqCst);
    info!("STOP listening ...");
}

fn rustpotter_init() -> Result<bool, String> {

    // init rustpotter
    let rustpotter_config = RustpotterConfig {
        fmt: WavFmt::default(),
        detector: DetectorConfig {
            avg_threshold: 0.,
            threshold: 0.5,
            min_scores: 15,
            score_mode: ScoreMode::Average,
            comparator_band_size: 5,
            comparator_ref: 0.22
        },
        filters: FiltersConfig {
            gain_normalizer: GainNormalizationConfig {
                enabled: true,
                gain_ref: None,
                min_gain: 0.7,
                max_gain: 1.0,
            },
            band_pass: BandPassConfig {
                enabled: true,
                low_cutoff: 80.,
                high_cutoff: 400.,
            }
        }
    };
    let mut rustpotter = Rustpotter::new(&rustpotter_config).unwrap();

    // load a wakeword
    let rustpotter_wake_word_files: [&str; 5] = [
        "rustpotter/jarvis-default.rpw",
        "rustpotter/jarvis-community-1.rpw",
        "rustpotter/jarvis-community-2.rpw",
        "rustpotter/jarvis-community-3.rpw",
        "rustpotter/jarvis-community-4.rpw",
        // "rustpotter/jarvis-community-5.rpw",
    ];

    for rpw in rustpotter_wake_word_files {
        rustpotter.add_wakeword_from_file(rpw).unwrap();
    }

    // store rustpotter
    if RUSTPOTTER.get().is_none() {
        RUSTPOTTER.set(Mutex::new(rustpotter));
    }

    // start recording
    start_recording()
}

fn vosk_init() -> Result<bool, String> {
    start_recording()
}

fn picovoice_init() -> Result<bool, String> {
    // VARS
    let porcupine: Porcupine;
    let picovoice_api_key: String;

    // Retrieve API key from DB
    if let Some(pkey) = DB.lock().unwrap().get::<String>("api_key__picovoice") {
        picovoice_api_key = pkey;
    } else {
        warn!("Picovoice API key is not set!");
        return Err("Picovoice API key is not set!".into());
    }

    // Create instance of Porcupine with the given API key
    match PorcupineBuilder::new_with_keyword_paths(picovoice_api_key, &[Path::new(config::KEYWORDS_PATH).join("jarvis_windows.ppn")])
        .sensitivities(&[1.0f32]) // max sensitivity possible
        .init() {
            Ok(pinstance) => {
                // porcupine successfully initialized with the valid API key
                info!("Porcupine successfully initialized with the valid API key ...");
                porcupine = pinstance;
            }
            Err(e) => {
                error!("Porcupine error: either API key is not valid or there is no internet connection");
                error!("Error details: {}", e);
                return Err(
                    "Porcupine error: either API key is not valid or there is no internet connection"
                        .into(),
                );
            }
    }

    // store
    if PORCUPINE.get().is_none() {
        PORCUPINE.set(porcupine);
    }

    // start recording
    start_recording()
}