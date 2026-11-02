use crate::AppState;
use jarvis_core::i18n;
use std::collections::HashMap;

// Get all translations for frontend
#[tauri::command]
pub fn get_translations() -> HashMap<String, String> {
    i18n::get_all_translations()
}

// Get single translation
#[tauri::command]
pub fn translate(key: &str) -> String {
    i18n::t(key)
}

// Get current language
#[tauri::command]
pub fn get_current_language() -> String {
    i18n::get_language()
}

// Set language and get new translations
#[tauri::command]
pub fn set_language(state: tauri::State<'_, AppState>, lang: &str) -> HashMap<String, String> {
    // update i18n
    i18n::set_language(lang);

    if let Err(e) = state.settings.write("language", lang) {
        log::error!("Failed to save language setting: {}", e);
    }

    // return new translations
    i18n::get_all_translations()
}

// Get supported languages
#[tauri::command]
pub fn get_supported_languages() -> Vec<&'static str> {
    i18n::SUPPORTED_LANGUAGES.to_vec()
}
