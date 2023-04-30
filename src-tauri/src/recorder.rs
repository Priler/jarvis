// use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicU32, Ordering};
use log::{info, warn, error};
use atomic_enum::atomic_enum;

mod pvrecorder;
// mod cpal;
// mod portaudio;

use crate::DB;

#[atomic_enum]
#[derive(PartialEq)]
pub enum RecorderType {
    Cpal,
    PvRecorder,
    PortAudio
}

pub static RECORDER_TYPE: AtomicRecorderType = AtomicRecorderType::new(RecorderType::PvRecorder); // use pvrecorder as default
pub static FRAME_LENGTH: AtomicU32 = AtomicU32::new(0);


pub fn init() {
    match RECORDER_TYPE.load(Ordering::SeqCst) {
        RecorderType::PvRecorder => {
            // Init Pv Recorder
            info!("Initializing Pv Recorder audio backend.");
            match pvrecorder::init_microphone(get_selected_microphone_index(), FRAME_LENGTH.load(Ordering::SeqCst)) {
                false => {
                    // Switch to CPAL recorder
                    warn!("Pv Recorder audio backend failed.");
                    // RECORDER_TYPE.store(RecorderType::PortAudio, Ordering::SeqCst);

                    // init again
                    init();
                },
                _ => ()
            }
        },
        RecorderType::PortAudio => {
            // Init PortAudio
            info!("Initializing PortAudio audio backend");
            todo!();
            // match portaudio::init_microphone(get_selected_microphone_index(), FRAME_LENGTH.load(Ordering::SeqCst)) {
            //     false => {
            //         // Switch to PortAudio recorder
            //         error!("PortAudio audio backend failed.");
            //     },
            //     _ => ()
            // }
        },
        RecorderType::Cpal => {
            // Init CPAL
            info!("Initializing CPAL audio backend");
            todo!();
            // match cpal::init_microphone(get_selected_microphone_index(), FRAME_LENGTH.load(Ordering::SeqCst)) {
            //     false => {
            //         // Switch to CPAL recorder
            //         error!("CPAL audio backend failed.");
            //     },
            //     _ => ()
            // }
        }
    }
}

pub fn read_microphone(frame_buffer: &mut [i16]) {
    match RECORDER_TYPE.load(Ordering::SeqCst) {
        RecorderType::PvRecorder => {
            pvrecorder::read_microphone(frame_buffer);
        },
        RecorderType::PortAudio => {
            todo!();
            // portaudio::read_microphone(frame_buffer);
        },
        RecorderType::Cpal => {
            // cpal::read_microphone(frame_buffer);
            panic!("Cpal should be used via callback assignment");
        }
    }
}

pub fn start_recording() {
    match RECORDER_TYPE.load(Ordering::SeqCst) {
        RecorderType::PvRecorder => {
            pvrecorder::start_recording(get_selected_microphone_index(), FRAME_LENGTH.load(Ordering::SeqCst));
        },
        RecorderType::PortAudio => {
            todo!();
            // portaudio::start_recording(get_selected_microphone_index(), FRAME_LENGTH.load(Ordering::SeqCst));
        },
        RecorderType::Cpal => {
            // cpal::start_recording(get_selected_microphone_index(), FRAME_LENGTH.load(Ordering::SeqCst));
        }
    }
}

pub fn stop_recording() {
    match RECORDER_TYPE.load(Ordering::SeqCst) {
        RecorderType::PvRecorder => {
            pvrecorder::stop_recording();
        },
        RecorderType::PortAudio => {
            todo!();
            // portaudio::stop_recording();
        },
        RecorderType::Cpal => {
            // cpal::stop_recording();
        }
    }
}

pub fn get_selected_microphone_index() -> i32 {
    let selected_microphone: i32;

    // Retrieve microphone index
    if let Some(smic) = DB.lock().unwrap().get::<String>("selected_microphone") {
        selected_microphone = smic.parse().unwrap_or(-1);
    } else {
        selected_microphone = -1;
    }

    selected_microphone
}