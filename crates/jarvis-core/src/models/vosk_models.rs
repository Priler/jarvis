use std::fs;
use std::path::{Path, PathBuf};

use crate::{config, APP_DIR};

#[derive(Debug, Clone)]
pub struct VoskModelInfo {
    pub name: String,     // folder name: "vosk-model-small-ru-0.22"
    pub path: PathBuf,    // full path
    pub language: String, // extracted from name: "ru"
    pub size: String,     // "small", "large", etc.
}

// Scan for available Vosk models
pub fn scan_vosk_models() -> Vec<VoskModelInfo> {
    let models_dir = { APP_DIR.join(config::VOSK_MODELS_PATH) };
    let mut models = Vec::new();

    info!("VOSK MODELS DIR: {}", models_dir.display());

    if !models_dir.exists() {
        warn!("Vosk models directory not found: {}", models_dir.display());
        return models;
    }

    let entries = match fs::read_dir(models_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!("Failed to read vosk models directory: {}", e);
            return models;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        // must be a directory
        if !path.is_dir() {
            continue;
        }

        // check if it looks like a vosk model (has am/conf/graph folders or similar)
        if !is_vosk_model(&path) {
            continue;
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let (language, size) = parse_model_name(&name);

        models.push(VoskModelInfo {
            name,
            path,
            language,
            size,
        });
    }

    models.sort_by(|a, b| a.name.cmp(&b.name));
    models
}

// Check if directory looks like a Vosk model
fn is_vosk_model(path: &Path) -> bool {
    let required_files = [
        path.join("am").join("final.mdl"),
        path.join("conf").join("model.conf"),
        path.join("graph").join("HCLr.fst"),
        path.join("graph").join("Gr.fst"),
    ];

    required_files.iter().all(|file| {
        fs::metadata(file)
            .map(|meta| meta.is_file() && meta.len() > 0)
            .unwrap_or(false)
    })
}

// Extract language and size from model name
// e.g., "vosk-model-small-ru-0.22" -> ("ru", "small")
fn parse_model_name(name: &str) -> (String, String) {
    let parts: Vec<&str> = name.split('-').collect();

    let mut language = String::from("unknown");
    let mut size = String::from("unknown");

    // look for common size indicators
    for part in &parts {
        match *part {
            "small" | "big" | "large" | "lgraph" => size = part.to_string(),
            // language codes are usually 2 letters
            s if s.len() == 2 && s.chars().all(|c| c.is_alphabetic()) => {
                language = s.to_string();
            }
            _ => {}
        }
    }

    (language, size)
}

// Get model path by name
pub fn get_model_path(model_name: &str) -> Option<PathBuf> {
    let path = APP_DIR.join(config::VOSK_MODELS_PATH).join(model_name);
    if path.exists() && path.is_dir() && is_vosk_model(&path) {
        Some(path)
    } else {
        None
    }
}
