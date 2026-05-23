//! User profile system — named profiles with voice, model, memory, and privacy settings.
//! Profiles are persisted as TOML files in the Jarvis config directory.

use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static PROFILES_LOADED:  AtomicU64 = AtomicU64::new(0);
pub static PROFILES_SAVED:   AtomicU64 = AtomicU64::new(0);
pub static PROFILE_SWITCHES: AtomicU64 = AtomicU64::new(0);

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VoiceSettings {
    pub wake_word:        String,
    pub tts_voice:        String,
    pub stt_language:     String,
    pub vad_threshold:    f32,
    pub noise_reduction:  bool,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            wake_word:       "hey jarvis".to_string(),
            tts_voice:       "default".to_string(),
            stt_language:    "en-US".to_string(),
            vad_threshold:   0.5,
            noise_reduction: true,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelSettings {
    pub preferred_model:    String,
    pub runtime_profile:    String,  // "lite" | "standard" | "full"
    pub context_window:     u32,
    pub temperature:        f32,
    pub max_tokens:         u32,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            preferred_model: "auto".to_string(),
            runtime_profile: "lite".to_string(),
            context_window:  4096,
            temperature:     0.7,
            max_tokens:      512,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemorySettings {
    pub persistent_memory:    bool,
    pub conversation_history: u32,
    pub auto_index_documents: bool,
    pub memory_retention_days: u32,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            persistent_memory:    true,
            conversation_history: 50,
            auto_index_documents: false,
            memory_retention_days: 90,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PrivacySettings {
    pub telemetry:          bool,  // always false — enforced offline
    pub local_only:         bool,  // always true
    pub log_conversations:  bool,
    pub share_diagnostics:  bool,  // always false
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            telemetry:         false,
            local_only:        true,
            log_conversations: false,
            share_diagnostics: false,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserProfile {
    pub name:     String,
    pub voice:    VoiceSettings,
    pub model:    ModelSettings,
    pub memory:   MemorySettings,
    pub privacy:  PrivacySettings,
    pub created_at: u64,
    pub updated_at: u64,
}

impl UserProfile {
    pub fn default_profile() -> Self {
        let ts = ts_now();
        Self {
            name:       "Default".to_string(),
            voice:      VoiceSettings::default(),
            model:      ModelSettings::default(),
            memory:     MemorySettings::default(),
            privacy:    PrivacySettings::default(),
            created_at: ts,
            updated_at: ts,
        }
    }
}

struct ProfileState {
    profiles: HashMap<String, UserProfile>,
    active:   String,
}

static STATE: Lazy<Mutex<ProfileState>> = Lazy::new(|| Mutex::new(ProfileState {
    profiles: {
        let mut m = HashMap::new();
        m.insert("Default".to_string(), UserProfile::default_profile());
        m
    },
    active: "Default".to_string(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn profile_dir() -> std::path::PathBuf {
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("jarvis").join("profiles")
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn get_active() -> UserProfile {
    let s = STATE.lock().unwrap();
    s.profiles.get(&s.active).cloned().unwrap_or_else(UserProfile::default_profile)
}

pub fn get_profile(name: &str) -> Option<UserProfile> {
    STATE.lock().unwrap().profiles.get(name).cloned()
}

pub fn list_profiles() -> Vec<String> {
    STATE.lock().unwrap().profiles.keys().cloned().collect()
}

pub fn active_profile_name() -> String {
    STATE.lock().unwrap().active.clone()
}

pub fn switch_to(name: &str) -> bool {
    let mut s = STATE.lock().unwrap();
    if s.profiles.contains_key(name) {
        s.active = name.to_string();
        PROFILE_SWITCHES.fetch_add(1, Ordering::Relaxed);
        true
    } else {
        false
    }
}

pub fn create_profile(name: &str) -> UserProfile {
    let ts = ts_now();
    let profile = UserProfile {
        name: name.to_string(),
        created_at: ts,
        updated_at: ts,
        ..UserProfile::default_profile()
    };
    STATE.lock().unwrap().profiles.insert(name.to_string(), profile.clone());
    profile
}

pub fn save_profile(profile: &UserProfile) {
    let dir = profile_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(toml) = serde_json::to_string_pretty(profile) {
        let path = dir.join(format!("{}.json", profile.name));
        let _ = std::fs::write(path, toml);
    }
    let mut s = STATE.lock().unwrap();
    s.profiles.insert(profile.name.clone(), profile.clone());
    PROFILES_SAVED.fetch_add(1, Ordering::Relaxed);
}

pub fn load_profiles() {
    let dir = profile_dir();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if let Ok(profile) = serde_json::from_str::<UserProfile>(&content) {
                    STATE.lock().unwrap().profiles.insert(profile.name.clone(), profile);
                    PROFILES_LOADED.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

// Privacy enforcement: telemetry and share_diagnostics are always false
pub fn enforce_privacy(profile: &mut UserProfile) {
    profile.privacy.telemetry         = false;
    profile.privacy.local_only        = true;
    profile.privacy.share_diagnostics = false;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_exists() {
        let p = get_active();
        assert_eq!(p.name, "Default");
    }

    #[test]
    fn privacy_always_local() {
        let p = get_active();
        assert!(!p.privacy.telemetry);
        assert!(p.privacy.local_only);
        assert!(!p.privacy.share_diagnostics);
    }

    #[test]
    fn create_and_switch_profile() {
        create_profile("WorkProfile");
        assert!(switch_to("WorkProfile"));
        assert_eq!(active_profile_name(), "WorkProfile");
        switch_to("Default");
    }

    #[test]
    fn list_profiles_non_empty() {
        assert!(!list_profiles().is_empty());
    }

    #[test]
    fn enforce_privacy_blocks_telemetry() {
        let mut p = UserProfile::default_profile();
        p.privacy.telemetry = true;
        enforce_privacy(&mut p);
        assert!(!p.privacy.telemetry);
    }
}
