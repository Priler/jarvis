use once_cell::sync::OnceCell;
use std::path::PathBuf;
use std::sync::Mutex;

// use kira::{
//     manager::{backend::DefaultBackend, AudioManager, AudioManagerSettings},
//     sound::static_sound::{StaticSoundData, StaticSoundSettings},
// };

use kira::{
	AudioManager, AudioManagerSettings, DefaultBackend,
	sound::static_sound::StaticSoundData,
};

thread_local!(static MANAGER: OnceCell<Mutex<AudioManager>> = OnceCell::new());

pub fn init() -> Result<(), ()> {
    MANAGER.with(|m| {
        if !m.get().is_none() {
            return Ok(());
        } // already initialized

        // Create an audio manager. This plays sounds and manages resources.
        match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
            Ok(x) => {
                // store
                m.set(Mutex::new(x));

                // success
                Ok(())
            }
            Err(msg) => {
                error!("Failed to initialize audio stream.\nError details: {}", msg);

                // failed
                Err(())
            }
        }
    })
}

// @TODO. Cache sounds in memory? With a pool of a certain size, for instance.
pub fn play_sound(filename: &PathBuf) {
    // load the file
    match StaticSoundData::from_file(filename) {
        Ok(sound_data) => {
            // sound_data.duration() can be used in order to sleep, if (for some reason) blocking behaviour is required

            // play it (non-blocking)
            MANAGER.with(|m| {
                let audio_manager = &mut m.get().unwrap().lock().unwrap();
                audio_manager.play(sound_data.clone()).unwrap();
            });
        }
        Err(err) => {
            warn!("Cannot find sound file: {} (err: {})", filename.display(), err);
        }
    }
}
