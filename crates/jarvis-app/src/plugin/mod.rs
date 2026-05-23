#![allow(dead_code)]

use std::collections::HashMap;
use crate::bus::BusEvent;

// ── Manifest ──────────────────────────────────────────────────────────────────

pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub required_permissions: Vec<String>,
}

// ── Plugin trait ──────────────────────────────────────────────────────────────

/// A plugin extends the platform with additional agents, tools, or workflows.
///
/// Safety contract: plugins MUST NOT directly execute commands, modify runtime state,
/// or access memory outside their declared scope. All privileged actions go through
/// the governance layer.
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;
    fn on_load(&mut self) -> Result<(), String>;
    fn on_unload(&mut self);
    fn on_bus_event(&mut self, _event: &BusEvent) {}
}

// ── Plugin registry ───────────────────────────────────────────────────────────

pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self { plugins: HashMap::new() }
    }

    pub fn register(&mut self, mut plugin: Box<dyn Plugin>) -> Result<(), String> {
        let id = plugin.manifest().id.clone();
        if self.plugins.contains_key(&id) {
            return Err(format!("Plugin '{}' already registered", id));
        }
        plugin.on_load().map_err(|e| format!("Plugin '{}' load failed: {}", id, e))?;
        info!(
            "[PLUGINS] Loaded '{}' v{} — {}",
            plugin.manifest().name,
            plugin.manifest().version,
            plugin.manifest().description,
        );
        self.plugins.insert(id, plugin);
        Ok(())
    }

    pub fn unload(&mut self, id: &str) {
        if let Some(mut p) = self.plugins.remove(id) {
            p.on_unload();
            info!("[PLUGINS] Unloaded plugin '{}'", id);
        }
    }

    pub fn dispatch(&mut self, event: &BusEvent) {
        for plugin in self.plugins.values_mut() {
            plugin.on_bus_event(event);
        }
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }
}
