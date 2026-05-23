//! Windows runtime integration — startup launch, tray state, native notifications,
//! hotkey registration state, and Windows recovery integration.
//! All operations are local; no external services contacted.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static STARTUP_LAUNCH_REGISTERED: AtomicBool = AtomicBool::new(false);
pub static NATIVE_NOTIFICATIONS_SENT:  AtomicU64  = AtomicU64::new(0);
pub static TRAY_STATE_UPDATES:         AtomicU64  = AtomicU64::new(0);
pub static HOTKEYS_REGISTERED:         AtomicU64  = AtomicU64::new(0);
pub static RECOVERY_INTEGRATIONS:      AtomicU64  = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize)]
pub struct NativeNotification {
    pub id:        u64,
    pub title:     String,
    pub body:      String,
    pub icon:      String,
    pub timestamp: u64,
    pub action:    Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TrayMenuItem {
    pub label:   String,
    pub action:  String,
    pub enabled: bool,
    pub separator_before: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HotkeyRegistration {
    pub combination: String,
    pub action:      String,
    pub registered:  bool,
    pub id:          u32,
}

const NOTIFICATION_BUFFER: usize = 30;

struct WindowsState {
    notifications:      Vec<NativeNotification>,
    tray_menu:          Vec<TrayMenuItem>,
    hotkeys:            Vec<HotkeyRegistration>,
    next_notif_id:      u64,
    next_hotkey_id:     u32,
    startup_key_path:   String,
}

impl WindowsState {
    fn new() -> Self {
        Self {
            notifications: Vec::new(),
            tray_menu: vec![
                TrayMenuItem { label: "Open Jarvis".to_string(),         action: "open_dashboard".to_string(), enabled: true,  separator_before: false },
                TrayMenuItem { label: "Voice: Resume".to_string(),       action: "voice_resume".to_string(),   enabled: true,  separator_before: false },
                TrayMenuItem { label: "Memory Search".to_string(),       action: "memory_search".to_string(),  enabled: true,  separator_before: false },
                TrayMenuItem { label: "Settings".to_string(),            action: "open_settings".to_string(),  enabled: true,  separator_before: true  },
                TrayMenuItem { label: "Safe Mode".to_string(),           action: "safe_mode".to_string(),      enabled: true,  separator_before: false },
                TrayMenuItem { label: "Quit Jarvis".to_string(),         action: "quit".to_string(),           enabled: true,  separator_before: true  },
            ],
            hotkeys: vec![
                HotkeyRegistration { combination: "Ctrl+Shift+J".to_string(), action: "wake_jarvis".to_string(),        registered: true, id: 1 },
                HotkeyRegistration { combination: "Ctrl+Shift+M".to_string(), action: "open_memory".to_string(),        registered: true, id: 2 },
                HotkeyRegistration { combination: "Ctrl+Shift+D".to_string(), action: "open_dashboard".to_string(),     registered: true, id: 3 },
                HotkeyRegistration { combination: "Ctrl+Shift+P".to_string(), action: "pause_voice".to_string(),        registered: true, id: 4 },
                HotkeyRegistration { combination: "Ctrl+Alt+J".to_string(),   action: "emergency_safe_mode".to_string(), registered: true, id: 5 },
            ],
            next_notif_id:  1,
            next_hotkey_id: 6,
            startup_key_path: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\\JarvisAI".to_string(),
        }
    }
}

static STATE: Lazy<Mutex<WindowsState>> = Lazy::new(|| Mutex::new(WindowsState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Startup launch ────────────────────────────────────────────────────────────

pub fn register_startup_launch(exe_path: &str) -> bool {
    // In production: writes to HKCU Run registry key.
    // Here: records intent + persists to preferences.
    crate::preferences_runtime::set("startup_launch_exe", exe_path);
    crate::preferences_runtime::set("startup_launch", "true");
    STARTUP_LAUNCH_REGISTERED.store(true, Ordering::Relaxed);
    RECOVERY_INTEGRATIONS.fetch_add(1, Ordering::Relaxed);
    crate::production_logging::info("windows_runtime", "startup launch registered");
    true
}

pub fn unregister_startup_launch() {
    crate::preferences_runtime::set("startup_launch", "false");
    STARTUP_LAUNCH_REGISTERED.store(false, Ordering::Relaxed);
}

pub fn startup_launch_registered() -> bool { STARTUP_LAUNCH_REGISTERED.load(Ordering::Relaxed) }

// ── Native notifications ──────────────────────────────────────────────────────

pub fn send_notification(title: &str, body: &str, action: Option<&str>) -> u64 {
    let mut s = STATE.lock().unwrap();
    let id = s.next_notif_id;
    s.next_notif_id += 1;
    if s.notifications.len() >= NOTIFICATION_BUFFER { s.notifications.remove(0); }
    s.notifications.push(NativeNotification {
        id,
        title:     title.to_string(),
        body:      body.to_string(),
        icon:      "◈".to_string(),
        timestamp: ts_now(),
        action:    action.map(|a| a.to_string()),
    });
    NATIVE_NOTIFICATIONS_SENT.fetch_add(1, Ordering::Relaxed);
    // Also push to notification_center for Control Center display
    crate::notification_center::info("windows_runtime", &format!("{}: {}", title, body));
    id
}

pub fn recent_notifications(n: usize) -> Vec<NativeNotification> {
    let s = STATE.lock().unwrap();
    s.notifications.iter().rev().take(n).cloned().collect()
}

// ── Tray ──────────────────────────────────────────────────────────────────────

pub fn tray_menu() -> Vec<TrayMenuItem> { STATE.lock().unwrap().tray_menu.clone() }

pub fn update_tray_item(action: &str, enabled: bool, label: Option<&str>) {
    let mut s = STATE.lock().unwrap();
    if let Some(item) = s.tray_menu.iter_mut().find(|i| i.action == action) {
        item.enabled = enabled;
        if let Some(l) = label { item.label = l.to_string(); }
        TRAY_STATE_UPDATES.fetch_add(1, Ordering::Relaxed);
    }
}

// ── Hotkeys ───────────────────────────────────────────────────────────────────

pub fn registered_hotkeys() -> Vec<HotkeyRegistration> {
    STATE.lock().unwrap().hotkeys.clone()
}

pub fn register_hotkey(combination: &str, action: &str) -> u32 {
    let mut s = STATE.lock().unwrap();
    let id = s.next_hotkey_id;
    s.next_hotkey_id += 1;
    s.hotkeys.push(HotkeyRegistration {
        combination: combination.to_string(),
        action:      action.to_string(),
        registered:  true,
        id,
    });
    HOTKEYS_REGISTERED.fetch_add(1, Ordering::Relaxed);
    id
}

pub fn dispatch_hotkey(combination: &str) -> Option<String> {
    let s = STATE.lock().unwrap();
    s.hotkeys.iter()
        .find(|h| h.registered && h.combination == combination)
        .map(|h| h.action.clone())
}

// ── Windows recovery integration ──────────────────────────────────────────────

pub fn register_recovery_integration() {
    // In production: registers with Windows Error Reporting exclusion + restart manager.
    RECOVERY_INTEGRATIONS.fetch_add(1, Ordering::Relaxed);
    crate::production_logging::info("windows_runtime",
        "Windows recovery integration registered");
}

#[derive(Debug, serde::Serialize)]
pub struct WindowsRuntimeSnapshot {
    pub startup_launch_registered: bool,
    pub native_notifications_sent: u64,
    pub tray_state_updates:        u64,
    pub hotkeys_registered_total:  u64,
    pub hotkeys_active:            usize,
    pub recovery_integrations:     u64,
}

pub fn snapshot() -> WindowsRuntimeSnapshot {
    let s = STATE.lock().unwrap();
    WindowsRuntimeSnapshot {
        startup_launch_registered: STARTUP_LAUNCH_REGISTERED.load(Ordering::Relaxed),
        native_notifications_sent: NATIVE_NOTIFICATIONS_SENT.load(Ordering::Relaxed),
        tray_state_updates:        TRAY_STATE_UPDATES.load(Ordering::Relaxed),
        hotkeys_registered_total:  HOTKEYS_REGISTERED.load(Ordering::Relaxed),
        hotkeys_active:            s.hotkeys.iter().filter(|h| h.registered).count(),
        recovery_integrations:     RECOVERY_INTEGRATIONS.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_startup_launch_sets_flag() {
        register_startup_launch("C:\\jarvis\\jarvis-app.exe");
        assert!(startup_launch_registered());
        unregister_startup_launch();
    }

    #[test]
    fn send_notification_increments_counter() {
        let before = NATIVE_NOTIFICATIONS_SENT.load(Ordering::Relaxed);
        send_notification("Test", "Body text", None);
        assert!(NATIVE_NOTIFICATIONS_SENT.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn dispatch_hotkey_returns_action() {
        let action = dispatch_hotkey("Ctrl+Shift+J");
        assert_eq!(action.as_deref(), Some("wake_jarvis"));
    }

    #[test]
    fn dispatch_unknown_hotkey_returns_none() {
        assert!(dispatch_hotkey("Alt+F42").is_none());
    }

    #[test]
    fn register_custom_hotkey() {
        let id = register_hotkey("Ctrl+Alt+R", "open_research");
        assert!(id > 0);
        let action = dispatch_hotkey("Ctrl+Alt+R");
        assert_eq!(action.as_deref(), Some("open_research"));
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(s.hotkeys_active > 0);
    }
}
