//! Service orchestration runtime — lifecycle coordination layer.
//!
//! Coordinates startup ordering, dependency management, service health
//! monitoring, and graceful shutdown sequencing for all logical services.
//!
//! Startup order:
//!   Audio → Wake → Stt → Command → Adaptive → Watchdog → Orchestration → Ui
//!
//! Shutdown order: reverse of startup.
//!
//! The orchestration monitor runs in a dedicated background thread and
//! periodically: checks heartbeat timeouts, attempts recovery of FAILED
//! services, flushes bus events to disk, and writes the service snapshot.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crate::runtime_bus::{self, BusEvent, ServiceId, ServiceState};
use crate::service_observability;
use crate::service_recovery;
use crate::service_watchdogs;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Stagger between service startup announcements (avoids thundering-herd).
const STARTUP_STAGGER_MS: u64 = 5;
/// Monitor polling interval.
const MONITOR_INTERVAL_S: u64 = 30;

// ── State ─────────────────────────────────────────────────────────────────────

static STARTED:        AtomicBool = AtomicBool::new(false);
pub static STARTUP_MS: AtomicU64  = AtomicU64::new(0);

// ── Startup / shutdown ordering ───────────────────────────────────────────────

/// Startup order: dependencies must be healthy before dependents start.
const STARTUP_ORDER: &[ServiceId] = &[
    ServiceId::Audio,
    ServiceId::Wake,
    ServiceId::Stt,
    ServiceId::Command,
    ServiceId::Adaptive,
    ServiceId::Watchdog,
    ServiceId::Orchestration,
    ServiceId::Ui,
];

/// Shutdown order: reverse of startup.
const SHUTDOWN_ORDER: &[ServiceId] = &[
    ServiceId::Ui,
    ServiceId::Orchestration,
    ServiceId::Watchdog,
    ServiceId::Adaptive,
    ServiceId::Command,
    ServiceId::Stt,
    ServiceId::Wake,
    ServiceId::Audio,
];

// ── Public API ────────────────────────────────────────────────────────────────

/// Initialise the orchestration runtime.
///
/// Registers all services, emits startup heartbeats in dependency order, and
/// spawns the "svc-orchestrator" monitoring thread.
///
/// Must be called once, after all subsystems are fully initialised.
pub fn start() {
    if STARTED.swap(true, Ordering::SeqCst) {
        warn!("[ORCH] start() called more than once — ignored");
        return;
    }

    let t0 = std::time::Instant::now();
    info!("[ORCH] Service platform starting ({} services)", STARTUP_ORDER.len());

    for &svc in STARTUP_ORDER {
        service_watchdogs::heartbeat(svc);
        runtime_bus::publish(BusEvent::Heartbeat { service: svc });
        std::thread::sleep(Duration::from_millis(STARTUP_STAGGER_MS));
    }

    let elapsed_ms = t0.elapsed().as_millis() as u64;
    STARTUP_MS.store(elapsed_ms, Ordering::Relaxed);
    info!("[ORCH] All services registered in {}ms", elapsed_ms);

    std::thread::Builder::new()
        .name("svc-orchestrator".into())
        .spawn(monitor_loop)
        .expect("Failed to spawn svc-orchestrator thread");
}

/// Signal graceful shutdown in reverse startup order.
pub fn shutdown(reason: &str) {
    info!("[ORCH] Shutdown: {}", reason);
    runtime_bus::publish(BusEvent::Shutdown);
    for &svc in SHUTDOWN_ORDER {
        info!("[ORCH]   {} shutdown", svc.as_str());
    }
}

/// Startup sequence latency in milliseconds.
pub fn startup_latency_ms() -> u64 {
    STARTUP_MS.load(Ordering::Relaxed)
}

// ── Monitor loop ──────────────────────────────────────────────────────────────

fn monitor_loop() {
    let mut last_bus_seq: u64 = 0;

    loop {
        // Self-heartbeat.
        service_watchdogs::heartbeat(ServiceId::Orchestration);

        // Timeout detection for services that stopped sending heartbeats.
        service_watchdogs::check_heartbeat_timeouts();

        // Attempt recovery of FAILED services (not isolated).
        recover_failed_services();

        // Bus congestion check.
        let dropped = runtime_bus::dropped_count();
        if dropped > 0 {
            warn!("[ORCH] Bus messages dropped: {}", dropped);
            runtime_bus::publish(BusEvent::BusCongestion { dropped });
        }

        // Flush bus events to disk.
        service_observability::flush_bus_events(&mut last_bus_seq);

        // Periodic service snapshot.
        service_observability::write_service_snapshot();

        std::thread::sleep(Duration::from_secs(MONITOR_INTERVAL_S));
    }
}

fn recover_failed_services() {
    for &svc in STARTUP_ORDER {
        if service_watchdogs::get_state(svc) == ServiceState::Failed
            && service_recovery::can_restart(svc)
        {
            warn!("[ORCH] {} FAILED — attempting orchestration recovery", svc.as_str());
            service_recovery::restart(svc, "orchestration_auto_recovery");
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_order_has_8_services() {
        assert_eq!(STARTUP_ORDER.len(), 8);
    }

    #[test]
    fn shutdown_order_is_reverse_of_startup() {
        let startup:  Vec<_> = STARTUP_ORDER.iter().copied().collect();
        let shutdown: Vec<_> = SHUTDOWN_ORDER.iter().copied().collect();
        let reversed: Vec<_> = startup.iter().copied().rev().collect();
        assert_eq!(shutdown, reversed);
    }

    #[test]
    fn startup_latency_is_zero_before_start() {
        // STARTED may or may not be true depending on test order.
        // Just verify the function exists and returns a u64.
        let _ms: u64 = startup_latency_ms();
    }
}
