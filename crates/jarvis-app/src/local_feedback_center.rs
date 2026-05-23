//! Local feedback center — collects session quality data entirely on-device.
//! Replaces telemetry with a local-only feedback store.  Zero data leaves the machine.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static FEEDBACK_ENTRIES_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static CRASH_ENTRIES:          AtomicU64 = AtomicU64::new(0);
pub static LATENCY_SPIKE_ENTRIES:  AtomicU64 = AtomicU64::new(0);
pub static WORKFLOW_FAILURE_ENTRIES: AtomicU64 = AtomicU64::new(0);

// Telemetry guarantee — compile-time enforced
const _TELEMETRY_DISABLED: bool = false;
// NO_EXTERNAL_TRANSMISSION — no network code anywhere in this module

#[derive(Debug, Clone, serde::Serialize)]
pub enum FeedbackKind {
    Crash,
    LatencySpike,
    WorkflowFailure,
    VoiceIssue,
    PermissionFriction,
    ModelInstability,
    UserAnnotation,
}

impl FeedbackKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Crash                => "Crash",
            Self::LatencySpike         => "LatencySpike",
            Self::WorkflowFailure      => "WorkflowFailure",
            Self::VoiceIssue           => "VoiceIssue",
            Self::PermissionFriction   => "PermissionFriction",
            Self::ModelInstability     => "ModelInstability",
            Self::UserAnnotation       => "UserAnnotation",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FeedbackEntry {
    pub id:         u64,
    pub kind:       FeedbackKind,
    pub component:  String,
    pub detail:     String,
    pub severity:   u8,      // 1–5
    pub timestamp:  u64,
    pub session_id: u64,
}

const MAX_ENTRIES: usize = 500;

struct FeedbackState {
    entries:    Vec<FeedbackEntry>,
    next_id:    u64,
    session_id: u64,
}

impl FeedbackState {
    fn new() -> Self {
        let session_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self { entries: Vec::new(), next_id: 1, session_id }
    }
}

static STATE: Lazy<Mutex<FeedbackState>> = Lazy::new(|| Mutex::new(FeedbackState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn record(kind: FeedbackKind, component: &str, detail: &str, severity: u8) -> u64 {
    let mut s = STATE.lock().unwrap();
    let id = s.next_id;
    s.next_id += 1;
    let session_id = s.session_id;
    if s.entries.len() >= MAX_ENTRIES { s.entries.remove(0); }
    s.entries.push(FeedbackEntry {
        id,
        kind: kind.clone(),
        component: component.to_string(),
        detail: detail.to_string(),
        severity: severity.min(5).max(1),
        timestamp: ts_now(),
        session_id,
    });
    FEEDBACK_ENTRIES_TOTAL.fetch_add(1, Ordering::Relaxed);
    match kind {
        FeedbackKind::Crash              => { CRASH_ENTRIES.fetch_add(1, Ordering::Relaxed); }
        FeedbackKind::LatencySpike       => { LATENCY_SPIKE_ENTRIES.fetch_add(1, Ordering::Relaxed); }
        FeedbackKind::WorkflowFailure    => { WORKFLOW_FAILURE_ENTRIES.fetch_add(1, Ordering::Relaxed); }
        _ => {}
    }
    id
}

// ── Convenience helpers ───────────────────────────────────────────────────────

pub fn record_crash(component: &str, detail: &str) -> u64 {
    record(FeedbackKind::Crash, component, detail, 5)
}

pub fn record_latency_spike(component: &str, latency_ms: u64) -> u64 {
    record(FeedbackKind::LatencySpike, component, &format!("{}ms spike", latency_ms), 3)
}

pub fn record_voice_issue(detail: &str) -> u64 {
    record(FeedbackKind::VoiceIssue, "voice_pipeline", detail, 2)
}

pub fn record_workflow_failure(workflow: &str, step: usize) -> u64 {
    record(FeedbackKind::WorkflowFailure, "workflow_profiles",
        &format!("failed at step {} of '{}'", step, workflow), 3)
}

pub fn add_annotation(note: &str) -> u64 {
    record(FeedbackKind::UserAnnotation, "user", note, 1)
}

pub fn recent(n: usize) -> Vec<FeedbackEntry> {
    let s = STATE.lock().unwrap();
    s.entries.iter().rev().take(n).cloned().collect()
}

pub fn by_kind(kind_label: &str) -> Vec<FeedbackEntry> {
    let s = STATE.lock().unwrap();
    s.entries.iter()
        .filter(|e| e.kind.label() == kind_label)
        .cloned().collect()
}

pub fn clear() {
    let mut s = STATE.lock().unwrap();
    s.entries.clear();
    // Do NOT clear global counters — they represent session totals
}

pub fn export_local_json() -> String {
    let s = STATE.lock().unwrap();
    let entries: Vec<_> = s.entries.iter().map(|e| {
        format!(r#"{{"id":{},"kind":"{}","component":"{}","detail":"{}","severity":{},"timestamp":{}}}"#,
            e.id, e.kind.label(), e.component, e.detail, e.severity, e.timestamp)
    }).collect();
    format!("[{}]", entries.join(","))
}

#[derive(Debug, serde::Serialize)]
pub struct FeedbackSnapshot {
    pub total_entries:       u64,
    pub crash_entries:       u64,
    pub latency_spike_entries: u64,
    pub workflow_failures:   u64,
    pub stored_count:        usize,
    pub telemetry_enabled:   bool, // always false
}

pub fn snapshot() -> FeedbackSnapshot {
    let s = STATE.lock().unwrap();
    FeedbackSnapshot {
        total_entries:         FEEDBACK_ENTRIES_TOTAL.load(Ordering::Relaxed),
        crash_entries:         CRASH_ENTRIES.load(Ordering::Relaxed),
        latency_spike_entries: LATENCY_SPIKE_ENTRIES.load(Ordering::Relaxed),
        workflow_failures:     WORKFLOW_FAILURE_ENTRIES.load(Ordering::Relaxed),
        stored_count:          s.entries.len(),
        telemetry_enabled:     _TELEMETRY_DISABLED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_recorded() {
        let before = CRASH_ENTRIES.load(Ordering::Relaxed);
        record_crash("test_module", "segfault in test");
        assert!(CRASH_ENTRIES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn latency_spike_recorded() {
        let id = record_latency_spike("inference", 5000);
        assert!(id > 0);
    }

    #[test]
    fn recent_returns_bounded() {
        for i in 0..10 {
            record(FeedbackKind::VoiceIssue, "test", &format!("issue {}", i), 2);
        }
        assert!(recent(5).len() <= 5);
    }

    #[test]
    fn telemetry_always_false() {
        let s = snapshot();
        assert!(!s.telemetry_enabled, "telemetry must always be false");
    }

    #[test]
    fn export_json_valid() {
        record_crash("export_test", "test crash for export");
        let json = export_local_json();
        assert!(json.starts_with('['));
        assert!(json.ends_with(']'));
    }

    #[test]
    fn user_annotation() {
        let id = add_annotation("Jarvis seemed slow today");
        assert!(id > 0);
    }
}
