use std::fs::File;
use std::io::BufReader;
use rodio::{Decoder, OutputStream, Sink};

#[tauri::command(async)]
pub fn play_sound(filename: &str, sleep: bool) {
    // Get a output stream handle to the default physical sound device
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle).unwrap();
    
    // Load a sound from a file, using a path relative to Cargo.toml
    // let filepath = format!("{PUBLIC_PATH}/sound/{filename}.wav");
    let filepath = filename;
    let file = BufReader::new(File::open(&filepath).unwrap());
    
    // Decode that sound file into a source
    let source = Decoder::new(file).unwrap();
    
    // Play the sound directly on the device
    println!("Playing {} ...", filepath);
    // stream_handle.play_raw(source.convert_samples());
    sink.append(source);

    if sleep {
        // The sound plays in a separate thread. This call will block the current thread until the sink
        // has finished playing all its queued sounds.
        sink.sleep_until_end();
    }
}