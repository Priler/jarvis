#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
use jarvis_core::APP_CONFIG_DIR;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkflowTrigger {
    Manual,
    VoiceCommand { phrase: String },
    Scheduled { interval_ms: u64 },
    SystemEvent { event: String },
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
pub struct WorkflowStep {
    pub id: String,
    pub description: String,
    pub command_text: String,
    pub continue_on_failure: bool,
    pub status: StepStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    Idle,
    Running { current_step: usize },
    Paused { at_step: usize },
    Completed,
    Failed { at_step: usize, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger: WorkflowTrigger,
    pub steps: Vec<WorkflowStep>,
    pub status: WorkflowStatus,
    pub created_at_ms: u64,
    pub last_run_ms: u64,
    pub run_count: u32,
    pub success_count: u32,
}

impl Workflow {
    pub fn new(id: impl Into<String>, name: impl Into<String>, trigger: WorkflowTrigger) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            trigger,
            steps: Vec::new(),
            status: WorkflowStatus::Idle,
            created_at_ms: now_ms(),
            last_run_ms: 0,
            run_count: 0,
            success_count: 0,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn add_step(
        &mut self,
        id: impl Into<String>,
        description: impl Into<String>,
        command_text: impl Into<String>,
    ) -> &mut Self {
        self.steps.push(WorkflowStep {
            id: id.into(),
            description: description.into(),
            command_text: command_text.into(),
            continue_on_failure: false,
            status: StepStatus::Pending,
        });
        self
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, WorkflowStatus::Running { .. })
    }

    pub fn current_step_index(&self) -> Option<usize> {
        if let WorkflowStatus::Running { current_step } = self.status {
            Some(current_step)
        } else {
            None
        }
    }

    pub fn current_step_text(&self) -> Option<&str> {
        self.current_step_index()
            .and_then(|i| self.steps.get(i))
            .map(|s| s.command_text.as_str())
    }

    pub fn step_descriptions(&self) -> Vec<String> {
        self.steps.iter().map(|s| s.description.clone()).collect()
    }
}

// ── Workflow engine ───────────────────────────────────────────────────────────

pub struct WorkflowEngine {
    workflows: HashMap<String, Workflow>,
    persistence_path: PathBuf,
    pub active_id: Option<String>,
}

impl WorkflowEngine {
    pub fn new() -> Self {
        let path = APP_CONFIG_DIR.get()
            .map(|d| d.join("workflows.json"))
            .unwrap_or_else(|| PathBuf::from("workflows.json"));

        let workflows = Self::load_from(&path);
        info!("[WORKFLOWS] Loaded {} workflow(s) from disk", workflows.len());

        Self { workflows, persistence_path: path, active_id: None }
    }

    pub fn register(&mut self, workflow: Workflow) {
        info!("[WORKFLOWS] Registered '{}' ({} steps, trigger={:?})",
            workflow.name, workflow.steps.len(), workflow.trigger);
        self.workflows.insert(workflow.id.clone(), workflow);
        self.save();
    }

    /// Start a workflow by id. Returns the command text of the first step, if any.
    pub fn trigger(&mut self, id: &str) -> Result<Option<String>, String> {
        let wf = self.workflows.get_mut(id)
            .ok_or_else(|| format!("Workflow '{}' not found", id))?;

        if wf.is_running() {
            return Err(format!("Workflow '{}' is already running", id));
        }

        for step in &mut wf.steps {
            step.status = StepStatus::Pending;
        }
        wf.status = WorkflowStatus::Running { current_step: 0 };
        wf.run_count += 1;
        wf.last_run_ms = now_ms();

        self.active_id = Some(id.to_string());
        let first = wf.steps.first().map(|s| s.command_text.clone());
        self.save();
        Ok(first)
    }

    /// Advance the active workflow after a step completes.
    /// Returns `Some(command_text)` for the next step, `None` when done.
    pub fn advance(&mut self, success: bool) -> Option<String> {
        let id = self.active_id.clone()?;
        let wf = self.workflows.get_mut(&id)?;

        let current = match wf.status {
            WorkflowStatus::Running { current_step } => current_step,
            _ => return None,
        };

        // Mark current step result.
        if let Some(step) = wf.steps.get_mut(current) {
            step.status = if success {
                StepStatus::Success
            } else {
                StepStatus::Failed { reason: "execution failed".to_string() }
            };
        }

        // On failure: stop unless continue_on_failure.
        if !success {
            let cont = wf.steps.get(current).map_or(false, |s| s.continue_on_failure);
            if !cont {
                wf.status = WorkflowStatus::Failed { at_step: current, reason: "step failed".to_string() };
                self.active_id = None;
                self.save();
                return None;
            }
        }

        let next = current + 1;
        if next >= wf.steps.len() {
            wf.status = WorkflowStatus::Completed;
            wf.success_count += 1;
            self.active_id = None;
            info!("[WORKFLOWS] '{}' completed ({} steps)", id, wf.steps.len());
            self.save();
            return None;
        }

        wf.status = WorkflowStatus::Running { current_step: next };
        let cmd = wf.steps.get(next).map(|s| s.command_text.clone());
        self.save();
        cmd
    }

    /// Find a workflow whose voice phrase matches the given text.
    pub fn find_by_voice_phrase(&self, text: &str) -> Option<String> {
        let t = text.to_lowercase();
        self.workflows.values().find_map(|wf| {
            if let WorkflowTrigger::VoiceCommand { ref phrase } = wf.trigger {
                if t.contains(&phrase.to_lowercase()) {
                    return Some(wf.id.clone());
                }
            }
            None
        })
    }

    pub fn active_workflow(&self) -> Option<&Workflow> {
        self.active_id.as_ref().and_then(|id| self.workflows.get(id))
    }

    pub fn get(&self, id: &str) -> Option<&Workflow> {
        self.workflows.get(id)
    }

    pub fn workflow_count(&self) -> usize {
        self.workflows.len()
    }

    // ── Persistence ───────────────────────────────────────────────────────────

    fn load_from(path: &PathBuf) -> HashMap<String, Workflow> {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => HashMap::new(),
        }
    }

    fn save(&self) {
        if let Some(parent) = self.persistence_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(&self.workflows) {
            if let Err(e) = std::fs::write(&self.persistence_path, content) {
                warn!("[WORKFLOWS] Failed to save: {}", e);
            }
        }
    }
}
