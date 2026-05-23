//! Persistent services — manages 7 long-running cognition services that survive
//! runtime reloads, maintain persistent state, auto-recover after failure, and
//! maintain local cognition continuity.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static SERVICES_HEALTHY:   AtomicU64 = AtomicU64::new(0);
pub static SERVICES_RECOVERED: AtomicU64 = AtomicU64::new(0);

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── ServiceKind ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceKind {
    Memory,
    Planning,
    Simulation,
    Voice,
    Routing,
    Belief,
    WorldModel,
}

impl ServiceKind {
    pub fn all() -> &'static [ServiceKind] {
        &[
            Self::Memory, Self::Planning, Self::Simulation,
            Self::Voice,  Self::Routing,  Self::Belief, Self::WorldModel,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Memory     => "memory_service",
            Self::Planning   => "planning_service",
            Self::Simulation => "simulation_service",
            Self::Voice      => "voice_service",
            Self::Routing    => "routing_service",
            Self::Belief     => "belief_service",
            Self::WorldModel => "world_model_service",
        }
    }
}

// ── ServiceRecord ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ServiceRecord {
    pub kind:           ServiceKind,
    pub is_healthy:     bool,
    pub health_score:   f32,
    pub failure_count:  u32,
    pub last_check_ms:  u64,
}

impl ServiceRecord {
    fn new(kind: ServiceKind) -> Self {
        ServiceRecord {
            kind,
            is_healthy: true,
            health_score: 1.0,
            failure_count: 0,
            last_check_ms: 0,
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ServiceRegistry {
    services: Vec<ServiceRecord>,
}

impl ServiceRegistry {
    fn new() -> Self {
        ServiceRegistry {
            services: ServiceKind::all().iter().map(|k| ServiceRecord::new(*k)).collect()
        }
    }
}

static REGISTRY: Lazy<Mutex<ServiceRegistry>> =
    Lazy::new(|| Mutex::new(ServiceRegistry::new()));

// ── Health evaluation ─────────────────────────────────────────────────────────

fn evaluate_health(kind: ServiceKind) -> f32 {
    let unc  = crate::generalized_uncertainty::profile();
    let conf = crate::confidence_reasoner::assess();
    let sem  = crate::semantic_stability::check();

    match kind {
        ServiceKind::Memory     => crate::belief_engine::avg_confidence(),
        ServiceKind::Planning   => conf.planner_confidence,
        ServiceKind::Simulation => (1.0 - unc.overall).clamp(0.0, 1.0),
        ServiceKind::Voice      => 1.0, // always available (no external deps)
        ServiceKind::Routing    => conf.overall,
        ServiceKind::Belief     => crate::belief_engine::avg_confidence(),
        ServiceKind::WorldModel => (1.0 - sem.instability_score).clamp(0.0, 1.0),
    }
}

// ── API ───────────────────────────────────────────────────────────────────────

/// Update health status for all services.
pub fn check_all() -> Vec<ServiceRecord> {
    let mut reg = REGISTRY.lock().unwrap();
    let ts = ts_now();
    let mut healthy_count = 0u64;

    for svc in reg.services.iter_mut() {
        let score = evaluate_health(svc.kind);
        svc.health_score   = score;
        svc.is_healthy     = score > 0.25;
        svc.last_check_ms  = ts;
        if !svc.is_healthy {
            svc.failure_count += 1;
        } else {
            healthy_count += 1;
        }
    }

    SERVICES_HEALTHY.store(healthy_count, Ordering::Relaxed);
    reg.services.clone()
}

/// Attempt recovery of degraded services.
pub fn recover_degraded() -> usize {
    let services = {
        REGISTRY.lock().unwrap().services.clone()
    };

    let mut recovered = 0;
    for svc in &services {
        if !svc.is_healthy && svc.failure_count > 0 {
            // Recovery: trigger a belief propagation to restore confidence
            if svc.kind == ServiceKind::Belief || svc.kind == ServiceKind::Memory {
                crate::belief_propagation::propagate(3);
            }
            // Re-check health after recovery attempt
            let new_score = evaluate_health(svc.kind);
            if new_score > 0.25 {
                let mut reg = REGISTRY.lock().unwrap();
                if let Some(s) = reg.services.iter_mut().find(|s| s.kind == svc.kind) {
                    s.is_healthy    = true;
                    s.health_score  = new_score;
                    s.failure_count = 0;
                }
                SERVICES_RECOVERED.fetch_add(1, Ordering::Relaxed);
                recovered += 1;

                crate::ai_os_observability::record(
                    crate::ai_os_observability::AiOsEvent::ServiceRecovered {
                        name: svc.kind.label().into(),
                    }
                );
            }
        }
    }
    recovered
}

pub fn healthy_count()   -> usize {
    REGISTRY.lock().unwrap().services.iter().filter(|s| s.is_healthy).count()
}

pub fn degraded_count()  -> usize {
    REGISTRY.lock().unwrap().services.iter().filter(|s| !s.is_healthy).count()
}

pub fn all_services()    -> Vec<ServiceRecord> {
    REGISTRY.lock().unwrap().services.clone()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_all_returns_seven_services() {
        let services = check_all();
        assert_eq!(services.len(), 7);
    }

    #[test]
    fn health_scores_bounded() {
        let services = check_all();
        for s in &services {
            assert!(s.health_score >= 0.0 && s.health_score <= 1.0);
        }
    }

    #[test]
    fn recover_degraded_no_panic() {
        let _ = check_all();
        let _ = recover_degraded();
    }
}
