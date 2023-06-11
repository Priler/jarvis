/*
    Abandoned temporary.
    Problems with frame size.
*/

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, StreamConfig, SampleRate, Host, Device, Stream, SampleFormat};
use log::{info, warn, error};

use once_cell::sync::OnceCell;
use std::sync::Arc;
use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

use crate::tauri_commands::cpal_data_callback;

static HOST: OnceCell<Host> = OnceCell::new();
thread_local!(static RECORDER: OnceCell<ArcSwap<Stream>> = OnceCell::new());
static SELECTED_MICROPHONE_IDX: AtomicI32 = AtomicI32::new(0);
static FRAME_LENGTH: AtomicU32 = AtomicU32::new(0);
static IS_RECORDING: AtomicBool = AtomicBool::new(false);

pub fn init_microphone(device_index: i32, frame_length: u32) -> bool {
    // init host & frame buffer for the callback
    if HOST.get().is_none() {
        HOST.set(cpal::default_host());

        // FRAME_BUFFER.set(Mutex::new(vec![0; FRAME_LENGTH.load(Ordering::SeqCst) as usize]));
    }

    // init microphone
    RECORDER.with(|recorder| {
        match recorder.get().is_none() {
            true => {
                if let Some(device) = get_device(device_index as usize) {
                    // store
                    recorder.set(ArcSwap::from_pointee(create_stream(device, frame_length)));
    
                    // remember current configuration
                    SELECTED_MICROPHONE_IDX.store(device_index, Ordering::SeqCst);
                    FRAME_LENGTH.store(frame_length, Ordering::SeqCst);
    
                    // success
                    true
                } else {
                    false
                }
            },
            false => {
                // check if re-initialization required (i.e. selecetd microphoneor frame-length was changed )
                if SELECTED_MICROPHONE_IDX.load(Ordering::SeqCst) != device_index
                   ||
                   FRAME_LENGTH.load(Ordering::SeqCst) != frame_length {
                    warn!("Selected microphone or frame length was changed, re-initializing ...");
                    // initialize again with new device index
                    if IS_RECORDING.load(Ordering::SeqCst) {
                        stop_recording();
                    }
    
                    // remember new configuration
                    SELECTED_MICROPHONE_IDX.store(device_index, Ordering::SeqCst);
                    FRAME_LENGTH.store(frame_length, Ordering::SeqCst);
    
                    if let Some(device) = get_device(device_index as usize) {
                        // store
                        recorder.get().unwrap().store(Arc::new(create_stream(device, frame_length)));
    
                        // success
                        return true
                    } else {
                        return false
                    }
                }
    
                // success
                true
            }
        }
    })
}

fn create_stream(device: Device, frame_length: u32) -> Stream {
    // get default input stream config
    // let default_config = device.default_input_config().unwrap();

    // create config for the stream
    // let config: StreamConfig = StreamConfig {
    //     channels: default_config.channels(),
    //     sample_rate: SampleRate(16000),
    //     buffer_size: BufferSize::Fixed(frame_length)
    // };

    let config = device
        .default_input_config()
        .expect("Failed to load default input config");

    let channels = config.channels();

    let err_fn = move |err| {
        eprintln!("an error occurred on stream: {}", err);
    };

    match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], info| {
                cpal_data_callback(data, channels);
            },
            err_fn,
            None
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], info| {
                cpal_data_callback(data, channels);
            },
            err_fn,
            None
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], info| {
                cpal_data_callback(data, channels);
            },
            err_fn,
            None
        ),
        _ => todo!()
    }.unwrap()
}

pub fn stereo_to_mono(input_data: &[i16]) -> Vec<i16> {
    let mut result = Vec::with_capacity(input_data.len() / 2);
    result.extend(
        input_data
            .chunks_exact(2)
            .map(|chunk| chunk[0] / 2 + chunk[1] / 2),
    );

    result
}

fn get_device(device_index: usize) -> Option<Device> {
    if let Some(device) = HOST.get().unwrap().input_devices().expect("Get devices error ...").nth(device_index) {
        Some(device)
    } else {
        if let Some(default) = HOST.get().unwrap().default_input_device() {
            Some(default)
        } else {
            error!("No default input device ...");

            None
        }
    }
}

pub fn start_recording(device_index: i32, frame_length: u32) {
    // ensure microphone is initialized
    init_microphone(device_index, frame_length);

    // start recording
    RECORDER.with(|recorder| {
        match recorder.get().unwrap().load().play() {
            Err(msg) => {
                error!("[CPAL] Audio stream PLAY error ... {:?}", msg);
            },
            _ => ()
        };

        IS_RECORDING.store(true, Ordering::SeqCst);
        info!("START recording from microphone ...");
    });
}

pub fn stop_recording() {
    // ensure microphone is initialized
    RECORDER.with(|recorder| {
        if !recorder.get().is_none() && IS_RECORDING.load(Ordering::SeqCst) {
            // pause instead of stop
            match recorder.get().unwrap().load().pause() {
                Err(msg) => {
                    error!("[CPAL] Audio stream PAUSE error ... {:?}", msg);
                },
                _ => ()
            };

            IS_RECORDING.store(false, Ordering::SeqCst);
            info!("STOP recording from microphone ...");
        }
    });
}