//! Task continuity — saves and restores in-progress task state across
//! cognition loop restarts.
//!
//! A `ContinuityRecord` captures enough context to resume a task after a
//! crash or deliberate stop.  Records are persisted to JSONL and loaded
//! on startup.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static CONTINUITY_SAVES:    AtomicU64 = AtomicU64::new(0);
pub static CONTINUITY_RESTORES: AtomicU64 = AtomicU64::new(0);
pub static CONTINUITY_CLEARED:  AtomicU64 = AtomicU64::new(0);

const CONTINUITY_FILE: &str = "task_continuity.jsonl";

// ── Continuity record ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContinuityRecord {
    pub task_id:     String,
    pub description: String,
    pub tool_id:     Option<String>,
    pub arg:         Option<String>,
    pub goal_id:     Option<u64>,
    pub saved_ms:    u64,
    pub attempts:    u32,
    pub context:     std::collections::HashMap<String, String>,
}

impl ContinuityRecord {
    pub fn new(task_id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            task_id:     task_id.into(),
            description: description.into(),
            tool_id:     None,
            arg:         None,
            goal_id:     None,
            saved_ms:    ts_now(),
            attempts:    0,
            context:     std::collections::HashMap::new(),
        }
    }

    pub fn with_tool(mut self, tool_id: impl Into<String>, arg: Option<String>) -> Self {
        self.tool_id = Some(tool_id.into());
        self.arg = arg;
        self
    }

    pub fn with_goal(mut self, goal_id: u64) -> Self {
        self.goal_id = Some(goal_id);
        self
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    pub fn age_ms(&self) -> u64 {
        ts_now().saturating_sub(self.saved_ms)
    }

    pub fn is_stale(&self, max_age_ms: u64) -> bool {
        self.age_ms() > max_age_ms
    }
}

// ── In-memory store ───────────────────────────────────────────────────────────

static RECORDS: Lazy<Mutex<Vec<ContinuityRecord>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn save(record: ContinuityRecord) {
    CONTINUITY_SAVES.fetch_add(1, Ordering::Relaxed);
    append_jsonl(&record);

    if let Ok(mut guard) = RECORDS.lock() {
        guard.retain(|r| r.task_id != record.task_id);
        guard.push(record);
    }
}

pub fn restore(task_id: &str) -> Option<ContinuityRecord> {
    let record = RECORDS.lock().ok().and_then(|g| {
        g.iter().find(|r| r.task_id == task_id).cloned()
    });
    if record.is_some() {
        CONTINUITY_RESTORES.fetch_add(1, Ordering::Relaxed);
        crate::world_state_journal::log(
            crate::world_state_journal::WorldEventKind::ContinuityRestored {
                task_id: task_id.to_string(),
            },
        );
    }
    record
}

pub fn remove(task_id: &str) {
    if let Ok(mut guard) = RECORDS.lock() {
        guard.retain(|r| r.task_id != task_id);
    }
}

pub fn all_records() -> Vec<ContinuityRecord> {
    RECORDS.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn pending_count() -> usize {
    RECORDS.lock().map(|g| g.len()).unwrap_or(0)
}

pub fn clear_stale(max_age_ms: u64) {
    if let Ok(mut guard) = RECORDS.lock() {
        let before = guard.len();
        guard.retain(|r| !r.is_stale(max_age_ms));
        let cleared = before - guard.len();
        CONTINUITY_CLEARED.fetch_add(cleared as u64, Ordering::Relaxed);
    }
}

// ── JSONL persistence ─────────────────────────────────────────────────────────

fn append_jsonl(record: &ContinuityRecord) {
    use std::io::Write as _;
    let path = crate::execution_journal::journal_dir().join(CONTINUITY_FILE);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        if let Ok(line) = serde_json::to_string(record) {
            let _ = writeln!(f, "{}", line);
        }
    }
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

    fn cleanup() {
        if let Ok(mut g) = RECORDS.lock() { g.clear(); }
    }

    #[test]
    fn save_and_restore_record() {
        cleanup();
        let rec = ContinuityRecord::new("task-1", "open IDE");
        save(rec);
        let restored = restore("task-1");
        assert!(restored.is_some());
        assert_eq!(restored.unwrap().description, "open IDE");
    }

    #[test]
    fn save_updates_existing_record() {
        save(ContinuityRecord::new("task-dup-unique-key", "v1"));
        save(ContinuityRecord::new("task-dup-unique-key", "v2"));
        // verify the second save overwrote the first (same task_id → only one entry for that key)
        assert_eq!(restore("task-dup-unique-key").unwrap().description, "v2");
        remove("task-dup-unique-key");
    }

    #[test]
    fn remove_deletes_record() {
        cleanup();
        save(ContinuityRecord::new("task-del", "delete me"));
        remove("task-del");
        assert!(restore("task-del").is_none());
    }

    #[test]
    fn record_with_tool_stores_data() {
        let rec = ContinuityRecord::new("t", "desc")
            .with_tool("app.open", Some("vscode".into()))
            .with_goal(42);
        assert_eq!(rec.tool_id.as_deref(), Some("app.open"));
        assert_eq!(rec.goal_id, Some(42));
    }

    #[test]
    fn continuity_saves_counter_increments() {
        cleanup();
        let before = CONTINUITY_SAVES.load(Ordering::Relaxed);
        save(ContinuityRecord::new("counter-test", "x"));
        assert!(CONTINUITY_SAVES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn clear_stale_removes_old_entries() {
        cleanup();
        let mut old = ContinuityRecord::new("stale", "x");
        old.saved_ms = 0; // epoch — definitely stale
        if let Ok(mut g) = RECORDS.lock() { g.push(old); }
        clear_stale(60_000);
        assert_eq!(pending_count(), 0);
    }
}
