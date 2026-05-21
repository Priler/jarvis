//! In-process typed message bus for the modular service platform.
//!
//! Provides a ring-buffer-backed bus with deterministic monotonic sequence
//! numbers and millisecond timestamps.  All service-layer events flow through
//! this bus — no hidden cross-service coupling.
//!
//! Properties:
//!   - Non-blocking publish (drops oldest on ring overflow, increments BUS_DROPPED)
//!   - Typed `BusEvent` with structured fields
//!   - Monotonic sequence numbers (never reset, never skip)
//!   - Separate from `bus.rs` (CognitiveBus handles goal/plan events)

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use once_cell::sync::Lazy;
use parking_lot::Mutex;

// ── Service model ─────────────────────────────────────────────────────────────

/// Logical service identifiers for the modular runtime platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum ServiceId {
    Audio,
    Wake,
    Stt,
    Command,
    Adaptive,
    Watchdog,
    Orchestration,
    Ui,
}

impl ServiceId {
    pub fn all() -> &'static [ServiceId] {
        &[
            ServiceId::Audio,
            ServiceId::Wake,
            ServiceId::Stt,
            ServiceId::Command,
            ServiceId::Adaptive,
            ServiceId::Watchdog,
            ServiceId::Orchestration,
            ServiceId::Ui,
        ]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ServiceId::Audio         => "audio",
            ServiceId::Wake          => "wake",
            ServiceId::Stt           => "stt",
            ServiceId::Command       => "command",
            ServiceId::Adaptive      => "adaptive",
            ServiceId::Watchdog      => "watchdog",
            ServiceId::Orchestration => "orchestration",
            ServiceId::Ui            => "ui",
        }
    }
}

/// Health / lifecycle state of a logical service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum ServiceState {
    Healthy,
    Degraded,
    Failed,
    Recovering,
    Isolated,
}

impl ServiceState {
    pub fn as_str(self) -> &'static str {
        match self {
            ServiceState::Healthy    => "healthy",
            ServiceState::Degraded   => "degraded",
            ServiceState::Failed     => "failed",
            ServiceState::Recovering => "recovering",
            ServiceState::Isolated   => "isolated",
        }
    }

    /// True if the service is operationally usable.
    pub fn is_operational(self) -> bool {
        matches!(self, ServiceState::Healthy | ServiceState::Degraded)
    }
}

// ── Bus event types ───────────────────────────────────────────────────────────

/// Typed bus event for the service platform infrastructure layer.
#[derive(Clone, Debug, serde::Serialize)]
pub enum BusEvent {
    /// A service transitioned between lifecycle states.
    ServiceStateChanged {
        service: ServiceId,
        from:    ServiceState,
        to:      ServiceState,
        reason:  String,
    },
    /// Periodic health score report for a service (0–100).
    ServiceHealthScore {
        service: ServiceId,
        score:   u8,
    },
    /// A recovery action was taken for a service.
    RecoveryAction {
        service: ServiceId,
        action:  String,
    },
    /// Heartbeat pulse — service is alive.
    Heartbeat {
        service: ServiceId,
    },
    /// Bus ring overflow — oldest messages were dropped.
    BusCongestion {
        dropped: u64,
    },
    /// Graceful shutdown signal.
    Shutdown,
}

// ── Bus message envelope ──────────────────────────────────────────────────────

/// Envelope wrapping a `BusEvent` with deterministic ordering metadata.
#[derive(Clone, Debug, serde::Serialize)]
pub struct BusMessage {
    /// Monotonic sequence number.  Never resets, never skips.
    pub seq:   u64,
    /// Wallclock timestamp — milliseconds since Unix epoch.
    pub ts_ms: u64,
    pub event: BusEvent,
}

// ── Bus internals ─────────────────────────────────────────────────────────────

const BUS_CAPACITY: usize = 512;

pub static BUS_PUBLISHED: AtomicU64 = AtomicU64::new(0);
pub static BUS_DROPPED:   AtomicU64 = AtomicU64::new(0);

static SEQ:  AtomicU64 = AtomicU64::new(0);
static RING: Lazy<Mutex<VecDeque<BusMessage>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(BUS_CAPACITY)));

// ── Public API ────────────────────────────────────────────────────────────────

/// Publish an event to the bus.
///
/// Non-blocking.  Evicts the oldest message on ring overflow.
pub fn publish(event: BusEvent) {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let msg = BusMessage { seq, ts_ms: now_ms(), event };
    let mut ring = RING.lock();
    if ring.len() >= BUS_CAPACITY {
        ring.pop_front();
        BUS_DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    ring.push_back(msg);
    BUS_PUBLISHED.fetch_add(1, Ordering::Relaxed);
}

/// Return up to `n` most-recent messages in chronological order.
pub fn drain_recent(n: usize) -> Vec<BusMessage> {
    let ring = RING.lock();
    let skip = ring.len().saturating_sub(n);
    ring.iter().skip(skip).cloned().collect()
}

/// Total messages published since startup.
pub fn event_count() -> u64 {
    BUS_PUBLISHED.load(Ordering::Relaxed)
}

/// Total messages dropped due to ring overflow.
pub fn dropped_count() -> u64 {
    BUS_DROPPED.load(Ordering::Relaxed)
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
    fn publish_increments_counter() {
        let before = event_count();
        publish(BusEvent::Heartbeat { service: ServiceId::Audio });
        assert!(event_count() > before);
    }

    #[test]
    fn drain_recent_returns_messages() {
        publish(BusEvent::Heartbeat { service: ServiceId::Wake });
        let msgs = drain_recent(4);
        assert!(!msgs.is_empty());
    }

    #[test]
    fn service_state_operational() {
        assert!(ServiceState::Healthy.is_operational());
        assert!(ServiceState::Degraded.is_operational());
        assert!(!ServiceState::Failed.is_operational());
        assert!(!ServiceState::Isolated.is_operational());
        assert!(!ServiceState::Recovering.is_operational());
    }

    #[test]
    fn service_id_all_has_8_variants() {
        assert_eq!(ServiceId::all().len(), 8);
    }

    #[test]
    fn messages_are_monotonically_ordered() {
        publish(BusEvent::Heartbeat { service: ServiceId::Stt });
        publish(BusEvent::Heartbeat { service: ServiceId::Command });
        let msgs = drain_recent(8);
        let seqs: Vec<u64> = msgs.iter().map(|m| m.seq).collect();
        for w in seqs.windows(2) {
            assert!(w[0] < w[1], "sequence not monotonic: {:?}", w);
        }
    }
}
