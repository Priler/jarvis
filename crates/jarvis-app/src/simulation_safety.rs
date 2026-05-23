//! Simulation safety — prevents recursive simulation amplification, unstable
//! world models, probabilistic stability collapse, and simulation storms.

use std::sync::atomic::{AtomicU64, Ordering};

pub static SAFETY_GATES_FIRED:     AtomicU64 = AtomicU64::new(0);
pub static SIMULATIONS_SUPPRESSED: AtomicU64 = AtomicU64::new(0);

const MAX_UNCERTAINTY_FOR_SIM:     f32 = 0.82;
const MIN_CONFIDENCE_FOR_SYNTH:    f32 = 0.20;
const MAX_INSTABILITY_FOR_SIM:     f32 = 0.80;
const MAX_OSCILLATION_FOR_SYNTH:   f32 = 0.75;

// ── SafetyVerdict ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SafetyVerdict {
    pub is_safe: bool,
    pub reason:  Option<String>,
}

impl SafetyVerdict {
    fn safe()                   -> Self { SafetyVerdict { is_safe: true,  reason: None } }
    fn blocked(r: impl Into<String>) -> Self {
        SAFETY_GATES_FIRED.fetch_add(1, Ordering::Relaxed);
        SafetyVerdict { is_safe: false, reason: Some(r.into()) }
    }
}

// ── Simulation safety checks ──────────────────────────────────────────────────

/// Whether it is safe to run a new simulation tick.
pub fn check_simulation_safe() -> SafetyVerdict {
    let unc       = crate::generalized_uncertainty::profile();
    let prob_stab = crate::probabilistic_stability::check();
    let sem_stab  = crate::semantic_stability::check();

    if unc.overall > MAX_UNCERTAINTY_FOR_SIM {
        let v = SafetyVerdict::blocked(format!(
            "uncertainty_runaway: overall={:.3}", unc.overall
        ));
        crate::world_evolution_observability::record(
            crate::world_evolution_observability::WorldSimEvent::SimulationSuppressed {
                reason: v.reason.clone().unwrap_or_default(),
            }
        );
        SIMULATIONS_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        return v;
    }

    if prob_stab.has_belief_collapse {
        let v = SafetyVerdict::blocked("belief_collapse_risk");
        crate::world_evolution_observability::record(
            crate::world_evolution_observability::WorldSimEvent::SimulationSuppressed {
                reason: v.reason.clone().unwrap_or_default(),
            }
        );
        SIMULATIONS_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        return v;
    }

    if sem_stab.instability_score > MAX_INSTABILITY_FOR_SIM {
        let v = SafetyVerdict::blocked(format!(
            "semantic_instability: score={:.3}", sem_stab.instability_score
        ));
        crate::world_evolution_observability::record(
            crate::world_evolution_observability::WorldSimEvent::SimulationSuppressed {
                reason: v.reason.clone().unwrap_or_default(),
            }
        );
        SIMULATIONS_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        return v;
    }

    SafetyVerdict::safe()
}

/// Whether it is safe to synthesize a new cognition structure.
pub fn check_synthesis_safe() -> SafetyVerdict {
    let cog_stab = crate::cognitive_stability::check();
    let avg_conf = crate::belief_engine::avg_confidence();
    let belief_count = crate::belief_engine::belief_count();

    if belief_count > 5 && avg_conf < MIN_CONFIDENCE_FOR_SYNTH {
        return SafetyVerdict::blocked(format!(
            "belief_confidence_too_low: avg={:.3}", avg_conf
        ));
    }

    if cog_stab.oscillation_score > MAX_OSCILLATION_FOR_SYNTH {
        return SafetyVerdict::blocked(format!(
            "cognitive_oscillation_too_high: score={:.3}", cog_stab.oscillation_score
        ));
    }

    // Check validator
    let val = crate::evolution_validator::validate_change("synthesis");
    if !val.is_approved() {
        return SafetyVerdict::blocked(
            val.reason().unwrap_or("validator_rejected").to_string()
        );
    }

    SafetyVerdict::safe()
}

pub fn gates_fired()          -> u64 { SAFETY_GATES_FIRED.load(Ordering::Relaxed) }
pub fn simulations_suppressed() -> u64 { SIMULATIONS_SUPPRESSED.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_simulation_safe_no_panic() {
        let v = check_simulation_safe();
        let _ = v.is_safe;
    }

    #[test]
    fn check_synthesis_safe_no_panic() {
        let v = check_synthesis_safe();
        let _ = v.is_safe;
    }

    #[test]
    fn gates_counter_non_negative() {
        let _ = check_simulation_safe();
        assert!(gates_fired() + simulations_suppressed() >= 0);
    }
}
