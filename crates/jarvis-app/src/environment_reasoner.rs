//! Environment reasoner — infers semantic desktop state from world-state.
//!
//! Answers questions like:
//!   "Is the IDE workspace fully loaded?"
//!   "Is there an error dialog blocking the desktop?"
//!   "Is the terminal waiting for input?"
//!
//! Used by the multimodal planner to skip or adapt steps.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::dialog_detector::{self, DetectedDialog};
use crate::world_state;

pub static REASONING_RUNS:     AtomicU64 = AtomicU64::new(0);
pub static REASONING_POSITIVE: AtomicU64 = AtomicU64::new(0);
pub static REASONING_NEGATIVE: AtomicU64 = AtomicU64::new(0);

// ── Environment state ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum EnvironmentState {
    /// Desktop is ready; focused app is responsive.
    Ready,
    /// An application is launching but not yet interactive.
    AppLaunching { process: String },
    /// Application is loading a workspace/project.
    WorkspaceLoading { app: String },
    /// Application workspace is fully loaded.
    WorkspaceReady { app: String },
    /// A crash or fatal error dialog is visible.
    CrashDetected { hint: String },
    /// A permission/UAC dialog requires user action.
    PermissionRequired { hint: String },
    /// A terminal or REPL is waiting for user input.
    TerminalWaiting,
    /// An installer is frozen or unresponsive.
    InstallerFrozen,
    /// Browser is blocked (e.g. security warning, login wall).
    BrowserBlocked { hint: String },
    /// Unable to determine state — ambiguous evidence.
    Ambiguous { reason: String },
}

impl EnvironmentState {
    pub fn is_actionable(&self) -> bool {
        matches!(self, EnvironmentState::Ready | EnvironmentState::WorkspaceReady { .. }
                     | EnvironmentState::TerminalWaiting)
    }

    pub fn requires_intervention(&self) -> bool {
        matches!(self, EnvironmentState::CrashDetected { .. }
                     | EnvironmentState::PermissionRequired { .. }
                     | EnvironmentState::InstallerFrozen)
    }

    pub fn can_skip_launch(&self, process_hint: &str) -> bool {
        match self {
            EnvironmentState::WorkspaceReady { app } => {
                app.to_lowercase().contains(&process_hint.to_lowercase())
            }
            _ => false,
        }
    }
}

// ── Reasoning result ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReasoningResult {
    pub state:      EnvironmentState,
    pub confidence: f32,
    pub evidence:   Vec<String>,
    pub dialog:     Option<DetectedDialog>,
}

impl ReasoningResult {
    pub fn ready() -> Self {
        Self {
            state: EnvironmentState::Ready,
            confidence: 0.85,
            evidence: vec!["no blocking dialogs".to_string(), "world-state active".to_string()],
            dialog: None,
        }
    }
}

// ── Environment reasoner ──────────────────────────────────────────────────────

pub struct EnvironmentReasoner;

impl EnvironmentReasoner {
    /// Reason about the current desktop environment state.
    pub fn reason() -> ReasoningResult {
        REASONING_RUNS.fetch_add(1, Ordering::Relaxed);

        // P1: check for dialogs first — they override everything else.
        let dialog = dialog_detector::DialogDetector::scan();
        if let Some(ref d) = dialog {
            use crate::ui_state::DialogKind;
            let result = match d.kind {
                DialogKind::Crash => ReasoningResult {
                    state: EnvironmentState::CrashDetected { hint: d.title_hint.clone() },
                    confidence: d.confidence,
                    evidence: d.evidence.clone(),
                    dialog: Some(d.clone()),
                },
                DialogKind::Permission => ReasoningResult {
                    state: EnvironmentState::PermissionRequired { hint: d.title_hint.clone() },
                    confidence: d.confidence,
                    evidence: d.evidence.clone(),
                    dialog: Some(d.clone()),
                },
                _ => ReasoningResult {
                    state: EnvironmentState::Ambiguous {
                        reason: format!("dialog kind {:?} detected", d.kind),
                    },
                    confidence: d.confidence * 0.7,
                    evidence: d.evidence.clone(),
                    dialog: Some(d.clone()),
                },
            };
            REASONING_NEGATIVE.fetch_add(1, Ordering::Relaxed);
            return result;
        }

        // P2: check world state for blocking modals.
        if world_state::has_blocking_modal() {
            REASONING_NEGATIVE.fetch_add(1, Ordering::Relaxed);
            return ReasoningResult {
                state: EnvironmentState::Ambiguous {
                    reason: "blocking modal window detected".to_string(),
                },
                confidence: 0.80,
                evidence: vec!["has_blocking_modal=true".to_string()],
                dialog: None,
            };
        }

        // P3: infer from focused window title.
        let title = world_state::focused_window_title().to_lowercase();
        let result = if title.is_empty() {
            ReasoningResult {
                state: EnvironmentState::Ambiguous {
                    reason: "no focused window".to_string(),
                },
                confidence: 0.50,
                evidence: vec!["focused_window=none".to_string()],
                dialog: None,
            }
        } else if title.contains("loading") || title.contains("starting") {
            let app = world_state::focused_window_title();
            ReasoningResult {
                state: EnvironmentState::WorkspaceLoading { app: app.clone() },
                confidence: 0.75,
                evidence: vec![format!("title contains loading/starting: '{}'", title)],
                dialog: None,
            }
        } else {
            ReasoningResult::ready()
        };

        if result.state.is_actionable() {
            REASONING_POSITIVE.fetch_add(1, Ordering::Relaxed);
        } else {
            REASONING_NEGATIVE.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    /// Check if a specific application appears ready in world state.
    pub fn is_app_ready(process_hint: &str) -> bool {
        world_state::app_is_running(process_hint)
    }

    /// Return `true` if the runtime can safely skip launching `process_hint`
    /// because it is already running and responsive.
    pub fn can_skip_launch(process_hint: &str) -> bool {
        if !world_state::app_is_running(process_hint) { return false; }
        if world_state::has_blocking_modal() { return false; }
        true
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_runs_counter_increments() {
        let before = REASONING_RUNS.load(Ordering::Relaxed);
        EnvironmentReasoner::reason();
        assert!(REASONING_RUNS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn environment_state_ready_is_actionable() {
        assert!(EnvironmentState::Ready.is_actionable());
    }

    #[test]
    fn crash_state_requires_intervention() {
        let s = EnvironmentState::CrashDetected { hint: "app.exe".into() };
        assert!(s.requires_intervention());
        assert!(!s.is_actionable());
    }

    #[test]
    fn can_skip_launch_when_workspace_ready() {
        let s = EnvironmentState::WorkspaceReady { app: "Visual Studio Code".into() };
        assert!(s.can_skip_launch("visual studio code"));
        assert!(!s.can_skip_launch("notepad"));
    }

    #[test]
    fn classify_text_crash_triggers_negative_reasoning() {
        // Use classify_text directly to avoid screen capture dependency
        let d = crate::dialog_detector::DialogDetector::classify_text(
            "Application has stopped working — crash detected", 0.9,
        );
        assert!(d.is_some());
        assert!(d.unwrap().requires_user_action());
    }

    #[test]
    fn reasoning_result_ready_has_high_confidence() {
        let r = ReasoningResult::ready();
        assert!(r.confidence >= 0.80);
        assert!(r.state.is_actionable());
    }
}
