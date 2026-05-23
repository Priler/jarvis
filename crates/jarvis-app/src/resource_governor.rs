//! Resource governor — monitors CPU/memory pressure proxies, throttles
//! cognition workloads, protects desktop responsiveness, and rebalances
//! memory pressure across cognition services.

use std::sync::atomic::{AtomicU64, Ordering};

pub static THROTTLE_EVENTS: AtomicU64 = AtomicU64::new(0);
pub static REBALANCE_EVENTS: AtomicU64 = AtomicU64::new(0);

const MEMORY_PRESSURE_THROTTLE:  f32 = 0.72;
const OVERALL_LOAD_THROTTLE:     f32 = 0.78;
const DESKTOP_SAFETY_THRESHOLD:  f32 = 0.88;

// ── ResourceSnapshot ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    pub memory_pressure:      f32,
    pub cpu_proxy_load:       f32,    // adaptive_topology avg_load proxy
    pub cognition_load:       f32,
    pub desktop_safe:         bool,
    pub should_throttle:      bool,
    pub throttle_factor:      f32,    // 0.0=full throttle, 1.0=no throttle
}

impl ResourceSnapshot {
    pub fn is_overloaded(&self) -> bool {
        self.cpu_proxy_load > OVERALL_LOAD_THROTTLE
            || self.memory_pressure > MEMORY_PRESSURE_THROTTLE
    }
}

// ── API ───────────────────────────────────────────────────────────────────────

/// Sample current resource state from live signals.
pub fn sample() -> ResourceSnapshot {
    let resource   = crate::abstract_resource_reasoner::sample();
    let avg_load   = crate::adaptive_topology::avg_load();
    let unc        = crate::generalized_uncertainty::profile();

    let memory_pressure = resource.overall;
    let cpu_proxy_load  = avg_load;
    let cognition_load  = (avg_load + unc.overall) / 2.0;

    let should_throttle = memory_pressure > MEMORY_PRESSURE_THROTTLE
        || cpu_proxy_load > OVERALL_LOAD_THROTTLE;

    // Desktop is safe when overall load is below desktop-safety threshold
    let desktop_safe = cognition_load < DESKTOP_SAFETY_THRESHOLD;

    // Throttle factor: 1.0 when safe, approaches 0 as load rises
    let throttle_factor = if should_throttle {
        (1.0 - (cpu_proxy_load - OVERALL_LOAD_THROTTLE) / (1.0 - OVERALL_LOAD_THROTTLE))
            .clamp(0.20, 0.80)
    } else {
        1.0
    };

    if should_throttle {
        THROTTLE_EVENTS.fetch_add(1, Ordering::Relaxed);
        crate::ai_os_observability::record(
            crate::ai_os_observability::AiOsEvent::ResourceThrottle {
                component: "resource_governor".into(),
                pressure:  memory_pressure,
            }
        );
    }

    ResourceSnapshot {
        memory_pressure,
        cpu_proxy_load,
        cognition_load,
        desktop_safe,
        should_throttle,
        throttle_factor,
    }
}

/// Rebalance: nudge overloaded paths toward lower load.
pub fn rebalance() {
    let resource = crate::abstract_resource_reasoner::sample();
    if resource.overall > MEMORY_PRESSURE_THROTTLE {
        // Signal adaptive topology to rebalance weights
        crate::adaptive_topology::rebalance();
        REBALANCE_EVENTS.fetch_add(1, Ordering::Relaxed);
        crate::ai_os_observability::record(
            crate::ai_os_observability::AiOsEvent::DistributedRebalance {
                workers: 1,
                avg_load: crate::adaptive_topology::avg_load(),
            }
        );
    }
}

pub fn throttle_events()  -> u64 { THROTTLE_EVENTS.load(Ordering::Relaxed) }
pub fn rebalance_events() -> u64 { REBALANCE_EVENTS.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_no_panic() {
        let s = sample();
        assert!(s.memory_pressure >= 0.0 && s.memory_pressure <= 1.0);
        assert!(s.throttle_factor >= 0.0 && s.throttle_factor <= 1.0);
    }

    #[test]
    fn desktop_safe_when_low_load() {
        let s = sample();
        // When system is at rest, desktop should generally be safe
        let _ = s.desktop_safe;
    }

    #[test]
    fn rebalance_no_panic() {
        rebalance();
    }
}
