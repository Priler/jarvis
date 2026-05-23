//! Runtime preferences — fast-access key/value settings that persist across restarts.

use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static PREFS_READ:  AtomicU64 = AtomicU64::new(0);
pub static PREFS_WRITE: AtomicU64 = AtomicU64::new(0);

const PREFS_FILE: &str = "preferences.json";

// ── Defaults ──────────────────────────────────────────────────────────────────

fn default_prefs() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("voice_enabled".to_string(),       "true".to_string());
    m.insert("model_auto_select".to_string(),   "true".to_string());
    m.insert("memory_enabled".to_string(),      "true".to_string());
    m.insert("rag_enabled".to_string(),         "true".to_string());
    m.insert("performance_mode".to_string(),    "balanced".to_string()); // minimal|balanced|full
    m.insert("log_level".to_string(),           "info".to_string());
    m.insert("ui_theme".to_string(),            "dark".to_string());
    m.insert("startup_scan_models".to_string(), "true".to_string());
    m.insert("safe_mode".to_string(),           "false".to_string());
    m.insert("telemetry".to_string(),           "false".to_string()); // always false
    m
}

static PREFS: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| {
    Mutex::new(default_prefs())
});

fn prefs_path() -> std::path::PathBuf {
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("jarvis").join(PREFS_FILE)
}

// ── Persistence ───────────────────────────────────────────────────────────────

pub fn init() {
    let path = prefs_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(loaded) = serde_json::from_str::<HashMap<String, String>>(&content) {
            let mut p = PREFS.lock().unwrap();
            for (k, v) in loaded {
                // Never allow telemetry to be enabled
                if k == "telemetry" { continue; }
                p.insert(k, v);
            }
        }
    }
}

fn save() {
    let path = prefs_path();
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let p = PREFS.lock().unwrap();
    if let Ok(json) = serde_json::to_string_pretty(&*p) {
        let _ = std::fs::write(&path, json);
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn get(key: &str) -> Option<String> {
    PREFS_READ.fetch_add(1, Ordering::Relaxed);
    PREFS.lock().unwrap().get(key).cloned()
}

pub fn get_or(key: &str, default: &str) -> String {
    get(key).unwrap_or_else(|| default.to_string())
}

pub fn get_bool(key: &str, default: bool) -> bool {
    get(key).map(|v| v == "true").unwrap_or(default)
}

pub fn get_f32(key: &str, default: f32) -> f32 {
    get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

pub fn set(key: &str, value: &str) {
    if key == "telemetry" { return; } // telemetry always false
    PREFS_WRITE.fetch_add(1, Ordering::Relaxed);
    PREFS.lock().unwrap().insert(key.to_string(), value.to_string());
    save();
}

pub fn set_bool(key: &str, value: bool) {
    set(key, if value { "true" } else { "false" });
}

pub fn all() -> HashMap<String, String> {
    PREFS.lock().unwrap().clone()
}

pub fn reset_to_defaults() {
    let mut p = PREFS.lock().unwrap();
    *p = default_prefs();
    drop(p);
    save();
}

pub fn is_voice_enabled()     -> bool { get_bool("voice_enabled", true) }
pub fn is_memory_enabled()    -> bool { get_bool("memory_enabled", true) }
pub fn is_rag_enabled()       -> bool { get_bool("rag_enabled", true) }
pub fn is_safe_mode()         -> bool { get_bool("safe_mode", false) }
pub fn performance_mode()     -> String { get_or("performance_mode", "balanced") }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_telemetry_false() {
        assert_eq!(get_or("telemetry", "false"), "false");
    }

    #[test]
    fn telemetry_cannot_be_set() {
        set("telemetry", "true");
        assert_eq!(get_or("telemetry", "false"), "false");
    }

    #[test]
    fn set_and_get_preference() {
        set("log_level", "debug");
        assert_eq!(get_or("log_level", "info"), "debug");
        set("log_level", "info"); // restore
    }

    #[test]
    fn get_bool_default() {
        assert!(get_bool("voice_enabled", true));
    }

    #[test]
    fn all_returns_map() {
        let prefs = all();
        assert!(!prefs.is_empty());
        assert!(prefs.contains_key("voice_enabled"));
    }
}
