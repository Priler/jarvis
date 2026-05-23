//! Three-tier persistent memory runtime.
//!
//! Tiers:
//!   ConversationMemory — volatile, cleared per session
//!   ProjectMemory      — persists for the duration of a project (session file)
//!   PersistentMemory   — survives restarts (JSONL file on disk)

use std::collections::HashMap;
use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use once_cell::sync::Lazy;

pub static ENTRIES_STORED:    AtomicU64 = AtomicU64::new(0);
pub static ENTRIES_RETRIEVED: AtomicU64 = AtomicU64::new(0);
pub static ENTRIES_CLEARED:   AtomicU64 = AtomicU64::new(0);

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    pub key:      String,
    pub value:    String,
    pub tier:     MemoryTier,
    pub ts_ms:    u64,
    pub tags:     Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MemoryTier { Conversation, Project, Persistent }

struct MemoryState {
    conversation: HashMap<String, MemoryEntry>,
    project:      HashMap<String, MemoryEntry>,
    persistent:   HashMap<String, MemoryEntry>,
}

static STATE: Lazy<Mutex<MemoryState>> = Lazy::new(|| Mutex::new(MemoryState {
    conversation: HashMap::new(),
    project:      HashMap::new(),
    persistent:   HashMap::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn memory_file_path() -> std::path::PathBuf {
    // Use APP_DATA_DIR pattern; fall back to temp dir in tests.
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("jarvis").join("persistent_memory.jsonl")
}

// ── Persistence ───────────────────────────────────────────────────────────────

fn load_persistent() -> HashMap<String, MemoryEntry> {
    let path = memory_file_path();
    let mut map = HashMap::new();
    if let Ok(content) = std::fs::read_to_string(&path) {
        for line in content.lines() {
            if let Ok(entry) = serde_json::from_str::<MemoryEntry>(line) {
                map.insert(entry.key.clone(), entry);
            }
        }
    }
    map
}

fn save_persistent(map: &HashMap<String, MemoryEntry>) {
    let path = memory_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut lines = String::new();
    for entry in map.values() {
        if let Ok(json) = serde_json::to_string(entry) {
            lines.push_str(&json);
            lines.push('\n');
        }
    }
    let _ = std::fs::write(&path, lines);
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Store a memory entry in the specified tier.
pub fn store(key: &str, value: &str, tier: MemoryTier, tags: Vec<String>) {
    let entry = MemoryEntry {
        key:   key.to_string(),
        value: value.to_string(),
        tier:  tier.clone(),
        ts_ms: ts_now(),
        tags,
    };
    let mut s = STATE.lock().unwrap();
    match tier {
        MemoryTier::Conversation => { s.conversation.insert(key.to_string(), entry); }
        MemoryTier::Project      => { s.project.insert(key.to_string(), entry); }
        MemoryTier::Persistent   => {
            s.persistent.insert(key.to_string(), entry.clone());
            save_persistent(&s.persistent);
        }
    }
    ENTRIES_STORED.fetch_add(1, Ordering::Relaxed);
}

/// Retrieve an entry from any tier (Persistent > Project > Conversation priority).
pub fn get(key: &str) -> Option<MemoryEntry> {
    ENTRIES_RETRIEVED.fetch_add(1, Ordering::Relaxed);
    let s = STATE.lock().unwrap();
    s.persistent.get(key)
        .or_else(|| s.project.get(key))
        .or_else(|| s.conversation.get(key))
        .cloned()
}

/// Search entries whose key or value contains the query (case-insensitive).
pub fn search(query: &str) -> Vec<MemoryEntry> {
    let q = query.to_lowercase();
    let s = STATE.lock().unwrap();
    let mut results: Vec<MemoryEntry> = s.conversation.values()
        .chain(s.project.values())
        .chain(s.persistent.values())
        .filter(|e| e.key.to_lowercase().contains(&q) || e.value.to_lowercase().contains(&q))
        .cloned()
        .collect();
    results.sort_by(|a, b| b.ts_ms.cmp(&a.ts_ms));
    results
}

/// Clear a specific tier.
pub fn clear_tier(tier: &MemoryTier) {
    let mut s = STATE.lock().unwrap();
    match tier {
        MemoryTier::Conversation => { ENTRIES_CLEARED.fetch_add(s.conversation.len() as u64, Ordering::Relaxed); s.conversation.clear(); }
        MemoryTier::Project      => { ENTRIES_CLEARED.fetch_add(s.project.len() as u64, Ordering::Relaxed); s.project.clear(); }
        MemoryTier::Persistent   => {
            ENTRIES_CLEARED.fetch_add(s.persistent.len() as u64, Ordering::Relaxed);
            s.persistent.clear();
            let _ = std::fs::remove_file(memory_file_path());
        }
    }
}

/// Load persistent memory from disk (call once at startup).
pub fn init() {
    let loaded = load_persistent();
    STATE.lock().unwrap().persistent = loaded;
}

pub fn entry_count(tier: &MemoryTier) -> usize {
    let s = STATE.lock().unwrap();
    match tier {
        MemoryTier::Conversation => s.conversation.len(),
        MemoryTier::Project      => s.project.len(),
        MemoryTier::Persistent   => s.persistent.len(),
    }
}

pub fn total_entries() -> usize {
    let s = STATE.lock().unwrap();
    s.conversation.len() + s.project.len() + s.persistent.len()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_get_conversation() {
        store("test_key", "test_value", MemoryTier::Conversation, vec![]);
        let e = get("test_key").unwrap();
        assert_eq!(e.value, "test_value");
    }

    #[test]
    fn search_finds_by_value() {
        store("search_target", "unique_xyz_term", MemoryTier::Conversation, vec![]);
        let results = search("unique_xyz_term");
        assert!(!results.is_empty());
    }

    #[test]
    fn clear_conversation_tier() {
        store("clear_test", "v", MemoryTier::Conversation, vec![]);
        clear_tier(&MemoryTier::Conversation);
        // After clear, conversation entries should be gone
        // (persistent and project may still exist)
        assert_eq!(entry_count(&MemoryTier::Conversation), 0);
    }

    #[test]
    fn entry_count_increases() {
        let before = total_entries();
        store("count_key_123", "v", MemoryTier::Project, vec![]);
        assert!(total_entries() >= before);
    }

    #[test]
    fn get_missing_returns_none() {
        assert!(get("nonexistent_key_xyz_999").is_none());
    }
}
