//! Screen-state journal — append-only audit log for visual environment events.
//!
//! Logs window changes, OCR reads, dialog detections, verification results,
//! UI failures, and environment transitions to `screen_state_journal.jsonl`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::VecDeque;

pub static SCREEN_JOURNAL_ENTRIES: AtomicU64 = AtomicU64::new(0);

const JOURNAL_FILE: &str = "screen_state_journal.jsonl";
const MAX_IN_MEMORY: usize = 200;

// ── Event kinds ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub enum ScreenEventKind {
    WindowOpened      { title: String, process: String },
    WindowClosed      { title: String },
    WindowFocused     { title: String, process: String },
    DialogDetected    { kind: String, title_hint: String, blocking: bool },
    DialogDismissed   { title_hint: String },
    OcrCompleted      { confidence: f32, text_len: usize, backend: String },
    OcrFailed         { reason: String },
    VerificationPass  { tool_id: String, source: String },
    VerificationFail  { tool_id: String, source: String, reason: String },
    EnvironmentState  { state: String, confidence: f32 },
    UiInteraction     { anchor: String, kind: String, success: bool },
    SafetyBlock       { tool_id: String, reason: String },
    WorldStateUpdated { window_count: usize, has_modal: bool },
}

// ── Journal entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScreenJournalEntry {
    pub ts_ms: u64,
    pub seq:   u64,
    pub kind:  ScreenEventKind,
}

// ── Ring buffer ───────────────────────────────────────────────────────────────

static RING: Lazy<Mutex<VecDeque<ScreenJournalEntry>>> =
    Lazy::new(|| Mutex::new(VecDeque::new()));
static SEQ: AtomicU64 = AtomicU64::new(0);

// ── Public API ────────────────────────────────────────────────────────────────

pub fn log(kind: ScreenEventKind) {
    let entry = ScreenJournalEntry {
        ts_ms: now_ms(),
        seq:   SEQ.fetch_add(1, Ordering::Relaxed),
        kind,
    };

    let mut ring = RING.lock();
    if ring.len() >= MAX_IN_MEMORY {
        ring.pop_front();
    }
    ring.push_back(entry.clone());
    drop(ring);

    if let Ok(line) = serde_json::to_string(&entry) {
        let _ = append_line(JOURNAL_FILE, &line);
    }
    SCREEN_JOURNAL_ENTRIES.fetch_add(1, Ordering::Relaxed);
}

pub fn recent_entries(n: usize) -> Vec<ScreenJournalEntry> {
    let ring = RING.lock();
    let skip = ring.len().saturating_sub(n);
    ring.iter().skip(skip).cloned().collect()
}

pub fn entry_count() -> u64 {
    SEQ.load(Ordering::Relaxed)
}

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
        log(ScreenEventKind::WindowOpened {
            title: "VS Code".into(), process: "code".into(),
        });
        assert!(entry_count() > before);
    }

    #[test]
    fn recent_entries_returns_last_logged() {
        log(ScreenEventKind::DialogDetected {
            kind: "Error".into(), title_hint: "Fatal".into(), blocking: true,
        });
        let entries = recent_entries(5);
        assert!(!entries.is_empty());
    }

    #[test]
    fn ring_bounded_at_max() {
        for i in 0..MAX_IN_MEMORY + 5 {
            log(ScreenEventKind::OcrCompleted {
                confidence: 0.9, text_len: i, backend: "stub".into(),
            });
        }
        let entries = recent_entries(MAX_IN_MEMORY + 20);
        assert!(entries.len() <= MAX_IN_MEMORY);
    }

    #[test]
    fn screen_journal_entries_counter_increments() {
        let before = SCREEN_JOURNAL_ENTRIES.load(Ordering::Relaxed);
        log(ScreenEventKind::EnvironmentState {
            state: "Ready".into(), confidence: 0.9,
        });
        assert!(SCREEN_JOURNAL_ENTRIES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn world_state_updated_event_logged() {
        log(ScreenEventKind::WorldStateUpdated { window_count: 3, has_modal: false });
        let entries = recent_entries(10);
        let found = entries.iter().any(|e| matches!(
            &e.kind,
            ScreenEventKind::WorldStateUpdated { window_count, .. } if *window_count == 3
        ));
        assert!(found);
    }
}
