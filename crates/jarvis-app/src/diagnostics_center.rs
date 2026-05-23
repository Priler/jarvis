//! Diagnostics center — aggregates runtime health signals into a single snapshot.
//! Monitors: crashes, model failures, voice failures, scheduler overload,
//! memory issues, and permission violations.

use std::sync::atomic::{AtomicU64, Ordering};

pub static DIAGNOSTICS_RUNS: AtomicU64 = AtomicU64::new(0);

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticsSnapshot {
    pub ts_ms:                u64,

    // Runtime
    pub runtime_crashes:      u64,
    pub modules_disabled:     u64,
    pub recovery_attempts:    u64,

    // Models
    pub model_failures:       u64,
    pub active_provider:      String,
    pub ollama_available:     bool,

    // Voice
    pub voice_errors:         u64,
    pub voice_active:         bool,

    // Memory & RAG
    pub memory_entries:       usize,
    pub knowledge_chunks:     usize,
    pub rag_queries:          u64,

    // Permissions & Security
    pub permission_denials:   u64,
    pub policy_violations:    u64,
    pub sandbox_blocks:       u64,

    // Safe mode
    pub safe_mode_active:     bool,
    pub safe_mode_entries:    u64,

    // Cognitive runtime
    pub continuity_score:     f32,
    pub scheduler_overload:   bool,

    // Overall health
    pub health_score:         f32,
    pub warnings:             Vec<String>,
}

impl DiagnosticsSnapshot {
    pub fn is_healthy(&self) -> bool { self.health_score >= 0.6 }
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn snapshot() -> DiagnosticsSnapshot {
    DIAGNOSTICS_RUNS.fetch_add(1, Ordering::Relaxed);
    let mut warnings = Vec::new();

    let runtime_crashes   = crate::runtime_hardening::total_crashes();
    let modules_disabled  = crate::runtime_hardening::modules_disabled();
    let recovery_attempts = crate::runtime_hardening::recovery_attempts();
    let model_failures    = crate::llm_provider_runtime::requests_failed();
    let active_provider   = format!("{:?}", crate::llm_provider_runtime::get_active());
    let ollama_available  = crate::model_manager::is_ollama_available();
    let memory_entries    = crate::memory_runtime::total_entries();
    let knowledge_chunks  = crate::knowledge_index::chunk_count();
    let rag_queries       = crate::rag_pipeline::queries_run();
    let permission_denials = crate::permission_runtime::PERMISSIONS_DENIED.load(Ordering::Relaxed);
    let policy_violations  = crate::security_policies::policy_violations();
    let sandbox_blocks     = crate::sandbox_runtime::blocked_total();
    let safe_mode_active   = crate::safe_mode::is_active();
    let safe_mode_entries  = crate::safe_mode::entries();
    let continuity_score   = crate::long_run_cognition::maintain().overall_continuity;
    let scheduler_overload = crate::resource_governor::sample().should_throttle;

    // Log-based error counts
    let voice_errors = crate::production_logging::error_count();
    let voice_active = crate::preferences_runtime::is_voice_enabled();

    // Warnings
    if modules_disabled > 0 {
        warnings.push(format!("{} module(s) auto-disabled due to crashes", modules_disabled));
    }
    if !ollama_available {
        warnings.push("Ollama not reachable at localhost:11434".to_string());
    }
    if safe_mode_active {
        warnings.push("System is running in safe mode".to_string());
    }
    if policy_violations > 0 {
        warnings.push(format!("{} security policy violation(s) detected", policy_violations));
    }
    if scheduler_overload {
        warnings.push("Scheduler load is above throttle threshold".to_string());
    }
    if continuity_score < 0.4 {
        warnings.push(format!("Cognitive continuity low: {:.2}", continuity_score));
    }

    // Health score: 1.0 = perfect, 0.0 = critical failure
    let mut health = 1.0f32;
    health -= (modules_disabled as f32 * 0.10).min(0.40);
    health -= (runtime_crashes as f32 * 0.02).min(0.20);
    health -= if !ollama_available { 0.10 } else { 0.0 };
    health -= if safe_mode_active { 0.15 } else { 0.0 };
    health -= (policy_violations as f32 * 0.05).min(0.20);
    health -= if scheduler_overload { 0.10 } else { 0.0 };
    let health_score = health.clamp(0.0, 1.0);

    DiagnosticsSnapshot {
        ts_ms: ts_now(),
        runtime_crashes,
        modules_disabled,
        recovery_attempts,
        model_failures,
        active_provider,
        ollama_available,
        voice_errors,
        voice_active,
        memory_entries,
        knowledge_chunks,
        rag_queries,
        permission_denials,
        policy_violations,
        sandbox_blocks,
        safe_mode_active,
        safe_mode_entries,
        continuity_score,
        scheduler_overload,
        health_score,
        warnings,
    }
}

pub fn print_summary(snap: &DiagnosticsSnapshot) -> String {
    format!(
        "[DIAGNOSTICS] health={:.2} | crashes={} | modules_disabled={} | ollama={} | safe_mode={} | continuity={:.2} | warnings={}",
        snap.health_score,
        snap.runtime_crashes,
        snap.modules_disabled,
        snap.ollama_available,
        snap.safe_mode_active,
        snap.continuity_score,
        snap.warnings.len()
    )
}

pub fn diagnostics_runs() -> u64 { DIAGNOSTICS_RUNS.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(s.health_score >= 0.0 && s.health_score <= 1.0);
    }

    #[test]
    fn snapshot_ts_non_zero() {
        let s = snapshot();
        assert!(s.ts_ms > 0);
    }

    #[test]
    fn print_summary_non_empty() {
        let s = snapshot();
        let summary = print_summary(&s);
        assert!(summary.contains("[DIAGNOSTICS]"));
    }

    #[test]
    fn diagnostics_runs_increments() {
        let before = DIAGNOSTICS_RUNS.load(Ordering::Relaxed);
        snapshot();
        assert!(DIAGNOSTICS_RUNS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn health_score_bounded() {
        for _ in 0..3 {
            let s = snapshot();
            assert!(s.health_score >= 0.0 && s.health_score <= 1.0);
        }
    }
}
