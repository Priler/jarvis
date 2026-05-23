//! Cognitive priority engine — prioritises work across the 6-tier priority scale.
//! safety > stability > recovery > user-critical goals > optimization > background.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static PRIORITY_DECISIONS: AtomicU64 = AtomicU64::new(0);
pub static SAFETY_OVERRIDES:   AtomicU64 = AtomicU64::new(0);
pub static BACKGROUND_DEFERRED: AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 100;

// ── Priority tier ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum CognitivePriority {
    Background     = 0,
    Optimization   = 1,
    UserCritical   = 2,
    Recovery       = 3,
    Stability      = 4,
    Safety         = 5,
}

impl CognitivePriority {
    pub fn label(self) -> &'static str {
        match self {
            Self::Background   => "background",
            Self::Optimization => "optimization",
            Self::UserCritical => "user_critical",
            Self::Recovery     => "recovery",
            Self::Stability    => "stability",
            Self::Safety       => "safety",
        }
    }

    pub fn weight(self) -> f32 {
        match self {
            Self::Background   => 0.10,
            Self::Optimization => 0.25,
            Self::UserCritical => 0.55,
            Self::Recovery     => 0.70,
            Self::Stability    => 0.85,
            Self::Safety       => 1.00,
        }
    }
}

// ── Priority item ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PriorityItem {
    pub id:          String,
    pub label:       String,
    pub priority:    CognitivePriority,
    pub urgency:     f32,
    pub confidence:  f32,
}

impl PriorityItem {
    pub fn score(&self) -> f32 {
        (self.priority.weight() * 0.5 + self.urgency * 0.3 + self.confidence * 0.2)
            .clamp(0.0, 1.0)
    }
}

// ── Priority decision ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PriorityDecision {
    Execute  { item_id: String, score: f32 },
    Defer    { item_id: String, reason: String },
    Override { item_id: String, by: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PriorityResult {
    pub decisions: Vec<PriorityDecision>,
    pub ts_ms:     u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct PriorityState {
    history: Vec<PriorityResult>,
}

static STATE: Lazy<Mutex<PriorityState>> = Lazy::new(|| Mutex::new(PriorityState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Evaluate a list of items and return priority decisions.
pub fn evaluate(items: &[PriorityItem]) -> PriorityResult {
    PRIORITY_DECISIONS.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    if items.is_empty() {
        return PriorityResult { decisions: Vec::new(), ts_ms: now };
    }

    // Check current runtime state for override conditions
    let unc = crate::uncertainty_engine::sample();
    let watchdog_frozen = crate::cognitive_watchdog::is_frozen();

    let mut decisions: Vec<PriorityDecision> = Vec::new();
    let mut sorted = items.to_vec();
    sorted.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(std::cmp::Ordering::Equal));

    for item in &sorted {
        // Safety override: if watchdog frozen, only Safety-tier executes
        if watchdog_frozen && item.priority < CognitivePriority::Safety {
            SAFETY_OVERRIDES.fetch_add(1, Ordering::Relaxed);
            decisions.push(PriorityDecision::Override {
                item_id: item.id.clone(),
                by:      "watchdog_frozen:safety_only".to_string(),
            });
            continue;
        }

        // Background deferred under high uncertainty
        if unc.overall >= 0.85 && item.priority == CognitivePriority::Background {
            BACKGROUND_DEFERRED.fetch_add(1, Ordering::Relaxed);
            decisions.push(PriorityDecision::Defer {
                item_id: item.id.clone(),
                reason:  "high_uncertainty".to_string(),
            });
            continue;
        }

        decisions.push(PriorityDecision::Execute {
            item_id: item.id.clone(),
            score:   item.score(),
        });
    }

    let result = PriorityResult { decisions, ts_ms: now };
    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
        s.history.push(result.clone());
    }
    result
}

/// Build a default priority item set from current runtime state.
pub fn default_items() -> Vec<PriorityItem> {
    let stability = crate::cognitive_stability::check();
    let unc = crate::uncertainty_engine::sample();

    vec![
        PriorityItem { id: "safety_guard".into(),    label: "Safety guard".into(),
            priority: CognitivePriority::Safety,     urgency: 1.0, confidence: 1.0 },
        PriorityItem { id: "cognition_stable".into(), label: "Stabilise cognition".into(),
            priority: CognitivePriority::Stability,  urgency: if stability.is_unstable() { 0.9 } else { 0.3 },
            confidence: 0.85 },
        PriorityItem { id: "reduce_unc".into(),       label: "Reduce uncertainty".into(),
            priority: CognitivePriority::Recovery,   urgency: unc.overall, confidence: 0.7 },
        PriorityItem { id: "bg_learning".into(),      label: "Background learning".into(),
            priority: CognitivePriority::Background, urgency: 0.2, confidence: 0.5 },
    ]
}

pub fn history(n: usize) -> Vec<PriorityResult> {
    STATE.lock().map(|s| s.history.iter().rev().take(n).cloned().collect()).unwrap_or_default()
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
    fn safety_ranked_highest() {
        let items = vec![
            PriorityItem { id: "bg".into(), label: "bg".into(),
                priority: CognitivePriority::Background, urgency: 0.9, confidence: 0.9 },
            PriorityItem { id: "safe".into(), label: "safe".into(),
                priority: CognitivePriority::Safety, urgency: 0.5, confidence: 0.5 },
        ];
        let result = evaluate(&items);
        // Safety item should be first execute
        let first_exec = result.decisions.iter().find_map(|d| {
            if let PriorityDecision::Execute { item_id, .. } = d { Some(item_id.clone()) } else { None }
        });
        assert_eq!(first_exec.as_deref(), Some("safe"));
    }

    #[test]
    fn empty_items_returns_empty() {
        let result = evaluate(&[]);
        assert!(result.decisions.is_empty());
    }

    #[test]
    fn default_items_non_empty() {
        let items = default_items();
        assert!(!items.is_empty());
    }

    #[test]
    fn priority_decisions_counter_increments() {
        let before = PRIORITY_DECISIONS.load(Ordering::Relaxed);
        evaluate(&default_items());
        assert!(PRIORITY_DECISIONS.load(Ordering::Relaxed) > before);
    }
}
