//! Live strategic arbitration — real-time resolution of competing goals,
//! workflows, recovery plans, and reasoning paths.
//! Priority order: safety > stability > reliability > latency > optimization.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static ARBITRATIONS_LIVE:  AtomicU64 = AtomicU64::new(0);
pub static SAFETY_OVERRIDES:   AtomicU64 = AtomicU64::new(0);
pub static DEFERRED_GOALS:     AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 60;

// ── Priority tier ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum PriorityTier {
    Safety       = 5,
    Stability    = 4,
    Reliability  = 3,
    Latency      = 2,
    Optimization = 1,
}

impl PriorityTier {
    pub fn weight(self) -> f32 {
        match self {
            Self::Safety       => 1.00,
            Self::Stability    => 0.80,
            Self::Reliability  => 0.60,
            Self::Latency      => 0.40,
            Self::Optimization => 0.20,
        }
    }
}

// ── Live goal ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveGoal {
    pub id:         String,
    pub label:      String,
    pub tier:       PriorityTier,
    pub confidence: f32,    // 0–1
    pub urgency:    f32,    // 0–1
    pub risk:       f32,    // 0–1
}

impl LiveGoal {
    pub fn score(&self) -> f32 {
        (self.tier.weight() * 0.40
            + self.confidence * 0.25
            + self.urgency    * 0.25
            - self.risk       * 0.10)
            .clamp(0.0, 1.0)
    }
}

// ── Arbitration verdict ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum LiveVerdict {
    Execute  { goal_id: String, score: f32, tier: PriorityTier },
    Defer    { reason: String },
    SafetyHold { reason: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveArbitrationResult {
    pub verdict:    LiveVerdict,
    pub ranked:     Vec<(String, f32)>,   // (goal_id, score) descending
    pub ts_ms:      u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ArbState {
    history: Vec<LiveArbitrationResult>,
}

static STATE: Lazy<Mutex<ArbState>> = Lazy::new(|| Mutex::new(ArbState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Arbitrate a set of competing live goals.
pub fn arbitrate(goals: &[LiveGoal]) -> LiveArbitrationResult {
    ARBITRATIONS_LIVE.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    if goals.is_empty() {
        return LiveArbitrationResult {
            verdict: LiveVerdict::Defer { reason: "no_candidates".to_string() },
            ranked: Vec::new(),
            ts_ms: now,
        };
    }

    // Gate on watchdog — if cognition is frozen, hold everything non-safety
    if crate::cognitive_watchdog::is_frozen() {
        let safety_goal = goals.iter().find(|g| g.tier == PriorityTier::Safety);
        if let Some(sg) = safety_goal {
            SAFETY_OVERRIDES.fetch_add(1, Ordering::Relaxed);
            return LiveArbitrationResult {
                verdict: LiveVerdict::SafetyHold {
                    reason: format!("watchdog_frozen:safety_only:{}", sg.id),
                },
                ranked: vec![(sg.id.clone(), sg.score())],
                ts_ms: now,
            };
        }
        SAFETY_OVERRIDES.fetch_add(1, Ordering::Relaxed);
        return LiveArbitrationResult {
            verdict: LiveVerdict::SafetyHold { reason: "watchdog_frozen".to_string() },
            ranked:  Vec::new(),
            ts_ms:   now,
        };
    }

    // Check current uncertainty — if critical, hold optimisation-tier goals
    let unc = crate::uncertainty_engine::sample();
    let unc_critical = unc.overall >= 0.85;

    // Score and rank
    let mut ranked: Vec<(String, f32)> = goals.iter()
        .filter(|g| !unc_critical || g.tier >= PriorityTier::Reliability)
        .map(|g| (g.id.clone(), g.score()))
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if unc_critical {
        let deferred: Vec<_> = goals.iter()
            .filter(|g| g.tier < PriorityTier::Reliability)
            .collect();
        DEFERRED_GOALS.fetch_add(deferred.len() as u64, Ordering::Relaxed);
    }

    let verdict = if let Some((best_id, best_score)) = ranked.first() {
        let tier = goals.iter().find(|g| &g.id == best_id)
            .map(|g| g.tier)
            .unwrap_or(PriorityTier::Optimization);
        LiveVerdict::Execute {
            goal_id: best_id.clone(),
            score:   *best_score,
            tier,
        }
    } else {
        DEFERRED_GOALS.fetch_add(1, Ordering::Relaxed);
        LiveVerdict::Defer { reason: "all_filtered_by_uncertainty".to_string() }
    };

    // Publish degradation event if best score is low
    if let LiveVerdict::Execute { ref goal_id, score, .. } = verdict {
        if score < 0.35 {
            crate::meta_event_bus::publish(crate::meta_event_bus::MetaEvent::StrategyDegradation {
                strategy_id: goal_id.clone(),
                score_drop: 0.35 - score,
            });
        }
    }

    let result = LiveArbitrationResult { verdict, ranked, ts_ms: now };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
        s.history.push(result.clone());
    }

    result
}

/// Run arbitration over the default set of runtime goals derived from live state.
pub fn run_live() -> LiveArbitrationResult {
    let goals = build_default_goals();
    arbitrate(&goals)
}

fn build_default_goals() -> Vec<LiveGoal> {
    let unc = crate::uncertainty_engine::sample();
    let stability = crate::cognitive_stability::check();

    vec![
        LiveGoal {
            id:         "maintain_safety".to_string(),
            label:      "Maintain safety constraints".to_string(),
            tier:       PriorityTier::Safety,
            confidence: 0.95,
            urgency:    1.0,
            risk:       0.0,
        },
        LiveGoal {
            id:         "cognitive_stability".to_string(),
            label:      "Stabilise cognition".to_string(),
            tier:       PriorityTier::Stability,
            confidence: if stability.is_stable { 0.85 } else { 0.4 },
            urgency:    if stability.is_unstable() { 0.9 } else { 0.4 },
            risk:       stability.oscillation_score * 0.5,
        },
        LiveGoal {
            id:         "reduce_uncertainty".to_string(),
            label:      "Reduce epistemic uncertainty".to_string(),
            tier:       PriorityTier::Reliability,
            confidence: (1.0 - unc.overall).clamp(0.0, 1.0),
            urgency:    unc.overall,
            risk:       0.1,
        },
        LiveGoal {
            id:         "optimize_workflow".to_string(),
            label:      "Optimise workflow execution".to_string(),
            tier:       PriorityTier::Optimization,
            confidence: 0.6,
            urgency:    0.3,
            risk:       0.2,
        },
    ]
}

pub fn history() -> Vec<LiveArbitrationResult> {
    STATE.lock().map(|s| s.history.clone()).unwrap_or_default()
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
    fn arbitrate_selects_highest_tier() {
        let goals = vec![
            LiveGoal { id: "opt".into(), label: "opt".into(), tier: PriorityTier::Optimization,
                confidence: 0.9, urgency: 0.9, risk: 0.0 },
            LiveGoal { id: "safe".into(), label: "safe".into(), tier: PriorityTier::Safety,
                confidence: 0.9, urgency: 0.9, risk: 0.0 },
        ];
        let result = arbitrate(&goals);
        if let LiveVerdict::Execute { tier, .. } = result.verdict {
            assert_eq!(tier, PriorityTier::Safety);
        }
    }

    #[test]
    fn empty_goals_deferred() {
        let result = arbitrate(&[]);
        assert!(matches!(result.verdict, LiveVerdict::Defer { .. }));
    }

    #[test]
    fn run_live_returns_result() {
        let result = run_live();
        assert!(ARBITRATIONS_LIVE.load(Ordering::Relaxed) >= 1);
        let _ = result;
    }
}
