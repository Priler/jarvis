//! Hierarchical memory — per-layer bounded memory stores.
//! Each layer has its own isolated memory with appropriate capacity.
//! Cross-layer reads are allowed; writes are layer-local.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use crate::cognition_layers::CognitionLayer;

pub static MEMORY_WRITES:   AtomicU64 = AtomicU64::new(0);
pub static MEMORY_READS:    AtomicU64 = AtomicU64::new(0);
pub static MEMORY_EVICTIONS: AtomicU64 = AtomicU64::new(0);

// Per-layer capacity limits
fn layer_capacity(layer: CognitionLayer) -> usize {
    match layer {
        CognitionLayer::Reactive    =>  50,   // small, fast, interrupt-focused
        CognitionLayer::Tactical    => 200,   // active workflows
        CognitionLayer::Strategic   => 150,   // long-horizon plans
        CognitionLayer::Meta        => 100,   // cognition evaluation history
        CognitionLayer::Supervisory =>  80,   // global runtime state
    }
}

// ── Memory entry ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    pub key:        String,
    pub value:      String,
    pub confidence: f32,
    pub layer:      CognitionLayer,
    pub ts_ms:      u64,
}

// ── Layer store ───────────────────────────────────────────────────────────────

struct LayerStore {
    entries:  Vec<MemoryEntry>,
    index:    HashMap<String, usize>,   // key → index in entries
    capacity: usize,
}

impl LayerStore {
    fn new(capacity: usize) -> Self {
        Self { entries: Vec::with_capacity(capacity), index: HashMap::new(), capacity }
    }

    fn write(&mut self, entry: MemoryEntry) {
        if let Some(&idx) = self.index.get(&entry.key) {
            self.entries[idx] = entry;
        } else {
            if self.entries.len() >= self.capacity {
                // Evict oldest entry
                if let Some(oldest) = self.entries.first().map(|e| e.key.clone()) {
                    self.index.remove(&oldest);
                    self.entries.remove(0);
                    // Rebuild index after removal
                    self.index.clear();
                    for (i, e) in self.entries.iter().enumerate() {
                        self.index.insert(e.key.clone(), i);
                    }
                }
                MEMORY_EVICTIONS.fetch_add(1, Ordering::Relaxed);
            }
            let idx = self.entries.len();
            self.index.insert(entry.key.clone(), idx);
            self.entries.push(entry);
        }
    }

    fn read(&self, key: &str) -> Option<&MemoryEntry> {
        self.index.get(key).and_then(|&i| self.entries.get(i))
    }

    fn snapshot(&self) -> Vec<MemoryEntry> {
        self.entries.clone()
    }

    fn len(&self) -> usize { self.entries.len() }
}

// ── Global state ──────────────────────────────────────────────────────────────

struct HierarchicalMemoryState {
    layers: HashMap<u8, LayerStore>,   // CognitionLayer as u8 → store
}

impl HierarchicalMemoryState {
    fn new() -> Self {
        let mut layers = HashMap::new();
        for layer in CognitionLayer::all() {
            layers.insert(layer as u8, LayerStore::new(layer_capacity(layer)));
        }
        Self { layers }
    }

    fn store_mut(&mut self, layer: CognitionLayer) -> Option<&mut LayerStore> {
        self.layers.get_mut(&(layer as u8))
    }

    fn store(&self, layer: CognitionLayer) -> Option<&LayerStore> {
        self.layers.get(&(layer as u8))
    }
}

static STATE: Lazy<Mutex<HierarchicalMemoryState>> =
    Lazy::new(|| Mutex::new(HierarchicalMemoryState::new()));

// ── Public API ────────────────────────────────────────────────────────────────

/// Write a key-value pair to a layer's memory.
pub fn write(layer: CognitionLayer, key: impl Into<String>, value: impl Into<String>, confidence: f32) {
    MEMORY_WRITES.fetch_add(1, Ordering::Relaxed);
    let entry = MemoryEntry {
        key: key.into(), value: value.into(), confidence: confidence.clamp(0.0, 1.0),
        layer, ts_ms: ts_now(),
    };
    if let Ok(mut s) = STATE.lock() {
        if let Some(store) = s.store_mut(layer) {
            store.write(entry);
        }
    }
}

/// Read from a specific layer's memory.
pub fn read(layer: CognitionLayer, key: &str) -> Option<MemoryEntry> {
    MEMORY_READS.fetch_add(1, Ordering::Relaxed);
    STATE.lock().ok().and_then(|s| s.store(layer)?.read(key).cloned())
}

/// Read across all layers (highest-confidence wins when key conflicts).
pub fn read_cross_layer(key: &str) -> Option<MemoryEntry> {
    MEMORY_READS.fetch_add(1, Ordering::Relaxed);
    STATE.lock().ok().and_then(|s| {
        CognitionLayer::all().iter()
            .filter_map(|&layer| s.store(layer)?.read(key).cloned())
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
    })
}

/// Snapshot all entries for a layer.
pub fn snapshot(layer: CognitionLayer) -> Vec<MemoryEntry> {
    STATE.lock().map(|s| s.store(layer).map(|st| st.snapshot()).unwrap_or_default())
        .unwrap_or_default()
}

/// Entry count per layer.
pub fn size(layer: CognitionLayer) -> usize {
    STATE.lock().map(|s| s.store(layer).map(|st| st.len()).unwrap_or(0)).unwrap_or(0)
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read_same_layer() {
        write(CognitionLayer::Tactical, "wf:test", "active", 0.9);
        let entry = read(CognitionLayer::Tactical, "wf:test");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().value, "active");
    }

    #[test]
    fn cross_layer_read_returns_highest_confidence() {
        write(CognitionLayer::Strategic, "goal:main", "plan_v1", 0.7);
        write(CognitionLayer::Supervisory, "goal:main", "plan_v2", 0.9);
        let entry = read_cross_layer("goal:main");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().confidence, 0.9);
    }

    #[test]
    fn missing_key_returns_none() {
        let entry = read(CognitionLayer::Reactive, "nonexistent_key_xyz");
        assert!(entry.is_none());
    }

    #[test]
    fn write_counter_increments() {
        let before = MEMORY_WRITES.load(Ordering::Relaxed);
        write(CognitionLayer::Meta, "test_key", "val", 0.5);
        assert!(MEMORY_WRITES.load(Ordering::Relaxed) > before);
    }
}
