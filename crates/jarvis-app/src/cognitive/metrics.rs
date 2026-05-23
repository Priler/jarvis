#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};

pub static GOAL_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
pub static GOAL_SUCCESSES: AtomicU64 = AtomicU64::new(0);
pub static GOAL_FAILURES: AtomicU64 = AtomicU64::new(0);
pub static PLAN_REBUILDS: AtomicU64 = AtomicU64::new(0);
pub static MEMORY_HITS: AtomicU64 = AtomicU64::new(0);
pub static MEMORY_MISSES: AtomicU64 = AtomicU64::new(0);
pub static CLARIFICATIONS_ISSUED: AtomicU64 = AtomicU64::new(0);
pub static CLARIFICATIONS_RESOLVED: AtomicU64 = AtomicU64::new(0);
pub static DOMAIN_SWITCHES: AtomicU64 = AtomicU64::new(0);
pub static CONTEXT_RESOLUTIONS: AtomicU64 = AtomicU64::new(0);
pub static AUTONOMOUS_STEPS: AtomicU64 = AtomicU64::new(0);

pub fn goal_success_rate() -> f64 {
    let attempts = GOAL_ATTEMPTS.load(Ordering::Relaxed);
    if attempts == 0 {
        return 0.0;
    }
    GOAL_SUCCESSES.load(Ordering::Relaxed) as f64 / attempts as f64
}

pub fn memory_hit_rate() -> f64 {
    let hits = MEMORY_HITS.load(Ordering::Relaxed);
    let misses = MEMORY_MISSES.load(Ordering::Relaxed);
    let total = hits + misses;
    if total == 0 {
        return 0.0;
    }
    hits as f64 / total as f64
}

pub fn clarification_rate() -> f64 {
    let goals = GOAL_ATTEMPTS.load(Ordering::Relaxed);
    if goals == 0 {
        return 0.0;
    }
    CLARIFICATIONS_ISSUED.load(Ordering::Relaxed) as f64 / goals as f64
}
