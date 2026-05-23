//! Service-level recovery engine.
//!
//! Maintains a registry of restart callbacks per logical service.
//! Provides restart, rollback, and isolation primitives.
//!
//! Recovery principles:
//!   1. ISOLATED services are never automatically restarted.
//!   2. A recovery attempt transitions the service to RECOVERING first.
//!   3. If the restart callback returns false, the service is marked FAILED.
//!   4. Every action is published to the runtime bus and appended to disk.
//!   5. No nondeterministic recovery paths — callback result determines outcome.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::runtime_bus::{self, BusEvent, ServiceId, ServiceState};
use crate::service_watchdogs;

// ── Counters ──────────────────────────────────────────────────────────────────

pub static SERVICE_RECOVERIES:        AtomicU64 = AtomicU64::new(0);
pub static SERVICE_RECOVERY_FAILURES: AtomicU64 = AtomicU64::new(0);
pub static SERVICE_ISOLATIONS:        AtomicU64 = AtomicU64::new(0);

// ── Callback registry ─────────────────────────────────────────────────────────

type RestartFn = Box<dyn Fn() -> bool + Send + Sync>;

static CALLBACKS: Lazy<Mutex<HashMap<ServiceId, RestartFn>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// ── Public API ────────────────────────────────────────────────────────────────

/// Register a restart callback for a service.
///
/// The callback must return `true` if the service was successfully restarted.
/// Calling `register` twice for the same service overwrites the old callback.
pub fn register(service: ServiceId, callback: impl Fn() -> bool + Send + Sync + 'static) {
    CALLBACKS.lock().insert(service, Box::new(callback));
}

/// Attempt to restart a single service.
///
/// - ISOLATED services are refused (returns `false`).
/// - Transitions: current → RECOVERING → HEALTHY (success) or FAILED (failure).
/// - If no callback is registered, the service is assumed stateless and marked HEALTHY.
pub fn restart(service: ServiceId, reason: &str) -> bool {
    if service_watchdogs::get_state(service) == ServiceState::Isolated {
        warn!("[SVC-REC] {} ISOLATED — restart refused (reason={})", service.as_str(), reason);
        return false;
    }

    service_watchdogs::mark_recovering(service, reason);
    runtime_bus::publish(BusEvent::RecoveryAction {
        service,
        action: format!("restart reason={}", reason),
    });

    // Call registered callback or succeed silently if none.
    let success = {
        let cbs = CALLBACKS.lock();
        match cbs.get(&service) {
            Some(cb) => cb(),
            None     => true,
        }
    };

    if success {
        service_watchdogs::mark_healthy(service, "restart_succeeded");
        SERVICE_RECOVERIES.fetch_add(1, Ordering::Relaxed);
        info!("[SVC-REC] {} restarted OK", service.as_str());
    } else {
        service_watchdogs::mark_failed(service, "restart_failed");
        SERVICE_RECOVERY_FAILURES.fetch_add(1, Ordering::Relaxed);
        warn!("[SVC-REC] {} restart FAILED", service.as_str());
    }
    write_recovery_event(service, "restart", success, reason);
    success
}

/// Isolate a service that cannot be safely restarted.
///
/// An isolated service stays in ISOLATED until `clear_isolation` is called.
pub fn isolate(service: ServiceId, reason: &str) {
    service_watchdogs::isolate(service, reason);
    SERVICE_ISOLATIONS.fetch_add(1, Ordering::Relaxed);
    runtime_bus::publish(BusEvent::RecoveryAction {
        service,
        action: format!("isolate reason={}", reason),
    });
    warn!("[SVC-REC] {} ISOLATED reason={}", service.as_str(), reason);
    write_recovery_event(service, "isolate", false, reason);
}

/// Clear isolation (operator override).
pub fn clear_isolation(service: ServiceId, reason: &str) {
    service_watchdogs::mark_healthy(service, reason);
    info!("[SVC-REC] {} isolation cleared reason={}", service.as_str(), reason);
}

/// Returns true if the service is in a restartable state (not ISOLATED).
pub fn can_restart(service: ServiceId) -> bool {
    service_watchdogs::get_state(service) != ServiceState::Isolated
}

// ── Event log ─────────────────────────────────────────────────────────────────

fn write_recovery_event(service: ServiceId, action: &str, success: bool, reason: &str) {
    let ts = now_ms();
    let line = format!(
        "{{\"ts\":{},\"service\":\"{}\",\"action\":\"{}\",\"success\":{},\"reason\":\"{}\"}}",
        ts,
        service.as_str(),
        action,
        success,
        reason.replace('"', "\\\""),
    );
    if let Some(dir) = jarvis_core::APP_LOG_DIR.get() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(dir.join("service_recovery.jsonl"))
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn restart_succeeds_with_true_callback() {
        register(ServiceId::Wake, || true);
        service_watchdogs::mark_healthy(ServiceId::Wake, "test_reset");
        let before = SERVICE_RECOVERIES.load(Ordering::Relaxed);
        assert!(restart(ServiceId::Wake, "test_ok"));
        assert!(SERVICE_RECOVERIES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn restart_fails_with_false_callback() {
        register(ServiceId::Command, || false);
        service_watchdogs::mark_healthy(ServiceId::Command, "test_reset");
        let before = SERVICE_RECOVERY_FAILURES.load(Ordering::Relaxed);
        assert!(!restart(ServiceId::Command, "test_fail"));
        assert!(SERVICE_RECOVERY_FAILURES.load(Ordering::Relaxed) > before);
        // Restore for orchestration tests.
        service_watchdogs::mark_healthy(ServiceId::Command, "test_restore");
    }

    #[test]
    fn isolated_service_refuses_restart() {
        service_watchdogs::mark_healthy(ServiceId::Adaptive, "test_reset");
        isolate(ServiceId::Adaptive, "test_isolate");
        assert!(!restart(ServiceId::Adaptive, "test_restart_isolated"));
        // Clean up.
        clear_isolation(ServiceId::Adaptive, "test_cleanup");
    }
}
