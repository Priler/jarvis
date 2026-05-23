//! Advanced diagnostics — identifies runtime bottlenecks, pressure points,
//! and degradation patterns beyond the basic health score.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static DIAG_RUNS:        AtomicU64 = AtomicU64::new(0);
pub static BOTTLENECKS_FOUND: AtomicU64 = AtomicU64::new(0);
pub static ALERTS_EMITTED:   AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize)]
pub enum BottleneckKind {
    SchedulerLatency,
    ModelInferenceLatency,
    MemoryRetrieval,
    VoicePipeline,
    EmbeddingGeneration,
    GpuContention,
    DiskIO,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Bottleneck {
    pub kind:       BottleneckKind,
    pub component:  String,
    pub severity:   f32,     // 0.0–1.0
    pub detail:     String,
    pub detected_at: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagAlert {
    pub title:      String,
    pub detail:     String,
    pub severity:   String,
    pub timestamp:  u64,
}

const MAX_BOTTLENECKS: usize = 30;
const MAX_ALERTS:      usize = 50;

struct AdvDiagState {
    bottlenecks: Vec<Bottleneck>,
    alerts:      Vec<DiagAlert>,
}

impl AdvDiagState {
    fn new() -> Self { Self { bottlenecks: Vec::new(), alerts: Vec::new() } }
}

static STATE: Lazy<Mutex<AdvDiagState>> = Lazy::new(|| Mutex::new(AdvDiagState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn push_bottleneck(kind: BottleneckKind, component: &str, severity: f32, detail: &str) {
    let mut s = STATE.lock().unwrap();
    if s.bottlenecks.len() >= MAX_BOTTLENECKS { s.bottlenecks.remove(0); }
    s.bottlenecks.push(Bottleneck {
        kind, component: component.to_string(),
        severity, detail: detail.to_string(), detected_at: ts_now(),
    });
    BOTTLENECKS_FOUND.fetch_add(1, Ordering::Relaxed);
}

fn push_alert(title: &str, detail: &str, severity: &str) {
    let mut s = STATE.lock().unwrap();
    if s.alerts.len() >= MAX_ALERTS { s.alerts.remove(0); }
    s.alerts.push(DiagAlert {
        title: title.to_string(), detail: detail.to_string(),
        severity: severity.to_string(), timestamp: ts_now(),
    });
    ALERTS_EMITTED.fetch_add(1, Ordering::Relaxed);
}

// ── Analysis entry points ─────────────────────────────────────────────────────

pub fn analyze_model_latency(model: &str, latency_ms: u64, threshold_ms: u64) {
    if latency_ms > threshold_ms * 2 {
        let sev = (latency_ms as f32 / threshold_ms as f32 - 1.0).min(1.0);
        push_bottleneck(
            BottleneckKind::ModelInferenceLatency, model, sev,
            &format!("latency {}ms vs target {}ms", latency_ms, threshold_ms),
        );
        if sev > 0.8 {
            push_alert("Model Latency Spike", &format!("{} took {}ms", model, latency_ms), "High");
            crate::production_logging::warn("advanced_diagnostics",
                &format!("model latency spike: {}ms on {}", latency_ms, model));
        }
    }
}

pub fn analyze_memory_pressure(pressure_pct: u8) {
    if pressure_pct >= 75 {
        let sev = pressure_pct as f32 / 100.0;
        push_bottleneck(
            BottleneckKind::MemoryRetrieval, "memory_runtime", sev,
            &format!("memory pressure {}%", pressure_pct),
        );
        push_alert("Memory Pressure", &format!("{}% memory tier fill", pressure_pct),
            if pressure_pct >= 90 { "Critical" } else { "High" });
    }
}

pub fn analyze_voice_confidence(confidence: f32) {
    if confidence < 0.50 {
        let sev = 1.0 - confidence;
        push_bottleneck(
            BottleneckKind::VoicePipeline, "stt", sev,
            &format!("STT confidence {:.0}%", confidence * 100.0),
        );
    }
}

pub fn analyze_gpu_contention(vram_pct: f32) {
    if vram_pct >= 0.85 {
        push_bottleneck(
            BottleneckKind::GpuContention, "gpu_scheduler", vram_pct,
            &format!("VRAM {:.0}% utilized", vram_pct * 100.0),
        );
    }
}

pub fn analyze_scheduler_latency(tick_miss_ratio: f32) {
    if tick_miss_ratio >= 0.30 {
        push_bottleneck(
            BottleneckKind::SchedulerLatency, "cognition_loop", tick_miss_ratio,
            &format!("{:.0}% ticks missed", tick_miss_ratio * 100.0),
        );
    }
}

pub fn run_full_analysis() {
    DIAG_RUNS.fetch_add(1, Ordering::Relaxed);

    // Pull live values from other runtime modules
    let mem_pressure = crate::memory_pressure_guard::current_pressure_pct();
    analyze_memory_pressure(mem_pressure);

    let gpu = crate::resource_optimizer::snapshot().vram_pressure;
    analyze_gpu_contention(gpu);

    let stability = crate::long_run_stability::snapshot();
    for ev in &stability.recent_events {
        push_alert(
            &format!("{:?}", ev.kind),
            &ev.message,
            if ev.auto_throttled { "Auto-resolved" } else { "Warning" },
        );
    }
}

pub fn recent_bottlenecks(n: usize) -> Vec<Bottleneck> {
    let s = STATE.lock().unwrap();
    s.bottlenecks.iter().rev().take(n).cloned().collect()
}

pub fn recent_alerts(n: usize) -> Vec<DiagAlert> {
    let s = STATE.lock().unwrap();
    s.alerts.iter().rev().take(n).cloned().collect()
}

#[derive(Debug, serde::Serialize)]
pub struct AdvDiagSnapshot {
    pub diag_runs:         u64,
    pub bottlenecks_found: u64,
    pub alerts_emitted:    u64,
    pub active_bottlenecks: usize,
    pub recent_bottlenecks: Vec<Bottleneck>,
    pub recent_alerts:      Vec<DiagAlert>,
}

pub fn snapshot() -> AdvDiagSnapshot {
    let s = STATE.lock().unwrap();
    AdvDiagSnapshot {
        diag_runs:          DIAG_RUNS.load(Ordering::Relaxed),
        bottlenecks_found:  BOTTLENECKS_FOUND.load(Ordering::Relaxed),
        alerts_emitted:     ALERTS_EMITTED.load(Ordering::Relaxed),
        active_bottlenecks: s.bottlenecks.len(),
        recent_bottlenecks: s.bottlenecks.iter().rev().take(5).cloned().collect(),
        recent_alerts:      s.alerts.iter().rev().take(5).cloned().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_latency_bottleneck() {
        analyze_model_latency("llama3.2:3b", 6000, 1000);
        assert!(BOTTLENECKS_FOUND.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn memory_pressure_bottleneck() {
        analyze_memory_pressure(80);
        let bns = recent_bottlenecks(3);
        assert!(bns.iter().any(|b| matches!(b.kind, BottleneckKind::MemoryRetrieval)));
    }

    #[test]
    fn voice_low_confidence_bottleneck() {
        analyze_voice_confidence(0.30);
        let bns = recent_bottlenecks(3);
        assert!(bns.iter().any(|b| matches!(b.kind, BottleneckKind::VoicePipeline)));
    }

    #[test]
    fn run_full_no_panic() {
        run_full_analysis();
        assert!(DIAG_RUNS.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        let _ = s.diag_runs;
    }
}
