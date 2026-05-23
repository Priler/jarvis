mod intentclassifier;
mod embeddingclassifier;

use std::path::PathBuf;

use crate::{commands::{self, JCommandsList, JCommand}, config, models};
use once_cell::sync::OnceCell;

use crate::DB;

static BACKEND: OnceCell<String> = OnceCell::new();

pub async fn init(commands: &Vec<JCommandsList>) -> Result<(), String> {
    if BACKEND.get().is_some() {
        return Ok(());
    }

    let backend = DB.get().unwrap().read().intent_backend.clone();

    let effective_backend = match backend.as_str() {
        "none" => {
            info!("Intent recognition disabled");
            backend
        }
        "intent-classifier" => {
            info!("Initializing IntentClassifier backend.");
            intentclassifier::init(&commands).await?;
            info!("IntentClassifier backend initialized.");
            backend
        }
        // any other value is treated as a model ID for embedding classification
        model_id => {
            info!("Initializing EmbeddingClassifier with model '{}'.", model_id);
            match models::embedding::load(models::registry(), model_id) {
                Ok(model) => {
                    embeddingclassifier::init_with_model(model, &commands)?;
                    info!("EmbeddingClassifier backend initialized.");
                    backend
                }
                Err(e) => {
                    warn!(
                        "EmbeddingClassifier model '{}' not found in catalog: {}. Intent recognition disabled.",
                        model_id, e
                    );
                    "none".to_string()
                }
            }
        }
    };

    BACKEND.set(effective_backend).map_err(|_| "Backend already set")?;

    Ok(())
}

/// Reload the active intent backend with updated commands (no app restart required).
pub async fn reload(commands: &[JCommandsList]) -> Result<(), String> {
    match BACKEND.get().map(|s| s.as_str()) {
        Some("none") | None => Ok(()),
        Some("intent-classifier") => intentclassifier::reload(commands).await,
        Some(_) => embeddingclassifier::reload(commands),
    }
}

pub async fn classify(text: &str) -> Option<(String, f64)> {
    match BACKEND.get()?.as_str() {
        "none" => None,
        "intent-classifier" => {
            match intentclassifier::classify(text).await {
                Ok(prediction) => {
                    let confidence = prediction.confidence.value();
                    if confidence >= config::INTENT_CLASSIFIER_MIN_CONFIDENCE {
                        Some((prediction.intent.to_string(), confidence))
                    } else {
                        None
                    }
                }
                Err(e) => {
                    error!("Intent classification error: {}", e);
                    None
                }
            }
        }
        _ => {
            match embeddingclassifier::classify(text) {
                Ok((intent_id, confidence)) => {
                    if confidence >= config::EMBEDDING_MIN_CONFIDENCE {
                        Some((intent_id, confidence))
                    } else {
                        None
                    }
                }
                Err(e) => {
                    error!("Embedding classification error: {}", e);
                    None
                }
            }
        }
    }
}

// unified command lookup by intent ID - works for all backends
pub fn get_command_by_intent<'a>(
    commands: &'a [JCommandsList],
    intent_id: &str,
) -> Option<(&'a PathBuf, &'a JCommand)> {
    if matches!(BACKEND.get().map(|s| s.as_str()), Some("none")) {
        return None;
    }
    commands::get_command_by_id(commands, intent_id)
}
