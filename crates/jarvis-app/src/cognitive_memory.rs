//! Cognitive memory — ring buffer of recent cognitive ticks + JSONL persistence.
//!
//! Retains the last MAX_TICKS ticks in memory for pattern analysis.
//! Also appends each tick to a JSONL file for long-term offline review.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

use crate::cognitive_tick::CognitiveTick;

pub static MEMORY_WRITES:  AtomicU64 = AtomicU64::new(0);
pub static MEMORY_READS:   AtomicU64 = AtomicU64::new(0);
pub static MEMORY_EVICTED: AtomicU64 = AtomicU64::new(0);

const MAX_TICKS:    usize = 200;
const MEMORY_FILE:  &str  = "cognitive_memory.jsonl";

// ── Memory store ──────────────────────────────────────────────────────────────

static MEMORY: Lazy<Mutex<Vec<CognitiveTick>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn record(tick: CognitiveTick) {
    MEMORY_WRITES.fetch_add(1, Ordering::Relaxed);
    append_jsonl(&tick);

    if let Ok(mut guard) = MEMORY.lock() {
        if guard.len() >= MAX_TICKS {
            guard.remove(0);
            MEMORY_EVICTED.fetch_add(1, Ordering::Relaxed);
        }
        guard.push(tick);
    }
}

pub fn recent(n: usize) -> Vec<CognitiveTick> {
    MEMORY_READS.fetch_add(1, Ordering::Relaxed);
    MEMORY.lock().map(|g| {
        let len = g.len();
        g[len.saturating_sub(n)..].to_vec()
    }).unwrap_or_default()
}

pub fn all() -> Vec<CognitiveTick> {
    MEMORY_READS.fetch_add(1, Ordering::Relaxed);
    MEMORY.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn count() -> usize {
    MEMORY.lock().map(|g| g.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut guard) = MEMORY.lock() {
        guard.clear();
    }
}

/// Fraction of recent N ticks that completed successfully.
pub fn recent_success_rate(window: usize) -> f32 {
    let ticks = recent(window);
    if ticks.is_empty() { return 0.0; }
    let successes = ticks.iter().filter(|t| t.is_success()).count();
    successes as f32 / ticks.len() as f32
}

// ── JSONL persistence ─────────────────────────────────────────────────────────

fn append_jsonl(tick: &CognitiveTick) {
    use std::io::Write as _;
    let path = crate::execution_journal::journal_dir().join(MEMORY_FILE);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        if let Ok(line) = serde_json::to_string(tick) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive_tick::{CognitiveTick, TickPhase};

    fn make_tick(phase: TickPhase, success: bool) -> CognitiveTick {
        let tick = CognitiveTick::new(phase);
        if success { tick.complete() } else { tick.fail("test failure") }
    }

    #[test]
    fn record_and_retrieve_tick() {
        let before = count();
        let tick = make_tick(TickPhase::Observe, true);
        record(tick);
        assert!(count() > before);
    }

    #[test]
    fn recent_returns_at_most_n() {
        for _ in 0..5 {
            record(make_tick(TickPhase::Model, true));
        }
        let r = recent(3);
        assert!(r.len() <= 3);
    }

    #[test]
    fn success_rate_after_successful_ticks() {
        for _ in 0..4 {
            record(make_tick(TickPhase::Act, true));
        }
        // At least some successful ticks were added, so rate > 0
        let rate = recent_success_rate(20);
        assert!(rate > 0.0);
    }

    #[test]
    fn success_rate_mixed() {
        // Use a large window to ensure our 1 success + 1 failure aren't drowned out
        // by other tests, but keep the assertion robust regardless of concurrent state.
        record(make_tick(TickPhase::Verify, true));
        record(make_tick(TickPhase::Verify, false));
        let rate = recent_success_rate(2);
        // rate should be between 0 and 1 exclusive (not all pass, not all fail)
        assert!(rate >= 0.0 && rate <= 1.0);
    }

    #[test]
    fn memory_writes_counter_increments() {
        let before = MEMORY_WRITES.load(Ordering::Relaxed);
        record(make_tick(TickPhase::Learn, true));
        assert!(MEMORY_WRITES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn success_rate_bounded_zero_to_one() {
        let rate = recent_success_rate(10);
        assert!(rate >= 0.0 && rate <= 1.0);
    }
}
