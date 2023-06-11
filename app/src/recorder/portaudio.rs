/*
    Abandoned temporary.
*/

use portaudio as pa;
use pa::{DeviceIndex, Stream};
use log::{info, warn, error};

use once_cell::sync::OnceCell;
use std::sync::{Arc, Mutex};
use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
 
thread_local!(static RECORDER: OnceCell<ArcSwap<Mutex<Stream<pa::Blocking<pa::stream::Buffer>, pa::Input<i16>>>>> = OnceCell::new());
static SELECTED_MICROPHONE_IDX: AtomicI32 = AtomicI32::new(0);
static FRAME_LENGTH: AtomicU32 = AtomicU32::new(0);
static IS_RECORDING: AtomicBool = AtomicBool::new(false);

const CHANNELS: i32 = 1;
const SAMPLE_RATE: f64 = 16_000.0;

pub fn init_microphone(device_index: i32, frame_length: u32) -> bool {
    RECORDER.with(|r| {
        match r.get().is_none() {
            true => {
                match create_stream(device_index, frame_length) {
                    Ok(stream) => {
                        // store
                        r.set(ArcSwap::from_pointee(Mutex::new(stream)));
    
                        // remember current configuration
                        SELECTED_MICROPHONE_IDX.store(device_index, Ordering::SeqCst);
                        FRAME_LENGTH.store(frame_length, Ordering::SeqCst);
    
                        // success
                        true
                    },
                    Err(msg) => {
                        error!("Failed to initialize portaudio.\nError details: {:?}", msg);
    
                        // fail
                        false
                    }
                }
            },
            _ => {
                // check if re-initialization required (i.e. selecetd microphoneor frame-length was changed )
                if SELECTED_MICROPHONE_IDX.load(Ordering::SeqCst) != device_index
                   ||
                   FRAME_LENGTH.load(Ordering::SeqCst) != frame_length {
                    warn!("Selected microphone or frame length was changed, re-initializing ...");
                    // initialize again with new device index
                    if IS_RECORDING.load(Ordering::SeqCst) {
                        // RECORDER.get().unwrap().load().stop().expect("Failed to start audio recording!");
                        stop_recording();
                    }

                    // store
                    match create_stream(device_index, frame_length) {
                        Ok(stream) => {
                            // store new stream
                            r.get().unwrap().store(Arc::new(Mutex::new(stream)));
        
                            // remember new configuration
                            SELECTED_MICROPHONE_IDX.store(device_index, Ordering::SeqCst);
                            FRAME_LENGTH.store(frame_length, Ordering::SeqCst);
        
                            // success
                            return true
                        },
                        Err(msg) => {
                            error!("Failed to initialize portaudio.\nError details: {:?}", msg);
        
                            // fail
                            return false
                        }
                    }
                }
    
                // success
                true
            }
        }
    })
}

fn create_stream(device_index: i32, frame_length: u32) -> Result<Stream<pa::Blocking<pa::stream::Buffer>, pa::Input<i16>>, pa::Error> {
    let pa_recorder: Result<pa::PortAudio, pa::Error> = pa::PortAudio::new();
    
    match pa_recorder {
        Ok(pa) => {
            let input_settings = match get_input_settings(DeviceIndex(device_index as u32), &pa, SAMPLE_RATE, frame_length, CHANNELS) {
                Ok(settings) => settings,
                Err(error) => panic!("{}", String::from(error))
            };
        
            // Construct a stream with input and output sample types of i16
            match pa.open_blocking_stream(input_settings) {
                Ok(strm) => Ok(strm),
                Err(error) => panic!("{}", error.to_string()),
            }
        },
        Err(msg) => Err(msg)
    }
}

fn get_input_latency(audio_port: &pa::PortAudio, input_index: pa::DeviceIndex) -> Result<f64, String>
{
    let input_device_information = audio_port.device_info(input_index).or_else(|error| Err(String::from(format!("{}", error))));
    Ok(input_device_information.unwrap().default_low_input_latency)
}

fn get_input_stream_parameters(input_index: pa::DeviceIndex, latency: f64, channels: i32) -> Result<pa::StreamParameters<i16>, String>
{
    const INTERLEAVED: bool = true;
    Ok(pa::StreamParameters::<i16>::new(input_index, channels, INTERLEAVED, latency))
}

fn get_input_settings(input_index: pa::DeviceIndex, audio_port: &pa::PortAudio, sample_rate: f64, frames: u32, channels: i32) -> Result<pa::InputStreamSettings<i16>, String>
{
    Ok(
        pa::InputStreamSettings::new(
            (get_input_stream_parameters(
                input_index,
                (get_input_latency(
                    &audio_port,
                    input_index,
                ))?,
                channels
            ))?,
            sample_rate,
            frames,
        )
    )
}

// We'll use this function to wait for read/write availability.
fn wait_for_stream<F>(f: F, name: &str) -> u32
where
    F: Fn() -> Result<pa::StreamAvailable, pa::error::Error>,
{
    loop {
        match f() {
            Ok(available) => match available {
                pa::StreamAvailable::Frames(frames) => return frames as u32,
                pa::StreamAvailable::InputOverflowed => println!("Input stream has overflowed"),
                pa::StreamAvailable::OutputUnderflowed => {
                    println!("Output stream has underflowed")
                }
            },
            Err(err) => panic!(
                "An error occurred while waiting for the {} stream: {}",
                name, err
            ),
        }
    }
}

pub fn read_microphone(frame_buffer: &mut [i16]) {
    // ensure microphone is initialized
    RECORDER.with(|r| {
        if !r.get().is_none() {
            let cell = r.get().unwrap().load();
            let mut lock = cell.lock();
            let stream = lock.as_mut().unwrap();

            // read to frame buffer
            let in_frames = wait_for_stream(|| stream.read_available(), "Read");

            if in_frames > 0 {
                // let input_samples = stream.read(in_frames).expect("Cannot read frames ...");
                // println!("Read {:?} frames from the input stream.", in_frames);

                let input_samples = stream.read(in_frames).expect("Cannot read frames ...");
                println!("Read: {} (required {})", input_samples.len(), frame_buffer.len());
                frame_buffer.copy_from_slice(input_samples.chunks(frame_buffer.len()).last().unwrap());
            }
            // r.get().unwrap().load().read(frame_buffer).expect("Failed to read audio frame");
        }
    });
}

pub fn start_recording(device_index: i32, frame_length: u32) {
    // ensure microphone is initialized
    init_microphone(device_index, frame_length);

    // start recording
    RECORDER.with(|r| {
        r.get().unwrap().load().lock().unwrap().start().expect("Failed to start audio recording!");
        IS_RECORDING.store(true, Ordering::SeqCst);
        info!("START recording from microphone ...");
    });
}

pub fn stop_recording() {
    RECORDER.with(|r| {
        if !r.get().is_none() && IS_RECORDING.load(Ordering::SeqCst) {
            // stop recording
            let pa = r.get().unwrap().load();
            r.get().unwrap().load().lock().unwrap().stop().expect("Failed to stop audio recording!");
            IS_RECORDING.store(false, Ordering::SeqCst);
            info!("STOP recording from microphone ...");
        }
    });
}