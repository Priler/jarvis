#[cfg(feature = "vosk")]
mod vosk;

use crate::config;
use once_cell::sync::OnceCell;

use crate::config::structs::SpeechToTextEngine;
pub use self::vosk::init_vosk;
pub use self::vosk::recognize_wake_word;
pub use self::vosk::recognize_speech;
pub use self::vosk::recognize_speech_with_partial;
pub use self::vosk::reset_speech_recognizer;
pub use self::vosk::reset_wake_recognizer;

static STT_TYPE: OnceCell<SpeechToTextEngine> = OnceCell::new();

pub fn init() -> Result<(), String> {
    if STT_TYPE.get().is_some() {
        return Ok(());
    }

    STT_TYPE.set(config::DEFAULT_SPEECH_TO_TEXT_ENGINE)
        .map_err(|_| "STT type already set".to_string())?;

    match STT_TYPE.get().unwrap() {
        SpeechToTextEngine::Vosk => {
            info!("Initializing Vosk STT backend.");
            vosk::init_vosk()?;
            info!("STT backend initialized.");
        }
    }

    Ok(())
}

/// Returns `(final_text, partial_text)` — exactly one is Some per frame.
pub fn recognize_with_partial(data: &[i16]) -> (Option<String>, Option<String>) {
    vosk::recognize_speech_with_partial(data)
}

pub fn recognize(data: &[i16], include_partial: bool) -> Option<String> {
    if include_partial {
        vosk::recognize_wake_word(data).map(|(text, _)| text)
    } else {
        vosk::recognize_speech(data)
    }
}
