#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use crate::bus::{BusEvent, CognitiveBus};

// ── Identity & state ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new(id: impl Into<String>) -> Self {
        AgentId(id.into())
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    Idle,
    Active,
    Processing,
    Recovering,
    Stopped,
    Failed { reason: String },
}

impl AgentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentState::Idle => "idle",
            AgentState::Active => "active",
            AgentState::Processing => "processing",
            AgentState::Recovering => "recovering",
            AgentState::Stopped => "stopped",
            AgentState::Failed { .. } => "failed",
        }
    }
}

// ── Capabilities & permissions ────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct AgentCapabilities {
    pub can_perceive_voice: bool,
    pub can_perceive_screen: bool,
    pub can_execute_commands: bool,
    pub can_manage_workflows: bool,
    pub can_read_memory: bool,
    pub can_write_memory: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum FilesystemScope {
    #[default]
    None,
    ReadOnly,
    ReadWrite,
    Unrestricted,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum AutomationScope {
    #[default]
    None,
    UserSpace,
    System,
}

#[derive(Debug, Clone, Default)]
pub struct AgentPermissions {
    pub filesystem: FilesystemScope,
    pub automation: AutomationScope,
    pub network_allowed: bool,
    pub max_execution_time_ms: u64,
}

// ── Health ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgentHealth {
    pub healthy: bool,
    pub last_active_ms: u64,
    pub error_count: u32,
    pub details: String,
}

// ── Agent trait ───────────────────────────────────────────────────────────────

pub trait Agent: Send + Sync {
    fn agent_id(&self) -> &AgentId;
    fn capabilities(&self) -> &AgentCapabilities;
    fn permissions(&self) -> &AgentPermissions;
    fn state(&self) -> AgentState;
    fn start(&mut self) -> Result<(), String>;
    fn stop(&mut self);
    fn handle_event(&mut self, event: &BusEvent) -> Option<BusEvent>;
    fn recover(&mut self) -> bool;
    fn health_check(&self) -> AgentHealth;
}

// ── Agent runtime ─────────────────────────────────────────────────────────────

pub struct AgentRuntime {
    agents: HashMap<AgentId, Box<dyn Agent>>,
    bus: Arc<CognitiveBus>,
}

impl AgentRuntime {
    pub fn new(bus: Arc<CognitiveBus>) -> Self {
        Self { agents: HashMap::new(), bus }
    }

    pub fn register(&mut self, agent: Box<dyn Agent>) -> Result<(), String> {
        let id = agent.agent_id().clone();
        if self.agents.contains_key(&id) {
            return Err(format!("Agent '{}' already registered", id));
        }
        info!("[AGENTS] Registered agent '{}'", id);
        self.agents.insert(id, agent);
        Ok(())
    }

    pub fn start_all(&mut self) {
        let mut started = 0usize;
        for (id, agent) in self.agents.iter_mut() {
            match agent.start() {
                Ok(()) => {
                    started += 1;
                    self.bus.publish(BusEvent::AgentStarted { id: id.0.clone() });
                }
                Err(e) => error!("[AGENTS] Failed to start '{}': {}", id, e),
            }
        }
        info!("[AGENTS] {}/{} agent(s) started", started, self.agents.len());
    }

    pub fn stop_all(&mut self) {
        for (id, agent) in self.agents.iter_mut() {
            agent.stop();
            self.bus.publish(BusEvent::AgentStopped { id: id.0.clone(), reason: "shutdown".to_string() });
        }
    }

    pub fn dispatch(&mut self, event: &BusEvent) {
        for (id, agent) in self.agents.iter_mut() {
            if let Some(response) = agent.handle_event(event) {
                debug!("[AGENTS] Agent '{}' emitted response", id);
                self.bus.publish(response);
            }
        }
    }

    pub fn run_health_checks(&self) {
        for (id, agent) in self.agents.iter() {
            let h = agent.health_check();
            if !h.healthy {
                warn!("[AGENTS] Unhealthy: '{}' — {}", id, h.details);
                self.bus.publish(BusEvent::HealthCheck {
                    component: format!("agent:{}", id),
                    healthy: false,
                    details: h.details.clone(),
                });
            }
        }
    }

    pub fn recover_failed(&mut self) {
        for (id, agent) in self.agents.iter_mut() {
            if matches!(agent.state(), AgentState::Failed { .. }) {
                info!("[AGENTS] Attempting recovery for '{}'", id);
                if agent.recover() {
                    info!("[AGENTS] Agent '{}' recovered", id);
                    self.bus.publish(BusEvent::AgentRecovered { id: id.0.clone() });
                } else {
                    error!("[AGENTS] Recovery failed for '{}'", id);
                }
            }
        }
    }

    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    pub fn states(&self) -> Vec<(String, &'static str)> {
        self.agents.iter()
            .map(|(id, a)| (id.0.clone(), a.state().as_str()))
            .collect()
    }
}

// ── Built-in stub agents ──────────────────────────────────────────────────────

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Perception agent — owns voice/audio awareness (delegates to stt_worker).
pub struct PerceptionAgent {
    id: AgentId,
    caps: AgentCapabilities,
    perms: AgentPermissions,
    state: AgentState,
    last_active: u64,
}

impl PerceptionAgent {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            id: AgentId::new("perception"),
            caps: AgentCapabilities {
                can_perceive_voice: true,
                can_perceive_screen: true,
                ..Default::default()
            },
            perms: AgentPermissions::default(),
            state: AgentState::Idle,
            last_active: 0,
        })
    }
}

impl Agent for PerceptionAgent {
    fn agent_id(&self) -> &AgentId { &self.id }
    fn capabilities(&self) -> &AgentCapabilities { &self.caps }
    fn permissions(&self) -> &AgentPermissions { &self.perms }
    fn state(&self) -> AgentState { self.state.clone() }
    fn start(&mut self) -> Result<(), String> { self.state = AgentState::Active; Ok(()) }
    fn stop(&mut self) { self.state = AgentState::Stopped; }
    fn handle_event(&mut self, event: &BusEvent) -> Option<BusEvent> {
        if matches!(event, BusEvent::VoiceIntent { .. } | BusEvent::ScreenChanged { .. }) {
            self.last_active = now_ms();
        }
        None
    }
    fn recover(&mut self) -> bool { self.state = AgentState::Active; true }
    fn health_check(&self) -> AgentHealth {
        AgentHealth { healthy: true, last_active_ms: self.last_active, error_count: 0, details: String::new() }
    }
}

/// Execution agent — supervises command dispatching.
pub struct ExecutionAgent {
    id: AgentId,
    caps: AgentCapabilities,
    perms: AgentPermissions,
    state: AgentState,
    commands_dispatched: u32,
    commands_failed: u32,
    last_active: u64,
}

impl ExecutionAgent {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            id: AgentId::new("execution"),
            caps: AgentCapabilities { can_execute_commands: true, ..Default::default() },
            perms: AgentPermissions {
                automation: AutomationScope::UserSpace,
                ..Default::default()
            },
            state: AgentState::Idle,
            commands_dispatched: 0,
            commands_failed: 0,
            last_active: 0,
        })
    }
}

impl Agent for ExecutionAgent {
    fn agent_id(&self) -> &AgentId { &self.id }
    fn capabilities(&self) -> &AgentCapabilities { &self.caps }
    fn permissions(&self) -> &AgentPermissions { &self.perms }
    fn state(&self) -> AgentState { self.state.clone() }
    fn start(&mut self) -> Result<(), String> { self.state = AgentState::Active; Ok(()) }
    fn stop(&mut self) { self.state = AgentState::Stopped; }
    fn handle_event(&mut self, event: &BusEvent) -> Option<BusEvent> {
        match event {
            BusEvent::CommandDispatched { .. } => {
                self.commands_dispatched += 1;
                self.last_active = now_ms();
            }
            BusEvent::CommandCompleted { success, .. } => {
                if !success { self.commands_failed += 1; }
                self.last_active = now_ms();
            }
            _ => {}
        }
        None
    }
    fn recover(&mut self) -> bool { self.state = AgentState::Active; true }
    fn health_check(&self) -> AgentHealth {
        let failure_rate = if self.commands_dispatched > 0 {
            self.commands_failed as f64 / self.commands_dispatched as f64
        } else { 0.0 };
        let healthy = failure_rate < 0.5;
        AgentHealth {
            healthy,
            last_active_ms: self.last_active,
            error_count: self.commands_failed,
            details: format!("dispatched={} failed={}", self.commands_dispatched, self.commands_failed),
        }
    }
}

/// Supervisor agent — monitors runtime health and coordinates recovery.
pub struct SupervisorAgent {
    id: AgentId,
    caps: AgentCapabilities,
    perms: AgentPermissions,
    state: AgentState,
    unhealthy_count: u32,
    last_active: u64,
}

impl SupervisorAgent {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            id: AgentId::new("supervisor"),
            caps: AgentCapabilities::default(),
            perms: AgentPermissions::default(),
            state: AgentState::Idle,
            unhealthy_count: 0,
            last_active: 0,
        })
    }
}

impl Agent for SupervisorAgent {
    fn agent_id(&self) -> &AgentId { &self.id }
    fn capabilities(&self) -> &AgentCapabilities { &self.caps }
    fn permissions(&self) -> &AgentPermissions { &self.perms }
    fn state(&self) -> AgentState { self.state.clone() }
    fn start(&mut self) -> Result<(), String> { self.state = AgentState::Active; Ok(()) }
    fn stop(&mut self) { self.state = AgentState::Stopped; }
    fn handle_event(&mut self, event: &BusEvent) -> Option<BusEvent> {
        if let BusEvent::HealthCheck { healthy: false, component, details } = event {
            self.unhealthy_count += 1;
            self.last_active = now_ms();
            warn!("[SUPERVISOR] Unhealthy component '{}': {}", component, details);
        }
        None
    }
    fn recover(&mut self) -> bool { self.state = AgentState::Active; true }
    fn health_check(&self) -> AgentHealth {
        AgentHealth { healthy: true, last_active_ms: self.last_active, error_count: 0, details: String::new() }
    }
}
