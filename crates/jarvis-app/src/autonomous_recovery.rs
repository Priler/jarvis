//! Autonomous recovery engine — recovers degraded cognition, restarts failed
//! services, rolls back unstable topology changes, and restores distributed
//! stability.

use std::sync::atomic::{AtomicU64, Ordering};

pub static RECOVERY_ACTIONS:       AtomicU64 = AtomicU64::new(0);
pub static TOPOLOGY_ROLLBACKS:     AtomicU64 = AtomicU64::new(0);
pub static STABILITY_RESTORATIONS: AtomicU64 = AtomicU64::new(0);

// ── RecoveryReport ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RecoveryReport {
    pub services_recovered:     usize,
    pub topology_rollbacks:     usize,
    pub stability_restorations: usize,
    pub total_actions:          usize,
    pub system_stable_after:    bool,
}

// ── Recovery logic ────────────────────────────────────────────────────────────

/// Run one recovery cycle.  Attempts to restore system to stable state.
pub fn recover() -> RecoveryReport {
    let mut services_recovered    = 0usize;
    let mut topology_rollbacks    = 0usize;
    let mut stability_restorations = 0usize;

    // 1. Recover degraded persistent services
    let recovered = crate::persistent_services::recover_degraded();
    if recovered > 0 {
        services_recovered += recovered;
        RECOVERY_ACTIONS.fetch_add(recovered as u64, Ordering::Relaxed);
        crate::ai_os_observability::record(
            crate::ai_os_observability::AiOsEvent::RecoveryAction {
                component: "persistent_services".into(),
                action: format!("recovered_{recovered}"),
            }
        );
    }

    // 2. Topology rollback: restore suppressed paths if load has dropped
    let suppressed = crate::adaptive_topology::suppressed_paths();
    for path in &suppressed {
        let load = crate::adaptive_topology::get_load(*path);
        if load < 0.35 {
            crate::adaptive_topology::restore_path(*path);
            topology_rollbacks += 1;
            TOPOLOGY_ROLLBACKS.fetch_add(1, Ordering::Relaxed);
            crate::ai_os_observability::record(
                crate::ai_os_observability::AiOsEvent::RecoveryAction {
                    component: path.name().into(),
                    action: "topology_restore".into(),
                }
            );
        }
    }

    // 3. Stability restoration: rebalance if overall load is high
    let avg_load = crate::adaptive_topology::avg_load();
    if avg_load > 0.65 {
        crate::adaptive_topology::rebalance();
        crate::resource_governor::rebalance();
        stability_restorations += 1;
        STABILITY_RESTORATIONS.fetch_add(1, Ordering::Relaxed);
    }

    // 4. Redistribute distributed worker load if overloaded
    let worker_avg = crate::distributed_runtime::avg_worker_load();
    if worker_avg > 0.70 {
        crate::distributed_runtime::rebalance();
        stability_restorations += 1;
    }

    let total = services_recovered + topology_rollbacks + stability_restorations;
    let stab_after = crate::recursive_stability::check();

    RecoveryReport {
        services_recovered,
        topology_rollbacks,
        stability_restorations,
        total_actions: total,
        system_stable_after: stab_after.overall_stable,
    }
}

pub fn total_recovery_actions()     -> u64 { RECOVERY_ACTIONS.load(Ordering::Relaxed) }
pub fn total_topology_rollbacks()   -> u64 { TOPOLOGY_ROLLBACKS.load(Ordering::Relaxed) }
pub fn total_stability_restorations() -> u64 { STABILITY_RESTORATIONS.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recover_no_panic() {
        let r = recover();
        assert!(r.total_actions < 1000); // sanity
    }

    #[test]
    fn system_stable_after_is_bool() {
        let r = recover();
        let _ = r.system_stable_after;
    }

    #[test]
    fn counters_non_decreasing() {
        let before = total_recovery_actions();
        let _ = recover();
        assert!(total_recovery_actions() >= before);
    }
}
