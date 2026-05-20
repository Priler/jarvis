#![allow(dead_code)]

use std::time::SystemTime;
use serde::{Deserialize, Serialize};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Running,
    Success,
    Failed { reason: String },
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub description: String,
    pub command_text: String,
    pub status: StepStatus,
}

impl PlanStep {
    pub fn new(description: impl Into<String>, command_text: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            command_text: command_text.into(),
            status: StepStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    pub goal: String,
    pub steps: Vec<PlanStep>,
    pub current: usize,
    pub created_at_ms: u64,
}

impl TaskPlan {
    /// Build a single-step plan (the common case).
    pub fn single(goal: impl Into<String>, command_text: impl Into<String>) -> Self {
        let goal_str: String = goal.into();
        let cmd_str: String = command_text.into();
        Self {
            steps: vec![PlanStep::new(goal_str.clone(), cmd_str)],
            goal: goal_str,
            current: 0,
            created_at_ms: now_ms(),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.current >= self.steps.len()
    }

    pub fn current_step(&self) -> Option<&PlanStep> {
        self.steps.get(self.current)
    }

    pub fn current_step_mut(&mut self) -> Option<&mut PlanStep> {
        self.steps.get_mut(self.current)
    }

    pub fn advance(&mut self) {
        self.current += 1;
    }

    pub fn mark_current_success(&mut self) {
        if let Some(step) = self.current_step_mut() {
            step.status = StepStatus::Success;
        }
        self.advance();
    }

    pub fn mark_current_failed(&mut self, reason: impl Into<String>) {
        if let Some(step) = self.current_step_mut() {
            step.status = StepStatus::Failed { reason: reason.into() };
        }
    }

    pub fn step_descriptions(&self) -> Vec<String> {
        self.steps.iter().map(|s| s.description.clone()).collect()
    }

    pub fn success_count(&self) -> usize {
        self.steps.iter().filter(|s| s.status == StepStatus::Success).count()
    }
}
