mod pvrecorder;
mod wav_source;

// mod cpal;
// mod portaudio;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use once_cell::sync::OnceCell;

use crate::{config, config::structs::RecorderType, DB};

static RECORDER_TYPE: OnceCell<RecorderType> = OnceCell::new();
static FRAME_LENGTH: OnceCell<u32> = OnceCell::new();
static WAV_SOURCE: OnceCell<Mutex<wav_source::WavSource>> = OnceCell::new();
static WAV_DONE: AtomicBool = AtomicBool::new(false);
static WAV_PATH: OnceCell<String> = OnceCell::new();

pub fn init() -> Result<(), ()> {
    RECORDER_TYPE.set(config::DEFAULT_RECORDER_TYPE).unwrap();

    info!("Loading recorder ...");
    info!("Available audio_devices are:\n{:?}", get_audio_devices());

    match RECORDER_TYPE.get().unwrap() {
        RecorderType::PvRecorder => {
            info!("Initializing PvRecorder recording backend.");
            FRAME_LENGTH.set(512u32).unwrap();
            let selected_microphone = get_selected_microphone_index();
            match pvrecorder::init_microphone(
                selected_microphone,
                FRAME_LENGTH.get().unwrap().to_owned(),
            ) {
                false => {
                    error!("Recorder initialization failed.");
                    return Err(());
                }
                _ => {
                    info!(
                        "Recorder initialization success. Listening to microphone ({}): {}",
                        selected_microphone,
                        get_audio_device_name(selected_microphone)
                    );
                }
            }
        }
        RecorderType::PortAudio => {
            error!("PortAudio recorder backend is not implemented");
            return Err(());
        }
        RecorderType::Cpal => {
            error!("CPAL recorder backend is not implemented");
            return Err(());
        }
    }

    Ok(())
}

/// Initialize WAV replay mode instead of live microphone.
/// Loads the WAV file, resamples to 16 kHz mono i16 if needed,
/// and splits into 512-sample frames for pipeline injection.
pub fn init_wav(path: &str) -> Result<(), String> {
    WAV_PATH.set(path.to_string()).map_err(|_| "WAV path already set".to_string())?;
    FRAME_LENGTH
        .set(512u32)
        .map_err(|_| "Frame length already set".to_string())?;
    let source = wav_source::WavSource::load(path, 512, 16000)?;
    WAV_SOURCE
        .set(Mutex::new(source))
        .map_err(|_| "WAV source already set".to_string())?;
    Ok(())
}

pub fn get_wav_path() -> Option<&'static str> {
    WAV_PATH.get().map(|s| s.as_str())
}

pub fn is_wav_mode() -> bool {
    WAV_SOURCE.get().is_some()
}

pub fn is_wav_done() -> bool {
    WAV_DONE.load(Ordering::SeqCst)
}

pub fn read_microphone(frame_buffer: &mut [i16]) {
    if let Some(source) = WAV_SOURCE.get() {
        let (frame_opt, frame_dur) = {
            let mut src = source.lock().unwrap();
            (src.next_frame(), src.frame_duration())
        };
        match frame_opt {
            Some(f) => {
                frame_buffer.copy_from_slice(&f);
            }
            None => {
                // WAV exhausted — fill with silence and mark done
                if !WAV_DONE.load(Ordering::SeqCst) {
                    WAV_DONE.store(true, Ordering::SeqCst);
                    info!("[AUDIO_TEST] All WAV frames delivered — filling with silence");
                }
                frame_buffer.fill(0);
            }
        }
        std::thread::sleep(frame_dur);
        return;
    }
    match RECORDER_TYPE.get().unwrap() {
        RecorderType::PvRecorder => {
            pvrecorder::read_microphone(frame_buffer);
        }
        RecorderType::PortAudio | RecorderType::Cpal => {
            error!("Recorder backend {:?} not implemented — filling with silence", RECORDER_TYPE.get());
            frame_buffer.fill(0);
        }
    }
}

pub fn start_recording() -> Result<(), ()> {
    if is_wav_mode() {
        info!("[AUDIO_TEST] WAV mode — skipping PvRecorder start");
        return Ok(());
    }
    match RECORDER_TYPE.get().unwrap() {
        RecorderType::PvRecorder => {
            return pvrecorder::start_recording(
                get_selected_microphone_index(),
                FRAME_LENGTH.get().unwrap().to_owned(),
            );
        }
        RecorderType::PortAudio | RecorderType::Cpal => {
            error!("Recorder backend {:?} not implemented", RECORDER_TYPE.get());
            return Err(());
        }
    }
}

pub fn stop_recording() -> Result<(), ()> {
    if is_wav_mode() {
        return Ok(());
    }
    match RECORDER_TYPE.get().unwrap() {
        RecorderType::PvRecorder => pvrecorder::stop_recording(),
        RecorderType::PortAudio | RecorderType::Cpal => {
            error!("Recorder backend {:?} not implemented", RECORDER_TYPE.get());
            Err(())
        }
    }
}

/// Stop the recorder and immediately restart it on the same device.
///
/// Used by the watchdog L2 recovery path when the recorder appears frozen.
/// A 200 ms gap between stop and start gives the audio driver time to release
/// the device before re-acquiring it.
pub fn restart_recording() -> Result<(), ()> {
    if is_wav_mode() {
        return Ok(());
    }
    stop_recording().ok();
    std::thread::sleep(std::time::Duration::from_millis(200));
    start_recording()
}

pub fn get_selected_microphone_index() -> i32 {
    let idx = DB.get().unwrap().read().microphone;

    if idx > 0 {
        let devices = get_audio_devices();
        if (idx as usize) >= devices.len() {
            warn!(
                "Microphone index {} not found ({} available), falling back to default",
                idx,
                devices.len()
            );
            return -1;
        }
    }

    idx
}

pub fn get_audio_devices() -> Vec<String> {
    match RECORDER_TYPE.get() {
        Some(RecorderType::PvRecorder) => pvrecorder::list_audio_devices(),
        Some(RecorderType::PortAudio) | Some(RecorderType::Cpal) => vec![],
        None => pvrecorder::list_audio_devices(),
    }
}

pub fn get_audio_device_name(idx: i32) -> String {
    if is_wav_mode() {
        return String::from("[WAV test mode]");
    }
    match RECORDER_TYPE.get() {
        Some(RecorderType::PvRecorder) => pvrecorder::get_audio_device_name(idx),
        Some(RecorderType::PortAudio) | Some(RecorderType::Cpal) => String::new(),
        None => pvrecorder::get_audio_device_name(idx),
    }
}
