use std::path::PathBuf;
use std::sync::Arc;
use std::fs;

use once_cell::sync::OnceCell;
use parking_lot::RwLock;

use crate::commands::{self, JCommandsList};
use crate::i18n;
use crate::APP_CONFIG_DIR;
use crate::models::embedding::EmbeddingModel;

static CLASSIFIER: OnceCell<EmbeddingClassifierState> = OnceCell::new();

struct IntentVector {
    id: String,
    vector: Vec<f32>,
}

struct EmbeddingClassifierState {
    model: Arc<EmbeddingModel>,
    intents: RwLock<Vec<IntentVector>>,
}

const CACHE_FILE: &str = "embedding_intents.json";
const HASH_FILE: &str = "embedding_hash.txt";

// init with a model loaded through the registry
pub fn init_with_model(model: Arc<EmbeddingModel>, commands: &[JCommandsList]) -> Result<(), String> {
    if CLASSIFIER.get().is_some() {
        return Ok(());
    }

    info!("Initializing embedding classifier...");

    let intents = load_or_build_intents(&model, commands)?;
    info!("Embedding classifier ready with {} intents", intents.len());

    CLASSIFIER.set(EmbeddingClassifierState { model, intents: RwLock::new(intents) })
        .map_err(|_| "Classifier already set".to_string())?;

    Ok(())
}

/// Reload intent vectors from updated command list without recreating the model.
pub fn reload(commands: &[JCommandsList]) -> Result<(), String> {
    let state = CLASSIFIER.get().ok_or("Embedding classifier not initialized")?;
    info!("Reloading embedding classifier with {} command packs...", commands.len());
    let new_intents = build_intent_vectors(&state.model, commands)?;
    // Update cache
    let current_hash = commands::commands_hash(commands);
    if let Some(config_dir) = APP_CONFIG_DIR.get() {
        let cache_path = config_dir.join(CACHE_FILE);
        let hash_path = config_dir.join(HASH_FILE);
        if let Ok(json) = serde_json::to_string(&intents_to_cache(&new_intents)) {
            let _ = fs::write(&cache_path, json);
            let _ = fs::write(&hash_path, &current_hash);
        }
    }
    *state.intents.write() = new_intents;
    info!("Embedding classifier reloaded");
    Ok(())
}

fn load_or_build_intents(model: &EmbeddingModel, commands: &[JCommandsList]) -> Result<Vec<IntentVector>, String> {
    let current_hash = commands::commands_hash(commands);
    let config_dir = APP_CONFIG_DIR.get().ok_or("Config dir not set")?;
    let hash_path = config_dir.join(HASH_FILE);
    let cache_path = config_dir.join(CACHE_FILE);

    let should_retrain = if hash_path.exists() && cache_path.exists() {
        let stored_hash = fs::read_to_string(&hash_path).unwrap_or_default();
        stored_hash.trim() != current_hash
    } else {
        true
    };

    if should_retrain {
        info!("Building intent vectors from commands...");
        let intents = build_intent_vectors(model, commands)?;
        if let Ok(json) = serde_json::to_string(&intents_to_cache(&intents)) {
            let _ = fs::write(&cache_path, json);
            let _ = fs::write(&hash_path, &current_hash);
            info!("Intent vectors cached");
        }
        Ok(intents)
    } else {
        info!("Loading cached intent vectors...");
        load_cached_intents(&cache_path)
    }
}

fn build_intent_vectors(
    model: &EmbeddingModel,
    commands: &[JCommandsList],
) -> Result<Vec<IntentVector>, String> {
    let lang = i18n::get_language();
    let mut intents = Vec::new();

    for cmd_list in commands {
        for cmd in &cmd_list.commands {
            let phrases = cmd.get_phrases(&lang);
            if phrases.is_empty() {
                continue;
            }

            let texts: Vec<&str> = phrases.iter().map(|s| s.as_str()).collect();
            
            let embeddings = model.embedding.lock().embed(texts, None)
                .map_err(|e| format!("Embedding failed for '{}': {}", cmd.id, e))?;

            // average all phrase vectors into one intent vector
            let dim = embeddings[0].len();
            let mut avg = vec![0.0f32; dim];
            
            for emb in &embeddings {
                for (i, val) in emb.iter().enumerate() {
                    avg[i] += val;
                }
            }
            
            let count = embeddings.len() as f32;
            for val in &mut avg {
                *val /= count;
            }

            // normalize
            let norm: f32 = avg.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                for val in &mut avg {
                    *val /= norm;
                }
            }

            intents.push(IntentVector {
                id: cmd.id.clone(),
                vector: avg,
            });
        }
    }

    Ok(intents)
}

pub fn classify(text: &str) -> Result<(String, f64), String> {
    let state = CLASSIFIER.get().ok_or("Classifier not initialized")?;

    let embeddings = state.model.embedding.lock().embed(vec![text], None)
        .map_err(|e| format!("Failed to embed query: {}", e))?;

    let mut query_vec = embeddings.into_iter().next()
        .ok_or("Empty embedding result")?;

    let norm: f32 = query_vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in &mut query_vec { *val /= norm; }
    }

    let intents = state.intents.read();
    let mut best_idx: usize = 0;
    let mut best_score: f64 = -1.0;

    for (i, intent) in intents.iter().enumerate() {
        let score: f64 = query_vec.iter()
            .zip(intent.vector.iter())
            .map(|(a, b)| (*a as f64) * (*b as f64))
            .sum();
        if score > best_score {
            best_score = score;
            best_idx = i;
        }
    }

    let best_id = intents[best_idx].id.clone();
    debug!("Embedding classify: '{}' -> '{}' ({:.2}%)", text, best_id, best_score * 100.0);

    Ok((best_id, best_score))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedIntent {
    id: String,
    vector: Vec<f32>,
}

fn intents_to_cache(intents: &[IntentVector]) -> Vec<CachedIntent> {
    intents.iter().map(|i| CachedIntent {
        id: i.id.clone(),
        vector: i.vector.clone(),
    }).collect()
}

fn load_cached_intents(path: &PathBuf) -> Result<Vec<IntentVector>, String> {
    let json = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read cache: {}", e))?;
    
    let cached: Vec<CachedIntent> = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to parse cache: {}", e))?;
    
    Ok(cached.into_iter().map(|c| IntentVector {
        id: c.id,
        vector: c.vector,
    }).collect())
}
