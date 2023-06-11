use std::time::SystemTime;

use crate::{config, audio, recorder, listener, stt, commands, COMMANDS_LIST};
use rand::seq::SliceRandom;

pub fn start() -> Result<(), ()> {
    // start the loop
    main_loop()
}

fn main_loop() -> Result<(), ()> {
    let mut start: SystemTime;
    let sounds_directory = audio::get_sound_directory().unwrap();
    let frame_length: usize = 512; // default for every wake-word engine
    let mut frame_buffer: Vec<i16> = vec![0; frame_length];

    // play some run phrase
    // @TODO. Different sounds? Or better make it via commands or upcoming events system.
    audio::play_sound(&sounds_directory.join("run.wav"));

    // start recording
    match recorder::start_recording() {
        Ok(_) => info!("Recording started."),
        Err(_) => {
            error!("Cannot start recording.");
            return Err(()); // quit
        }
    }

    // the loop
    'wake_word: loop {
        // read from microphone
        recorder::read_microphone(&mut frame_buffer);

        // recognize wake-word
        match listener::data_callback(&frame_buffer) {
            Some(keyword_index) => {
                // wake-word activated, process further commands
                // capture current time
                start = SystemTime::now();

                // play some greet phrase
                // @TODO. Make it via commands or upcoming events system.
                audio::play_sound(&sounds_directory.join(format!("{}.wav", config::ASSISTANT_GREET_PHRASES.choose(&mut rand::thread_rng()).unwrap())));

                // wait for voice commands
                'voice_recognition: loop {
                    // read from microphone
                    recorder::read_microphone(&mut frame_buffer);

                    // stt part (without partials)
                    if let Some(mut recognized_voice) = stt::recognize(&frame_buffer, false) {
                        // something was recognized
                        info!("Recognized voice: {}", recognized_voice);

                        // filter recognized voice
                        // @TODO. Better recognized voice filtration.
                        recognized_voice = recognized_voice.to_lowercase();
                        for tbr in config::ASSISTANT_PHRASES_TBR {
                            recognized_voice = recognized_voice.replace(tbr, "");
                        }
                        recognized_voice = recognized_voice.trim().into();

                        // infer command
                        if let Some((cmd_path, cmd_config)) = commands::fetch_command(&recognized_voice, &COMMANDS_LIST.get().unwrap()) {
                            // some debug info
                            info!("Recognized voice (filtered): {}", recognized_voice);
                            info!("Command found: {:?}", cmd_path);
                            info!("Executing!");

                            // execute the command
                            match commands::execute_command(&cmd_path, &cmd_config) {
                                Ok(chain) => {
                                    // success
                                    info!("Command executed successfully.");

                                    if chain {
                                        // chain commands
                                        start = SystemTime::now();
                                    } else {
                                        // skip, if chaining is not required
                                        start = start.checked_sub(core::time::Duration::from_secs(1000)).unwrap();
                                    }

                                    continue 'voice_recognition; // continue voice recognition
                                },
                                Err(msg) => {
                                    // fail
                                    error!("Error executing command: {}", msg);
                                }
                            }
                        }

                        // return to wake-word listening after command execution (no matter successful or not)
                        break 'voice_recognition;
                    }

                    // only recognize voice for a certain period of time
                    match start.elapsed() {
                        Ok(elapsed) if elapsed > config::CMS_WAIT_DELAY => {
                            // return to wake-word listening after N seconds
                            break 'voice_recognition;
                        },
                        _ => ()
                    }
                }
            },
            None => ()
        }
    }

    Ok(())
}

fn keyword_callback(keyword_index: i32) {

}

pub fn close(code: i32) {
    info!("Closing application.");
    std::process::exit(code);
}