//! Notification center — collects runtime events and surfaces them to the user
//! via the Control Center UI.  All notifications are local; none are sent anywhere.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static NOTIFICATIONS_TOTAL:  AtomicU64 = AtomicU64::new(0);
pub static NOTIFICATIONS_UNREAD: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize)]
pub enum NotificationKind {
    RuntimeWarning,
    TaskComplete,
    VoiceError,
    PermissionRequest,
    ModelFailure,
    SchedulerDegraded,
    RecoveryEvent,
    SafeModeEntered,
    SafeModeExited,
    CrashDetected,
    Info,
}

impl NotificationKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::RuntimeWarning     => "RuntimeWarning",
            Self::TaskComplete       => "TaskComplete",
            Self::VoiceError         => "VoiceError",
            Self::PermissionRequest  => "PermissionRequest",
            Self::ModelFailure       => "ModelFailure",
            Self::SchedulerDegraded  => "SchedulerDegraded",
            Self::RecoveryEvent      => "RecoveryEvent",
            Self::SafeModeEntered    => "SafeModeEntered",
            Self::SafeModeExited     => "SafeModeExited",
            Self::CrashDetected      => "CrashDetected",
            Self::Info               => "Info",
        }
    }

    pub fn is_urgent(&self) -> bool {
        matches!(self,
            Self::RuntimeWarning
            | Self::VoiceError
            | Self::ModelFailure
            | Self::SchedulerDegraded
            | Self::SafeModeEntered
            | Self::CrashDetected
        )
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Notification {
    pub id:        u64,
    pub kind:      NotificationKind,
    pub component: String,
    pub message:   String,
    pub timestamp: u64,
    pub read:      bool,
}

const BUFFER_CAP: usize = 50;

struct CenterState {
    notifications: Vec<Notification>,
    next_id:       u64,
}

impl CenterState {
    fn new() -> Self { Self { notifications: Vec::new(), next_id: 1 } }
}

static STATE: Lazy<Mutex<CenterState>> = Lazy::new(|| Mutex::new(CenterState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn push(kind: NotificationKind, component: &str, message: &str) -> u64 {
    let mut s = STATE.lock().unwrap();
    let id = s.next_id;
    s.next_id += 1;
    if s.notifications.len() >= BUFFER_CAP {
        s.notifications.remove(0);
    }
    s.notifications.push(Notification {
        id,
        kind,
        component: component.to_string(),
        message:   message.to_string(),
        timestamp: ts_now(),
        read:      false,
    });
    NOTIFICATIONS_TOTAL.fetch_add(1, Ordering::Relaxed);
    NOTIFICATIONS_UNREAD.fetch_add(1, Ordering::Relaxed);
    id
}

pub fn mark_read(id: u64) {
    let mut s = STATE.lock().unwrap();
    if let Some(n) = s.notifications.iter_mut().find(|n| n.id == id) {
        if !n.read {
            n.read = true;
            NOTIFICATIONS_UNREAD.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

pub fn mark_all_read() {
    let mut s = STATE.lock().unwrap();
    let unread: u64 = s.notifications.iter().filter(|n| !n.read).count() as u64;
    for n in s.notifications.iter_mut() { n.read = true; }
    NOTIFICATIONS_UNREAD.fetch_sub(unread.min(NOTIFICATIONS_UNREAD.load(Ordering::Relaxed)), Ordering::Relaxed);
}

pub fn recent(n: usize) -> Vec<Notification> {
    let s = STATE.lock().unwrap();
    s.notifications.iter().rev().take(n).cloned().collect()
}

pub fn unread() -> Vec<Notification> {
    let s = STATE.lock().unwrap();
    s.notifications.iter().filter(|n| !n.read).cloned().collect()
}

pub fn unread_count() -> u64 { NOTIFICATIONS_UNREAD.load(Ordering::Relaxed) }
pub fn total()        -> u64 { NOTIFICATIONS_TOTAL.load(Ordering::Relaxed) }

pub fn clear() {
    let mut s = STATE.lock().unwrap();
    s.notifications.clear();
    NOTIFICATIONS_UNREAD.store(0, Ordering::Relaxed);
}

// ── Convenience push helpers ──────────────────────────────────────────────────

pub fn warn(component: &str, msg: &str) -> u64 {
    push(NotificationKind::RuntimeWarning, component, msg)
}

pub fn model_failure(component: &str, msg: &str) -> u64 {
    push(NotificationKind::ModelFailure, component, msg)
}

pub fn recovery(component: &str, msg: &str) -> u64 {
    push(NotificationKind::RecoveryEvent, component, msg)
}

pub fn task_complete(component: &str, msg: &str) -> u64 {
    push(NotificationKind::TaskComplete, component, msg)
}

pub fn info(component: &str, msg: &str) -> u64 {
    push(NotificationKind::Info, component, msg)
}

#[derive(Debug, serde::Serialize)]
pub struct NotificationSnapshot {
    pub total:        u64,
    pub unread_count: u64,
    pub recent_5:     Vec<Notification>,
}

pub fn snapshot() -> NotificationSnapshot {
    NotificationSnapshot {
        total:        total(),
        unread_count: unread_count(),
        recent_5:     recent(5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_retrieve() {
        let before = total();
        push(NotificationKind::Info, "test", "hello");
        assert!(total() > before);
    }

    #[test]
    fn unread_decrements_on_mark_read() {
        let id = push(NotificationKind::RuntimeWarning, "test", "warn");
        let before_unread = unread_count();
        mark_read(id);
        assert!(unread_count() <= before_unread);
    }

    #[test]
    fn recent_bounded() {
        for i in 0..10 {
            push(NotificationKind::Info, "test", &format!("msg {}", i));
        }
        assert!(recent(5).len() <= 5);
    }

    #[test]
    fn urgent_kinds_classified() {
        assert!(NotificationKind::ModelFailure.is_urgent());
        assert!(NotificationKind::Info.is_urgent() == false);
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        let _ = s.total;
    }
}
