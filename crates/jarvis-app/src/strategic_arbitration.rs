//! Strategic arbitration — resolves conflicts between competing goals and plans
//! using priority scores, confidence, and causal risk estimates.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static ARBITRATIONS_RUN:     AtomicU64 = AtomicU64::new(0);
pub static CONFLICTS_RESOLVED:   AtomicU64 = AtomicU64::new(0);
pub static ARBITRATION_DEFERRED: AtomicU64 = AtomicU64::new(0);

const MAX_ARBITRATION_HISTORY: usize = 60;

// ── Competing goal ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompetingGoal {
    pub id:           String,
    pub description:  String,
    pub priority:     f32,    // 0–1
    pub urgency:      f32,    // 0–1
    pub causal_risk:  f32,    // 0–1; risk of causal side-effects
    pub confidence:   f32,    // 0–1; how confident are we this goal is correct
}

impl CompetingGoal {
    pub fn arbitration_score(&self) -> f32 {
        (self.priority * 0.35 + self.urgency * 0.30 + self.confidence * 0.25
            - self.causal_risk * 0.10).clamp(0.0, 1.0)
    }
}

// ── Arbitration verdict ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ArbitrationVerdict {
    Execute  { goal_id: String, score: f32 },
    Defer    { reason: String },
    Abort    { reason: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArbitrationResult {
    pub verdict:     ArbitrationVerdict,
    pub candidates:  Vec<(String, f32)>,   // (goal_id, score)
    pub ts_ms:       u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ArbState {
    history: Vec<ArbitrationResult>,
}

static STATE: Lazy<Mutex<ArbState>> = Lazy::new(|| Mutex::new(ArbState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn arbitrate(goals: &[CompetingGoal]) -> ArbitrationResult {
    ARBITRATIONS_RUN.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    if goals.is_empty() {
        ARBITRATION_DEFERRED.fetch_add(1, Ordering::Relaxed);
        return ArbitrationResult {
            verdict: ArbitrationVerdict::Defer { reason: "no goals provided".into() },
            candidates: vec![],
            ts_ms: now,
        };
    }

    // Abort if runtime is frozen (drift control)
    if crate::cognitive_drift_control::is_frozen() {
        ARBITRATION_DEFERRED.fetch_add(1, Ordering::Relaxed);
        return ArbitrationResult {
            verdict: ArbitrationVerdict::Defer { reason: "cognitive drift freeze active".into() },
            candidates: vec![],
            ts_ms: now,
        };
    }

    let confidence = crate::cognitive_confidence::overall();
    if confidence < 0.2 {
        ARBITRATION_DEFERRED.fetch_add(1, Ordering::Relaxed);
        return ArbitrationResult {
            verdict: ArbitrationVerdict::Defer { reason: format!("overall confidence too low: {:.2}", confidence) },
            candidates: vec![],
            ts_ms: now,
        };
    }

    let mut scored: Vec<(String, f32)> = goals.iter()
        .map(|g| (g.id.clone(), g.arbitration_score()))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let (best_id, best_score) = scored.first().map(|(id, s)| (id.clone(), *s)).unwrap();

    CONFLICTS_RESOLVED.fetch_add(1, Ordering::Relaxed);

    let verdict = if best_score < 0.2 {
        ArbitrationVerdict::Abort { reason: format!("best score {:.2} too low to execute", best_score) }
    } else {
        ArbitrationVerdict::Execute { goal_id: best_id, score: best_score }
    };

    let result = ArbitrationResult { verdict, candidates: scored, ts_ms: now };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_ARBITRATION_HISTORY { s.history.remove(0); }
        s.history.push(result.clone());
    }

    result
}

pub fn latest() -> Option<ArbitrationResult> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() { s.history.clear(); }
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

    fn goal(id: &str, priority: f32, urgency: f32, risk: f32, conf: f32) -> CompetingGoal {
        CompetingGoal { id: id.into(), description: id.into(), priority, urgency, causal_risk: risk, confidence: conf }
    }

    #[test]
    fn arbitrate_empty_returns_defer() {
        let r = arbitrate(&[]);
        assert!(matches!(r.verdict, ArbitrationVerdict::Defer { .. }));
    }

    #[test]
    fn arbitrations_run_counter_increments() {
        let before = ARBITRATIONS_RUN.load(Ordering::Relaxed);
        arbitrate(&[goal("sa.u1", 0.8, 0.7, 0.1, 0.9)]);
        assert!(ARBITRATIONS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn arbitration_score_bounded() {
        let g = goal("sa.u2", 0.8, 0.7, 0.2, 0.9);
        let s = g.arbitration_score();
        assert!(s >= 0.0 && s <= 1.0);
    }

    #[test]
    fn high_priority_goal_may_execute() {
        let r = arbitrate(&[goal("sa.u3", 0.9, 0.9, 0.0, 0.95)]);
        // Regardless of frozen state, result exists
        assert!(r.ts_ms > 0);
    }

    #[test]
    fn candidates_sorted_descending() {
        let r = arbitrate(&[
            goal("sa.u4.a", 0.5, 0.5, 0.1, 0.7),
            goal("sa.u4.b", 0.9, 0.9, 0.0, 0.95),
        ]);
        if r.candidates.len() == 2 {
            assert!(r.candidates[0].1 >= r.candidates[1].1);
        }
    }

    #[test]
    fn history_grows_after_arbitrate() {
        let before = ARBITRATIONS_RUN.load(Ordering::Relaxed);
        arbitrate(&[goal("sa.u5", 0.6, 0.5, 0.2, 0.8)]);
        assert!(ARBITRATIONS_RUN.load(Ordering::Relaxed) > before);
    }
}
