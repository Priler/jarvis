//! Live meta-cognition observability — structured logging of all Phase 18
//! runtime events: simulations, causal updates, strategy arbitration,
//! uncertainty recalibration, reasoning degradation, watchdog interventions.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static OBS_EVENTS_LOGGED:    AtomicU64 = AtomicU64::new(0);
pub static OBS_DEGRADATIONS:     AtomicU64 = AtomicU64::new(0);
pub static OBS_WATCHDOG_EVENTS:  AtomicU64 = AtomicU64::new(0);

const MAX_LOG: usize = 500;

// ── Observability record ──────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetaObsRecord {
    pub id:       u64,
    pub category: ObsCategory,
    pub message:  String,
    pub severity: ObsSeverity,
    pub ts_ms:    u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ObsCategory {
    Simulation,
    CausalUpdate,
    Arbitration,
    UncertaintyRecalib,
    ReasoningDegradation,
    WatchdogEvent,
    MetaCycle,
    MemoryFusion,
    Counterfactual,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ObsSeverity {
    Info,
    Warning,
    Critical,
}

impl ObsSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Info     => "INFO",
            Self::Warning  => "WARN",
            Self::Critical => "CRIT",
        }
    }
}

impl ObsCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Simulation           => "simulation",
            Self::CausalUpdate         => "causal_update",
            Self::Arbitration          => "arbitration",
            Self::UncertaintyRecalib   => "uncertainty_recalib",
            Self::ReasoningDegradation => "reasoning_degradation",
            Self::WatchdogEvent        => "watchdog",
            Self::MetaCycle            => "meta_cycle",
            Self::MemoryFusion         => "memory_fusion",
            Self::Counterfactual       => "counterfactual",
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ObsState {
    log: Vec<MetaObsRecord>,
    seq: u64,
}

static STATE: Lazy<Mutex<ObsState>> = Lazy::new(|| Mutex::new(ObsState {
    log: Vec::with_capacity(128),
    seq: 0,
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn log(category: ObsCategory, message: impl Into<String>, severity: ObsSeverity) {
    OBS_EVENTS_LOGGED.fetch_add(1, Ordering::Relaxed);
    match severity {
        ObsSeverity::Critical | ObsSeverity::Warning => {
            match category {
                ObsCategory::WatchdogEvent        => { OBS_WATCHDOG_EVENTS.fetch_add(1, Ordering::Relaxed); }
                ObsCategory::ReasoningDegradation => { OBS_DEGRADATIONS.fetch_add(1, Ordering::Relaxed); }
                _ => {}
            }
        }
        _ => {}
    }

    if let Ok(mut s) = STATE.lock() {
        s.seq += 1;
        let id = s.seq;
        if s.log.len() >= MAX_LOG { s.log.remove(0); }
        s.log.push(MetaObsRecord {
            id,
            category,
            message:  message.into(),
            severity,
            ts_ms:    ts_now(),
        });
    }
}

/// Process events drained from the meta event bus and emit observability records.
pub fn process_bus_events(events: &[crate::meta_event_bus::MetaEvent]) {
    for event in events {
        let (cat, msg, sev) = classify_event(event);
        log(cat, msg, sev);
    }
}

fn classify_event(event: &crate::meta_event_bus::MetaEvent) -> (ObsCategory, String, ObsSeverity) {
    use crate::meta_event_bus::MetaEvent;
    match event {
        MetaEvent::ReasoningFailure { quality, cycle } =>
            (ObsCategory::ReasoningDegradation,
             format!("reasoning_failure cycle={cycle} quality={quality:.3}"),
             ObsSeverity::Warning),
        MetaEvent::SimulationResult { plan_id, success_prob, should_execute } =>
            (ObsCategory::Simulation,
             format!("sim plan={plan_id} prob={success_prob:.3} exec={should_execute}"),
             ObsSeverity::Info),
        MetaEvent::UncertaintyShift { dimension, old, new } =>
            (ObsCategory::UncertaintyRecalib,
             format!("unc_shift dim={dimension} {old:.3}→{new:.3}"),
             if (new - old).abs() >= 0.3 { ObsSeverity::Warning } else { ObsSeverity::Info }),
        MetaEvent::StrategyDegradation { strategy_id, score_drop } =>
            (ObsCategory::ReasoningDegradation,
             format!("strategy_degraded id={strategy_id} drop={score_drop:.3}"),
             ObsSeverity::Warning),
        MetaEvent::CausalInstability { cause, effect, strength_drop } =>
            (ObsCategory::CausalUpdate,
             format!("causal_instability {cause}→{effect} drop={strength_drop:.3}"),
             ObsSeverity::Warning),
        MetaEvent::PredictionCollapse { horizon, confidence_drop } =>
            (ObsCategory::ReasoningDegradation,
             format!("pred_collapse horizon={horizon} drop={confidence_drop:.3}"),
             ObsSeverity::Critical),
        MetaEvent::ReflectionEvent { insight, severity, is_failure } =>
            (ObsCategory::MetaCycle,
             format!("reflection insight={insight} sev={severity:.2} fail={is_failure}"),
             if *is_failure && *severity >= 0.7 { ObsSeverity::Critical } else { ObsSeverity::Info }),
        MetaEvent::WatchdogIntervention { kind, action } =>
            (ObsCategory::WatchdogEvent,
             format!("watchdog kind={kind:?} action={action}"),
             ObsSeverity::Warning),
        MetaEvent::UncertaintyRecalib { dimension, value } =>
            (ObsCategory::UncertaintyRecalib,
             format!("recalib dim={dimension} val={value:.3}"),
             ObsSeverity::Info),
        MetaEvent::MetaCycleCompleted { cycle_id, healthy } =>
            (ObsCategory::MetaCycle,
             format!("meta_cycle_done id={cycle_id} healthy={healthy}"),
             if *healthy { ObsSeverity::Info } else { ObsSeverity::Warning }),
    }
}

/// Recent observability records.
pub fn recent(limit: usize) -> Vec<MetaObsRecord> {
    STATE.lock().map(|s| {
        s.log.iter().rev().take(limit).cloned().collect()
    }).unwrap_or_default()
}

/// Full log snapshot.
pub fn snapshot() -> Vec<MetaObsRecord> {
    STATE.lock().map(|s| s.log.clone()).unwrap_or_default()
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
    fn log_and_retrieve() {
        log(ObsCategory::MetaCycle, "test_event", ObsSeverity::Info);
        let recs = recent(10);
        assert!(recs.iter().any(|r| r.message == "test_event"));
    }

    #[test]
    fn process_bus_events_no_panic() {
        let events = vec![
            crate::meta_event_bus::MetaEvent::MetaCycleCompleted { cycle_id: 1, healthy: true },
            crate::meta_event_bus::MetaEvent::ReasoningFailure   { quality: 0.3, cycle: 1 },
        ];
        process_bus_events(&events);
        assert!(OBS_EVENTS_LOGGED.load(Ordering::Relaxed) >= 2);
    }
}
