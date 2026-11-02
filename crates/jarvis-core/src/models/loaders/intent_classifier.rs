// intent-classifier crate wrapper

use intent_classifier::IntentClassifier;
use std::sync::Arc;

use crate::models::registry::ModelRegistry;

pub struct IntentClassifierModel {
    pub classifier: IntentClassifier,
}

unsafe impl Send for IntentClassifierModel {}
unsafe impl Sync for IntentClassifierModel {}

// init is async (IntentClassifier::new().await), so we create it
// outside the registry and insert it after
pub async fn load(
    registry: &ModelRegistry,
    model_id: &str,
) -> Result<Arc<IntentClassifierModel>, String> {
    if let Some(existing) = registry.get::<IntentClassifierModel>(model_id) {
        info!("IntentClassifier '{}' already loaded, reusing", model_id);
        return Ok(existing);
    }

    info!("Initializing IntentClassifier...");

    let classifier = IntentClassifier::new()
        .await
        .map_err(|e| format!("Failed to init IntentClassifier: {}", e))?;

    info!("IntentClassifier initialized");
    Ok(registry.insert(model_id, IntentClassifierModel { classifier }))
}
