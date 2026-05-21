//! Execution journal — append-only audit log for autonomous execution events.
//!
//! Every plan creation, step execution, verification, rollback, recovery,
//! hallucination block, and confirmation request is written as a JSONL entry
//! to `execution_journal.jsonl`.
//!
//! In-memory: keeps the last N entries for `recent_entries()` queries.
//! On-disk: append-only JSONL for durability and post-incident analysis.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::VecDeque;

pub static JOURNAL_ENTRIES_WRITTEN: AtomicU64 = AtomicU64::new(0);

const JOURNAL_FILE: &str = "execution_journal.jsonl";
const MAX_IN_MEMORY: usize = 100;

// ── Entry kinds ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub enum JournalKind {
    PlanCreated      { goal: String, node_count: usize },
    StepStarted      { node_id: String, tool_id: String },
    StepCompleted    { node_id: String, latency_ms: u64 },
    StepFailed       { node_id: String, reason: String },
    Verification     { node_id: String, passed: bool, checks: Vec<String> },
    Rollback         { node_id: String, strategy: String },
    Recovery         { node_id: String, outcome: String },
    HallucinationBlock { tool_id: String, guard: String, reason: String },
    ConfirmationRequested { tool_id: String, question: String },
    PlanAborted      { goal: String, reason: String },
}

// ── Journal entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct JournalEntry {
    pub ts_ms: u64,
    pub seq:   u64,
    pub kind:  JournalKind,
}

// ── Ring buffer ───────────────────────────────────────────────────────────────

static RING: Lazy<Mutex<VecDeque<JournalEntry>>> =
    Lazy::new(|| Mutex::new(VecDeque::new()));
static SEQ: AtomicU64 = AtomicU64::new(0);

// ── Public API ────────────────────────────────────────────────────────────────

/// Log a journal entry.  Non-blocking — appends to ring and writes to file.
pub fn log(kind: JournalKind) {
    let entry = JournalEntry {
        ts_ms: now_ms(),
        seq:   SEQ.fetch_add(1, Ordering::Relaxed),
        kind,
    };

    // Append to in-memory ring.
    let mut ring = RING.lock();
    if ring.len() >= MAX_IN_MEMORY {
        ring.pop_front();
    }
    ring.push_back(entry.clone());
    drop(ring);

    // Persist to disk.
    if let Ok(line) = serde_json::to_string(&entry) {
        let _ = append_line(JOURNAL_FILE, &line);
    }
    JOURNAL_ENTRIES_WRITTEN.fetch_add(1, Ordering::Relaxed);
}

/// Return the last `n` entries from the in-memory ring (newest last).
pub fn recent_entries(n: usize) -> Vec<JournalEntry> {
    let ring = RING.lock();
    let skip = ring.len().saturating_sub(n);
    ring.iter().skip(skip).cloned().collect()
}

/// Total entries written since startup.
pub fn entry_count() -> u64 {
    SEQ.load(Ordering::Relaxed)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn append_line(path: &str, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true).append(true).open(path)?;
    writeln!(f, "{}", line)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_increments_entry_count() {
        let before = entry_count();
        log(JournalKind::PlanCreated { goal: "test".into(), node_count: 3 });
        assert!(entry_count() > before);
    }

    #[test]
    fn recent_entries_returns_logged_entry() {
        log(JournalKind::StepStarted { node_id: "n1".into(), tool_id: "app.open".into() });
        let entries = recent_entries(5);
        assert!(!entries.is_empty());
    }

    #[test]
    fn hallucination_block_logged() {
        log(JournalKind::HallucinationBlock {
            tool_id: "jarvis.hacked".into(),
            guard: "G5".into(),
            reason: "tool not in registry".into(),
        });
        let entries = recent_entries(10);
        let found = entries.iter().any(|e| matches!(&e.kind, JournalKind::HallucinationBlock { tool_id, .. } if tool_id == "jarvis.hacked"));
        assert!(found);
    }

    #[test]
    fn ring_does_not_grow_unbounded() {
        for i in 0..MAX_IN_MEMORY + 10 {
            log(JournalKind::StepCompleted { node_id: format!("n{}", i), latency_ms: 0 });
        }
        let entries = recent_entries(MAX_IN_MEMORY + 20);
        assert!(entries.len() <= MAX_IN_MEMORY);
    }

    #[test]
    fn journal_entries_written_counter_increments() {
        let before = JOURNAL_ENTRIES_WRITTEN.load(Ordering::Relaxed);
        log(JournalKind::PlanAborted { goal: "test".into(), reason: "test abort".into() });
        assert!(JOURNAL_ENTRIES_WRITTEN.load(Ordering::Relaxed) > before);
    }
}
