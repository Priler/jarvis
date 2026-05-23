//! Permission runtime — explicit user approval required before sensitive operations.
//! No operation is silently executed; each sensitive action goes through the gate.

use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static PERMISSIONS_CHECKED:  AtomicU64 = AtomicU64::new(0);
pub static PERMISSIONS_GRANTED:  AtomicU64 = AtomicU64::new(0);
pub static PERMISSIONS_DENIED:   AtomicU64 = AtomicU64::new(0);
pub static PERMISSIONS_PENDING:  AtomicU64 = AtomicU64::new(0);

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PermissionKind {
    FileRead,
    FileWrite,
    TerminalExec,
    DesktopControl,
    NetworkLocal,
    BrowserControl,
    SystemCommand,
    MemoryWrite,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PermissionState { Granted, Denied, Pending, NotSet }

#[derive(Debug, Clone, serde::Serialize)]
pub struct PermissionEntry {
    pub kind:       PermissionKind,
    pub state:      PermissionState,
    pub resource:   String,
    pub granted_at: Option<u64>,
    pub denied_at:  Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PermissionRequest {
    pub id:       u64,
    pub kind:     PermissionKind,
    pub resource: String,
    pub reason:   String,
    pub ts_ms:    u64,
}

struct PermState {
    table:   HashMap<(PermissionKind, String), PermissionEntry>,
    pending: Vec<PermissionRequest>,
    seq:     u64,
}

static STATE: Lazy<Mutex<PermState>> = Lazy::new(|| Mutex::new(PermState {
    table:   HashMap::new(),
    pending: Vec::new(),
    seq:     0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Check if a permission is granted for a specific resource.
pub fn is_granted(kind: &PermissionKind, resource: &str) -> bool {
    PERMISSIONS_CHECKED.fetch_add(1, Ordering::Relaxed);
    let s = STATE.lock().unwrap();
    matches!(
        s.table.get(&(kind.clone(), resource.to_string())),
        Some(e) if e.state == PermissionState::Granted
    )
}

/// Request a permission. Returns request ID. Caller must await user approval.
pub fn request(kind: PermissionKind, resource: &str, reason: &str) -> u64 {
    let mut s = STATE.lock().unwrap();
    s.seq += 1;
    let id = s.seq;
    s.pending.push(PermissionRequest {
        id,
        kind,
        resource: resource.to_string(),
        reason:   reason.to_string(),
        ts_ms:    ts_now(),
    });
    PERMISSIONS_PENDING.fetch_add(1, Ordering::Relaxed);
    id
}

/// Grant a pending permission request.
pub fn grant(kind: &PermissionKind, resource: &str) {
    let mut s = STATE.lock().unwrap();
    s.table.insert((kind.clone(), resource.to_string()), PermissionEntry {
        kind:       kind.clone(),
        state:      PermissionState::Granted,
        resource:   resource.to_string(),
        granted_at: Some(ts_now()),
        denied_at:  None,
    });
    s.pending.retain(|p| !(&p.kind == kind && p.resource == resource));
    PERMISSIONS_GRANTED.fetch_add(1, Ordering::Relaxed);
    let pending = PERMISSIONS_PENDING.load(Ordering::Relaxed);
    if pending > 0 { PERMISSIONS_PENDING.fetch_sub(1, Ordering::Relaxed); }
}

/// Deny a permission.
pub fn deny(kind: &PermissionKind, resource: &str) {
    let mut s = STATE.lock().unwrap();
    s.table.insert((kind.clone(), resource.to_string()), PermissionEntry {
        kind:       kind.clone(),
        state:      PermissionState::Denied,
        resource:   resource.to_string(),
        granted_at: None,
        denied_at:  Some(ts_now()),
    });
    s.pending.retain(|p| !(&p.kind == kind && p.resource == resource));
    PERMISSIONS_DENIED.fetch_add(1, Ordering::Relaxed);
    let pending = PERMISSIONS_PENDING.load(Ordering::Relaxed);
    if pending > 0 { PERMISSIONS_PENDING.fetch_sub(1, Ordering::Relaxed); }
}

/// Revoke a previously granted permission.
pub fn revoke(kind: &PermissionKind, resource: &str) {
    STATE.lock().unwrap().table.remove(&(kind.clone(), resource.to_string()));
}

pub fn pending_requests() -> Vec<PermissionRequest> {
    STATE.lock().unwrap().pending.clone()
}

pub fn all_entries() -> Vec<PermissionEntry> {
    STATE.lock().unwrap().table.values().cloned().collect()
}

pub fn get_state(kind: &PermissionKind, resource: &str) -> PermissionState {
    STATE.lock().unwrap()
        .table.get(&(kind.clone(), resource.to_string()))
        .map(|e| e.state.clone())
        .unwrap_or(PermissionState::NotSet)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_not_granted_by_default() {
        assert!(!is_granted(&PermissionKind::FileWrite, "/sensitive/file.txt"));
    }

    #[test]
    fn grant_and_check() {
        grant(&PermissionKind::FileRead, "/test/read.txt");
        assert!(is_granted(&PermissionKind::FileRead, "/test/read.txt"));
    }

    #[test]
    fn deny_blocks_permission() {
        deny(&PermissionKind::TerminalExec, "rm -rf");
        assert!(!is_granted(&PermissionKind::TerminalExec, "rm -rf"));
        assert_eq!(get_state(&PermissionKind::TerminalExec, "rm -rf"), PermissionState::Denied);
    }

    #[test]
    fn request_adds_to_pending() {
        let id = request(PermissionKind::DesktopControl, "screen", "need to read screen");
        assert!(id > 0);
        let pending = pending_requests();
        assert!(!pending.is_empty());
    }

    #[test]
    fn revoke_removes_grant() {
        grant(&PermissionKind::NetworkLocal, "127.0.0.1");
        revoke(&PermissionKind::NetworkLocal, "127.0.0.1");
        assert_eq!(get_state(&PermissionKind::NetworkLocal, "127.0.0.1"), PermissionState::NotSet);
    }
}
