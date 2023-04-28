use porcupine::{BuiltinKeywords, Porcupine, PorcupineBuilder};
use pv_recorder::RecorderBuilder;
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::Path;

use crate::events::Payload;
use tauri::Manager;

use rand::seq::SliceRandom;
use std::time::SystemTime;

use crate::assistant_commands;
use crate::events;

use crate::config;
use crate::vosk;

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

    // vars
    let porcupine: Porcupine;
    let mut picovoice_api_key: String = String::from("");
    let selected_microphone: i32;

    let mut start = SystemTime::now();

    // Retrieve API key from DB
    if let Some(pkey) = DB.lock().unwrap().get::<String>("api_key__picovoice") {
        if !pkey.is_empty() {
            picovoice_api_key = pkey;
        }
    }

    if picovoice_api_key.is_empty() {
        return Err("Picovoice API key is not set!".into());
    }

    // Create instance of Porcupine with the given API key
    match PorcupineBuilder::new_with_keyword_paths(picovoice_api_key, &[Path::new(config::KEYWORDS_PATH).join("jarvis_windows.ppn")])
    .sensitivities(&[1.0f32]) // max sensitivity possible
    .init() {
        Ok(pinstance) => {
            // porcupine successfully initialized with the valid API key
            println!("Porcupine successfully initialized with the valid API key ...");
            porcupine = pinstance;
        }
        Err(e) => {
            println!("Porcupine error: either API key is not valid or there is no internet connection");
            println!("Error details: {}", e);
            return Err(
                "Porcupine error: either API key is not valid or there is no internet connection"
                    .into(),
            );
        }
    }

    // Retrieve microphone index
    if let Some(smic) = DB.lock().unwrap().get::<String>("selected_microphone") {
        selected_microphone = smic.parse().unwrap_or(-1);
    } else {
        selected_microphone = -1; // use default, if not selected
    }

    // Create recorder instance
    let recorder = RecorderBuilder::new()
        .device_index(selected_microphone)
        .frame_length(porcupine.frame_length() as i32)
        .init()
        .expect("Failed to initialize pvrecorder");

    // Start recording
    println!("Listening (microphone idx = {selected_microphone}) ...");
    recorder.start().expect("Failed to start audio recording");
    LISTENING.store(true, Ordering::SeqCst);

    // Greet user
    events::play("run", &app_handle);

    // Listen until stop flag will be true
    let mut frame_buffer = vec![0; porcupine.frame_length() as usize];
    while !STOP_LISTENING.load(Ordering::SeqCst) {
        recorder
            .read(&mut frame_buffer)
            .expect("Failed to read audio frame");

        if let Ok(keyword_index) = porcupine.process(&frame_buffer) {
            if keyword_index >= 0 {
                println!("Yes, sir! {}", keyword_index);
                events::play(
                    config::ASSISTANT_GREET_PHRASES
                        .choose(&mut rand::thread_rng())
                        .unwrap(),
                    &app_handle,
                );
                start = SystemTime::now();

                app_handle
                    .emit_all(events::EventTypes::AssistantGreet.get(), ())
                    .unwrap();

                loop {
                    recorder
                        .read(&mut frame_buffer)
                        .expect("Failed to read audio frame");

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
        }
    }

    // Stop listening
    println!("Stop listening ...");
    recorder.stop().expect("Failed to stop audio recording");
    LISTENING.store(false, Ordering::SeqCst);
    STOP_LISTENING.store(false, Ordering::SeqCst);

    Ok(true)
}
