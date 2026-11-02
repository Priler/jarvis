mod gliner;

use once_cell::sync::OnceCell;
use std::collections::HashMap;

use crate::commands::{SlotDefinition, SlotValue};
use crate::{models, DB};

static BACKEND: OnceCell<String> = OnceCell::new();

pub fn init() -> Result<(), String> {
    if BACKEND.get().is_some() {
        return Ok(());
    }

    let backend = DB
        .get()
        .map(|db| db.read().slots_backend.clone())
        .unwrap_or_else(|| "none".to_string());

    BACKEND
        .set(backend.clone())
        .map_err(|_| "Slot backend already set")?;

    match backend.as_str() {
        "none" => {
            info!("Slot extraction disabled");
        }
        // any model ID is treated as a GLiNER model for now
        model_id => {
            info!(
                "Initializing GLiNER slot extraction with model '{}'.",
                model_id
            );
            let model = models::gliner::load(models::registry(), model_id)?;
            gliner::init_with_model(model)?;
            info!("GLiNER slot extraction initialized.");
        }
    }

    Ok(())
}

pub fn extract(text: &str, slots: &HashMap<String, SlotDefinition>) -> HashMap<String, SlotValue> {
    if slots.is_empty() {
        return HashMap::new();
    }

    match BACKEND.get().map(|s| s.as_str()).unwrap_or("none") {
        "none" => HashMap::new(),
        _ => match gliner::extract(text, slots) {
            Ok(result) => result,
            Err(e) => {
                error!("GLiNER slot extraction failed: {}", e);
                HashMap::new()
            }
        },
    }
}
