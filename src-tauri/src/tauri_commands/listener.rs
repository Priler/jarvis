use porcupine::{Porcupine, PorcupineBuilder};
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::Path;
use log::{info, warn, error};

// use crate::events::Payload;
use tauri::Manager;

use rand::seq::SliceRandom;
use std::time::SystemTime;

use crate::assistant_commands;
use crate::events;

use crate::config;
use crate::vosk;
use crate::recorder;

use crate::COMMANDS;
use crate::DB;

// track listening state
static LISTENING: AtomicBool = AtomicBool::new(false);

// stop listening with Atomic flag (to make it work between different threads)
static STOP_LISTENING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn is_listening() -> bool {
    LISTENING.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn stop_listening() {
    if is_listening() {
        STOP_LISTENING.store(true, Ordering::SeqCst);
    }

    // wait until listening stops
    while is_listening() {}
}

#[tauri::command(async)]
pub fn start_listening(app_handle: tauri::AppHandle) -> Result<bool, String> {
    // only one listener thread is allowed
    if is_listening() {
        return Err("Already listening.".into());
    }

    // Retrieve selected wake-word engine from DB
    let selected_wake_word_engine;
    if let Some(wwengine) = DB.lock().unwrap().get::<String>("selected_wake_word_engine") {
        // from db
        selected_wake_word_engine = wwengine;
    } else {
        // default
        selected_wake_word_engine = config::WAKE_WORD_ENGINES.first().expect("No wake-word engines found ...").to_string(); // set default wake_word engine
    }

    // call selected wake-word engine listener command
    match selected_wake_word_engine.as_str() {
        "rustpotter" => {
            info!("Starting rustpotter wake-word engine ...");
            return picovoice_listen(&app_handle, |_app| {
                // Greet user
                events::play("run", &app_handle);
            }, |app, kidx| keyword_callback(app, kidx));
        },
        "picovoice" => {
            info!("Starting picovoice wake-word engine ...");
            return picovoice_listen(&app_handle, |_app| {
                // Greet user
                events::play("run", &app_handle);
            }, |app, kidx| keyword_callback(app, kidx));
        },
        _ => Err("No wake-word engine selected ...".into())
    }
}

pub fn keyword_callback(app_handle: &tauri::AppHandle, _keyword_index: i32) {
    // vars
    let mut start: SystemTime = SystemTime::now();
    let mut frame_buffer = vec![0; recorder::FRAME_LENGTH.load(Ordering::SeqCst) as usize];

    // play greet phrase
    events::play(
        config::ASSISTANT_GREET_PHRASES
            .choose(&mut rand::thread_rng())
            .unwrap(),
        &app_handle,
    );

    // emit assistant greet event
    app_handle
        .emit_all(events::EventTypes::AssistantGreet.get(), ())
        .unwrap();

    // the loop
    while !STOP_LISTENING.load(Ordering::SeqCst) {
        recorder::read_microphone(&mut frame_buffer);

        // vosk part (partials included)
        if let Some(mut test) = vosk::recognize(&frame_buffer) {
            if !test.is_empty() {
                println!("Recognized: {}", test);

                // some filtration
                test = test.to_lowercase();
                for tbr in config::ASSISTANT_PHRASES_TBR {
                    test = test.replace(tbr, "");
                }

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
                        &app_handle,
                    );

                    match cmd_result {
                        Ok(_) => {
                            println!("Command executed successfully!");
                            start = SystemTime::now(); // listen for more commands
                            continue;
                        }
                        Err(error_message) => {
                            println!("Error executing command: {}", error_message);
                        }
                    }

                    app_handle
                        .emit_all(events::EventTypes::AssistantWaiting.get(), ())
                        .unwrap();
                    break; // return to picovoice after command execution (no matter successfull or not)
                }
            }
        }

        match start.elapsed() {
            Ok(elapsed) if elapsed > config::CMS_WAIT_DELAY => {
                // return to picovoice after N seconds
                app_handle
                    .emit_all(events::EventTypes::AssistantWaiting.get(), ())
                    .unwrap();
                break;
            }
            _ => (),
        }
    }
}

pub fn picovoice_listen<'s, S, K>(app_handle: &tauri::AppHandle, start_callback: S, mut keyword_callback: K) -> Result<bool, String>
    where S: Fn(&tauri::AppHandle),
          K: FnMut(&tauri::AppHandle, i32) {

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

    // Start recording
    let mut frame_buffer = vec![0; porcupine.frame_length() as usize];
    recorder::FRAME_LENGTH.store(porcupine.frame_length(), Ordering::SeqCst);
    recorder::start_recording();
    LISTENING.store(true, Ordering::SeqCst);

    // run start callback
    start_callback(app_handle);

    // Listen until stop flag will be true
    while !STOP_LISTENING.load(Ordering::SeqCst) {
        recorder::read_microphone(&mut frame_buffer);

        if let Ok(keyword_index) = porcupine.process(&frame_buffer) {
            if keyword_index >= 0 {
                // println!("Yes, sir! {}", keyword_index);
                keyword_callback(&app_handle, keyword_index);
            }
        }
    }

    // Stop listening
    recorder::stop_recording();
    LISTENING.store(false, Ordering::SeqCst);
    STOP_LISTENING.store(false, Ordering::SeqCst);

    Ok(true)
}
