//! Reflection runtime — analyses the cognitive memory for patterns of failure
//! and derives actionable insights.
//!
//! Insights are surfaced to the planner and attention runtime but never
//! trigger autonomous execution on their own.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static REFLECTIONS_RUN:       AtomicU64 = AtomicU64::new(0);
pub static INSIGHTS_GENERATED:    AtomicU64 = AtomicU64::new(0);
pub static INSIGHTS_APPLIED:      AtomicU64 = AtomicU64::new(0);

const MAX_STORED_INSIGHTS: usize = 50;

// ── Insight kinds ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum InsightKind {
    FrequentPhaseFailure { phase: String, failure_rate: f32 },
    LowSuccessRate       { window: usize, rate: f32 },
    SlowTickPhase        { phase: String, avg_ms: u64 },
    ConsistentAnomalies  { label: String, count: u32 },
    IdleCycleExcess      { idle_fraction: f32 },
    GoalStarvation,
}

impl InsightKind {
    pub fn actionable(&self) -> bool {
        !matches!(self, InsightKind::IdleCycleExcess { .. })
    }

    pub fn description(&self) -> String {
        match self {
            InsightKind::FrequentPhaseFailure { phase, failure_rate } =>
                format!("phase {} fails {:.0}% of the time", phase, failure_rate * 100.0),
            InsightKind::LowSuccessRate { window, rate } =>
                format!("only {:.0}% success over last {} ticks", rate * 100.0, window),
            InsightKind::SlowTickPhase { phase, avg_ms } =>
                format!("phase {} averages {}ms", phase, avg_ms),
            InsightKind::ConsistentAnomalies { label, count } =>
                format!("anomaly '{}' detected {} times recently", label, count),
            InsightKind::IdleCycleExcess { idle_fraction } =>
                format!("{:.0}% of ticks are idle", idle_fraction * 100.0),
            InsightKind::GoalStarvation =>
                "goals exist but none progressed in recent ticks".to_string(),
        }
    }
}

// ── Reflection insight ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReflectionInsight {
    pub kind:       InsightKind,
    pub confidence: f32,
    pub ts_ms:      u64,
}

impl ReflectionInsight {
    pub fn new(kind: InsightKind, confidence: f32) -> Self {
        Self { kind, confidence, ts_ms: ts_now() }
    }
}

// ── Insight store ─────────────────────────────────────────────────────────────

static INSIGHTS: Lazy<Mutex<Vec<ReflectionInsight>>> = Lazy::new(|| Mutex::new(Vec::new()));

fn store_insight(insight: ReflectionInsight) {
    INSIGHTS_GENERATED.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut guard) = INSIGHTS.lock() {
        if guard.len() >= MAX_STORED_INSIGHTS {
            guard.remove(0);
        }
        guard.push(insight.clone());
    }
    crate::world_state_journal::log(crate::world_state_journal::WorldEventKind::ReflectionInsight {
        insight:    insight.kind.description(),
        confidence: insight.confidence,
    });
}

// ── Reflection runtime ────────────────────────────────────────────────────────

pub struct ReflectionRuntime;

impl ReflectionRuntime {
    pub fn reflect() -> Vec<ReflectionInsight> {
        REFLECTIONS_RUN.fetch_add(1, Ordering::Relaxed);
        let mut new_insights = Vec::new();

        // Insight 1: overall success rate
        {
            use crate::cognitive_memory;
            let rate = cognitive_memory::recent_success_rate(20);
            if rate < 0.60 {
                let insight = ReflectionInsight::new(
                    InsightKind::LowSuccessRate { window: 20, rate },
                    0.85,
                );
                store_insight(insight.clone());
                new_insights.push(insight);
            }
        }

        // Insight 2: idle cycle excess
        {
            use crate::cognitive_memory;
            use crate::cognitive_tick::TickPhase;
            let recent = cognitive_memory::recent(30);
            if !recent.is_empty() {
                let idle_count = recent.iter()
                    .filter(|t| t.phase == TickPhase::Idle)
                    .count();
                let fraction = idle_count as f32 / recent.len() as f32;
                if fraction > 0.70 {
                    let insight = ReflectionInsight::new(
                        InsightKind::IdleCycleExcess { idle_fraction: fraction },
                        0.80,
                    );
                    store_insight(insight.clone());
                    new_insights.push(insight);
                }
            }
        }

        // Insight 3: goal starvation — active goals but no completions recently
        {
            use crate::goal_runtime::GoalRuntime;
            use crate::cognitive_memory;
            let has_goals = GoalRuntime::has_active_goals();
            let recent = cognitive_memory::recent(10);
            let any_completed_recently = recent.iter().any(|t| {
                t.notes.iter().any(|n| n.contains("goal_completed"))
            });
            if has_goals && !any_completed_recently && !recent.is_empty() {
                let insight = ReflectionInsight::new(InsightKind::GoalStarvation, 0.70);
                store_insight(insight.clone());
                new_insights.push(insight);
            }
        }

        // Insight 4: consistent anomalies from the detector
        {
            use crate::anomaly_detector::AnomalyDetector;
            let anomalies = AnomalyDetector::scan();
            if anomalies.len() >= 2 {
                let label = anomalies.first().map(|a| a.kind.label()).unwrap_or("unknown");
                let insight = ReflectionInsight::new(
                    InsightKind::ConsistentAnomalies { label: label.to_string(), count: anomalies.len() as u32 },
                    0.75,
                );
                store_insight(insight.clone());
                new_insights.push(insight);
            }
        }

        new_insights
    }

    pub fn recent_insights(n: usize) -> Vec<ReflectionInsight> {
        INSIGHTS_APPLIED.fetch_add(1, Ordering::Relaxed);
        INSIGHTS.lock().map(|g| {
            let len = g.len();
            g[len.saturating_sub(n)..].to_vec()
        }).unwrap_or_default()
    }

    pub fn all_insights() -> Vec<ReflectionInsight> {
        INSIGHTS.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn clear() {
        if let Ok(mut guard) = INSIGHTS.lock() {
            guard.clear();
        }
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
    fn reflect_runs_without_panic() {
        ReflectionRuntime::clear();
        let _ = ReflectionRuntime::reflect();
    }

    #[test]
    fn insight_description_non_empty() {
        let kinds = [
            InsightKind::LowSuccessRate    { window: 20, rate: 0.4 },
            InsightKind::GoalStarvation,
            InsightKind::IdleCycleExcess   { idle_fraction: 0.8 },
        ];
        for k in &kinds {
            assert!(!k.description().is_empty());
        }
    }

    #[test]
    fn reflections_run_counter_increments() {
        let before = REFLECTIONS_RUN.load(Ordering::Relaxed);
        ReflectionRuntime::reflect();
        assert!(REFLECTIONS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn recent_insights_bounded() {
        let insights = ReflectionRuntime::recent_insights(5);
        assert!(insights.len() <= 5);
    }

    #[test]
    fn insight_kind_actionable() {
        assert!(InsightKind::GoalStarvation.actionable());
        assert!(!InsightKind::IdleCycleExcess { idle_fraction: 0.8 }.actionable());
    }

    #[test]
    fn store_and_retrieve_insight() {
        ReflectionRuntime::clear();
        store_insight(ReflectionInsight::new(InsightKind::GoalStarvation, 0.9));
        let all = ReflectionRuntime::all_insights();
        assert!(!all.is_empty());
    }
}
