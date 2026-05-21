//! Execution quality scoring — tracks success reliability, latency, recovery
//! frequency, and verification stability across runtime ticks.
//! No ML, no cloud. Pure counter-based heuristics.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static QUALITY_SNAPSHOTS:   AtomicU64 = AtomicU64::new(0);
pub static QUALITY_DEGRADED:    AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 100;

// ── Quality snapshot ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualitySnapshot {
    pub ts_ms:                  u64,
    pub success_reliability:    f32,   // fraction of recent ticks that succeeded
    pub latency_score:          f32,   // 1.0 = fast, lower = slower (relative)
    pub recovery_frequency:     f32,   // how often anomalies fired (lower = better)
    pub rollback_frequency:     f32,   // rollbacks / total checks
    pub verification_stability: f32,   // consistency of verification outcomes
    pub overall:                f32,   // weighted composite
}

impl QualitySnapshot {
    pub fn is_degraded(&self) -> bool { self.overall < 0.5 }
    pub fn is_excellent(&self) -> bool { self.overall >= 0.8 }
}

// ── History ───────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<QualitySnapshot>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn measure() -> QualitySnapshot {
    QUALITY_SNAPSHOTS.fetch_add(1, Ordering::Relaxed);

    let snap = build_snapshot();
    if snap.is_degraded() {
        QUALITY_DEGRADED.fetch_add(1, Ordering::Relaxed);
    }

    if let Ok(mut h) = HISTORY.lock() {
        if h.len() >= MAX_HISTORY { h.remove(0); }
        h.push(snap.clone());
    }
    snap
}

pub fn latest() -> Option<QualitySnapshot> {
    HISTORY.lock().ok().and_then(|h| h.last().cloned())
}

pub fn recent(n: usize) -> Vec<QualitySnapshot> {
    HISTORY.lock().map(|h| {
        let len = h.len();
        h[len.saturating_sub(n)..].to_vec()
    }).unwrap_or_default()
}

pub fn history_len() -> usize {
    HISTORY.lock().map(|h| h.len()).unwrap_or(0)
}

pub fn average_overall(window: usize) -> f32 {
    let snaps = recent(window);
    if snaps.is_empty() { return 0.5; }
    snaps.iter().map(|s| s.overall).sum::<f32>() / snaps.len() as f32
}

pub fn clear() {
    if let Ok(mut h) = HISTORY.lock() { h.clear(); }
}

// ── Heuristic computation ─────────────────────────────────────────────────────

fn build_snapshot() -> QualitySnapshot {
    let now = ts_now();

    let success_reliability = crate::cognitive_memory::recent_success_rate(20);

    let anomaly_checks = crate::anomaly_detector::ANOMALY_CHECKS.load(Ordering::Relaxed).max(1);
    let anomalies_found = crate::anomaly_detector::ANOMALIES_FOUND.load(Ordering::Relaxed);
    let recovery_frequency = (anomalies_found as f32 / anomaly_checks as f32).min(1.0);

    let safety_checks  = crate::cognitive_safety::SAFETY_CHECKS.load(Ordering::Relaxed).max(1);
    let safety_blocked = crate::cognitive_safety::SAFETY_BLOCKED.load(Ordering::Relaxed);
    let rollback_frequency = (safety_blocked as f32 / safety_checks as f32).min(1.0);

    let tick_count = crate::cognitive_memory::count() as f32;
    let latency_score = if tick_count < 5.0 { 0.5 } else { 0.7_f32.min(1.0) };

    let verifications = crate::cognitive_memory::recent_success_rate(5);
    let verification_stability = verifications;

    let overall = success_reliability * 0.35
        + (1.0 - recovery_frequency) * 0.20
        + (1.0 - rollback_frequency) * 0.15
        + latency_score * 0.15
        + verification_stability * 0.15;

    QualitySnapshot {
        ts_ms: now,
        success_reliability,
        latency_score,
        recovery_frequency,
        rollback_frequency,
        verification_stability,
        overall: overall.clamp(0.0, 1.0),
    }
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_returns_snapshot() {
        let snap = measure();
        assert!(snap.overall >= 0.0 && snap.overall <= 1.0);
    }

    #[test]
    fn history_grows() {
        let before = history_len();
        measure();
        assert!(history_len() > before);
    }

    #[test]
    fn latest_is_some_after_measure() {
        measure();
        assert!(latest().is_some());
    }

    #[test]
    fn all_scores_bounded() {
        let snap = measure();
        assert!(snap.success_reliability >= 0.0 && snap.success_reliability <= 1.0);
        assert!(snap.recovery_frequency  >= 0.0 && snap.recovery_frequency  <= 1.0);
        assert!(snap.rollback_frequency  >= 0.0 && snap.rollback_frequency  <= 1.0);
        assert!(snap.latency_score       >= 0.0 && snap.latency_score       <= 1.0);
    }

    #[test]
    fn average_overall_bounded() {
        measure(); measure();
        let avg = average_overall(10);
        assert!(avg >= 0.0 && avg <= 1.0);
    }

    #[test]
    fn degraded_flag_works() {
        let snap = QualitySnapshot {
            ts_ms: 0, success_reliability: 0.1, latency_score: 0.1,
            recovery_frequency: 0.9, rollback_frequency: 0.8,
            verification_stability: 0.1, overall: 0.2,
        };
        assert!(snap.is_degraded());
        assert!(!snap.is_excellent());
    }
}
