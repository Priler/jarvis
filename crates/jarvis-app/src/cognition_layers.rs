//! Cognition layer definitions — the 5-tier hierarchy used throughout Phase 19.
//! Defines layer identities, event types, and shared cognition signals.

use std::sync::atomic::{AtomicU64, Ordering};

pub static EVENTS_ROUTED:    AtomicU64 = AtomicU64::new(0);
pub static ESCALATIONS:      AtomicU64 = AtomicU64::new(0);
pub static DE_ESCALATIONS:   AtomicU64 = AtomicU64::new(0);

// ── Layer identity ────────────────────────────────────────────────────────────

/// The 5 cognition layers, ordered from fastest/lowest to slowest/highest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
         serde::Serialize, serde::Deserialize)]
pub enum CognitionLayer {
    Reactive    = 0,   // immediate deterministic reactions
    Tactical    = 1,   // workflow execution, tool orchestration
    Strategic   = 2,   // long-horizon planning, multi-workflow
    Meta        = 3,   // reasoning evaluation, uncertainty calibration
    Supervisory = 4,   // global arbitration, layer coordination
}

impl CognitionLayer {
    pub fn label(self) -> &'static str {
        match self {
            Self::Reactive    => "reactive",
            Self::Tactical    => "tactical",
            Self::Strategic   => "strategic",
            Self::Meta        => "meta",
            Self::Supervisory => "supervisory",
        }
    }

    /// Maximum time budget (ms) for a single work item at this layer.
    pub fn latency_budget_ms(self) -> u64 {
        match self {
            Self::Reactive    =>   50,
            Self::Tactical    =>  500,
            Self::Strategic   => 2000,
            Self::Meta        => 3000,
            Self::Supervisory => 1000,
        }
    }

    /// Relative CPU weight (used by resource_scheduler).
    pub fn cpu_weight(self) -> f32 {
        match self {
            Self::Reactive    => 1.0,
            Self::Tactical    => 0.8,
            Self::Strategic   => 0.5,
            Self::Meta        => 0.4,
            Self::Supervisory => 0.3,
        }
    }

    pub fn all() -> [CognitionLayer; 5] {
        [Self::Reactive, Self::Tactical, Self::Strategic, Self::Meta, Self::Supervisory]
    }
}

// ── Cognition event ───────────────────────────────────────────────────────────

/// Events that flow through the cognitive router to the appropriate layer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CognitionEvent {
    // Reactive events
    CriticalAnomaly      { kind: String, severity: f32 },
    SafetyInterrupt      { reason: String },
    WakeSignal           { source: String },

    // Tactical events
    WorkflowStarted      { workflow_id: String },
    WorkflowCompleted    { workflow_id: String, success: bool },
    ToolExecuted         { tool_id: String, success: bool, latency_ms: u64 },
    RecoveryTriggered    { reason: String },

    // Strategic events
    LongHorizonGoalAdded { goal_id: u64, description: String },
    EnvironmentDrifted   { drift_score: f32 },
    ResourcePressure     { resource: String, pressure: f32 },

    // Meta events
    ReasoningDegraded    { quality: f32, cycle: u64 },
    UncertaintySpike     { dimension: String, value: f32 },
    StrategyChanged      { old_strategy: String, new_strategy: String },

    // Supervisory events
    LayerOverloaded      { layer: CognitionLayer, queue_depth: usize },
    PriorityShift        { from: String, to: String },
    CognitionEscalated   { from: CognitionLayer, to: CognitionLayer, reason: String },
}

impl CognitionEvent {
    pub fn natural_layer(&self) -> CognitionLayer {
        match self {
            Self::CriticalAnomaly { .. }
            | Self::SafetyInterrupt { .. }
            | Self::WakeSignal { .. }          => CognitionLayer::Reactive,

            Self::WorkflowStarted   { .. }
            | Self::WorkflowCompleted { .. }
            | Self::ToolExecuted    { .. }
            | Self::RecoveryTriggered { .. }   => CognitionLayer::Tactical,

            Self::LongHorizonGoalAdded { .. }
            | Self::EnvironmentDrifted { .. }
            | Self::ResourcePressure   { .. }  => CognitionLayer::Strategic,

            Self::ReasoningDegraded  { .. }
            | Self::UncertaintySpike { .. }
            | Self::StrategyChanged  { .. }    => CognitionLayer::Meta,

            Self::LayerOverloaded    { .. }
            | Self::PriorityShift    { .. }
            | Self::CognitionEscalated { .. }  => CognitionLayer::Supervisory,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::CriticalAnomaly      { .. } => "critical_anomaly",
            Self::SafetyInterrupt      { .. } => "safety_interrupt",
            Self::WakeSignal           { .. } => "wake_signal",
            Self::WorkflowStarted      { .. } => "workflow_started",
            Self::WorkflowCompleted    { .. } => "workflow_completed",
            Self::ToolExecuted         { .. } => "tool_executed",
            Self::RecoveryTriggered    { .. } => "recovery_triggered",
            Self::LongHorizonGoalAdded { .. } => "lh_goal_added",
            Self::EnvironmentDrifted   { .. } => "env_drifted",
            Self::ResourcePressure     { .. } => "resource_pressure",
            Self::ReasoningDegraded    { .. } => "reasoning_degraded",
            Self::UncertaintySpike     { .. } => "uncertainty_spike",
            Self::StrategyChanged      { .. } => "strategy_changed",
            Self::LayerOverloaded      { .. } => "layer_overloaded",
            Self::PriorityShift        { .. } => "priority_shift",
            Self::CognitionEscalated   { .. } => "cognition_escalated",
        }
    }

    /// True if this event demands immediate reactive-layer attention.
    pub fn is_critical(&self) -> bool {
        matches!(self, Self::CriticalAnomaly { severity, .. } if *severity >= 0.8)
            || matches!(self, Self::SafetyInterrupt { .. })
    }
}

// ── Layer processing result ───────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayerResult {
    pub layer:      CognitionLayer,
    pub event:      String,           // event label
    pub handled:    bool,
    pub escalate:   Option<CognitionLayer>,   // escalate to this layer if Some
    pub duration_ms: u64,
}

impl LayerResult {
    pub fn ok(layer: CognitionLayer, event: &str, duration_ms: u64) -> Self {
        EVENTS_ROUTED.fetch_add(1, Ordering::Relaxed);
        LayerResult { layer, event: event.to_string(), handled: true, escalate: None, duration_ms }
    }

    pub fn escalate(layer: CognitionLayer, event: &str, to: CognitionLayer, duration_ms: u64) -> Self {
        ESCALATIONS.fetch_add(1, Ordering::Relaxed);
        LayerResult { layer, event: event.to_string(), handled: false, escalate: Some(to), duration_ms }
    }

    pub fn skip(layer: CognitionLayer, event: &str) -> Self {
        LayerResult { layer, event: event.to_string(), handled: false, escalate: None, duration_ms: 0 }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_ordering_reactive_lowest() {
        assert!(CognitionLayer::Reactive < CognitionLayer::Supervisory);
        assert!(CognitionLayer::Tactical < CognitionLayer::Strategic);
    }

    #[test]
    fn event_natural_layer_routes_correctly() {
        let ev = CognitionEvent::SafetyInterrupt { reason: "test".into() };
        assert_eq!(ev.natural_layer(), CognitionLayer::Reactive);

        let ev = CognitionEvent::WorkflowStarted { workflow_id: "wf1".into() };
        assert_eq!(ev.natural_layer(), CognitionLayer::Tactical);

        let ev = CognitionEvent::ReasoningDegraded { quality: 0.3, cycle: 1 };
        assert_eq!(ev.natural_layer(), CognitionLayer::Meta);
    }

    #[test]
    fn critical_events_flagged() {
        let ev = CognitionEvent::SafetyInterrupt { reason: "x".into() };
        assert!(ev.is_critical());
        let ev = CognitionEvent::CriticalAnomaly { kind: "k".into(), severity: 0.9 };
        assert!(ev.is_critical());
        let ev = CognitionEvent::CriticalAnomaly { kind: "k".into(), severity: 0.5 };
        assert!(!ev.is_critical());
    }

    #[test]
    fn all_layers_returns_five() {
        assert_eq!(CognitionLayer::all().len(), 5);
    }
}
