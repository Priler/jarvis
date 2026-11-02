use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::sync::Arc;
use vosk::{DecodingState, Recognizer};

use crate::models::vosk::VoskModel;
use crate::DB;
use crate::{config, i18n, models, vosk_models};

// the model Arc keeps the vosk::Model alive for the recognizers
static VOSK_MODEL: OnceCell<Arc<VoskModel>> = OnceCell::new();
static WAKE_RECOGNIZER: OnceCell<Mutex<Recognizer>> = OnceCell::new();
static SPEECH_RECOGNIZER: OnceCell<Mutex<Recognizer>> = OnceCell::new();

pub fn init_vosk() -> Result<(), String> {
    if VOSK_MODEL.get().is_some() {
        return Ok(());
    }

    let model_path = get_configured_model_path()?;
    let model_id = format!("vosk:{}", model_path.display());

    // load through registry (shared if anything else needs the same model)
    let vosk = models::vosk::load(models::registry(), &model_id, model_path.to_str().unwrap())?;

    // language-specific wake grammar
    let lang = i18n::get_language();
    let wake_grammar = config::get_wake_grammar(&lang);
    info!("Wake grammar for '{}': {:?}", lang, wake_grammar);

    let mut wake_recognizer = Recognizer::new_with_grammar(&vosk.model, 16000.0, wake_grammar)
        .ok_or("Failed to create wake word recognizer")?;

    wake_recognizer.set_max_alternatives(1);

    let mut speech_recognizer =
        Recognizer::new(&vosk.model, 16000.0).ok_or("Failed to create speech recognizer")?;

    speech_recognizer.set_max_alternatives(config::VOSK_SPEECH_RECOGNIZER_MAX_ALTERNATIVES);
    speech_recognizer.set_words(config::VOSK_SPEECH_RECOGNIZER_WORDS);
    speech_recognizer.set_partial_words(config::VOSK_SPEECH_PARTIAL_WORDS);

    VOSK_MODEL.set(vosk).map_err(|_| "Model already set")?;
    WAKE_RECOGNIZER
        .set(Mutex::new(wake_recognizer))
        .map_err(|_| "Wake recognizer already set")?;
    SPEECH_RECOGNIZER
        .set(Mutex::new(speech_recognizer))
        .map_err(|_| "Speech recognizer already set")?;

    Ok(())
}

pub fn recognize_wake_word(data: &[i16]) -> Option<(String, f32)> {
    let mut recognizer = WAKE_RECOGNIZER.get()?.lock();

    match recognizer.accept_waveform(data) {
        Ok(DecodingState::Running) => None,
        Ok(DecodingState::Finalized) => {
            let result = recognizer.result();

            if let Some(alternatives) = result.multiple() {
                if let Some(best) = alternatives.alternatives.first() {
                    if !best.text.is_empty() {
                        return Some((best.text.to_string(), best.confidence));
                    }
                }
            }

            None
        }
        _ => None,
    }
}

pub fn recognize_speech(data: &[i16]) -> Option<String> {
    let mut recognizer = SPEECH_RECOGNIZER.get()?.lock();

    match recognizer.accept_waveform(data) {
        Ok(DecodingState::Finalized) => recognizer
            .result()
            .multiple()
            .and_then(|m| m.alternatives.first().map(|a| a.text.to_string())),
        _ => None,
    }
}

pub fn reset_speech_recognizer() {
    if let Some(recognizer) = SPEECH_RECOGNIZER.get() {
        recognizer.lock().reset();
    }
}

pub fn reset_wake_recognizer() {
    if let Some(recognizer) = WAKE_RECOGNIZER.get() {
        recognizer.lock().reset();
    }
}

fn get_configured_model_path() -> Result<std::path::PathBuf, String> {
    // try to get from settings
    if let Some(db) = DB.get() {
        let settings = db.read();
        if !settings.vosk_model.is_empty() {
            if let Some(path) = vosk_models::get_model_path(&settings.vosk_model) {
                return Ok(path);
            }
            warn!(
                "Configured Vosk model '{}' not found, falling back to auto-detect",
                settings.vosk_model
            );
        }
    }

    // auto-detect: prefer model matching current language
    let available = vosk_models::scan_vosk_models();
    let language = i18n::get_language();

    let lang_code = match language.as_str() {
        "ru" => "ru",
        "en" => "us",
        "ua" => "uk",
        other => other,
    };

    if let Some(matched) = available.iter().find(|m| m.language == lang_code) {
        info!(
            "Auto-detected Vosk model for '{}': {}",
            language, matched.name
        );
        return Ok(matched.path.clone());
    }

    if let Some(first) = available.first() {
        info!(
            "Auto-detected Vosk model (no language match): {}",
            first.name
        );
        return Ok(first.path.clone());
    }

    // fallback to legacy path
    let legacy_path = std::path::Path::new(config::VOSK_MODEL_PATH);
    if legacy_path.exists() {
        return Ok(legacy_path.to_path_buf());
    }

    Err("No Vosk models found".into())
}
