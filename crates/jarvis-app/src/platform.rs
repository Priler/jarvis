#![allow(dead_code)]

use std::sync::Arc;

use crate::agents::{AgentRuntime, PerceptionAgent, ExecutionAgent, SupervisorAgent};
use crate::bus::CognitiveBus;
use crate::cognitive::CognitiveRuntime;
use crate::governance::GovernanceLayer;
use crate::perception::PerceptionLayer;
use crate::plugin::PluginRegistry;
use crate::scheduler::CognitiveScheduler;
use crate::workflows::WorkflowEngine;

/// Central platform struct. Owned by the app thread; passed as `&mut AppPlatform`
/// through the event-handling call chain.
pub struct AppPlatform {
    pub cognitive: CognitiveRuntime,
    pub agents: AgentRuntime,
    pub governance: GovernanceLayer,
    pub workflows: WorkflowEngine,
    pub scheduler: CognitiveScheduler,
    pub perception: PerceptionLayer,
    pub plugins: PluginRegistry,
    pub bus: Arc<CognitiveBus>,
}

impl AppPlatform {
    pub fn new() -> Self {
        let bus = CognitiveBus::new();

        let cognitive = CognitiveRuntime::new();
        let agents = {
            let mut rt = AgentRuntime::new(Arc::clone(&bus));
            let _ = rt.register(PerceptionAgent::new());
            let _ = rt.register(ExecutionAgent::new());
            let _ = rt.register(SupervisorAgent::new());
            rt
        };
        let governance = GovernanceLayer::new();
        let workflows = WorkflowEngine::new();
        let scheduler = CognitiveScheduler::new(Arc::clone(&bus));
        let perception = PerceptionLayer::new(Arc::clone(&bus));
        let plugins = PluginRegistry::new();

        Self {
            cognitive,
            agents,
            governance,
            workflows,
            scheduler,
            perception,
            plugins,
            bus,
        }
    }

    /// Start all agents and log the platform inventory.
    pub fn start(&mut self) {
        self.agents.start_all();
        info!(
            "[PLATFORM] OS Platform ready — agents={} workflows={} plugins={}",
            self.agents.agent_count(),
            self.workflows.workflow_count(),
            self.plugins.plugin_count(),
        );
    }

    /// Dispatch an event through agents and plugins.
    pub fn dispatch(&mut self, event: crate::bus::BusEvent) {
        self.bus.publish(event.clone());
        self.agents.dispatch(&event);
        self.plugins.dispatch(&event);
    }
}
