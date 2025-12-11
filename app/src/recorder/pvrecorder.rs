use once_cell::sync::OnceCell;
use pv_recorder::{PvRecorder, PvRecorderBuilder};
use std::sync::atomic::{AtomicBool, Ordering};

static RECORDER: OnceCell<PvRecorder> = OnceCell::new();
static IS_RECORDING: AtomicBool = AtomicBool::new(false);

pub fn init_microphone(device_index: i32, frame_length: u32) -> bool {
    match RECORDER.get().is_none() {
        true => {
            let pv_recorder = PvRecorderBuilder::new(frame_length as i32)
                .device_index(device_index)
                // .frame_length(frame_length as i32)
                .init();

            match pv_recorder {
                Ok(pv) => {
                    // store
                    RECORDER.set(pv);

                    // success
                    true
                }
                Err(msg) => {
                    error!("Failed to initialize pvrecorder.\nError details: {:?}", msg);

                    // fail
                    false
                }
            }
        }
        _ => true, // already initialized
    }
}

pub fn read_microphone(frame_buffer: &mut [i16]) {
    // ensure microphone is initialized
    if !RECORDER.get().is_none() {
        // read to frame buffer

        let frame = RECORDER.get().unwrap().read();

        match frame {
            Ok(f) => {
                frame_buffer.copy_from_slice(f.as_slice());
            }
            Err(msg) => {
                // @TODO: Fix? PvRecorder always wait for PCM buffer size of 512.
                error!("Failed to read audio frame. {:?}", msg);
            }
        }
    }
}

pub fn start_recording(device_index: i32, frame_length: u32) -> Result<(), ()> {
    // ensure microphone is initialized
    init_microphone(device_index, frame_length);

    // start recording
    match RECORDER.get().unwrap().start() {
        Ok(_) => {
            info!("START recording from microphone ...");

            // change recording state
            IS_RECORDING.store(true, Ordering::SeqCst);

            // success
            Ok(())
        }
        Err(msg) => {
            error!("Failed to start audio recording!");

            // fail
            Err(())
        }
    }
}

pub fn stop_recording() -> Result<(), ()> {
    // ensure microphone is initialized & recording is in process
    if !RECORDER.get().is_none() && IS_RECORDING.load(Ordering::SeqCst) {
        // stop recording
        match RECORDER.get().unwrap().stop() {
            Ok(_) => {
                info!("STOP recording from microphone ...");

                // change recording state
                IS_RECORDING.store(false, Ordering::SeqCst);

                // success
                return Ok(());
            }
            Err(msg) => {
                error!("Failed to stop audio recording!");

                // fail
                return Err(());
            }
        }
    }

    Ok(()) // if already stopped or not yet initialized
}

pub fn list_audio_devices() -> Vec<String> {
    let audio_devices = PvRecorderBuilder::default().get_available_devices();
    match audio_devices {
        Ok(audio_devices) => audio_devices,
        Err(err) => panic!("Failed to get audio devices: {}", err),
    }
}

pub fn get_audio_device_name(idx: i32) -> String {
    let audio_devices = list_audio_devices();
    let mut first_device: String = String::new();

    for (_idx, device) in audio_devices.iter().enumerate() {
        if idx as usize == _idx {
            return device.to_string();
        }

        if _idx == 0 {
            first_device = device.to_string()
        }
    }

    // return first device as default, if none were matched
    first_device
}
