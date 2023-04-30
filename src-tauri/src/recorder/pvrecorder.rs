use pv_recorder::{Recorder, RecorderBuilder};
use log::{info, warn, error};

use once_cell::sync::OnceCell;
use std::sync::Arc;
use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
 
static RECORDER: OnceCell<ArcSwap<Recorder>> = OnceCell::new();
static SELECTED_MICROPHONE_IDX: AtomicI32 = AtomicI32::new(0);
static FRAME_LENGTH: AtomicU32 = AtomicU32::new(0);
static IS_RECORDING: AtomicBool = AtomicBool::new(false);

pub fn init_microphone(device_index: i32, frame_length: u32) -> bool {
    match RECORDER.get().is_none() {
        true => {
            let pv_recorder = RecorderBuilder::new()
                .device_index(device_index)
                .frame_length(frame_length as i32)
                .init();

            match pv_recorder {
                Ok(pv) => {
                    // store
                    RECORDER.set(ArcSwap::from_pointee(pv));

                    // remember current configuration
                    SELECTED_MICROPHONE_IDX.store(device_index, Ordering::SeqCst);
                    FRAME_LENGTH.store(frame_length, Ordering::SeqCst);

                    // success
                    true
                },
                Err(msg) => {
                    error!("Failed to initialize pvrecorder.\nError details: {:?}", msg);

                    // fail
                    false
                }
            }
        },
        _ => {
            // check if re-initialization required (i.e. selecetd microphoneor frame-length was changed )
            if SELECTED_MICROPHONE_IDX.load(Ordering::SeqCst) != device_index
               ||
               RECORDER.get().unwrap().load().frame_length() != frame_length as usize {
                warn!("Selected microphone or frame length was changed, re-initializing ...");
                // initialize again with new device index
                if IS_RECORDING.load(Ordering::SeqCst) {
                    // RECORDER.get().unwrap().load().stop().expect("Failed to start audio recording!");
                    stop_recording();
                }

                // remember new configuration
                SELECTED_MICROPHONE_IDX.store(device_index, Ordering::SeqCst);
                FRAME_LENGTH.store(frame_length, Ordering::SeqCst);

                // store
                RECORDER.get().unwrap().store(Arc::new(RecorderBuilder::new()
                    .device_index(device_index)
                    .frame_length(frame_length as i32)
                    .init()
                    .expect("Failed to initialize pvrecorder")));
            }

            // success
            true
        }
    }
}

pub fn read_microphone(frame_buffer: &mut [i16]) {
    // ensure microphone is initialized
    if !RECORDER.get().is_none() {
        // read to frame buffer
        match RECORDER.get().unwrap().load().read(frame_buffer) {
            Err(msg) => {
                // @TODO: Fix somehow. PvRecorder always wait for PCM buffer size of 512.
                // error!("Failed to read audio frame. {:?}", msg);
                // eprintln!("Failed to read audio frame. {:?}", msg);
            },
            _ => ()
        }
    }
}

pub fn start_recording(device_index: i32, frame_length: u32) {
    // ensure microphone is initialized
    init_microphone(device_index, frame_length);

    // start recording
    RECORDER.get().unwrap().load().start().expect("Failed to start audio recording!");
    IS_RECORDING.store(true, Ordering::SeqCst);
    info!("START recording from microphone ...");
}

pub fn stop_recording() {
    // ensure microphone is initialized
    if !RECORDER.get().is_none() && IS_RECORDING.load(Ordering::SeqCst) {
        // stop recording
        RECORDER.get().unwrap().load().stop().expect("Failed to stop audio recording!");
        IS_RECORDING.store(false, Ordering::SeqCst);
        info!("STOP recording from microphone ...");
    }
}