//! UI state model — current state of desktop UI elements.
//!
//! Provides serializable types for windows, dialogs, focus state, and UI
//! element kinds.  Updated by the world-state runtime on each snapshot cycle.

use std::collections::HashMap;

// ── UI element kind ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum UiElementKind {
    Button,
    TextField,
    MenuItem,
    Dialog,
    Notification,
    ProgressBar,
    Window,
    Tab,
    Toolbar,
    StatusBar,
    Checkbox,
    Unknown,
}

// ── Dialog kind ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum DialogKind {
    Confirmation,
    Error,
    Warning,
    Loading,
    Permission,
    FileOpen,
    FileSave,
    About,
    Update,
    Crash,
    Unknown,
}

impl DialogKind {
    pub fn is_blocking_by_default(&self) -> bool {
        matches!(self, DialogKind::Error | DialogKind::Permission | DialogKind::Crash
                     | DialogKind::Confirmation)
    }
}

// ── Dialog state ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct DialogState {
    pub id:          String,
    pub kind:        DialogKind,
    pub title:       String,
    pub message:     Option<String>,
    pub buttons:     Vec<String>,
    pub is_blocking: bool,
}

impl DialogState {
    pub fn is_dangerous(&self) -> bool {
        matches!(self.kind, DialogKind::Permission | DialogKind::Error | DialogKind::Crash)
            && self.is_blocking
    }
    pub fn requires_immediate_response(&self) -> bool {
        self.kind == DialogKind::Crash || self.kind == DialogKind::Permission
    }
}

// ── Focused window ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct FocusedWindow {
    pub title:        String,
    pub process_name: String,
    /// Opaque OS window handle hint — not dereferenced directly.
    pub hwnd_hint:    Option<u64>,
}

// ── Application state ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AppLoadState {
    NotRunning,
    Launching,
    Loading,
    Ready,
    Busy,
    Error,
    Crashed,
}

// ── UI state ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct UiState {
    pub focused:            Option<FocusedWindow>,
    pub active_dialogs:     Vec<DialogState>,
    pub notification_count: usize,
    pub is_screen_locked:   bool,
    pub elements:           HashMap<String, UiElementKind>,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            focused: None,
            active_dialogs: Vec::new(),
            notification_count: 0,
            is_screen_locked: false,
            elements: HashMap::new(),
        }
    }

    pub fn has_blocking_dialog(&self) -> bool {
        self.active_dialogs.iter().any(|d| d.is_blocking)
    }

    pub fn has_dangerous_dialog(&self) -> bool {
        self.active_dialogs.iter().any(|d| d.is_dangerous())
    }

    pub fn focused_title(&self) -> &str {
        self.focused.as_ref().map(|f| f.title.as_str()).unwrap_or("")
    }

    pub fn focused_process(&self) -> &str {
        self.focused.as_ref().map(|f| f.process_name.as_str()).unwrap_or("")
    }

    pub fn dialog_count(&self) -> usize {
        self.active_dialogs.len()
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_blocking_error() -> DialogState {
        DialogState {
            id: "err1".into(), kind: DialogKind::Error,
            title: "Fatal Error".into(), message: Some("Segfault".into()),
            buttons: vec!["OK".into()], is_blocking: true,
        }
    }

    #[test]
    fn empty_ui_state_no_dialogs() {
        let s = UiState::new();
        assert!(!s.has_blocking_dialog());
        assert!(!s.has_dangerous_dialog());
        assert_eq!(s.dialog_count(), 0);
    }

    #[test]
    fn blocking_error_dialog_detected() {
        let mut s = UiState::new();
        s.active_dialogs.push(make_blocking_error());
        assert!(s.has_blocking_dialog());
        assert!(s.has_dangerous_dialog());
    }

    #[test]
    fn non_blocking_about_dialog_not_dangerous() {
        let d = DialogState {
            id: "a1".into(), kind: DialogKind::About, title: "About".into(),
            message: None, buttons: vec!["Close".into()], is_blocking: false,
        };
        assert!(!d.is_dangerous());
    }

    #[test]
    fn permission_blocking_requires_immediate_response() {
        let d = DialogState {
            id: "p1".into(), kind: DialogKind::Permission, title: "UAC".into(),
            message: None, buttons: vec!["Yes".into(), "No".into()], is_blocking: true,
        };
        assert!(d.requires_immediate_response());
        assert!(d.is_dangerous());
    }

    #[test]
    fn focused_title_empty_when_no_focus() {
        let s = UiState::new();
        assert_eq!(s.focused_title(), "");
        assert_eq!(s.focused_process(), "");
    }

    #[test]
    fn dialog_kind_blocking_classification() {
        assert!(DialogKind::Crash.is_blocking_by_default());
        assert!(DialogKind::Permission.is_blocking_by_default());
        assert!(!DialogKind::About.is_blocking_by_default());
        assert!(!DialogKind::Loading.is_blocking_by_default());
    }
}
