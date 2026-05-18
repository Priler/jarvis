use crate::{config, keychain};
use serde::{Deserialize, Serialize};

use crate::config::structs::SpeechToTextEngine;
use crate::config::structs::WakeWordEngine;
use crate::config::structs::NoiseSuppressionBackend;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Settings {
    pub microphone: i32,
    pub voice: String,

    pub wake_word_engine: WakeWordEngine,

    // backend selections (string IDs matching model or code backend IDs)
    #[serde(default = "default_intent_backend")]
    pub intent_backend: String,
    #[serde(default = "default_slots_backend")]
    pub slots_backend: String,
    #[serde(default = "default_vad_backend")]
    pub vad_backend: String,

    pub gliner_model: String,

    pub speech_to_text_engine: SpeechToTextEngine,
    pub vosk_model: String,

    // audio processing
    pub noise_suppression: NoiseSuppressionBackend,
    pub gain_normalizer: bool,

    #[serde(default = "default_language")]
    pub language: String,

    pub api_keys: ApiKeys,

    #[serde(default)]
    pub ollama: OllamaConfig,
}

fn default_intent_backend() -> String { config::DEFAULT_INTENT_BACKEND.to_string() }
fn default_slots_backend() -> String { config::DEFAULT_SLOTS_BACKEND.to_string() }
fn default_vad_backend() -> String { config::DEFAULT_VAD_BACKEND.to_string() }
fn default_language() -> String { crate::i18n::detect_system_language().to_string() }

// ### KEY-VALUE ACCESS

impl Settings {
    /// read a setting by key. returns None for unknown keys.
    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "selected_microphone"       => Some(self.microphone.to_string()),
            "assistant_voice"           => Some(self.voice.clone()),
            "selected_wake_word_engine" => Some(format!("{:?}", self.wake_word_engine)),
            "intent_backend"            => Some(self.intent_backend.clone()),
            "slots_backend"             => Some(self.slots_backend.clone()),
            "vad_backend"               => Some(self.vad_backend.clone()),
            "selected_gliner_model"     => Some(self.gliner_model.clone()),
            "selected_vosk_model"       => Some(self.vosk_model.clone()),
            "speech_to_text_engine"     => Some(format!("{:?}", self.speech_to_text_engine)),
            "noise_suppression"         => Some(format!("{:?}", self.noise_suppression)),
            "gain_normalizer"           => Some(self.gain_normalizer.to_string()),
            "language"                  => Some(self.language.clone()),
            "api_key__picovoice"        => keychain::get_api_key("picovoice").ok(),
            "ollama_url"                => Some(self.ollama.url.clone()),
            "ollama_model"              => Some(self.ollama.model.clone()),
            _ => None,
        }
    }

    /// write a setting by key. returns Err for unknown keys or invalid values.
    pub fn set(&mut self, key: &str, val: &str) -> Result<(), String> {
        match key {
            "selected_microphone" => {
                self.microphone = val.parse::<i32>()
                    .map_err(|_| format!("invalid integer: '{}'", val))?;
            }
            "assistant_voice" => {
                self.voice = val.to_string();
            }
            "selected_wake_word_engine" => {
                self.wake_word_engine = match val.to_lowercase().as_str() {
                    "rustpotter" => WakeWordEngine::Rustpotter,
                    "vosk"       => WakeWordEngine::Vosk,
                    "porcupine"  => WakeWordEngine::Porcupine,
                    _ => return Err(format!("unknown wake word engine: '{}'", val)),
                };
            }
            "intent_backend" => {
                self.intent_backend = val.to_string();
            }
            "slots_backend" => {
                self.slots_backend = val.to_string();
            }
            "vad_backend" => {
                self.vad_backend = val.to_string();
            }
            "selected_gliner_model" => {
                self.gliner_model = val.to_string();
            }
            "selected_vosk_model" => {
                self.vosk_model = val.to_string();
            }
            "noise_suppression" => {
                self.noise_suppression = match val.to_lowercase().as_str() {
                    "none"        => NoiseSuppressionBackend::None,
                    "nnnoiseless" => NoiseSuppressionBackend::Nnnoiseless,
                    _ => return Err(format!("unknown noise suppression backend: '{}'", val)),
                };
            }
            "gain_normalizer" => {
                self.gain_normalizer = match val.to_lowercase().as_str() {
                    "true"  => true,
                    "false" => false,
                    _ => return Err(format!("expected 'true' or 'false', got: '{}'", val)),
                };
            }
            "language" => {
                self.language = val.to_string();
            }
            "api_key__picovoice" => {
                keychain::set_api_key("picovoice", val)
                    .map_err(|e| format!("failed to store API key in OS keyring: {}", e))?;
                // Do not store in the Settings struct — key lives only in keyring.
            }
            "ollama_url" => {
                self.ollama.url = val.to_string();
            }
            "ollama_model" => {
                self.ollama.model = val.to_string();
            }
            _ => return Err(format!("unknown setting: '{}'", key)),
        }
        Ok(())
    }

    /// all valid setting keys (for enumeration, debugging, etc.)
    pub fn keys() -> &'static [&'static str] {
        &[
            "selected_microphone",
            "assistant_voice",
            "selected_wake_word_engine",
            "intent_backend",
            "slots_backend",
            "vad_backend",
            "selected_gliner_model",
            "selected_vosk_model",
            "speech_to_text_engine",
            "noise_suppression",
            "gain_normalizer",
            "language",
            "api_key__picovoice",
            "ollama_url",
            "ollama_model",
        ]
    }
}

// ### DEFAULT

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            microphone: -1,
            voice: String::from(""),

            wake_word_engine: config::DEFAULT_WAKE_WORD_ENGINE,

            intent_backend: config::DEFAULT_INTENT_BACKEND.to_string(),
            slots_backend: config::DEFAULT_SLOTS_BACKEND.to_string(),
            vad_backend: config::DEFAULT_VAD_BACKEND.to_string(),

            gliner_model: String::new(),
            speech_to_text_engine: config::DEFAULT_SPEECH_TO_TEXT_ENGINE,
            vosk_model: String::from(""),

            noise_suppression: config::DEFAULT_NOISE_SUPPRESSION,
            gain_normalizer: config::DEFAULT_GAIN_NORMALIZER,

            language: crate::i18n::detect_system_language().to_string(),

            api_keys: ApiKeys {
                picovoice: String::from(""),
            },
            ollama: OllamaConfig {
                url: String::from("http://localhost:11434"),
                model: String::from(""),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiKeys {
    // SEC-6: key is stored in the OS keyring, not in the JSON settings file.
    // skip_serializing ensures new saves never write the key to disk.
    // The field is still deserializable so existing JSON files can be migrated
    // on first load (see db::init_settings migration step).
    #[serde(skip_serializing, default)]
    pub picovoice: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OllamaConfig {
    pub url: String,
    pub model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        OllamaConfig {
            url: String::from("http://localhost:11434"),
            model: String::from(""),
        }
    }
}
