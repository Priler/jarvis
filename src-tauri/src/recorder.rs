use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use pv_recorder::{Recorder, RecorderBuilder};
use log::{info};

use crate::DB;

pub static FRAME_LENGTH: AtomicU32 = AtomicU32::new(0);
static RECORDER: OnceCell<Recorder> = OnceCell::new();
pub static IS_RECORDING: AtomicBool = AtomicBool::new(false);

fn init_microphone() {
    if RECORDER.get().is_none() {
        RECORDER.get_or_init(|| RecorderBuilder::new()
        .device_index(get_selected_microphone_index())
        .frame_length(FRAME_LENGTH.load(Ordering::SeqCst) as i32)
        .init()
        .expect("Failed to initialize pvrecorder"));

        info!("Microphone recorder initialized!")
    }
}

pub fn read_microphone(frame_buffer: &mut [i16]) {
    // ensure microphone is initialized
    init_microphone();

    // read to frame buffer
    RECORDER.get().unwrap().read(frame_buffer).expect("Failed to read audio frame");
}

pub fn start_recording() {
    // ensure microphone is initialized
    init_microphone();

    RECORDER.get().unwrap().start().expect("Failed to start audio recording!");
    IS_RECORDING.store(true, Ordering::SeqCst);
    info!("START recording from microphone ...");
}

pub fn stop_recording() {
    // ensure microphone is initialized
    init_microphone();

    RECORDER.get().unwrap().start().expect("Failed to start audio recording!");
    IS_RECORDING.store(false, Ordering::SeqCst);
    info!("STOP recording from microphone ...");
}

pub fn get_selected_microphone_index() -> i32 {
    let selected_microphone: i32;

    // Retrieve microphone index
    if let Some(smic) = DB.lock().unwrap().get::<String>("selected_microphone") {
        selected_microphone = smic.parse().unwrap_or(-1);
    } else {
        selected_microphone = -1;
    }

    // return microphone index
    info!("Selected microphone index = {selected_microphone}");
    selected_microphone
}