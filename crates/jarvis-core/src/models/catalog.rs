use std::fs;
use std::path::Path;

use super::structs::{BackendOption, ModelDef, Task};

// scan the models directory for folders containing model.toml
pub fn scan_models(models_dir: &Path) -> Vec<ModelDef> {
    let mut models = Vec::new();

    if !models_dir.exists() {
        warn!("Models directory not found: {:?}", models_dir);
        return models;
    }

    let entries = match fs::read_dir(models_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!("Failed to read models dir: {}", e);
            return models;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let toml_path = path.join("model.toml");
        if !toml_path.exists() {
            continue;
        }

        match load_model_def(&toml_path, &path) {
            Ok(def) => {
                info!(
                    "Found model: {} ({}) - tasks: {:?}",
                    def.name, def.id, def.tasks
                );
                models.push(def);
            }
            Err(e) => warn!("Failed to load model from {:?}: {}", path, e),
        }
    }

    models
}

fn load_model_def(toml_path: &Path, model_dir: &Path) -> Result<ModelDef, String> {
    let content = fs::read_to_string(toml_path).map_err(|e| format!("read error: {}", e))?;

    let parsed: ModelToml = toml::from_str(&content).map_err(|e| format!("parse error: {}", e))?;

    let mut def = parsed.model;
    def.path = model_dir.to_path_buf();

    Ok(def)
}

#[derive(serde::Deserialize)]
struct ModelToml {
    model: ModelDef,
}

// Code backends per task
pub fn code_backends(task: Task) -> Vec<BackendOption> {
    match task {
        Task::Intent => vec![BackendOption {
            id: "intent-classifier".into(),
            name: "Intent Classifier".into(),
            model_id: None,
        }],
        Task::Slots => vec![],
        Task::Vad => vec![
            BackendOption {
                id: "energy".into(),
                name: "Energy-based".into(),
                model_id: None,
            },
            BackendOption {
                id: "nnnoiseless".into(),
                name: "Nnnoiseless".into(),
                model_id: None,
            },
        ],
        Task::NoiseSuppression => vec![BackendOption {
            id: "nnnoiseless".into(),
            name: "Nnnoiseless".into(),
            model_id: None,
        }],
        Task::Stt => vec![BackendOption {
            id: "vosk".into(),
            name: "Vosk".into(),
            model_id: None,
        }],
    }
}

// get all available options for a task:
// "none" first, then code backends, then AI models from catalog
pub fn get_options(task: Task, models: &[ModelDef]) -> Vec<BackendOption> {
    let mut options = vec![BackendOption {
        id: "none".into(),
        name: "Disabled".into(),
        model_id: None,
    }];

    options.extend(code_backends(task));

    for model in models {
        if model.tasks.contains(&task) {
            options.push(BackendOption {
                id: model.id.clone(),
                name: model.name.clone(),
                model_id: Some(model.id.clone()),
            });
        }
    }

    options
}

pub fn is_valid_backend(task: Task, backend_id: &str, models: &[ModelDef]) -> bool {
    if backend_id == "none" {
        return true;
    }

    if code_backends(task).iter().any(|b| b.id == backend_id) {
        return true;
    }

    models
        .iter()
        .any(|m| m.id == backend_id && m.tasks.contains(&task))
}
