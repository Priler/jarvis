//! Per-service health watchdog registry.
//!
//! Maintains lifecycle state, health score, restart count, and last heartbeat
//! for each logical service.  All state transitions publish events to the
//! runtime bus — no hidden state changes.
//!
//! Service states:
//!   HEALTHY    — heartbeat received, health_score ≥ 60
//!   DEGRADED   — health_score < 60 or missed heartbeat
//!   FAILED     — explicitly marked failed by caller
//!   RECOVERING — recovery action in progress
//!   ISOLATED   — quarantined; no automatic recovery

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::runtime_bus::{self, BusEvent, ServiceId, ServiceState};

// ── Configuration ─────────────────────────────────────────────────────────────

/// Heartbeat timeout: a service with no heartbeat for this long → DEGRADED.
const HEARTBEAT_TIMEOUT_S: u64 = 60;

// ── Counters ──────────────────────────────────────────────────────────────────

pub static TOTAL_RESTARTS:          AtomicU64 = AtomicU64::new(0);
pub static TOTAL_ISOLATIONS:        AtomicU64 = AtomicU64::new(0);
pub static TOTAL_STATE_TRANSITIONS: AtomicU64 = AtomicU64::new(0);

// ── Registry ──────────────────────────────────────────────────────────────────

struct ServiceEntry {
    state:          ServiceState,
    health_score:   u8,
    restart_count:  u32,
    last_heartbeat: Option<Instant>,
}

impl ServiceEntry {
    fn new() -> Self {
        Self {
            state:          ServiceState::Healthy,
            health_score:   100,
            restart_count:  0,
            last_heartbeat: None,
        }
    }
}

static REGISTRY: Lazy<Mutex<HashMap<ServiceId, ServiceEntry>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for &id in ServiceId::all() {
        m.insert(id, ServiceEntry::new());
    }
    Mutex::new(m)
});

// ── Public read-only snapshot ─────────────────────────────────────────────────

pub struct ServiceSnapshot {
    pub id:            ServiceId,
    pub state:         ServiceState,
    pub health_score:  u8,
    pub restart_count: u32,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Record a heartbeat from a service.
///
/// Clears DEGRADED state caused by a previous heartbeat timeout.
pub fn heartbeat(service: ServiceId) {
    let mut reg = REGISTRY.lock();
    if let Some(entry) = reg.get_mut(&service) {
        let prev = entry.state;
        entry.last_heartbeat = Some(Instant::now());
        if prev == ServiceState::Degraded {
            do_transition(entry, service, prev, ServiceState::Healthy, "heartbeat_recovered");
        }
    }
}

/// Report a health score (0–100) for a service.
///
/// score < 60  and state == Healthy   → Degraded
/// score < 25  and state == Degraded  → Failed
pub fn report_health(service: ServiceId, score: u8) {
    let mut reg = REGISTRY.lock();
    if let Some(entry) = reg.get_mut(&service) {
        entry.health_score = score;
        let prev = entry.state;
        if prev == ServiceState::Healthy && score < 60 {
            do_transition(entry, service, prev, ServiceState::Degraded,
                &format!("health_score={}", score));
        } else if prev == ServiceState::Degraded && score < 25 {
            do_transition(entry, service, prev, ServiceState::Failed,
                &format!("health_score={}", score));
        }
        runtime_bus::publish(BusEvent::ServiceHealthScore { service, score });
    }
}

/// Mark a service as FAILED.  No-op if already FAILED or ISOLATED.
pub fn mark_failed(service: ServiceId, reason: &str) {
    let mut reg = REGISTRY.lock();
    if let Some(entry) = reg.get_mut(&service) {
        let prev = entry.state;
        if prev != ServiceState::Failed && prev != ServiceState::Isolated {
            do_transition(entry, service, prev, ServiceState::Failed, reason);
        }
    }
}

/// Mark a service as RECOVERING (increments restart_count).
pub fn mark_recovering(service: ServiceId, reason: &str) {
    let mut reg = REGISTRY.lock();
    if let Some(entry) = reg.get_mut(&service) {
        let prev = entry.state;
        entry.restart_count += 1;
        TOTAL_RESTARTS.fetch_add(1, Ordering::Relaxed);
        do_transition(entry, service, prev, ServiceState::Recovering, reason);
    }
}

/// Mark a service as HEALTHY (used after successful recovery).
pub fn mark_healthy(service: ServiceId, reason: &str) {
    let mut reg = REGISTRY.lock();
    if let Some(entry) = reg.get_mut(&service) {
        let prev = entry.state;
        do_transition(entry, service, prev, ServiceState::Healthy, reason);
    }
}

/// Isolate a service — quarantines it from automatic recovery.
pub fn isolate(service: ServiceId, reason: &str) {
    let mut reg = REGISTRY.lock();
    if let Some(entry) = reg.get_mut(&service) {
        let prev = entry.state;
        if prev != ServiceState::Isolated {
            TOTAL_ISOLATIONS.fetch_add(1, Ordering::Relaxed);
            do_transition(entry, service, prev, ServiceState::Isolated, reason);
        }
    }
}

/// Get the current state of a service.
pub fn get_state(service: ServiceId) -> ServiceState {
    REGISTRY.lock().get(&service).map(|e| e.state).unwrap_or(ServiceState::Failed)
}

/// Get the current health score of a service.
pub fn get_health_score(service: ServiceId) -> u8 {
    REGISTRY.lock().get(&service).map(|e| e.health_score).unwrap_or(0)
}

/// Check all HEALTHY services for missed heartbeats; degrade on timeout.
///
/// Called periodically by the orchestration monitor (≈ every 30 s).
pub fn check_heartbeat_timeouts() {
    let timeout = Duration::from_secs(HEARTBEAT_TIMEOUT_S);
    let mut reg = REGISTRY.lock();
    // Collect IDs that need degrading (can't mutate during iteration).
    let degrade: Vec<ServiceId> = reg.iter()
        .filter(|(_, e)| {
            e.state == ServiceState::Healthy
                && e.last_heartbeat.map_or(false, |t| t.elapsed() > timeout)
        })
        .map(|(&id, _)| id)
        .collect();
    for id in degrade {
        if let Some(entry) = reg.get_mut(&id) {
            let prev = entry.state;
            do_transition(entry, id, prev, ServiceState::Degraded, "heartbeat_timeout");
        }
    }
}

/// Snapshot of all service states for observability.
pub fn snapshot() -> Vec<ServiceSnapshot> {
    REGISTRY.lock()
        .iter()
        .map(|(&id, e)| ServiceSnapshot {
            id,
            state:         e.state,
            health_score:  e.health_score,
            restart_count: e.restart_count,
        })
        .collect()
}

// ── Transition helper ─────────────────────────────────────────────────────────

/// Apply a state transition, publish to bus, and log.
///
/// Called exclusively while `REGISTRY` lock is held.
/// Bus publish is non-blocking, so no deadlock risk.
fn do_transition(
    entry:   &mut ServiceEntry,
    id:      ServiceId,
    from:    ServiceState,
    to:      ServiceState,
    reason:  &str,
) {
    entry.state = to;
    TOTAL_STATE_TRANSITIONS.fetch_add(1, Ordering::Relaxed);
    runtime_bus::publish(BusEvent::ServiceStateChanged {
        service: id,
        from,
        to,
        reason: reason.to_string(),
    });
    info!(
        "[SVC-WD] {} {:?} → {:?} reason={}",
        id.as_str(), from, to, reason
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_8_services() {
        assert_eq!(snapshot().len(), 8);
    }

    #[test]
    fn isolate_increments_counter() {
        let before = TOTAL_ISOLATIONS.load(Ordering::Relaxed);
        // Reset Ui service to Healthy so isolation fires.
        mark_healthy(ServiceId::Ui, "test_reset");
        isolate(ServiceId::Ui, "test_isolation");
        assert!(TOTAL_ISOLATIONS.load(Ordering::Relaxed) > before);
        // Restore for other tests.
        mark_healthy(ServiceId::Ui, "test_restore");
    }

    #[test]
    fn mark_failed_sets_state() {
        mark_healthy(ServiceId::Stt, "test_reset");
        mark_failed(ServiceId::Stt, "test_fail");
        assert_eq!(get_state(ServiceId::Stt), ServiceState::Failed);
        mark_healthy(ServiceId::Stt, "test_restore");
    }

    #[test]
    fn heartbeat_clears_degraded() {
        mark_healthy(ServiceId::Audio, "test_reset");
        // Force Degraded via low health score.
        report_health(ServiceId::Audio, 30);
        assert_eq!(get_state(ServiceId::Audio), ServiceState::Degraded);
        heartbeat(ServiceId::Audio);
        assert_eq!(get_state(ServiceId::Audio), ServiceState::Healthy);
    }
}
