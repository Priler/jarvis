//! Cognitive tick — atomic unit of the cognition loop cycle.
//!
//! Each tick captures a point-in-time observation, the phase it belongs to,
//! and the outcome of that phase.  Ticks are passed between loop stages and
//! recorded in cognitive memory.

use std::sync::atomic::{AtomicU64, Ordering};

pub static TICKS_CREATED:   AtomicU64 = AtomicU64::new(0);
pub static TICKS_COMPLETED: AtomicU64 = AtomicU64::new(0);
pub static TICKS_FAILED:    AtomicU64 = AtomicU64::new(0);

// ── Tick phase ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TickPhase {
    Observe,
    Model,
    Reason,
    Predict,
    Plan,
    Act,
    Verify,
    Learn,
    Adapt,
    Idle,
}

impl std::fmt::Display for TickPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// ── Tick outcome ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TickOutcome {
    Completed,
    Skipped { reason: String },
    Failed   { reason: String },
    Blocked  { reason: String },
}

impl TickOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, TickOutcome::Completed)
    }
}

// ── Cognitive tick ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CognitiveTick {
    pub tick_id:    u64,
    pub ts_ms:      u64,
    pub phase:      TickPhase,
    pub outcome:    TickOutcome,
    pub duration_ms: u64,
    pub notes:      Vec<String>,
}

impl CognitiveTick {
    pub fn new(phase: TickPhase) -> Self {
        let id = TICKS_CREATED.fetch_add(1, Ordering::Relaxed);
        Self {
            tick_id: id,
            ts_ms: ts_now(),
            phase,
            outcome: TickOutcome::Completed,
            duration_ms: 0,
            notes: Vec::new(),
        }
    }

    pub fn complete(mut self) -> Self {
        let elapsed = ts_now().saturating_sub(self.ts_ms);
        self.duration_ms = elapsed;
        self.outcome = TickOutcome::Completed;
        TICKS_COMPLETED.fetch_add(1, Ordering::Relaxed);
        self
    }

    pub fn fail(mut self, reason: impl Into<String>) -> Self {
        let elapsed = ts_now().saturating_sub(self.ts_ms);
        self.duration_ms = elapsed;
        self.outcome = TickOutcome::Failed { reason: reason.into() };
        TICKS_FAILED.fetch_add(1, Ordering::Relaxed);
        self
    }

    pub fn skip(mut self, reason: impl Into<String>) -> Self {
        self.outcome = TickOutcome::Skipped { reason: reason.into() };
        self
    }

    pub fn block(mut self, reason: impl Into<String>) -> Self {
        self.outcome = TickOutcome::Blocked { reason: reason.into() };
        self
    }

    pub fn note(mut self, msg: impl Into<String>) -> Self {
        self.notes.push(msg.into());
        self
    }

    pub fn is_success(&self) -> bool {
        self.outcome.is_success()
    }
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_created_completes_successfully() {
        let tick = CognitiveTick::new(TickPhase::Observe).complete();
        assert!(tick.is_success());
        assert_eq!(tick.phase, TickPhase::Observe);
    }

    #[test]
    fn tick_fail_sets_failed_outcome() {
        let tick = CognitiveTick::new(TickPhase::Act).fail("blocked by safety");
        assert!(!tick.is_success());
        assert!(matches!(tick.outcome, TickOutcome::Failed { .. }));
    }

    #[test]
    fn tick_skip_sets_skipped_outcome() {
        let tick = CognitiveTick::new(TickPhase::Plan).skip("no goals active");
        assert!(matches!(tick.outcome, TickOutcome::Skipped { .. }));
    }

    #[test]
    fn tick_note_appends() {
        let tick = CognitiveTick::new(TickPhase::Idle).note("idle cycle").complete();
        assert_eq!(tick.notes.len(), 1);
    }

    #[test]
    fn ticks_created_counter_increments() {
        let before = TICKS_CREATED.load(Ordering::Relaxed);
        let _ = CognitiveTick::new(TickPhase::Reason);
        assert!(TICKS_CREATED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn tick_phase_display() {
        assert_eq!(TickPhase::Observe.to_string(), "Observe");
        assert_eq!(TickPhase::Idle.to_string(), "Idle");
    }
}
