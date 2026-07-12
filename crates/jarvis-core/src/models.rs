mod registry;
mod catalog;
pub mod structs;
pub mod loaders;

pub mod vosk_models;
pub mod gliner_models;

// re-export loaders
#[cfg(feature = "intent")]
pub use loaders::embedding;

#[cfg(feature = "jarvis_app")]
pub use loaders::gliner;

#[cfg(feature = "jarvis_app")]
pub use loaders::ort_model;

#[cfg(feature = "intent")]
pub use loaders::intent_classifier;

#[cfg(feature = "vosk")]
pub use loaders::vosk;

#[cfg(feature = "nnnoiseless")]
pub use loaders::nnnoiseless;

pub use registry::ModelRegistry;
pub use structs::{Task, ModelDef, BackendOption};

use once_cell::sync::OnceCell;

use crate::APP_DIR;

pub const MODELS_PATH: &str = "resources/models";

static REGISTRY: OnceCell<ModelRegistry> = OnceCell::new();

pub fn init() -> Result<(), String> {
    if REGISTRY.get().is_some() {
        return Ok(());
    }

    let registry = ModelRegistry::new();

    let models_dir = APP_DIR.join(MODELS_PATH);
    let models = catalog::scan_models(&models_dir);
    info!("Found {} model(s) in {:?}", models.len(), models_dir);
    registry.set_catalog(models);

    REGISTRY.set(registry)
        .map_err(|_| "Models registry already initialized".to_string())?;

    Ok(())
}

pub fn registry() -> &'static ModelRegistry {
    REGISTRY.get().expect("Models registry not initialized - call models::init() first")
}

pub fn get_options(task: Task) -> Vec<BackendOption> {
    registry().with_catalog(|models| catalog::get_options(task, models))
}

pub fn is_valid_backend(task: Task, backend_id: &str) -> bool {
    registry().with_catalog(|models| catalog::is_valid_backend(task, backend_id, models))
}
