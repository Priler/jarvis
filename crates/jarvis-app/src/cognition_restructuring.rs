//! Cognition restructuring — suppresses unstable paths, merges redundant
//! reasoning pipelines, splits overloaded ones, and rebalances scheduling.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static RESTRUCTURINGS_APPLIED: AtomicU64 = AtomicU64::new(0);
pub static PATHS_SUPPRESSED:       AtomicU64 = AtomicU64::new(0);
pub static PATHS_RESTORED:         AtomicU64 = AtomicU64::new(0);

// At most this many restructuring actions per tick (prevents cascades)
const MAX_ACTIONS_PER_TICK: usize = 2;
// Load below which a suppressed path is eligible for restoration
const RESTORE_LOAD_THRESHOLD: f32 = 0.40;

use crate::adaptive_topology::CognitionPath;

const MAX_HISTORY: usize = 200;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── RestructuringAction ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RestructuringAction {
    SuppressPath         { path: CognitionPath, reason: String },
    RestorePath          { path: CognitionPath },
    RebalanceScheduler   { target: CognitionPath, load_delta: f32 },
    MergeRedundantPaths  { from: CognitionPath, into_path: CognitionPath },
}

impl RestructuringAction {
    pub fn label(&self) -> String {
        match self {
            Self::SuppressPath { path, .. }        => format!("[RESTRUCTURE] suppress {}", path.name()),
            Self::RestorePath { path }              => format!("[RESTRUCTURE] restore {}", path.name()),
            Self::RebalanceScheduler { target, .. } => format!("[RESTRUCTURE] rebalance {}", target.name()),
            Self::MergeRedundantPaths { from, into_path } =>
                format!("[RESTRUCTURE] merge {} → {}", from.name(), into_path.name()),
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<(u64, RestructuringAction)>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Core restructuring logic ──────────────────────────────────────────────────

/// Analyse topology and generate at most MAX_ACTIONS_PER_TICK restructuring actions.
pub fn plan_restructuring() -> Vec<RestructuringAction> {
    let overloaded  = crate::adaptive_topology::overloaded_paths();
    let suppressed  = crate::adaptive_topology::suppressed_paths();
    let all_loads   = crate::adaptive_topology::all_loads();

    let mut actions: Vec<RestructuringAction> = Vec::new();

    // 1. Suppress overloaded paths (validate first)
    for path in &overloaded {
        if actions.len() >= MAX_ACTIONS_PER_TICK { break; }
        let result = crate::evolution_validator::validate_change(path.name());
        if result.is_approved() {
            actions.push(RestructuringAction::SuppressPath {
                path:   *path,
                reason: "overload_threshold_exceeded".into(),
            });
        }
    }

    // 2. Restore paths that have been suppressed and whose load dropped
    for path in &suppressed {
        if actions.len() >= MAX_ACTIONS_PER_TICK { break; }
        let current_load = crate::adaptive_topology::get_load(*path);
        if current_load < RESTORE_LOAD_THRESHOLD {
            actions.push(RestructuringAction::RestorePath { path: *path });
        }
    }

    // 3. Merge two overloaded paths if one stable path can absorb traffic
    if actions.is_empty() && overloaded.len() >= 2 {
        if let (Some(&from), Some(&stable)) = (
            overloaded.first(),
            all_loads.iter().find(|l| l.is_stable && !l.is_overloaded && !l.suppressed)
                .map(|l| &l.path),
        ) {
            if from != stable {
                let result = crate::evolution_validator::validate_change("merge");
                if result.is_approved() {
                    actions.push(RestructuringAction::MergeRedundantPaths {
                        from,
                        into_path: stable,
                    });
                }
            }
        }
    }

    actions
}

/// Apply a list of restructuring actions.
pub fn apply(actions: &[RestructuringAction]) -> usize {
    let mut applied = 0;
    for action in actions {
        match action {
            RestructuringAction::SuppressPath { path, reason } => {
                crate::adaptive_topology::suppress_path(*path);
                crate::topology_memory::record(crate::topology_memory::TopologyEvent::PathSuppressed {
                    path: path.name().into(),
                    load: crate::adaptive_topology::get_load(*path),
                });
                PATHS_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
                eprintln!("[RESTRUCTURE] Suppressed {} — {reason}", path.name());
            }
            RestructuringAction::RestorePath { path } => {
                crate::adaptive_topology::restore_path(*path);
                crate::topology_memory::record(crate::topology_memory::TopologyEvent::PathRestored {
                    path: path.name().into(),
                });
                PATHS_RESTORED.fetch_add(1, Ordering::Relaxed);
                eprintln!("[RESTRUCTURE] Restored {}", path.name());
            }
            RestructuringAction::RebalanceScheduler { target, load_delta } => {
                crate::adaptive_topology::update_load(*target, *load_delta);
                crate::topology_memory::record(crate::topology_memory::TopologyEvent::SchedulerAdaptation {
                    target: target.name().into(),
                    action: "rebalance".into(),
                    delta:  *load_delta,
                });
            }
            RestructuringAction::MergeRedundantPaths { from, into_path } => {
                // Redirect all weight from `from` to `into_path`
                let from_weight = crate::adaptive_topology::get_weight(*from);
                crate::adaptive_topology::suppress_path(*from);
                crate::adaptive_topology::update_load(*into_path,
                    (crate::adaptive_topology::get_load(*into_path) * 0.70
                    + from_weight * 0.30).clamp(0.0, 1.0));
                crate::topology_memory::record(crate::topology_memory::TopologyEvent::TopologyChange {
                    component: from.name().into(),
                    old_state: "active".into(),
                    new_state: format!("merged_into_{}", into_path.name()),
                });
            }
        }
        let mut h = HISTORY.lock().unwrap();
        if h.len() >= MAX_HISTORY { h.remove(0); }
        h.push((ts_now(), action.clone()));
        RESTRUCTURINGS_APPLIED.fetch_add(1, Ordering::Relaxed);
        applied += 1;
    }
    applied
}

/// Plan and immediately apply restructuring in one call.
pub fn restructure() -> usize {
    let actions = plan_restructuring();
    apply(&actions)
}

pub fn recent_actions(n: usize) -> Vec<(u64, RestructuringAction)> {
    HISTORY.lock().unwrap().iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_no_panic() {
        let _ = plan_restructuring();
    }

    #[test]
    fn restructure_bounded() {
        // Even if all paths are overloaded, at most MAX_ACTIONS_PER_TICK applied
        let actions = plan_restructuring();
        assert!(actions.len() <= MAX_ACTIONS_PER_TICK);
    }
}
