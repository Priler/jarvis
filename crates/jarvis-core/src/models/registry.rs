use parking_lot::{Mutex, RwLock};
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use super::structs::ModelDef;

// central model registry. loads models once and shares them between components.
// completely type-agnostic
pub struct ModelRegistry {
    loaded: Mutex<HashMap<String, Arc<dyn Any + Send + Sync>>>,
    catalog: RwLock<Vec<ModelDef>>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            loaded: Mutex::new(HashMap::new()),
            catalog: RwLock::new(Vec::new()),
        }
    }

    pub fn set_catalog(&self, defs: Vec<ModelDef>) {
        *self.catalog.write() = defs;
    }

    // read access to catalog without cloning the whole vec
    pub fn with_catalog<R>(&self, f: impl FnOnce(&[ModelDef]) -> R) -> R {
        f(&self.catalog.read())
    }

    pub fn get_model_def(&self, id: &str) -> Option<ModelDef> {
        self.catalog.read().iter().find(|m| m.id == id).cloned()
    }

    // get a loaded model, downcasted to the expected type
    pub fn get<T: 'static + Send + Sync>(&self, id: &str) -> Option<Arc<T>> {
        self.loaded.lock().get(id)?.clone().downcast::<T>().ok()
    }

    // get or load a model. if two components request the same id,
    // the model only loads once.
    //
    // the lock is released before calling the loader to avoid deadlocks
    // if the loader tries to load a dependency through the registry.
    pub fn get_or_load<T: 'static + Send + Sync>(
        &self,
        id: &str,
        loader: impl FnOnce(&ModelDef) -> Result<T, String>,
    ) -> Result<Arc<T>, String> {
        // fast path: already loaded
        if let Some(existing) = self.get::<T>(id) {
            info!("Model '{}' already loaded, reusing", id);
            return Ok(existing);
        }

        // grab model def (releases catalog lock immediately)
        let def = self
            .get_model_def(id)
            .ok_or_else(|| format!("Model '{}' not found in catalog", id))?;

        // run loader without holding any lock
        info!("Loading model '{}' from {:?}...", id, def.path);
        let model = loader(&def)?;
        let arc = Arc::new(model);

        // insert (check again in case another thread loaded it meanwhile)
        let mut map = self.loaded.lock();
        if let Some(existing) = map.get(id) {
            if let Ok(typed) = existing.clone().downcast::<T>() {
                info!("Model '{}' was loaded by another thread, reusing", id);
                return Ok(typed);
            }
        }

        map.insert(id.to_string(), arc.clone());
        info!("Model '{}' loaded and registered", id);

        Ok(arc)
    }

    // insert a model directly (for models not in the catalog,
    // or loaded through non-standard means like async init)
    pub fn insert<T: 'static + Send + Sync>(&self, id: &str, model: T) -> Arc<T> {
        let arc = Arc::new(model);
        self.loaded.lock().insert(id.to_string(), arc.clone());
        arc
    }

    pub fn unload(&self, id: &str) -> bool {
        let removed = self.loaded.lock().remove(id).is_some();
        if removed {
            info!("Model '{}' unloaded from registry", id);
        }
        removed
    }

    pub fn is_loaded(&self, id: &str) -> bool {
        self.loaded.lock().contains_key(id)
    }

    pub fn loaded_ids(&self) -> Vec<String> {
        self.loaded.lock().keys().cloned().collect()
    }
}
