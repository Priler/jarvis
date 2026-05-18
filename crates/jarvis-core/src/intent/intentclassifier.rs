use intent_classifier::{
    IntentPrediction, IntentError,
    TrainingExample, TrainingSource, IntentId
};

use std::sync::Arc;
use std::fs;

use crate::commands::{self, JCommandsList};
use crate::models;
use crate::models::intent_classifier::IntentClassifierModel;
use crate::{APP_CONFIG_DIR, i18n};

use once_cell::sync::OnceCell;

static MODEL: OnceCell<Arc<IntentClassifierModel>> = OnceCell::new();

const TRAINING_CACHE_FILE: &str = "intent_training.json";
const COMMANDS_HASH_FILE: &str = "commands_hash.txt";

pub async fn init(commands: &[JCommandsList]) -> Result<(), String> {
    let current_hash = commands::commands_hash(&commands);
    
    let model = models::intent_classifier::load(models::registry(), "intent-classifier").await?;
    
    // check if we can use cached training data
    let config_dir = APP_CONFIG_DIR.get().ok_or("Config dir not set")?;
    let hash_path = config_dir.join(COMMANDS_HASH_FILE);
    let cache_path = config_dir.join(TRAINING_CACHE_FILE);
    
    let should_retrain = if hash_path.exists() && cache_path.exists() {
        let stored_hash = fs::read_to_string(&hash_path).unwrap_or_default();
        stored_hash.trim() != current_hash
    } else {
        true
    };
    
    if should_retrain {
        info!("Training intent classifier with {} commands...", commands.len());
        train_classifier(&model.classifier, &commands).await?;
        
        if let Ok(export) = model.classifier.export_training_data().await {
            let _ = fs::write(&cache_path, export);
            let _ = fs::write(&hash_path, &current_hash);
            info!("Training data cached.");
        }
    } else {
        info!("Loading cached training data...");
        if let Ok(data) = fs::read_to_string(&cache_path) {
            model.classifier.import_training_data(&data).await
                .map_err(|e| format!("Failed to import training data: {}", e))?;
        }
    }
    
    MODEL.set(model).map_err(|_| "Model already set")?;
    
    Ok(())
}

/// Re-train the classifier with updated commands. Adds new examples on top of existing ones;
/// for a fully clean retrain the app must be restarted (OnceCell MODEL cannot be replaced).
pub async fn reload(commands: &[JCommandsList]) -> Result<(), String> {
    let model = MODEL.get().ok_or("IntentClassifier not initialized")?;
    train_classifier(&model.classifier, commands).await?;
    let current_hash = commands::commands_hash(commands);
    if let Some(config_dir) = APP_CONFIG_DIR.get() {
        let cache_path = config_dir.join(TRAINING_CACHE_FILE);
        let hash_path = config_dir.join(COMMANDS_HASH_FILE);
        if let Ok(export) = model.classifier.export_training_data().await {
            let _ = fs::write(&cache_path, export);
            let _ = fs::write(&hash_path, &current_hash);
        }
    }
    info!("IntentClassifier reloaded");
    Ok(())
}

pub async fn classify(text: &str) -> Result<IntentPrediction, IntentError> {
    let model = MODEL.get().expect("IntentClassifier not initialized");
    model.classifier.predict_intent(text).await
}

async fn train_classifier(
    classifier: &intent_classifier::IntentClassifier,
    commands: &[JCommandsList]
) -> Result<(), String> {
    let lang = i18n::get_language();
    info!("Training intent classifier for language: {}", lang);

    let mut total_examples = 0;

    for assistant_cmd in commands {
        for cmd in &assistant_cmd.commands {
            let phrases = cmd.get_phrases(&lang);
            
            for phrase in phrases.iter() {
                let example = TrainingExample {
                    text: phrase.clone(),
                    intent: IntentId::from(cmd.id.as_str()),
                    confidence: 1.0,
                    source: TrainingSource::Programmatic,
                };
                
                classifier.add_training_example(example).await
                    .map_err(|e| format!("Failed to add training example: {}", e))?;
                
                total_examples += 1;
            }
        }
    }

    info!("Added {} training examples for language '{}'", total_examples, lang);
    Ok(())
}
