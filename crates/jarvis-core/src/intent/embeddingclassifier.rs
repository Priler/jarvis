use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use once_cell::sync::OnceCell;

use crate::commands::JCommandsList;
use crate::i18n;
use crate::models::embedding::EmbeddingModel;
use crate::APP_CONFIG_DIR;

// no outer Mutex needed - state is immutable after init.
// the embedding model has its own internal Mutex.
static CLASSIFIER: OnceCell<EmbeddingClassifierState> = OnceCell::new();

struct IntentVector {
    id: String,
    vector: Vec<f32>,
}

struct EmbeddingClassifierState {
    model: Arc<EmbeddingModel>,
    intents: Vec<IntentVector>,
}

// model is Arc (Send+Sync), intents are read-only after init
unsafe impl Send for EmbeddingClassifierState {}
unsafe impl Sync for EmbeddingClassifierState {}

const CACHE_FILE: &str = "embedding_intents.json";
const HASH_FILE: &str = "embedding_hash.txt";

// init with a model loaded through the registry
pub fn init_with_model(
    model: Arc<EmbeddingModel>,
    commands: &[JCommandsList],
) -> Result<(), String> {
    if CLASSIFIER.get().is_some() {
        return Ok(());
    }

    info!("Initializing embedding classifier...");

    let current_hash = crate::commands::commands_hash(commands);
    let config_dir = APP_CONFIG_DIR.get().ok_or("Config dir not set")?;
    let hash_path = config_dir.join(HASH_FILE);
    let cache_path = config_dir.join(CACHE_FILE);

    // check if cached vectors are still valid
    let should_retrain = if hash_path.exists() && cache_path.exists() {
        let stored_hash = fs::read_to_string(&hash_path).unwrap_or_default();
        stored_hash.trim() != current_hash
    } else {
        true
    };

    let intents = if should_retrain {
        info!("Building intent vectors from commands...");
        let intents = build_intent_vectors(&model, commands)?;

        // cache to disk
        if let Ok(json) = serde_json::to_string(&intents_to_cache(&intents)) {
            let _ = fs::write(&cache_path, json);
            let _ = fs::write(&hash_path, &current_hash);
            info!("Intent vectors cached");
        }

        intents
    } else {
        info!("Loading cached intent vectors...");
        load_cached_intents(&cache_path)?
    };

    info!("Embedding classifier ready with {} intents", intents.len());

    CLASSIFIER
        .set(EmbeddingClassifierState { model, intents })
        .map_err(|_| "Classifier already set".to_string())?;

    Ok(())
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

            let embeddings = model
                .embedding
                .lock()
                .embed(texts, None)
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

    // only the embedding model needs locking, intents are read-only
    let embeddings = state
        .model
        .embedding
        .lock()
        .embed(vec![text], None)
        .map_err(|e| format!("Failed to embed query: {}", e))?;

    let mut query_vec = embeddings
        .into_iter()
        .next()
        .ok_or("Empty embedding result")?;

    // normalize query
    let norm: f32 = query_vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in &mut query_vec {
            *val /= norm;
        }
    }

    // cosine similarity - track index, clone only the winner
    let mut best_idx: usize = 0;
    let mut best_score: f64 = -1.0;

    for (i, intent) in state.intents.iter().enumerate() {
        let score: f64 = query_vec
            .iter()
            .zip(intent.vector.iter())
            .map(|(a, b)| (*a as f64) * (*b as f64))
            .sum();

        if score > best_score {
            best_score = score;
            best_idx = i;
        }
    }

    let best_id = state.intents[best_idx].id.clone();
    debug!(
        "Embedding classify: '{}' -> '{}' ({:.2}%)",
        text,
        best_id,
        best_score * 100.0
    );

    Ok((best_id, best_score))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedIntent {
    id: String,
    vector: Vec<f32>,
}

fn intents_to_cache(intents: &[IntentVector]) -> Vec<CachedIntent> {
    intents
        .iter()
        .map(|i| CachedIntent {
            id: i.id.clone(),
            vector: i.vector.clone(),
        })
        .collect()
}

fn load_cached_intents(path: &PathBuf) -> Result<Vec<IntentVector>, String> {
    let json = fs::read_to_string(path).map_err(|e| format!("Failed to read cache: {}", e))?;

    let cached: Vec<CachedIntent> =
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse cache: {}", e))?;

    Ok(cached
        .into_iter()
        .map(|c| IntentVector {
            id: c.id,
            vector: c.vector,
        })
        .collect())
}
