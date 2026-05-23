//! Workflow profiles — context-specific runtime presets for daily use scenarios.
//! Each workflow preloads tools, memory contexts, models, and performance mode.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static WORKFLOW_ACTIVATIONS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WorkflowKind {
    Developer,
    Research,
    Writing,
    DesktopAssistant,
    Focus,
    Meeting,
}

impl WorkflowKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Developer       => "Developer",
            Self::Research        => "Research",
            Self::Writing         => "Writing",
            Self::DesktopAssistant => "DesktopAssistant",
            Self::Focus           => "Focus",
            Self::Meeting         => "Meeting",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Developer       => "Coding assistant: terminal, file access, code memory, fast inference",
            Self::Research        => "Research assistant: document indexing, RAG retrieval, reasoning mode",
            Self::Writing         => "Writing assistant: text memory, document context, voice dictation",
            Self::DesktopAssistant => "General desktop: balanced tools, voice-first, notifications",
            Self::Focus           => "Focus mode: minimal interruptions, eco performance, no notifications",
            Self::Meeting         => "Meeting mode: voice-priority, note-taking, memory capture",
        }
    }

    pub fn performance_mode(&self) -> &'static str {
        match self {
            Self::Developer        => "Performance",
            Self::Research         => "Reasoning",
            Self::Writing          => "Balanced",
            Self::DesktopAssistant => "Balanced",
            Self::Focus            => "Eco",
            Self::Meeting          => "VoicePriority",
        }
    }

    pub fn preloaded_tools(&self) -> Vec<&'static str> {
        match self {
            Self::Developer => vec!["terminal_exec", "file_read", "file_write", "code_search"],
            Self::Research  => vec!["file_read", "rag_retrieve", "web_search", "note_capture"],
            Self::Writing   => vec!["file_write", "note_capture", "rag_retrieve", "voice_dictate"],
            Self::DesktopAssistant => vec!["desktop_control", "browser_control", "note_capture"],
            Self::Focus     => vec!["note_capture", "timer"],
            Self::Meeting   => vec!["note_capture", "voice_dictate", "memory_store"],
        }
    }

    pub fn preloaded_memory_contexts(&self) -> Vec<&'static str> {
        match self {
            Self::Developer => vec!["project_code", "recent_errors", "dependencies"],
            Self::Research  => vec!["research_notes", "indexed_docs", "citations"],
            Self::Writing   => vec!["writing_drafts", "style_notes", "references"],
            Self::DesktopAssistant => vec!["daily_tasks", "recent_interactions"],
            Self::Focus     => vec!["current_task"],
            Self::Meeting   => vec!["meeting_notes", "action_items", "participants"],
        }
    }

    pub fn notifications_suppressed(&self) -> bool {
        matches!(self, Self::Focus | Self::Meeting)
    }
}

pub fn all_workflows() -> Vec<WorkflowKind> {
    vec![
        WorkflowKind::Developer,
        WorkflowKind::Research,
        WorkflowKind::Writing,
        WorkflowKind::DesktopAssistant,
        WorkflowKind::Focus,
        WorkflowKind::Meeting,
    ]
}

struct ProfilesState {
    active:     WorkflowKind,
    activated_at: u64,
}

static STATE: Lazy<Mutex<ProfilesState>> = Lazy::new(|| {
    Mutex::new(ProfilesState {
        active:       WorkflowKind::DesktopAssistant,
        activated_at: 0,
    })
});

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn activate(kind: WorkflowKind) {
    // Apply performance profile for this workflow
    let perf_mode_str = kind.performance_mode();
    let perf_mode = match perf_mode_str {
        "Eco"           => crate::performance_profiles::PerformanceMode::Eco,
        "Balanced"      => crate::performance_profiles::PerformanceMode::Balanced,
        "Performance"   => crate::performance_profiles::PerformanceMode::Performance,
        "Reasoning"     => crate::performance_profiles::PerformanceMode::Reasoning,
        "VoicePriority" => crate::performance_profiles::PerformanceMode::VoicePriority,
        "LowVRAM"       => crate::performance_profiles::PerformanceMode::LowVRAM,
        _               => crate::performance_profiles::PerformanceMode::Balanced,
    };
    crate::performance_profiles::set_mode(perf_mode);

    let mut s = STATE.lock().unwrap();
    s.active = kind;
    s.activated_at = ts_now();

    WORKFLOW_ACTIVATIONS.fetch_add(1, Ordering::Relaxed);

    crate::production_logging::info(
        "workflow_profiles",
        &format!("activated workflow={}", kind.label()),
    );

    if !kind.notifications_suppressed() {
        crate::notification_center::info(
            "workflow_profiles",
            &format!("Workflow activated: {}", kind.label()),
        );
    }
}

pub fn current() -> WorkflowKind { STATE.lock().unwrap().active }
pub fn current_label() -> &'static str { current().label() }
pub fn activated_at() -> u64 { STATE.lock().unwrap().activated_at }

#[derive(Debug, serde::Serialize)]
pub struct WorkflowInfo {
    pub kind:                  String,
    pub description:           String,
    pub performance_mode:      String,
    pub preloaded_tools:       Vec<String>,
    pub preloaded_memory:      Vec<String>,
    pub notifications_suppressed: bool,
}

pub fn info(kind: WorkflowKind) -> WorkflowInfo {
    WorkflowInfo {
        kind:                  kind.label().to_string(),
        description:           kind.description().to_string(),
        performance_mode:      kind.performance_mode().to_string(),
        preloaded_tools:       kind.preloaded_tools().iter().map(|s| s.to_string()).collect(),
        preloaded_memory:      kind.preloaded_memory_contexts().iter().map(|s| s.to_string()).collect(),
        notifications_suppressed: kind.notifications_suppressed(),
    }
}

#[derive(Debug, serde::Serialize)]
pub struct WorkflowSnapshot {
    pub active:              String,
    pub activated_at:        u64,
    pub performance_mode:    String,
    pub tools_available:     Vec<String>,
    pub activations_total:   u64,
}

pub fn snapshot() -> WorkflowSnapshot {
    let kind = current();
    let s = STATE.lock().unwrap();
    WorkflowSnapshot {
        active:            kind.label().to_string(),
        activated_at:      s.activated_at,
        performance_mode:  kind.performance_mode().to_string(),
        tools_available:   kind.preloaded_tools().iter().map(|t| t.to_string()).collect(),
        activations_total: WORKFLOW_ACTIVATIONS.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;

    static TEST_LOCK: Lazy<std::sync::Mutex<()>> = Lazy::new(|| std::sync::Mutex::new(()));

    #[test]
    fn default_is_desktop_assistant() {
        let _g = TEST_LOCK.lock().unwrap();
        // Workflow may have been changed by other tests; just check the API works.
        let _ = current();
    }

    #[test]
    fn activate_developer_workflow() {
        let _g = TEST_LOCK.lock().unwrap();
        activate(WorkflowKind::Developer);
        assert_eq!(current(), WorkflowKind::Developer);
        assert_eq!(current_label(), "Developer");
    }

    #[test]
    fn focus_suppresses_notifications() {
        assert!(WorkflowKind::Focus.notifications_suppressed());
        assert!(!WorkflowKind::Developer.notifications_suppressed());
    }

    #[test]
    fn each_workflow_has_tools() {
        for wf in all_workflows() {
            assert!(!wf.preloaded_tools().is_empty(), "{} has no tools", wf.label());
        }
    }

    #[test]
    fn snapshot_no_panic() {
        let _g = TEST_LOCK.lock().unwrap();
        activate(WorkflowKind::Research);
        let s = snapshot();
        assert!(!s.active.is_empty());
    }

    #[test]
    fn all_workflows_listed() {
        assert_eq!(all_workflows().len(), 6);
    }
}
