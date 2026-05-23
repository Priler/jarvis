//! Local analytics — collects performance and reliability metrics entirely on-device.
//! Zero data leaves the machine.  Replaces telemetry with local self-diagnostics.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

// ── Global counters ───────────────────────────────────────────────────────────

pub static CRASHES_RECORDED:      AtomicU64 = AtomicU64::new(0);
pub static TOOL_FAILURES_RECORDED: AtomicU64 = AtomicU64::new(0);
pub static LATENCY_SAMPLES:       AtomicU64 = AtomicU64::new(0);

// ── Latency tracking ─────────────────────────────────────────────────────────

const MAX_LATENCY_SAMPLES: usize = 256;

struct LatencyBucket {
    label:   String,
    samples: Vec<u64>,
}

impl LatencyBucket {
    fn new(label: &str) -> Self { Self { label: label.to_string(), samples: Vec::new() } }

    fn add(&mut self, ms: u64) {
        if self.samples.len() >= MAX_LATENCY_SAMPLES { self.samples.remove(0); }
        self.samples.push(ms);
    }

    fn avg_ms(&self) -> u64 {
        if self.samples.is_empty() { return 0; }
        self.samples.iter().sum::<u64>() / self.samples.len() as u64
    }

    fn p95_ms(&self) -> u64 {
        if self.samples.is_empty() { return 0; }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        let idx = (sorted.len() as f64 * 0.95) as usize;
        sorted[idx.min(sorted.len() - 1)]
    }
}

// ── Tool failure log ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolFailureEntry {
    pub tool:       String,
    pub error:      String,
    pub timestamp:  u64,
}

const MAX_TOOL_FAILURES: usize = 100;

// ── Voice quality samples ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct VoiceQualitySample {
    pub confidence:   f32,   // 0.0–1.0 STT confidence
    pub latency_ms:   u64,
    pub wakeword_hit: bool,
    pub timestamp:    u64,
}

const MAX_VOICE_SAMPLES: usize = 100;

// ── Memory pressure log ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryPressureEntry {
    pub level_pct: u8,
    pub timestamp: u64,
}

const MAX_PRESSURE_ENTRIES: usize = 100;

// ── State ─────────────────────────────────────────────────────────────────────

struct AnalyticsState {
    latency_buckets:  Vec<LatencyBucket>,
    tool_failures:    Vec<ToolFailureEntry>,
    voice_samples:    Vec<VoiceQualitySample>,
    memory_pressure:  Vec<MemoryPressureEntry>,
    crash_modules:    Vec<String>,
}

impl AnalyticsState {
    fn new() -> Self {
        Self {
            latency_buckets: vec![
                LatencyBucket::new("wake_response"),
                LatencyBucket::new("voice_interrupt"),
                LatencyBucket::new("tool_response"),
                LatencyBucket::new("memory_retrieval"),
                LatencyBucket::new("model_inference"),
            ],
            tool_failures:   Vec::new(),
            voice_samples:   Vec::new(),
            memory_pressure: Vec::new(),
            crash_modules:   Vec::new(),
        }
    }

    fn bucket_mut(&mut self, label: &str) -> Option<&mut LatencyBucket> {
        self.latency_buckets.iter_mut().find(|b| b.label == label)
    }
}

static STATE: Lazy<Mutex<AnalyticsState>> = Lazy::new(|| Mutex::new(AnalyticsState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn record_latency(bucket: &str, ms: u64) {
    let mut s = STATE.lock().unwrap();
    if let Some(b) = s.bucket_mut(bucket) {
        b.add(ms);
        LATENCY_SAMPLES.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn record_crash(module: &str) {
    let mut s = STATE.lock().unwrap();
    s.crash_modules.push(module.to_string());
    CRASHES_RECORDED.fetch_add(1, Ordering::Relaxed);
}

pub fn record_tool_failure(tool: &str, error: &str) {
    let mut s = STATE.lock().unwrap();
    if s.tool_failures.len() >= MAX_TOOL_FAILURES { s.tool_failures.remove(0); }
    s.tool_failures.push(ToolFailureEntry {
        tool:      tool.to_string(),
        error:     error.to_string(),
        timestamp: ts_now(),
    });
    TOOL_FAILURES_RECORDED.fetch_add(1, Ordering::Relaxed);
}

pub fn record_voice_quality(confidence: f32, latency_ms: u64, wakeword_hit: bool) {
    let mut s = STATE.lock().unwrap();
    if s.voice_samples.len() >= MAX_VOICE_SAMPLES { s.voice_samples.remove(0); }
    s.voice_samples.push(VoiceQualitySample {
        confidence, latency_ms, wakeword_hit, timestamp: ts_now(),
    });
}

pub fn record_memory_pressure(level_pct: u8) {
    let mut s = STATE.lock().unwrap();
    if s.memory_pressure.len() >= MAX_PRESSURE_ENTRIES { s.memory_pressure.remove(0); }
    s.memory_pressure.push(MemoryPressureEntry { level_pct, timestamp: ts_now() });
}

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct LatencyStats {
    pub label:  String,
    pub avg_ms: u64,
    pub p95_ms: u64,
    pub count:  usize,
}

#[derive(Debug, serde::Serialize)]
pub struct AnalyticsSnapshot {
    pub crashes_total:       u64,
    pub tool_failures_total: u64,
    pub latency_samples:     u64,
    pub latency_stats:       Vec<LatencyStats>,
    pub avg_voice_confidence: f32,
    pub avg_memory_pressure:  f32,
    pub recent_crashes:      Vec<String>,
    pub recent_tool_failures: Vec<ToolFailureEntry>,
}

pub fn snapshot() -> AnalyticsSnapshot {
    let s = STATE.lock().unwrap();

    let latency_stats = s.latency_buckets.iter().map(|b| LatencyStats {
        label:  b.label.clone(),
        avg_ms: b.avg_ms(),
        p95_ms: b.p95_ms(),
        count:  b.samples.len(),
    }).collect();

    let avg_voice_confidence = if s.voice_samples.is_empty() {
        0.0
    } else {
        s.voice_samples.iter().map(|v| v.confidence).sum::<f32>() / s.voice_samples.len() as f32
    };

    let avg_memory_pressure = if s.memory_pressure.is_empty() {
        0.0
    } else {
        s.memory_pressure.iter().map(|m| m.level_pct as f32).sum::<f32>()
            / s.memory_pressure.len() as f32
    };

    let recent_crashes = s.crash_modules.iter().rev().take(10).cloned().collect();
    let recent_tool_failures = s.tool_failures.iter().rev().take(10).cloned().collect();

    AnalyticsSnapshot {
        crashes_total:        CRASHES_RECORDED.load(Ordering::Relaxed),
        tool_failures_total:  TOOL_FAILURES_RECORDED.load(Ordering::Relaxed),
        latency_samples:      LATENCY_SAMPLES.load(Ordering::Relaxed),
        latency_stats,
        avg_voice_confidence,
        avg_memory_pressure,
        recent_crashes,
        recent_tool_failures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_latency_no_panic() {
        record_latency("wake_response", 120);
        record_latency("voice_interrupt", 80);
        assert!(LATENCY_SAMPLES.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn record_crash_increments() {
        let before = CRASHES_RECORDED.load(Ordering::Relaxed);
        record_crash("test_module");
        assert!(CRASHES_RECORDED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn record_tool_failure_increments() {
        let before = TOOL_FAILURES_RECORDED.load(Ordering::Relaxed);
        record_tool_failure("file_write", "permission denied");
        assert!(TOOL_FAILURES_RECORDED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn snapshot_no_panic() {
        record_latency("model_inference", 350);
        record_voice_quality(0.92, 180, true);
        record_memory_pressure(45);
        let s = snapshot();
        assert!(!s.latency_stats.is_empty());
    }

    #[test]
    fn latency_bucket_stats() {
        let mut b = LatencyBucket::new("test");
        b.add(100);
        b.add(200);
        b.add(300);
        assert_eq!(b.avg_ms(), 200);
        assert!(b.p95_ms() >= 200);
    }
}
