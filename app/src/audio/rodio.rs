/*
    Abandoned temporary.
    Problems with blocking behaviour.
    Possible fixes are running rodio in a separate thread or smthng.
*/

use std::fs::File;
use std::path::PathBuf;
use std::io::BufReader;
use once_cell::sync::OnceCell;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

// static STREAM: OnceCell<OutputStream> = OnceCell::new();
static STREAM_HANDLE: OnceCell<OutputStreamHandle> = OnceCell::new();
static SINK: OnceCell<Sink> = OnceCell::new();

pub fn init() -> Result<(), ()> {
    if !STREAM_HANDLE.get().is_none() {return Ok(());} // already initialized

    // get output stream handle to the default physical sound device
    match OutputStream::try_default() {
        Ok(out) => {
            // divide
            let (_stream, stream_handle) = out;

            // create sink
            let sink;
            match Sink::try_new(&stream_handle) {
                Ok(s) => {
                    info!("Sink initialized.");
                    sink = s;
                },
                Err(msg) => {
                    error!("Cannot create sink.\nError details: {}", msg);

                    // failed
                    return Err(())
                }
            }

            // store
            // STREAM.set(_stream).unwrap();
            STREAM_HANDLE.set(stream_handle);
            SINK.set(sink);

            // success
            Ok(())
        },
        Err(msg) => {
            error!("Failed to initialize audio stream.\nError details: {}", msg);

            // failed
            Err(())
        }
    }
}

pub fn play_sound(filename: &PathBuf, sleep: bool) {
    // Load a sound from a file, using a path relative to Cargo.toml
    // let filepath = format!("{PUBLIC_PATH}/sound/{filename}.wav");
    let file = BufReader::new(File::open(&filename).unwrap());

    // Decode that sound file into a source
    let source = Decoder::new(file).unwrap();

    // Play the sound directly on the device
    // STREAM_HANDLE.get().unwrap().play_raw(source.convert_samples());
    SINK.get().unwrap().append(source);

    if sleep {
        // The sound plays in a separate thread. This call will block the current thread until the sink
        // has finished playing all its queued sounds.
        SINK.get().unwrap().sleep_until_end();
    }
}