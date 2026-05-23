//! Resource optimizer — dynamic VRAM/CPU/GPU management to prevent desktop freezing.
//! Tracks model idle time, signals unload recommendations, rebalances workloads.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::collections::HashMap;

pub static UNLOAD_SUGGESTIONS:  AtomicU64 = AtomicU64::new(0);
pub static REBALANCE_EVENTS:    AtomicU64 = AtomicU64::new(0);
pub static OPTIMIZER_RUNS:      AtomicU64 = AtomicU64::new(0);

const MODEL_IDLE_TIMEOUT_MS: u64 = 5 * 60 * 1_000; // 5 minutes
const VRAM_HIGH_WATERMARK:   f32 = 0.85; // 85% VRAM = high pressure
const CPU_HIGH_WATERMARK:    f32 = 0.80; // 80% CPU = high pressure

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelUsageEntry {
    pub model_name:   String,
    pub last_used_ms: u64,
    pub query_count:  u64,
    pub idle_ms:      u64,
    pub suggest_unload: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum ResourceDecision {
    NoAction,
    SuggestModelUnload(String),
    ReduceGpuAlloc(u8),
    EngageEcoMode,
    PauseIndexing,
}

struct OptimizerState {
    model_usage:      HashMap<String, (u64, u64)>, // name → (last_used_ms, query_count)
    vram_pressure:    f32,
    cpu_pressure:     f32,
    indexing_paused:  bool,
}

impl OptimizerState {
    fn new() -> Self {
        Self {
            model_usage:     HashMap::new(),
            vram_pressure:   0.0,
            cpu_pressure:    0.0,
            indexing_paused: false,
        }
    }
}

static STATE: Lazy<Mutex<OptimizerState>> = Lazy::new(|| Mutex::new(OptimizerState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Feed ──────────────────────────────────────────────────────────────────────

pub fn record_model_query(model_name: &str) {
    let mut s = STATE.lock().unwrap();
    let entry = s.model_usage.entry(model_name.to_string()).or_insert((0, 0));
    entry.0 = ts_now();
    entry.1 += 1;
}

pub fn update_vram_pressure(pct: f32) {
    STATE.lock().unwrap().vram_pressure = pct.clamp(0.0, 1.0);
}

pub fn update_cpu_pressure(pct: f32) {
    STATE.lock().unwrap().cpu_pressure = pct.clamp(0.0, 1.0);
}

// ── Optimize ──────────────────────────────────────────────────────────────────

pub fn run_optimization() -> Vec<ResourceDecision> {
    OPTIMIZER_RUNS.fetch_add(1, Ordering::Relaxed);
    let mut decisions = Vec::new();
    let now = ts_now();

    let s = STATE.lock().unwrap();
    let vram = s.vram_pressure;
    let cpu  = s.cpu_pressure;
    let usage = s.model_usage.clone();
    drop(s);

    // Check idle models
    for (name, (last_used, _)) in &usage {
        let idle_ms = now.saturating_sub(*last_used);
        if idle_ms > MODEL_IDLE_TIMEOUT_MS && vram > 0.5 {
            decisions.push(ResourceDecision::SuggestModelUnload(name.clone()));
            UNLOAD_SUGGESTIONS.fetch_add(1, Ordering::Relaxed);
            crate::production_logging::info("resource_optimizer",
                &format!("suggest unload idle model: {} (idle {}s)", name, idle_ms / 1000));
        }
    }

    // High VRAM pressure
    if vram >= VRAM_HIGH_WATERMARK {
        let current_alloc = crate::performance_profiles::gpu_alloc_pct();
        if current_alloc > 40 {
            let new_alloc = (current_alloc - 15).max(20);
            decisions.push(ResourceDecision::ReduceGpuAlloc(new_alloc));
            REBALANCE_EVENTS.fetch_add(1, Ordering::Relaxed);
        }
        decisions.push(ResourceDecision::PauseIndexing);
        STATE.lock().unwrap().indexing_paused = true;
    } else {
        STATE.lock().unwrap().indexing_paused = false;
    }

    // High CPU pressure
    if cpu >= CPU_HIGH_WATERMARK {
        decisions.push(ResourceDecision::EngageEcoMode);
        crate::performance_profiles::set_mode(crate::performance_profiles::PerformanceMode::Eco);
        REBALANCE_EVENTS.fetch_add(1, Ordering::Relaxed);
    }

    decisions
}

pub fn is_indexing_paused() -> bool {
    STATE.lock().unwrap().indexing_paused
}

pub fn model_usage_entries() -> Vec<ModelUsageEntry> {
    let s = STATE.lock().unwrap();
    let now = ts_now();
    s.model_usage.iter().map(|(name, (last_used, count))| {
        let idle_ms = now.saturating_sub(*last_used);
        ModelUsageEntry {
            model_name:     name.clone(),
            last_used_ms:   *last_used,
            query_count:    *count,
            idle_ms,
            suggest_unload: idle_ms > MODEL_IDLE_TIMEOUT_MS,
        }
    }).collect()
}

#[derive(Debug, serde::Serialize)]
pub struct OptimizerSnapshot {
    pub unload_suggestions: u64,
    pub rebalance_events:   u64,
    pub optimizer_runs:     u64,
    pub vram_pressure:      f32,
    pub cpu_pressure:       f32,
    pub indexing_paused:    bool,
    pub tracked_models:     usize,
}

pub fn snapshot() -> OptimizerSnapshot {
    let s = STATE.lock().unwrap();
    OptimizerSnapshot {
        unload_suggestions: UNLOAD_SUGGESTIONS.load(Ordering::Relaxed),
        rebalance_events:   REBALANCE_EVENTS.load(Ordering::Relaxed),
        optimizer_runs:     OPTIMIZER_RUNS.load(Ordering::Relaxed),
        vram_pressure:      s.vram_pressure,
        cpu_pressure:       s.cpu_pressure,
        indexing_paused:    s.indexing_paused,
        tracked_models:     s.model_usage.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_model_query_no_panic() {
        record_model_query("llama3.2:3b");
        assert!(!model_usage_entries().is_empty());
    }

    #[test]
    fn optimization_run_no_panic() {
        let decisions = run_optimization();
        let _ = decisions.len();
        assert!(OPTIMIZER_RUNS.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn high_vram_triggers_pause() {
        update_vram_pressure(0.90);
        let decisions = run_optimization();
        let paused = decisions.iter().any(|d| matches!(d, ResourceDecision::PauseIndexing));
        assert!(paused);
        update_vram_pressure(0.0); // restore
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(s.optimizer_runs > 0);
    }
}
