//! Community profiles — shareable user configuration bundles, preset library,
//! profile import/export, and contribution tracking (offline-only, no network).
//! All operations are local; no external services contacted.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static PROFILES_IMPORTED:  AtomicU64 = AtomicU64::new(0);
pub static PROFILES_EXPORTED:  AtomicU64 = AtomicU64::new(0);
pub static PROFILES_ACTIVATED: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum ProfileKind {
    UserPreset,
    CommunityPreset,
    RolePreset,
    WorkflowPreset,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProfileSettings {
    pub performance_mode:   String,
    pub voice_enabled:      bool,
    pub voice_sensitivity:  f32,
    pub hotkeys:            Vec<(String, String)>, // (combo, action)
    pub tray_icon:          String,
    pub memory_max_entries: usize,
    pub safe_mode_policy:   String,
    pub workflow_kind:      String,
}

impl Default for ProfileSettings {
    fn default() -> Self {
        Self {
            performance_mode:   "Balanced".to_string(),
            voice_enabled:      true,
            voice_sensitivity:  0.7,
            hotkeys:            vec![
                ("Ctrl+Shift+J".to_string(), "wake_jarvis".to_string()),
            ],
            tray_icon:          "◈".to_string(),
            memory_max_entries: 10_000,
            safe_mode_policy:   "auto".to_string(),
            workflow_kind:      "DesktopAssistant".to_string(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommunityProfile {
    pub id:          String,
    pub name:        String,
    pub description: String,
    pub kind:        ProfileKind,
    pub author:      String,
    pub tags:        Vec<String>,
    pub settings:    ProfileSettings,
    pub active:      bool,
    pub created_at:  u64,
}

struct ProfileState {
    profiles: Vec<CommunityProfile>,
    active_id: Option<String>,
}

impl ProfileState {
    fn new() -> Self {
        Self {
            profiles: vec![
                CommunityProfile {
                    id:          "developer-default".to_string(),
                    name:        "Developer".to_string(),
                    description: "Optimized for software development — max performance, code-aware focus".to_string(),
                    kind:        ProfileKind::RolePreset,
                    author:      "jarvis-team".to_string(),
                    tags:        vec!["dev".to_string(), "code".to_string(), "performance".to_string()],
                    settings:    ProfileSettings {
                        performance_mode: "Performance".to_string(),
                        voice_enabled:    false,
                        workflow_kind:    "Developer".to_string(),
                        ..Default::default()
                    },
                    active:     false,
                    created_at: 1_716_000_000,
                },
                CommunityProfile {
                    id:          "researcher-default".to_string(),
                    name:        "Researcher".to_string(),
                    description: "Deep reasoning mode with high memory capacity".to_string(),
                    kind:        ProfileKind::RolePreset,
                    author:      "jarvis-team".to_string(),
                    tags:        vec!["research".to_string(), "reasoning".to_string(), "memory".to_string()],
                    settings:    ProfileSettings {
                        performance_mode:   "Reasoning".to_string(),
                        memory_max_entries: 50_000,
                        workflow_kind:      "Research".to_string(),
                        ..Default::default()
                    },
                    active:     false,
                    created_at: 1_716_000_100,
                },
                CommunityProfile {
                    id:          "voice-first".to_string(),
                    name:        "Voice First".to_string(),
                    description: "Hands-free operation — high voice sensitivity, low-latency response".to_string(),
                    kind:        ProfileKind::RolePreset,
                    author:      "jarvis-team".to_string(),
                    tags:        vec!["voice".to_string(), "hands-free".to_string(), "accessibility".to_string()],
                    settings:    ProfileSettings {
                        performance_mode:  "VoicePriority".to_string(),
                        voice_sensitivity: 0.5,
                        workflow_kind:     "Meeting".to_string(),
                        ..Default::default()
                    },
                    active:     false,
                    created_at: 1_716_000_200,
                },
                CommunityProfile {
                    id:          "minimal-privacy".to_string(),
                    name:        "Minimal Privacy".to_string(),
                    description: "Minimal memory footprint, no voice, safe-mode auto-on policy".to_string(),
                    kind:        ProfileKind::UserPreset,
                    author:      "jarvis-team".to_string(),
                    tags:        vec!["privacy".to_string(), "minimal".to_string(), "safe".to_string()],
                    settings:    ProfileSettings {
                        performance_mode:   "Eco".to_string(),
                        voice_enabled:      false,
                        memory_max_entries: 1_000,
                        safe_mode_policy:   "always".to_string(),
                        ..Default::default()
                    },
                    active:     false,
                    created_at: 1_716_000_300,
                },
            ],
            active_id: None,
        }
    }
}

static STATE: Lazy<Mutex<ProfileState>> = Lazy::new(|| Mutex::new(ProfileState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Browse ────────────────────────────────────────────────────────────────────

pub fn list_all() -> Vec<CommunityProfile> {
    STATE.lock().unwrap().profiles.clone()
}

pub fn by_kind(kind: &ProfileKind) -> Vec<CommunityProfile> {
    STATE.lock().unwrap().profiles.iter()
        .filter(|p| &p.kind == kind)
        .cloned()
        .collect()
}

pub fn search(query: &str) -> Vec<CommunityProfile> {
    let q = query.to_lowercase();
    STATE.lock().unwrap().profiles.iter()
        .filter(|p| {
            p.name.to_lowercase().contains(&q)
                || p.description.to_lowercase().contains(&q)
                || p.tags.iter().any(|t| t.contains(&q))
        })
        .cloned()
        .collect()
}

pub fn get(id: &str) -> Option<CommunityProfile> {
    STATE.lock().unwrap().profiles.iter().find(|p| p.id == id).cloned()
}

// ── Activate / deactivate ─────────────────────────────────────────────────────

pub fn activate(id: &str) -> bool {
    let mut s = STATE.lock().unwrap();
    for p in s.profiles.iter_mut() { p.active = false; }
    if let Some(p) = s.profiles.iter_mut().find(|p| p.id == id) {
        p.active = true;
        s.active_id = Some(id.to_string());
        PROFILES_ACTIVATED.fetch_add(1, Ordering::Relaxed);
        // Apply settings (fire-and-forget to avoid deadlock — real impl would post message)
        crate::production_logging::info("community_profiles",
            &format!("profile activated: {}", id));
        return true;
    }
    false
}

pub fn deactivate_all() {
    let mut s = STATE.lock().unwrap();
    for p in s.profiles.iter_mut() { p.active = false; }
    s.active_id = None;
}

pub fn active_profile() -> Option<CommunityProfile> {
    let s = STATE.lock().unwrap();
    s.active_id.as_ref().and_then(|id| s.profiles.iter().find(|p| &p.id == id).cloned())
}

// ── Import / export ───────────────────────────────────────────────────────────

pub fn import(profile: CommunityProfile) -> bool {
    let mut s = STATE.lock().unwrap();
    if s.profiles.iter().any(|p| p.id == profile.id) { return false; }
    s.profiles.push(profile);
    PROFILES_IMPORTED.fetch_add(1, Ordering::Relaxed);
    true
}

pub fn export(id: &str) -> Option<String> {
    let s = STATE.lock().unwrap();
    let result = s.profiles.iter().find(|p| p.id == id)
        .map(|p| serde_json::to_string(p).unwrap_or_default());
    if result.is_some() { PROFILES_EXPORTED.fetch_add(1, Ordering::Relaxed); }
    result
}

pub fn delete(id: &str) -> bool {
    let mut s = STATE.lock().unwrap();
    let before = s.profiles.len();
    s.profiles.retain(|p| p.id != id);
    s.profiles.len() < before
}

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct ProfilesSnapshot {
    pub profiles_imported:  u64,
    pub profiles_exported:  u64,
    pub profiles_activated: u64,
    pub total_profiles:     usize,
    pub active_profile_id:  Option<String>,
}

pub fn snapshot() -> ProfilesSnapshot {
    let s = STATE.lock().unwrap();
    ProfilesSnapshot {
        profiles_imported:  PROFILES_IMPORTED.load(Ordering::Relaxed),
        profiles_exported:  PROFILES_EXPORTED.load(Ordering::Relaxed),
        profiles_activated: PROFILES_ACTIVATED.load(Ordering::Relaxed),
        total_profiles:     s.profiles.len(),
        active_profile_id:  s.active_id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[test]
    fn list_includes_presets() {
        assert!(list_all().len() >= 4);
    }

    #[test]
    fn search_finds_researcher() {
        let results = search("reasoning");
        assert!(results.iter().any(|p| p.id == "researcher-default"));
    }

    #[test]
    fn activate_profile() {
        let _g = TEST_LOCK.lock().unwrap();
        let before = PROFILES_ACTIVATED.load(Ordering::Relaxed);
        assert!(activate("developer-default"));
        assert!(PROFILES_ACTIVATED.load(Ordering::Relaxed) > before);
        let active = active_profile();
        assert!(active.is_some());
        assert_eq!(active.unwrap().id, "developer-default");
        deactivate_all();
    }

    #[test]
    fn import_and_export() {
        let _g = TEST_LOCK.lock().unwrap();
        let p = CommunityProfile {
            id:          "test-import-profile".to_string(),
            name:        "Test Import".to_string(),
            description: "testing".to_string(),
            kind:        ProfileKind::UserPreset,
            author:      "test".to_string(),
            tags:        vec![],
            settings:    ProfileSettings::default(),
            active:      false,
            created_at:  ts_now(),
        };
        let before_import = PROFILES_IMPORTED.load(Ordering::Relaxed);
        assert!(import(p));
        assert!(PROFILES_IMPORTED.load(Ordering::Relaxed) > before_import);
        let json = export("test-import-profile");
        assert!(json.is_some());
        assert!(PROFILES_EXPORTED.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn snapshot_no_panic() {
        let _g = TEST_LOCK.lock().unwrap();
        let s = snapshot();
        assert!(s.total_profiles > 0);
    }
}
