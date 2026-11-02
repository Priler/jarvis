use jarvis_core::{gliner_models, vosk_models};
use serde::Serialize;

#[derive(Serialize)]
pub struct VoskModel {
    pub name: String,
    pub language: String,
    pub size: String,
}

#[derive(Serialize)]
pub struct GlinerVariant {
    pub display_name: String,
    pub value: String,
}

#[tauri::command]
pub fn list_vosk_models() -> Vec<VoskModel> {
    vosk_models::scan_vosk_models()
        .into_iter()
        .map(|m| VoskModel {
            name: m.name,
            language: m.language,
            size: m.size,
        })
        .collect()
}

#[tauri::command]
pub fn list_gliner_models() -> Vec<GlinerVariant> {
    gliner_models::scan_gliner_variants()
        .into_iter()
        .map(|m| GlinerVariant {
            display_name: m.display_name,
            value: m.value,
        })
        .collect()
}
