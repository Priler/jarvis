#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::Arc;
use parking_lot::{Mutex, RwLock};

// ── Risk level (shared with governance) ───────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
        }
    }
}

// ── Bus event taxonomy ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum BusEvent {
    // Perception
    VoiceIntent { text: String, domain: String, confidence: f64 },
    ScreenChanged { window_title: String, is_browser: bool, is_media: bool },

    // Cognitive
    GoalStarted { id: u64, goal: String },
    GoalCompleted { id: u64, success: bool },
    PlanCreated { goal: String, steps: Vec<String> },
    MemoryUpdated { kind: String, summary: String },
    ClarificationIssued { question: String },

    // Execution
    CommandDispatched { intent_id: String, text: String },
    CommandCompleted { intent_id: String, success: bool },

    // Workflow
    WorkflowTriggered { id: String, name: String },
    WorkflowStepExecuted { workflow_id: String, step: String, index: usize, success: bool },
    WorkflowCompleted { id: String, success: bool },

    // Governance
    GovernanceAlert { risk_level: RiskLevel, action: String, blocked: bool },

    // Agent lifecycle
    AgentStarted { id: String },
    AgentStopped { id: String, reason: String },
    AgentRecovered { id: String },

    // Runtime health
    HealthCheck { component: String, healthy: bool, details: String },

    // Scheduler
    JobScheduled { id: String, job_type: String, due_at_ms: u64 },
    JobExecuted { id: String, success: bool },
}

// ── Cognitive Bus ─────────────────────────────────────────────────────────────

type BusCallback = Arc<dyn Fn(&BusEvent) + Send + Sync>;

pub struct CognitiveBus {
    subscribers: RwLock<Vec<BusCallback>>,
    event_log: Mutex<VecDeque<BusEvent>>,
}

impl CognitiveBus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            subscribers: RwLock::new(Vec::new()),
            event_log: Mutex::new(VecDeque::new()),
        })
    }

    pub fn subscribe<F: Fn(&BusEvent) + Send + Sync + 'static>(&self, callback: F) {
        self.subscribers.write().push(Arc::new(callback));
    }

    pub fn publish(&self, event: BusEvent) {
        // Append to ring-buffer log.
        let mut log = self.event_log.lock();
        if log.len() >= 1000 {
            log.pop_front();
        }
        log.push_back(event.clone());
        drop(log);

        // Notify all registered subscribers.
        let subs = self.subscribers.read();
        for sub in subs.iter() {
            sub(&event);
        }
    }

    pub fn recent_events(&self, n: usize) -> Vec<BusEvent> {
        self.event_log.lock().iter().rev().take(n).cloned().collect()
    }

    pub fn event_count(&self) -> usize {
        self.event_log.lock().len()
    }
}
