//! Service-level observability for the modular runtime platform.
//!
//! Reads service state from the watchdog registry and from the runtime bus
//! ring to produce structured JSON event logs.
//!
//! Output files (in `APP_LOG_DIR`):
//!   - `service_events.jsonl`    — service state transitions and recovery actions
//!   - `service_snapshot.json`   — point-in-time state of all services (overwrite)
//!
//! Design: observability reads from `service_watchdogs` (snapshot) and the
//! runtime bus ring (events).  It does NOT write to the watchdog registry —
//! strictly read-only from the perspective of the service registry.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use crate::runtime_bus::{self, BusEvent, ServiceId, ServiceState};
use crate::service_watchdogs;

// ── Counters ──────────────────────────────────────────────────────────────────

pub static EVENTS_FLUSHED: AtomicU64 = AtomicU64::new(0);

// ── Bus event flusher ─────────────────────────────────────────────────────────

/// Flush new bus events since `*last_seq` to `service_events.jsonl`.
///
/// `last_seq` is owned by the caller (orchestration monitor) and persists
/// between calls, providing at-least-once delivery of bus events to disk.
pub fn flush_bus_events(last_seq: &mut u64) {
    let msgs = runtime_bus::drain_recent(256);
    for msg in &msgs {
        if msg.seq <= *last_seq {
            continue;
        }
        *last_seq = msg.seq;
        let line = match &msg.event {
            BusEvent::ServiceStateChanged { service, from, to, reason } => {
                format!(
                    "{{\"ts\":{},\"seq\":{},\"kind\":\"state_change\",\"service\":\"{}\",\
                     \"from\":\"{}\",\"to\":\"{}\",\"reason\":\"{}\"}}",
                    msg.ts_ms, msg.seq, service.as_str(),
                    from.as_str(), to.as_str(),
                    reason.replace('"', "\\\""),
                )
            }
            BusEvent::RecoveryAction { service, action } => {
                format!(
                    "{{\"ts\":{},\"seq\":{},\"kind\":\"recovery\",\"service\":\"{}\",\"action\":\"{}\"}}",
                    msg.ts_ms, msg.seq, service.as_str(),
                    action.replace('"', "\\\""),
                )
            }
            BusEvent::ServiceHealthScore { service, score } => {
                format!(
                    "{{\"ts\":{},\"seq\":{},\"kind\":\"health\",\"service\":\"{}\",\"score\":{}}}",
                    msg.ts_ms, msg.seq, service.as_str(), score,
                )
            }
            BusEvent::BusCongestion { dropped } => {
                format!(
                    "{{\"ts\":{},\"seq\":{},\"kind\":\"bus_congestion\",\"dropped\":{}}}",
                    msg.ts_ms, msg.seq, dropped,
                )
            }
            BusEvent::Shutdown => {
                format!(
                    "{{\"ts\":{},\"seq\":{},\"kind\":\"shutdown\"}}",
                    msg.ts_ms, msg.seq,
                )
            }
            _ => continue,
        };
        append_jsonl("service_events.jsonl", &line);
        EVENTS_FLUSHED.fetch_add(1, Ordering::Relaxed);
    }
}

// ── Snapshot writer ───────────────────────────────────────────────────────────

/// Write a JSON snapshot of all service states to `service_snapshot.json`.
///
/// Overwrites on each call — always reflects latest state.
pub fn write_service_snapshot() {
    let entries    = service_watchdogs::snapshot();
    let published  = runtime_bus::event_count();
    let dropped    = runtime_bus::dropped_count();
    let ts         = now_ms();

    let mut parts: Vec<String> = entries.iter().map(|s| {
        format!(
            "{{\"service\":\"{}\",\"state\":\"{}\",\"health\":{},\"restarts\":{}}}",
            s.id.as_str(),
            s.state.as_str(),
            s.health_score,
            s.restart_count,
        )
    }).collect();
    parts.sort();

    let json = format!(
        "{{\"ts\":{},\"bus_published\":{},\"bus_dropped\":{},\"services\":[{}]}}",
        ts, published, dropped, parts.join(","),
    );

    if let Some(dir) = jarvis_core::APP_LOG_DIR.get() {
        let path = dir.join("service_snapshot.json");
        if let Err(e) = std::fs::write(&path, &json) {
            warn!("[SVC-OBS] Failed to write service snapshot: {}", e);
        }
    }
}

// ── Human-readable summary ────────────────────────────────────────────────────

/// Return a human-readable summary of all service states.
pub fn service_summary() -> String {
    let mut entries = service_watchdogs::snapshot();
    entries.sort_by_key(|s| s.id.as_str());
    entries.iter().map(|s| {
        format!(
            "  {:14} {:10} health={:3} restarts={}",
            s.id.as_str(),
            s.state.as_str(),
            s.health_score,
            s.restart_count,
        )
    }).collect::<Vec<_>>().join("\n")
}

/// Return count of services in each state.
pub fn state_counts() -> [u32; 5] {
    let entries = service_watchdogs::snapshot();
    let mut counts = [0u32; 5]; // [healthy, degraded, failed, recovering, isolated]
    for s in &entries {
        match s.state {
            ServiceState::Healthy    => counts[0] += 1,
            ServiceState::Degraded   => counts[1] += 1,
            ServiceState::Failed     => counts[2] += 1,
            ServiceState::Recovering => counts[3] += 1,
            ServiceState::Isolated   => counts[4] += 1,
        }
    }
    counts
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn append_jsonl(filename: &str, line: &str) {
    if let Some(dir) = jarvis_core::APP_LOG_DIR.get() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(dir.join(filename))
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_summary_not_empty() {
        let s = service_summary();
        assert!(!s.is_empty());
    }

    #[test]
    fn service_summary_contains_all_services() {
        let s = service_summary();
        for id in ServiceId::all() {
            assert!(s.contains(id.as_str()), "Missing service: {}", id.as_str());
        }
    }

    #[test]
    fn state_counts_total_8() {
        let counts = state_counts();
        let total: u32 = counts.iter().sum();
        assert_eq!(total, 8);
    }

    #[test]
    fn flush_bus_events_does_not_double_write() {
        let mut last_seq = u64::MAX; // Nothing newer exists.
        let before = EVENTS_FLUSHED.load(Ordering::Relaxed);
        flush_bus_events(&mut last_seq);
        assert_eq!(EVENTS_FLUSHED.load(Ordering::Relaxed), before);
    }
}
