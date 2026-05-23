//! Plugin runtime — local plugin registration, sandboxed execution dispatch,
//! capability declarations, and plugin lifecycle management.
//! All operations are local; no external services contacted.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static PLUGINS_REGISTERED: AtomicU64 = AtomicU64::new(0);
pub static PLUGIN_CALLS:       AtomicU64 = AtomicU64::new(0);
pub static PLUGIN_ERRORS:      AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum PluginStatus { Active, Disabled, Error, Sandboxed }

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum Capability {
    ReadMemory,
    WriteMemory,
    ExecuteTools,
    AccessVoice,
    ReadScreen,
    NetworkAccess, // always denied in offline build
    FileRead,
    FileWrite,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginManifest {
    pub id:           String,
    pub name:         String,
    pub version:      String,
    pub author:       String,
    pub description:  String,
    pub capabilities: Vec<Capability>,
    pub entry_point:  String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Plugin {
    pub manifest:     PluginManifest,
    pub status:       PluginStatus,
    pub call_count:   u64,
    pub error_count:  u64,
    pub registered_at: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginCallResult {
    pub plugin_id: String,
    pub success:   bool,
    pub output:    Option<String>,
    pub error:     Option<String>,
}

const PLUGIN_CALL_LOG_MAX: usize = 200;

struct PluginState {
    plugins:  Vec<Plugin>,
    call_log: Vec<PluginCallResult>,
}

impl PluginState {
    fn new() -> Self { Self { plugins: Vec::new(), call_log: Vec::new() } }
}

static STATE: Lazy<Mutex<PluginState>> = Lazy::new(|| Mutex::new(PluginState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Registration ──────────────────────────────────────────────────────────────

pub fn register(manifest: PluginManifest) -> bool {
    let mut s = STATE.lock().unwrap();
    if s.plugins.iter().any(|p| p.manifest.id == manifest.id) { return false; }
    // Deny NetworkAccess in offline build
    let denied_caps: Vec<_> = manifest.capabilities.iter()
        .filter(|c| **c == Capability::NetworkAccess)
        .collect();
    let status = if denied_caps.is_empty() { PluginStatus::Active } else { PluginStatus::Sandboxed };
    crate::production_logging::info("plugin_runtime",
        &format!("plugin registered: {} v{}", &manifest.name, &manifest.version));
    s.plugins.push(Plugin {
        manifest,
        status,
        call_count: 0,
        error_count: 0,
        registered_at: ts_now(),
    });
    PLUGINS_REGISTERED.fetch_add(1, Ordering::Relaxed);
    true
}

pub fn unregister(plugin_id: &str) -> bool {
    let mut s = STATE.lock().unwrap();
    let before = s.plugins.len();
    s.plugins.retain(|p| p.manifest.id != plugin_id);
    s.plugins.len() < before
}

pub fn enable(plugin_id: &str) -> bool {
    let mut s = STATE.lock().unwrap();
    if let Some(p) = s.plugins.iter_mut().find(|p| p.manifest.id == plugin_id) {
        if p.status == PluginStatus::Disabled { p.status = PluginStatus::Active; return true; }
    }
    false
}

pub fn disable(plugin_id: &str) -> bool {
    let mut s = STATE.lock().unwrap();
    if let Some(p) = s.plugins.iter_mut().find(|p| p.manifest.id == plugin_id) {
        p.status = PluginStatus::Disabled;
        return true;
    }
    false
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub fn call(plugin_id: &str, method: &str, _args: &str) -> PluginCallResult {
    PLUGIN_CALLS.fetch_add(1, Ordering::Relaxed);
    let mut s = STATE.lock().unwrap();

    let plugin = s.plugins.iter_mut().find(|p| p.manifest.id == plugin_id);
    let result = match plugin {
        None => PluginCallResult {
            plugin_id: plugin_id.to_string(),
            success:   false,
            output:    None,
            error:     Some(format!("plugin not found: {}", plugin_id)),
        },
        Some(p) if p.status == PluginStatus::Disabled => {
            PluginCallResult {
                plugin_id: plugin_id.to_string(),
                success:   false,
                output:    None,
                error:     Some("plugin is disabled".to_string()),
            }
        }
        Some(p) => {
            p.call_count += 1;
            // Stub dispatch — real impl would invoke WASM sandbox or named pipe
            PluginCallResult {
                plugin_id: plugin_id.to_string(),
                success:   true,
                output:    Some(format!("{}::{} ok", plugin_id, method)),
                error:     None,
            }
        }
    };

    if !result.success { PLUGIN_ERRORS.fetch_add(1, Ordering::Relaxed); }
    if s.call_log.len() >= PLUGIN_CALL_LOG_MAX { s.call_log.remove(0); }
    s.call_log.push(result.clone());
    result
}

// ── Query ─────────────────────────────────────────────────────────────────────

pub fn list_plugins() -> Vec<Plugin> { STATE.lock().unwrap().plugins.clone() }

pub fn active_plugins() -> Vec<Plugin> {
    STATE.lock().unwrap().plugins.iter()
        .filter(|p| p.status == PluginStatus::Active)
        .cloned()
        .collect()
}

pub fn plugin_by_id(id: &str) -> Option<Plugin> {
    STATE.lock().unwrap().plugins.iter().find(|p| p.manifest.id == id).cloned()
}

pub fn has_capability(plugin_id: &str, cap: &Capability) -> bool {
    STATE.lock().unwrap().plugins.iter()
        .find(|p| p.manifest.id == plugin_id)
        .map(|p| p.manifest.capabilities.contains(cap))
        .unwrap_or(false)
}

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct PluginSnapshot {
    pub plugins_registered: u64,
    pub plugin_calls:       u64,
    pub plugin_errors:      u64,
    pub active_count:       usize,
    pub disabled_count:     usize,
}

pub fn snapshot() -> PluginSnapshot {
    let s = STATE.lock().unwrap();
    PluginSnapshot {
        plugins_registered: PLUGINS_REGISTERED.load(Ordering::Relaxed),
        plugin_calls:       PLUGIN_CALLS.load(Ordering::Relaxed),
        plugin_errors:      PLUGIN_ERRORS.load(Ordering::Relaxed),
        active_count:       s.plugins.iter().filter(|p| p.status == PluginStatus::Active).count(),
        disabled_count:     s.plugins.iter().filter(|p| p.status == PluginStatus::Disabled).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id:           id.to_string(),
            name:         format!("Plugin {}", id),
            version:      "1.0.0".to_string(),
            author:       "test".to_string(),
            description:  "test plugin".to_string(),
            capabilities: vec![Capability::ReadMemory],
            entry_point:  "main".to_string(),
        }
    }

    #[test]
    fn register_plugin() {
        let before = PLUGINS_REGISTERED.load(Ordering::Relaxed);
        assert!(register(sample_manifest("test-plugin-alpha")));
        assert!(PLUGINS_REGISTERED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn duplicate_register_fails() {
        register(sample_manifest("test-plugin-dup"));
        assert!(!register(sample_manifest("test-plugin-dup")));
    }

    #[test]
    fn call_active_plugin_succeeds() {
        register(sample_manifest("test-plugin-call"));
        let result = call("test-plugin-call", "run", "{}");
        assert!(result.success);
    }

    #[test]
    fn call_unknown_plugin_fails() {
        let result = call("nonexistent-xyz", "run", "{}");
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn disable_prevents_call() {
        register(sample_manifest("test-plugin-disable"));
        disable("test-plugin-disable");
        let result = call("test-plugin-disable", "run", "{}");
        assert!(!result.success);
    }

    #[test]
    fn network_access_plugin_sandboxed() {
        let mut m = sample_manifest("test-plugin-net");
        m.capabilities.push(Capability::NetworkAccess);
        register(m);
        let p = plugin_by_id("test-plugin-net").unwrap();
        assert_eq!(p.status, PluginStatus::Sandboxed);
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        let _ = s.plugin_calls;
    }
}
