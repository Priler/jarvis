//! Desktop integration — system tray state, hotkeys, focus detection, window tracking.
//! All local; no external process injection or privileged system calls.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static HOTKEY_EVENTS:   AtomicU64 = AtomicU64::new(0);
pub static FOCUS_CHANGES:   AtomicU64 = AtomicU64::new(0);
pub static TRAY_UPDATES:    AtomicU64 = AtomicU64::new(0);
static STARTUP_LAUNCH_ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, serde::Serialize)]
pub struct TrayState {
    pub icon:         String,
    pub tooltip:      String,
    pub menu_items:   Vec<String>,
    pub notification_count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HotkeyBinding {
    pub combination:  String,
    pub action:       String,
    pub enabled:      bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FocusState {
    pub active_window: String,
    pub application:   String,
    pub focused_at:    u64,
    pub jarvis_relevant: bool,
}

struct DesktopState {
    tray:            TrayState,
    hotkeys:         Vec<HotkeyBinding>,
    focus:           FocusState,
    window_history:  Vec<String>,
}

impl DesktopState {
    fn new() -> Self {
        Self {
            tray: TrayState {
                icon:               "◈".to_string(),
                tooltip:            "Jarvis v1 Beta — Running".to_string(),
                menu_items:         vec![
                    "Open Control Center".to_string(),
                    "Pause Jarvis".to_string(),
                    "Settings".to_string(),
                    "Quit Jarvis".to_string(),
                ],
                notification_count: 0,
            },
            hotkeys: vec![
                HotkeyBinding { combination: "Ctrl+Shift+J".to_string(), action: "Wake Jarvis".to_string(),         enabled: true },
                HotkeyBinding { combination: "Ctrl+Shift+M".to_string(), action: "Open Memory Search".to_string(),  enabled: true },
                HotkeyBinding { combination: "Ctrl+Shift+D".to_string(), action: "Open Dashboard".to_string(),      enabled: true },
                HotkeyBinding { combination: "Ctrl+Shift+P".to_string(), action: "Pause / Resume Voice".to_string(), enabled: true },
                HotkeyBinding { combination: "Escape".to_string(),       action: "Cancel Current Action".to_string(), enabled: true },
            ],
            focus: FocusState {
                active_window:   String::new(),
                application:     String::new(),
                focused_at:      0,
                jarvis_relevant: false,
            },
            window_history: Vec::new(),
        }
    }
}

static STATE: Lazy<Mutex<DesktopState>> = Lazy::new(|| Mutex::new(DesktopState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tray ──────────────────────────────────────────────────────────────────────

pub fn update_tray(icon: &str, tooltip: &str, notification_count: u64) {
    let mut s = STATE.lock().unwrap();
    s.tray.icon               = icon.to_string();
    s.tray.tooltip            = tooltip.to_string();
    s.tray.notification_count = notification_count;
    TRAY_UPDATES.fetch_add(1, Ordering::Relaxed);
}

pub fn tray_state() -> TrayState { STATE.lock().unwrap().tray.clone() }

// ── Hotkeys ───────────────────────────────────────────────────────────────────

pub fn handle_hotkey(combination: &str) -> Option<String> {
    let s = STATE.lock().unwrap();
    let action = s.hotkeys.iter()
        .find(|h| h.enabled && h.combination == combination)
        .map(|h| h.action.clone());
    drop(s);
    if action.is_some() {
        HOTKEY_EVENTS.fetch_add(1, Ordering::Relaxed);
    }
    action
}

pub fn hotkeys() -> Vec<HotkeyBinding> { STATE.lock().unwrap().hotkeys.clone() }

// ── Focus tracking ────────────────────────────────────────────────────────────

static JARVIS_RELEVANT_APPS: &[&str] = &[
    "code", "terminal", "notepad", "wordpad", "excel", "word", "powershell",
    "cmd", "vim", "nvim", "emacs", "cursor", "jetbrains",
];

pub fn update_focus(window_title: &str, application: &str) {
    let relevant = JARVIS_RELEVANT_APPS.iter().any(|a| application.to_lowercase().contains(a));
    let mut s = STATE.lock().unwrap();
    if s.focus.application != application {
        if s.window_history.len() >= 20 { s.window_history.remove(0); }
        s.window_history.push(format!("{} — {}", application, window_title));
        FOCUS_CHANGES.fetch_add(1, Ordering::Relaxed);
    }
    s.focus = FocusState {
        active_window:   window_title.to_string(),
        application:     application.to_string(),
        focused_at:      ts_now(),
        jarvis_relevant: relevant,
    };
}

pub fn current_focus() -> FocusState { STATE.lock().unwrap().focus.clone() }
pub fn is_focus_relevant() -> bool   { STATE.lock().unwrap().focus.jarvis_relevant }

// ── Startup launch ────────────────────────────────────────────────────────────

pub fn set_startup_launch(enabled: bool) {
    STARTUP_LAUNCH_ENABLED.store(enabled, Ordering::Relaxed);
    crate::preferences_runtime::set("startup_launch", if enabled { "true" } else { "false" });
}

pub fn startup_launch_enabled() -> bool { STARTUP_LAUNCH_ENABLED.load(Ordering::Relaxed) }

#[derive(Debug, serde::Serialize)]
pub struct DesktopSnapshot {
    pub tray_tooltip:      String,
    pub hotkey_events:     u64,
    pub focus_changes:     u64,
    pub tray_updates:      u64,
    pub startup_launch:    bool,
    pub active_app:        String,
    pub focus_relevant:    bool,
    pub hotkeys_count:     usize,
}

pub fn snapshot() -> DesktopSnapshot {
    let s = STATE.lock().unwrap();
    DesktopSnapshot {
        tray_tooltip:   s.tray.tooltip.clone(),
        hotkey_events:  HOTKEY_EVENTS.load(Ordering::Relaxed),
        focus_changes:  FOCUS_CHANGES.load(Ordering::Relaxed),
        tray_updates:   TRAY_UPDATES.load(Ordering::Relaxed),
        startup_launch: STARTUP_LAUNCH_ENABLED.load(Ordering::Relaxed),
        active_app:     s.focus.application.clone(),
        focus_relevant: s.focus.jarvis_relevant,
        hotkeys_count:  s.hotkeys.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_handled() {
        let action = handle_hotkey("Ctrl+Shift+J");
        assert_eq!(action.as_deref(), Some("Wake Jarvis"));
    }

    #[test]
    fn unknown_hotkey_returns_none() {
        let action = handle_hotkey("Ctrl+Z");
        assert!(action.is_none());
    }

    #[test]
    fn focus_update_tracks_app() {
        update_focus("main.rs — VS Code", "code");
        assert!(is_focus_relevant());
        assert_eq!(current_focus().application, "code");
    }

    #[test]
    fn non_relevant_app() {
        update_focus("Solitaire", "solitaire");
        let s = STATE.lock().unwrap();
        assert!(!s.focus.jarvis_relevant);
    }

    #[test]
    fn tray_update() {
        update_tray("◈", "Jarvis — Active", 3);
        let t = tray_state();
        assert_eq!(t.notification_count, 3);
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(s.hotkeys_count > 0);
    }
}
