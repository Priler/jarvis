use serde::{Deserialize, Serialize};
use crate::config;

use crate::config::structs::WakeWordEngine;
use crate::config::structs::SpeechToTextEngine;

#[derive(Serialize, Deserialize, Debug)]
pub struct Settings {
    pub microphone: i32,
    pub voice: String,

    pub wake_word_engine: WakeWordEngine,
    pub speech_to_text_engine: SpeechToTextEngine,

    pub api_keys: ApiKeys
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            microphone: -1,
            voice: String::from(""),

            wake_word_engine: config::DEFAULT_WAKE_WORD_ENGINE,
            speech_to_text_engine: config::DEFAULT_SPEECH_TO_TEXT_ENGINE,

            api_keys: ApiKeys {
                picovoice: String::from(""),
                openai: String::from("")
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiKeys {
    pub picovoice: String,
    pub openai: String
}