//! Meta-reflection — analyzes why reasoning failed or succeeded, generating
//! structural insights from cross-module signals.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static REFLECTIONS_RUN:     AtomicU64 = AtomicU64::new(0);
pub static FAILURES_DIAGNOSED:  AtomicU64 = AtomicU64::new(0);
pub static INSIGHTS_GENERATED:  AtomicU64 = AtomicU64::new(0);

const MAX_REFLECTION_HISTORY: usize = 60;

// ── Reflection insight ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReflectionInsight {
    pub source:      String,
    pub insight:     String,
    pub severity:    f32,    // 0–1
    pub is_failure:  bool,
}

// ── Reflection report ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReflectionReport {
    pub cycle:           u64,
    pub insights:        Vec<ReflectionInsight>,
    pub failure_count:   usize,
    pub overall_health:  f32,
    pub summary:         String,
    pub ts_ms:           u64,
}

impl ReflectionReport {
    pub fn has_critical_failures(&self) -> bool {
        self.insights.iter().any(|i| i.is_failure && i.severity >= 0.8)
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ReflectionState {
    history: Vec<ReflectionReport>,
    cycle:   u64,
}

static STATE: Lazy<Mutex<ReflectionState>> = Lazy::new(|| Mutex::new(ReflectionState {
    history: Vec::new(),
    cycle:   0,
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn reflect() -> ReflectionReport {
    REFLECTIONS_RUN.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let cycle = STATE.lock().map(|mut s| { s.cycle += 1; s.cycle }).unwrap_or(0);

    let mut insights = Vec::new();

    // Reasoning quality
    if let Some(q) = crate::reasoning_analyzer::latest() {
        if q.is_low() {
            insights.push(ReflectionInsight {
                source: "reasoning_analyzer".into(),
                insight: format!("reasoning quality degraded to {:.2}", q.overall),
                severity: 1.0 - q.overall,
                is_failure: true,
            });
            FAILURES_DIAGNOSED.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Uncertainty spike
    if let Some(snap) = crate::uncertainty_engine::latest() {
        if snap.high_count > 2 {
            insights.push(ReflectionInsight {
                source: "uncertainty_engine".into(),
                insight: format!("{} dimensions at high uncertainty", snap.high_count),
                severity: snap.overall,
                is_failure: snap.high_count > 4,
            });
        }
    }

    // Drift events
    let drift_events = crate::cognitive_drift_control::recent_events(5);
    if !drift_events.is_empty() {
        insights.push(ReflectionInsight {
            source: "cognitive_drift_control".into(),
            insight: format!("{} drift events in last 5 checks", drift_events.len()),
            severity: (drift_events.len() as f32 / 5.0).min(1.0),
            is_failure: crate::cognitive_drift_control::is_frozen(),
        });
        if crate::cognitive_drift_control::is_frozen() {
            FAILURES_DIAGNOSED.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Failure patterns
    if crate::failure_pattern_analyzer::has_critical_pattern() {
        insights.push(ReflectionInsight {
            source: "failure_pattern_analyzer".into(),
            insight: "critical failure pattern active".into(),
            severity: 0.9,
            is_failure: true,
        });
        FAILURES_DIAGNOSED.fetch_add(1, Ordering::Relaxed);
    }

    // Low confidence
    if let Some(c) = crate::cognitive_confidence::latest() {
        if c.is_critical() {
            insights.push(ReflectionInsight {
                source: "cognitive_confidence".into(),
                insight: format!("confidence critical at {:.2}", c.overall),
                severity: 1.0 - c.overall,
                is_failure: true,
            });
            FAILURES_DIAGNOSED.fetch_add(1, Ordering::Relaxed);
        }
    }

    INSIGHTS_GENERATED.fetch_add(insights.len() as u64, Ordering::Relaxed);

    let failure_count = insights.iter().filter(|i| i.is_failure).count();
    let overall_health = crate::cognitive_confidence::overall();
    let summary = if failure_count == 0 {
        "no failures detected".into()
    } else {
        format!("{} failure(s): {}", failure_count, insights.iter().filter(|i| i.is_failure).map(|i| i.source.as_str()).collect::<Vec<_>>().join(", "))
    };

    let report = ReflectionReport { cycle, insights, failure_count, overall_health, summary, ts_ms: now };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_REFLECTION_HISTORY { s.history.remove(0); }
        s.history.push(report.clone());
    }

    report
}

pub fn latest() -> Option<ReflectionReport> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() { s.history.clear(); s.cycle = 0; }
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
    fn reflect_returns_report() {
        let r = reflect();
        assert!(r.ts_ms > 0);
    }

    #[test]
    fn reflections_run_counter_increments() {
        let before = REFLECTIONS_RUN.load(Ordering::Relaxed);
        reflect();
        assert!(REFLECTIONS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn cycle_increments() {
        let r1 = reflect();
        let r2 = reflect();
        assert!(r2.cycle > r1.cycle);
    }

    #[test]
    fn overall_health_bounded() {
        let r = reflect();
        assert!(r.overall_health >= 0.0 && r.overall_health <= 1.0);
    }

    #[test]
    fn has_critical_failures_when_severity_high() {
        let r = ReflectionReport {
            cycle: 1,
            insights: vec![ReflectionInsight {
                source: "test".into(), insight: "test".into(), severity: 0.9, is_failure: true,
            }],
            failure_count: 1,
            overall_health: 0.3,
            summary: "test".into(),
            ts_ms: 0,
        };
        assert!(r.has_critical_failures());
    }

    #[test]
    fn history_grows_after_reflect() {
        let before = REFLECTIONS_RUN.load(Ordering::Relaxed);
        reflect();
        assert!(REFLECTIONS_RUN.load(Ordering::Relaxed) > before);
    }
}
