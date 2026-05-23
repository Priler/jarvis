//! Performance profiler — tracks latency at all critical system boundaries.
//! Provides P50/P95/P99 percentiles for startup, inference, voice, memory retrieval.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static PROFILE_SAMPLES_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static THRESHOLD_VIOLATIONS:  AtomicU64 = AtomicU64::new(0);

const MAX_SAMPLES: usize = 512;

// ── Latency buckets ───────────────────────────────────────────────────────────

pub const BUCKET_STARTUP:          &str = "startup";
pub const BUCKET_INFERENCE:        &str = "inference";
pub const BUCKET_VOICE_WAKE:       &str = "voice_wake";
pub const BUCKET_VOICE_STT:        &str = "voice_stt";
pub const BUCKET_VOICE_TTS:        &str = "voice_tts";
pub const BUCKET_MEMORY_RETRIEVAL: &str = "memory_retrieval";
pub const BUCKET_RAG_RETRIEVE:     &str = "rag_retrieve";
pub const BUCKET_EMBEDDING:        &str = "embedding";
pub const BUCKET_SCHEDULER_TICK:   &str = "scheduler_tick";
pub const BUCKET_DESKTOP_RENDER:   &str = "desktop_render";

// Latency thresholds (ms) — violations get logged
const THRESHOLDS: &[(&str, u64)] = &[
    (BUCKET_STARTUP,          5_000),
    (BUCKET_INFERENCE,        3_000),
    (BUCKET_VOICE_WAKE,         150),
    (BUCKET_VOICE_STT,          800),
    (BUCKET_VOICE_TTS,          500),
    (BUCKET_MEMORY_RETRIEVAL,   200),
    (BUCKET_RAG_RETRIEVE,       500),
    (BUCKET_EMBEDDING,          100),
    (BUCKET_SCHEDULER_TICK,   1_000),
    (BUCKET_DESKTOP_RENDER,     100),
];

struct LatencySamples {
    label:    String,
    samples:  Vec<u64>,
    threshold_ms: u64,
}

impl LatencySamples {
    fn new(label: &str, threshold_ms: u64) -> Self {
        Self { label: label.to_string(), samples: Vec::new(), threshold_ms }
    }

    fn add(&mut self, ms: u64) {
        if self.samples.len() >= MAX_SAMPLES { self.samples.remove(0); }
        self.samples.push(ms);
    }

    fn percentile(&self, p: f32) -> u64 {
        if self.samples.is_empty() { return 0; }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        // nearest-rank: (n*p - 1).max(0) gives 0-based index
        let idx = ((sorted.len() as f32 * p - 1.0).max(0.0) as usize).min(sorted.len() - 1);
        sorted[idx]
    }

    fn avg(&self) -> u64 {
        if self.samples.is_empty() { return 0; }
        self.samples.iter().sum::<u64>() / self.samples.len() as u64
    }

    fn min(&self) -> u64 { self.samples.iter().copied().min().unwrap_or(0) }
    fn max(&self) -> u64 { self.samples.iter().copied().max().unwrap_or(0) }
    fn count(&self) -> usize { self.samples.len() }
}

struct ProfilerState {
    buckets: Vec<LatencySamples>,
}

impl ProfilerState {
    fn new() -> Self {
        let mut buckets = Vec::new();
        for &(label, threshold) in THRESHOLDS {
            buckets.push(LatencySamples::new(label, threshold));
        }
        Self { buckets }
    }

    fn bucket_mut(&mut self, label: &str) -> Option<&mut LatencySamples> {
        self.buckets.iter_mut().find(|b| b.label == label)
    }

    fn bucket(&self, label: &str) -> Option<&LatencySamples> {
        self.buckets.iter().find(|b| b.label == label)
    }
}

static STATE: Lazy<Mutex<ProfilerState>> = Lazy::new(|| Mutex::new(ProfilerState::new()));

pub fn record(bucket: &str, latency_ms: u64) {
    let mut s = STATE.lock().unwrap();
    if let Some(b) = s.bucket_mut(bucket) {
        let threshold = b.threshold_ms;
        b.add(latency_ms);
        PROFILE_SAMPLES_TOTAL.fetch_add(1, Ordering::Relaxed);
        if latency_ms > threshold {
            THRESHOLD_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
            drop(s);
            crate::production_logging::warn("performance_profiler",
                &format!("{} latency {}ms > threshold {}ms", bucket, latency_ms, threshold));
        }
    }
}

/// Time a closure and record the result in the given bucket.
pub fn time<F: FnOnce() -> R, R>(bucket: &str, f: F) -> R {
    let start = std::time::Instant::now();
    let result = f();
    record(bucket, start.elapsed().as_millis() as u64);
    result
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BucketStats {
    pub label:        String,
    pub count:        usize,
    pub avg_ms:       u64,
    pub min_ms:       u64,
    pub max_ms:       u64,
    pub p50_ms:       u64,
    pub p95_ms:       u64,
    pub p99_ms:       u64,
    pub threshold_ms: u64,
    pub over_threshold: bool,
}

pub fn stats(bucket: &str) -> Option<BucketStats> {
    let s = STATE.lock().unwrap();
    s.bucket(bucket).map(|b| {
        let avg = b.avg();
        BucketStats {
            label:         b.label.clone(),
            count:         b.count(),
            avg_ms:        avg,
            min_ms:        b.min(),
            max_ms:        b.max(),
            p50_ms:        b.percentile(0.50),
            p95_ms:        b.percentile(0.95),
            p99_ms:        b.percentile(0.99),
            threshold_ms:  b.threshold_ms,
            over_threshold: avg > b.threshold_ms,
        }
    })
}

pub fn all_stats() -> Vec<BucketStats> {
    let s = STATE.lock().unwrap();
    s.buckets.iter().map(|b| {
        let avg = b.avg();
        BucketStats {
            label:         b.label.clone(),
            count:         b.count(),
            avg_ms:        avg,
            min_ms:        b.min(),
            max_ms:        b.max(),
            p50_ms:        b.percentile(0.50),
            p95_ms:        b.percentile(0.95),
            p99_ms:        b.percentile(0.99),
            threshold_ms:  b.threshold_ms,
            over_threshold: avg > b.threshold_ms,
        }
    }).collect()
}

#[derive(Debug, serde::Serialize)]
pub struct ProfilerSnapshot {
    pub samples_total:      u64,
    pub threshold_violations: u64,
    pub buckets:            Vec<BucketStats>,
}

pub fn snapshot() -> ProfilerSnapshot {
    ProfilerSnapshot {
        samples_total:       PROFILE_SAMPLES_TOTAL.load(Ordering::Relaxed),
        threshold_violations: THRESHOLD_VIOLATIONS.load(Ordering::Relaxed),
        buckets:             all_stats(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve_stats() {
        record(BUCKET_INFERENCE, 500);
        record(BUCKET_INFERENCE, 600);
        record(BUCKET_INFERENCE, 400);
        let s = stats(BUCKET_INFERENCE).unwrap();
        assert_eq!(s.count, 3);
        assert!(s.avg_ms > 0);
    }

    #[test]
    fn threshold_violation_recorded() {
        let before = THRESHOLD_VIOLATIONS.load(Ordering::Relaxed);
        record(BUCKET_VOICE_WAKE, 500); // threshold is 150ms
        assert!(THRESHOLD_VIOLATIONS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn time_closure() {
        let result = time(BUCKET_EMBEDDING, || 42u64 + 1);
        assert_eq!(result, 43);
    }

    #[test]
    fn percentile_correct() {
        let mut b = LatencySamples::new("test", 1000);
        for i in 1..=100u64 { b.add(i); }
        assert_eq!(b.percentile(0.50), 50);
        assert_eq!(b.percentile(0.95), 95);
    }

    #[test]
    fn all_stats_populated() {
        let stats = all_stats();
        assert_eq!(stats.len(), THRESHOLDS.len());
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(!s.buckets.is_empty());
    }
}
