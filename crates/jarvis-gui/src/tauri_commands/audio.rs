use jarvis_core::recorder;
// use rodio::{Decoder, OutputStream, Sink};
use jarvis_core::audio;
use std::path::PathBuf;

#[tauri::command]
pub fn pv_get_audio_devices() -> Vec<String> {
    recorder::get_audio_devices()
}

#[tauri::command]
pub fn pv_get_audio_device_name(idx: i32) -> String {
    recorder::get_audio_device_name(idx)
}

#[tauri::command(async)]
pub fn play_sound(filename: &str) {
    let path = PathBuf::from(filename);
    audio::play_sound(&path);
}
