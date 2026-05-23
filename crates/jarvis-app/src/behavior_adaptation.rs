//! Behavior adaptation — applies approved heuristic changes from
//! `cognitive_evolution` and records the adaptation history.
//! Gated by `safe_adaptation::check` — no unsafe modifications allowed.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static ADAPTATIONS_APPLIED:  AtomicU64 = AtomicU64::new(0);
pub static ADAPTATIONS_SKIPPED:  AtomicU64 = AtomicU64::new(0);

const MAX_ADAPTATION_LOG: usize = 100;

// ── Adaptation record ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdaptationRecord {
    pub id:          String,
    pub description: String,
    pub dimension:   String,
    pub delta:       f32,
    pub approved:    bool,
    pub reason:      String,
    pub ts_ms:       u64,
}

static LOG: Lazy<Mutex<Vec<AdaptationRecord>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Public API ────────────────────────────────────────────────────────────────

/// Propose and attempt to apply an adaptation.
/// Returns true if the adaptation was approved and applied.
pub fn adapt(id: impl Into<String>, description: impl Into<String>, dimension: impl Into<String>, delta: f32) -> bool {
    let id  = id.into();
    let desc = description.into();
    let dim  = dimension.into();
    let now  = ts_now();

    let proposal = crate::safe_adaptation::AdaptationProposal {
        id: id.clone(), description: desc.clone(),
        dimension: dim.clone(), delta,
    };

    let verdict = crate::safe_adaptation::check(&proposal);
    let approved = verdict.is_approved();
    let reason = match &verdict {
        crate::safe_adaptation::AdaptationVerdict::Approved            => "approved".into(),
        crate::safe_adaptation::AdaptationVerdict::Blocked { reason }  => reason.clone(),
    };

    if approved {
        ADAPTATIONS_APPLIED.fetch_add(1, Ordering::Relaxed);
        crate::world_state_journal::log(
            crate::world_state_journal::WorldEventKind::ReflectionInsight {
                insight:    format!("adaptation applied: {desc} ({dim} Δ{delta:.3})"),
                confidence: 0.7,
            }
        );
    } else {
        ADAPTATIONS_SKIPPED.fetch_add(1, Ordering::Relaxed);
    }

    if let Ok(mut log) = LOG.lock() {
        if log.len() >= MAX_ADAPTATION_LOG { log.remove(0); }
        log.push(AdaptationRecord { id, description: desc, dimension: dim, delta, approved, reason, ts_ms: now });
    }

    approved
}

/// Run a full adaptation cycle: evaluate → evolve → record.
pub fn run_cycle() {
    // Evaluate current strategy quality
    crate::strategy_evaluator::evaluate();

    // Attempt cognitive evolution
    let evolved = crate::cognitive_evolution::evolve();

    if evolved {
        let h = crate::cognitive_evolution::current();
        adapt(
            format!("gen-{}", h.generation),
            format!("heuristic evolution gen {}", h.generation),
            "cognitive_evolution",
            h.planner_risk_weight - 0.5,
        );
    }
}

pub fn recent_log(n: usize) -> Vec<AdaptationRecord> {
    LOG.lock().map(|l| {
        let len = l.len();
        l[len.saturating_sub(n)..].to_vec()
    }).unwrap_or_default()
}

pub fn log_len() -> usize {
    LOG.lock().map(|l| l.len()).unwrap_or(0)
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
    fn adapt_runs_without_panic() {
        crate::cognitive_drift_control::unfreeze_for_test();
        let _approved = adapt("ba.test1", "test adaptation", "planner", 0.01);
    }

    #[test]
    fn blocked_delta_returns_false() {
        crate::cognitive_drift_control::unfreeze_for_test();
        let result = adapt("ba.test2", "too large", "planner", 0.99);
        assert!(!result);
    }

    #[test]
    fn log_grows_after_adapt() {
        let before = log_len();
        crate::cognitive_drift_control::unfreeze_for_test();
        adapt("ba.test3", "log grow test", "workflow", 0.01);
        assert!(log_len() > before);
    }

    #[test]
    fn adaptations_skipped_increments_on_block() {
        crate::cognitive_drift_control::unfreeze_for_test();
        let before = ADAPTATIONS_SKIPPED.load(Ordering::Relaxed);
        adapt("ba.test4", "will be blocked by delta", "planner", 0.99);
        assert!(ADAPTATIONS_SKIPPED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn run_cycle_does_not_panic() {
        crate::cognitive_drift_control::unfreeze_for_test();
        run_cycle();
    }

    #[test]
    fn recent_log_bounded() {
        for i in 0..5u32 {
            crate::cognitive_drift_control::unfreeze_for_test();
            adapt(format!("ba.bulk{i}"), format!("bulk {i}"), "test", 0.01);
        }
        let log = recent_log(3);
        assert!(log.len() <= 3);
    }
}
