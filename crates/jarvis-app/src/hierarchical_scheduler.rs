//! Hierarchical cognition scheduler — governs per-layer tick cadences.
//! Separate from meta_scheduler (which governs meta-cognition subsystems).
//! Ensures each layer runs at its appropriate frequency without interference.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use crate::cognition_layers::CognitionLayer;

pub static HIER_TICKS:     AtomicU64 = AtomicU64::new(0);
pub static LAYERS_SKIPPED: AtomicU64 = AtomicU64::new(0);
pub static LAYERS_RUN:     AtomicU64 = AtomicU64::new(0);

// Per-layer tick cadence (ms)
fn layer_cadence(layer: CognitionLayer) -> u64 {
    match layer {
        CognitionLayer::Reactive    =>   500,   // fast: 2x/s
        CognitionLayer::Tactical    => 1_000,   // 1x/s
        CognitionLayer::Strategic   => 5_000,   // 1x/5s
        CognitionLayer::Meta        => 3_000,   // 1x/3s (driven by meta_scheduler)
        CognitionLayer::Supervisory => 2_000,   // 1x/2s
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct HierSched {
    last_run: HashMap<u8, u64>,   // CognitionLayer as u8 → last_run_ts_ms
}

static STATE: Lazy<Mutex<HierSched>> = Lazy::new(|| Mutex::new(HierSched {
    last_run: HashMap::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns true if the given layer is due to run.  Records last-run if due.
pub fn is_due(layer: CognitionLayer) -> bool {
    HIER_TICKS.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();
    let key = layer as u8;
    if let Ok(mut s) = STATE.lock() {
        let last = s.last_run.get(&key).copied().unwrap_or(0);
        if now.saturating_sub(last) >= layer_cadence(layer) {
            s.last_run.insert(key, now);
            LAYERS_RUN.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            LAYERS_SKIPPED.fetch_add(1, Ordering::Relaxed);
            false
        }
    } else {
        false
    }
}

/// Force a layer to be immediately eligible on next `is_due` call.
pub fn allow_now(layer: CognitionLayer) {
    if let Ok(mut s) = STATE.lock() {
        s.last_run.insert(layer as u8, 0);
    }
}

/// Time remaining (ms) before layer is next due (0 = due now).
pub fn time_until_due(layer: CognitionLayer) -> u64 {
    let now = ts_now();
    STATE.lock().map(|s| {
        let last = s.last_run.get(&(layer as u8)).copied().unwrap_or(0);
        layer_cadence(layer).saturating_sub(now.saturating_sub(last))
    }).unwrap_or(0)
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
    fn reactive_due_after_allow_now() {
        allow_now(CognitionLayer::Reactive);
        assert!(is_due(CognitionLayer::Reactive));
    }

    #[test]
    fn second_call_within_cadence_skips() {
        allow_now(CognitionLayer::Supervisory);
        assert!(is_due(CognitionLayer::Supervisory));
        assert!(!is_due(CognitionLayer::Supervisory));
    }

    #[test]
    fn time_until_due_after_run_is_positive() {
        allow_now(CognitionLayer::Strategic);
        is_due(CognitionLayer::Strategic);
        let remaining = time_until_due(CognitionLayer::Strategic);
        assert!(remaining > 0);
    }
}
