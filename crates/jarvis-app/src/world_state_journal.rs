//! World-state journal — append-only JSONL log of environment evolution events.
//!
//! Distinct from `screen_state_journal` (Phase 14 visual/UI events):
//! this journal records semantic world-model transitions, anomaly detections,
//! predictions, and goal state changes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static WORLD_JOURNAL_ENTRIES: AtomicU64 = AtomicU64::new(0);

const MAX_IN_MEMORY: usize = 300;
const JOURNAL_FILE:  &str  = "world_state_journal.jsonl";

// ── Event kinds ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum WorldEventKind {
    EnvironmentTransition { from: String, to: String, confidence: f32 },
    AnomalyDetected       { anomaly: String, severity: String },
    AnomalyResolved       { anomaly: String },
    PredictionMade        { prediction: String, confidence: f32 },
    PredictionVerified    { prediction: String, correct: bool },
    GoalCreated           { goal_id: u64, description: String },
    GoalCompleted         { goal_id: u64 },
    GoalAbandoned         { goal_id: u64, reason: String },
    WorkflowPatternLearned { pattern: String, occurrences: u32 },
    ReflectionInsight     { insight: String, confidence: f32 },
    CognitionLoopTick     { tick_id: u64, phase: String, outcome: String },
    WorldModelUpdated     { window_count: usize, active_app: Option<String> },
    ContinuityRestored    { task_id: String },
    AttentionShifted      { from: Option<String>, to: String, priority: String },
}

// ── Journal entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorldJournalEntry {
    pub ts_ms: u64,
    pub kind:  WorldEventKind,
}

// ── In-memory ring buffer ─────────────────────────────────────────────────────

static ENTRIES: Lazy<Mutex<Vec<WorldJournalEntry>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn log(kind: WorldEventKind) {
    WORLD_JOURNAL_ENTRIES.fetch_add(1, Ordering::Relaxed);
    let entry = WorldJournalEntry { ts_ms: ts_now(), kind };

    if let Ok(mut guard) = ENTRIES.lock() {
        if guard.len() >= MAX_IN_MEMORY {
            guard.remove(0);
        }
        guard.push(entry.clone());
    }

    append_jsonl(&entry);
}

pub fn recent_entries(n: usize) -> Vec<WorldJournalEntry> {
    ENTRIES.lock().map(|g| {
        let len = g.len();
        g[len.saturating_sub(n)..].to_vec()
    }).unwrap_or_default()
}

pub fn entry_count() -> u64 {
    WORLD_JOURNAL_ENTRIES.load(Ordering::Relaxed)
}

// ── JSONL append ──────────────────────────────────────────────────────────────

fn append_jsonl(entry: &WorldJournalEntry) {
    use std::io::Write as _;
    let path = crate::execution_journal::journal_dir().join(JOURNAL_FILE);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        if let Ok(line) = serde_json::to_string(entry) {
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

    #[test]
    fn log_increments_counter() {
        let before = WORLD_JOURNAL_ENTRIES.load(Ordering::Relaxed);
        log(WorldEventKind::WorldModelUpdated { window_count: 3, active_app: Some("vscode".into()) });
        assert!(WORLD_JOURNAL_ENTRIES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn recent_entries_returns_last_n() {
        log(WorldEventKind::CognitionLoopTick {
            tick_id: 1, phase: "Observe".into(), outcome: "Completed".into(),
        });
        log(WorldEventKind::CognitionLoopTick {
            tick_id: 2, phase: "Model".into(), outcome: "Completed".into(),
        });
        let entries = recent_entries(1);
        assert!(!entries.is_empty());
    }

    #[test]
    fn anomaly_event_roundtrips_serde() {
        let kind = WorldEventKind::AnomalyDetected {
            anomaly: "FrozenApp".into(),
            severity: "high".into(),
        };
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("FrozenApp"));
    }

    #[test]
    fn goal_events_serialize() {
        let created = WorldEventKind::GoalCreated { goal_id: 42, description: "open IDE".into() };
        let done    = WorldEventKind::GoalCompleted { goal_id: 42 };
        let j1 = serde_json::to_string(&created).unwrap();
        let j2 = serde_json::to_string(&done).unwrap();
        assert!(j1.contains("GoalCreated"));
        assert!(j2.contains("GoalCompleted"));
    }

    #[test]
    fn entry_count_non_zero_after_log() {
        log(WorldEventKind::AnomalyResolved { anomaly: "FrozenApp".into() });
        assert!(entry_count() > 0);
    }
}
