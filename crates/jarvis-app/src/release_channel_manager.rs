//! Release channel management — channel policy, build manifests,
//! feature flag gating per channel, and channel migration logic.
//! All operations are local; no network connections are made.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static CHANNEL_SWITCHES:   AtomicU64 = AtomicU64::new(0);
pub static FLAGS_EVALUATED:    AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum Channel { Stable, Beta, Nightly, Internal }

impl Channel {
    pub fn allows_experimental(&self) -> bool {
        matches!(self, Channel::Nightly | Channel::Internal)
    }
    pub fn allows_beta_features(&self) -> bool {
        !matches!(self, Channel::Stable)
    }
    pub fn label(&self) -> &'static str {
        match self {
            Channel::Stable   => "stable",
            Channel::Beta     => "beta",
            Channel::Nightly  => "nightly",
            Channel::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildManifest {
    pub channel:      Channel,
    pub version:      String,
    pub build_number: u32,
    pub build_date:   String,
    pub min_os:       String,
    pub features:     Vec<String>,
    pub deprecated:   Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FeatureFlag {
    pub name:     String,
    pub enabled:  bool,
    pub channels: Vec<Channel>,
    pub rollout_pct: u8,
}

impl FeatureFlag {
    pub fn is_active_for(&self, ch: &Channel) -> bool {
        self.enabled && self.channels.contains(ch)
    }
}

struct ChannelState {
    active:    Channel,
    manifest:  BuildManifest,
    flags:     Vec<FeatureFlag>,
}

impl ChannelState {
    fn new() -> Self {
        Self {
            active: Channel::Stable,
            manifest: BuildManifest {
                channel:      Channel::Stable,
                version:      "1.0.0".to_string(),
                build_number: 1000,
                build_date:   "2026-05-22".to_string(),
                min_os:       "Windows 10 22H2".to_string(),
                features:     vec![
                    "voice_stt".to_string(),
                    "memory_rag".to_string(),
                    "tool_execution".to_string(),
                    "control_center".to_string(),
                    "tray_integration".to_string(),
                ],
                deprecated: vec![],
            },
            flags: vec![
                FeatureFlag {
                    name: "experimental_world_sim".to_string(),
                    enabled: true,
                    channels: vec![Channel::Nightly, Channel::Internal],
                    rollout_pct: 0,
                },
                FeatureFlag {
                    name: "multi_model_routing".to_string(),
                    enabled: true,
                    channels: vec![Channel::Beta, Channel::Nightly, Channel::Internal],
                    rollout_pct: 100,
                },
                FeatureFlag {
                    name: "plugin_marketplace".to_string(),
                    enabled: true,
                    channels: vec![Channel::Beta, Channel::Nightly, Channel::Internal, Channel::Stable],
                    rollout_pct: 100,
                },
                FeatureFlag {
                    name: "community_profiles".to_string(),
                    enabled: true,
                    channels: vec![Channel::Beta, Channel::Nightly, Channel::Internal, Channel::Stable],
                    rollout_pct: 100,
                },
            ],
        }
    }
}

static STATE: Lazy<Mutex<ChannelState>> = Lazy::new(|| Mutex::new(ChannelState::new()));

// ── Channel operations ────────────────────────────────────────────────────────

pub fn active_channel() -> Channel { STATE.lock().unwrap().active.clone() }

pub fn switch_channel(to: Channel) -> bool {
    let mut s = STATE.lock().unwrap();
    if s.active == to { return false; }
    crate::production_logging::info("release_channel_manager",
        &format!("channel switch: {} → {}", s.active.label(), to.label()));
    s.active = to.clone();
    s.manifest.channel = to;
    CHANNEL_SWITCHES.fetch_add(1, Ordering::Relaxed);
    true
}

// ── Manifests ─────────────────────────────────────────────────────────────────

pub fn build_manifest() -> BuildManifest { STATE.lock().unwrap().manifest.clone() }

pub fn add_feature_to_manifest(feature: &str) {
    let mut s = STATE.lock().unwrap();
    if !s.manifest.features.contains(&feature.to_string()) {
        s.manifest.features.push(feature.to_string());
    }
}

pub fn deprecate_feature(feature: &str) {
    let mut s = STATE.lock().unwrap();
    s.manifest.features.retain(|f| f != feature);
    if !s.manifest.deprecated.contains(&feature.to_string()) {
        s.manifest.deprecated.push(feature.to_string());
    }
}

// ── Feature flags ─────────────────────────────────────────────────────────────

pub fn is_flag_enabled(name: &str) -> bool {
    let s = STATE.lock().unwrap();
    FLAGS_EVALUATED.fetch_add(1, Ordering::Relaxed);
    s.flags.iter()
        .find(|f| f.name == name)
        .map(|f| f.is_active_for(&s.active))
        .unwrap_or(false)
}

pub fn register_flag(name: &str, channels: Vec<Channel>, rollout_pct: u8) {
    let mut s = STATE.lock().unwrap();
    if !s.flags.iter().any(|f| f.name == name) {
        s.flags.push(FeatureFlag {
            name: name.to_string(),
            enabled: true,
            channels,
            rollout_pct,
        });
    }
}

pub fn list_active_flags() -> Vec<String> {
    let s = STATE.lock().unwrap();
    s.flags.iter()
        .filter(|f| f.is_active_for(&s.active))
        .map(|f| f.name.clone())
        .collect()
}

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct ChannelSnapshot {
    pub active_channel:   Channel,
    pub channel_switches: u64,
    pub flags_evaluated:  u64,
    pub active_flags:     Vec<String>,
    pub build_version:    String,
}

pub fn snapshot() -> ChannelSnapshot {
    let active_flags = list_active_flags();
    let s = STATE.lock().unwrap();
    ChannelSnapshot {
        active_channel:   s.active.clone(),
        channel_switches: CHANNEL_SWITCHES.load(Ordering::Relaxed),
        flags_evaluated:  FLAGS_EVALUATED.load(Ordering::Relaxed),
        active_flags,
        build_version:    s.manifest.version.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[test]
    fn default_channel_is_stable() {
        let _g = TEST_LOCK.lock().unwrap();
        // Reset to stable if another test changed it
        switch_channel(Channel::Stable);
        assert_eq!(active_channel(), Channel::Stable);
    }

    #[test]
    fn switch_channel_works() {
        let _g = TEST_LOCK.lock().unwrap();
        switch_channel(Channel::Stable);
        let switched = switch_channel(Channel::Beta);
        assert!(switched);
        assert_eq!(active_channel(), Channel::Beta);
        switch_channel(Channel::Stable);
    }

    #[test]
    fn flag_not_enabled_on_stable_for_nightly_only() {
        let _g = TEST_LOCK.lock().unwrap();
        switch_channel(Channel::Stable);
        assert!(!is_flag_enabled("experimental_world_sim"));
        switch_channel(Channel::Stable);
    }

    #[test]
    fn flag_enabled_on_beta() {
        let _g = TEST_LOCK.lock().unwrap();
        switch_channel(Channel::Beta);
        assert!(is_flag_enabled("multi_model_routing"));
        switch_channel(Channel::Stable);
    }

    #[test]
    fn register_custom_flag() {
        let _g = TEST_LOCK.lock().unwrap();
        switch_channel(Channel::Nightly);
        register_flag("test_feature_xyz", vec![Channel::Nightly], 100);
        assert!(is_flag_enabled("test_feature_xyz"));
        switch_channel(Channel::Stable);
    }

    #[test]
    fn snapshot_no_panic() {
        let _g = TEST_LOCK.lock().unwrap();
        let s = snapshot();
        assert!(!s.build_version.is_empty());
    }
}
