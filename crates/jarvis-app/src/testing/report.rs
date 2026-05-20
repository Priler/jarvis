#![allow(dead_code)]

//! Report generation — JSON machine-readable + human-readable text summary.

use std::path::Path;

use super::assertions::{AssertionEngine, AssertionResult};
use super::failure_classifier::{self, FailureClass};
use super::session_log::{SessionJournal, LatencyStats};

// ── Classified failure ────────────────────────────────────────────────────────

/// A failed assertion tagged with its failure class.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ClassifiedFailure {
    pub assertion_id: String,
    pub class: FailureClass,
    pub message: String,
}

// ── Report ────────────────────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ReplayReport {
    pub wav_path: String,
    pub run_duration_ms: u64,
    pub total_events: usize,
    pub wake_sessions: u32,
    pub command_sessions: u32,
    pub dirty_session_closes: u32,
    pub illegal_transitions: u32,
    pub speech_reco_contaminations: u32,
    pub assertions_passed: usize,
    pub assertions_failed: usize,
    pub passed: bool,
    pub assertions: Vec<AssertionResult>,
    pub defects_detected: Vec<String>,

    // ── Phase 5 additions ─────────────────────────────────────────────────────

    /// Failed assertions tagged with their failure class for certification reporting.
    #[serde(default)]
    pub classified_failures: Vec<ClassifiedFailure>,

    /// Cached p95 wake-detection latency (ms) — extracted from RuntimeMetrics.
    /// `None` when no wake activations occurred.
    #[serde(default)]
    pub latency_p95_wake_ms: Option<u64>,

    /// Cached p95 STT latency (ms).
    #[serde(default)]
    pub latency_p95_stt_ms: Option<u64>,

    /// Cached p95 pipeline latency (ms).
    #[serde(default)]
    pub latency_p95_pipeline_ms: Option<u64>,
}

impl ReplayReport {
    pub fn build(
        wav_path: &str,
        run_duration_ms: u64,
        journal: &SessionJournal,
        engine: &AssertionEngine,
    ) -> Self {
        // Derive known defects from failed assertions.
        let mut defects = Vec::new();
        for r in &engine.results {
            if !r.passed {
                match r.id.as_str() {
                    "A001" | "A009" => defects.push("P0-2: SPEECH_RECOGNIZER fed in VoiceActive (pre-wake contamination)".into()),
                    "A002" => defects.push("LV-3: Illegal state transition executed".into()),
                    "A003" => defects.push("SS-1: Wake session leak / zombie session".into()),
                    "A004" => defects.push("SS-3: Command session leak".into()),
                    "A005" => defects.push("P1-3: Conversation depth not reset after timeout".into()),
                    "A006" => defects.push("P1-1: Debounce bypassed — likely REMAINDER not cleared".into()),
                    "A007" => defects.push("LV-4: Recognizer not reset after session end".into()),
                    "A008" => defects.push("P0-1: Speaking gate stuck (Rodio backend or watchdog fired)".into()),
                    "A010" => defects.push("IPC: Event ordering violation — GUI may desync".into()),
                    "A011" => defects.push("ARCH-1: Command session opened outside wake session boundary".into()),
                    "A012" => defects.push("P0-1: Speech recognizer contamination — fed outside CommandMode".into()),
                    "A013" => defects.push("P0-2: Wake recognizer not reset after session end".into()),
                    "A014" => defects.push("P0-1: Duplicate command — SPEECH_RECOGNIZER fed pre-wake audio (regression)".into()),
                    _ => {}
                }
            }
        }
        defects.dedup();

        // Build classified failure list from failed assertions.
        let classified_failures: Vec<ClassifiedFailure> = engine.results.iter()
            .filter(|r| !r.passed)
            .flat_map(|r| {
                let class = failure_classifier::classify_assertion(&r.id);
                r.failures.iter().map(move |msg| ClassifiedFailure {
                    assertion_id: r.id.clone(),
                    class: class.clone(),
                    message: msg.clone(),
                })
            })
            .collect();

        // Cache latency p95 values for scenario validation.
        let latency = journal.compute_latency();
        let latency_p95_wake_ms = latency.wake_detection_p95_ms;
        let latency_p95_stt_ms = latency.stt_p95_ms;
        let latency_p95_pipeline_ms = latency.pipeline_p95_ms;

        Self {
            wav_path: wav_path.to_string(),
            run_duration_ms,
            total_events: journal.events().len(),
            wake_sessions: journal.wake_opens,
            command_sessions: journal.command_opens,
            dirty_session_closes: journal.dirty_closes,
            illegal_transitions: journal.illegal_transitions,
            speech_reco_contaminations: journal.speech_reco_in_voice_active,
            assertions_passed: engine.passed_count(),
            assertions_failed: engine.failed_count(),
            passed: engine.all_passed(),
            assertions: engine.results.clone(),
            defects_detected: defects,
            classified_failures,
            latency_p95_wake_ms,
            latency_p95_stt_ms,
            latency_p95_pipeline_ms,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
    }

    pub fn write_to_file(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_json())
            .map_err(|e| format!("Failed to write report to {:?}: {}", path, e))
    }

    pub fn to_summary(&self) -> String {
        let status = if self.passed { "PASS" } else { "FAIL" };
        let bar = "─".repeat(60);
        let mut out = String::new();

        out.push_str(&format!("\n{}\n", bar));
        out.push_str(&format!(" JARVIS Runtime Validation — {}\n", status));
        out.push_str(&format!("{}\n", bar));
        out.push_str(&format!(" WAV       : {}\n", self.wav_path));
        out.push_str(&format!(" Duration  : {}ms\n", self.run_duration_ms));
        out.push_str(&format!(" Events    : {}\n", self.total_events));
        out.push_str(&format!(
            " Sessions  : wake={} cmd={} dirty_close={}\n",
            self.wake_sessions, self.command_sessions, self.dirty_session_closes
        ));
        out.push_str(&format!(
            " Assertions: {}/{} passed\n",
            self.assertions_passed,
            self.assertions_passed + self.assertions_failed
        ));
        out.push_str(&format!("{}\n", bar));

        for r in &self.assertions {
            let icon = if r.passed { "✓" } else { "✗" };
            out.push_str(&format!(" {} [{}] {}\n", icon, r.id, r.description));
            for f in &r.failures {
                out.push_str(&format!("    → {}\n", f));
            }
        }

        if !self.defects_detected.is_empty() {
            out.push_str(&format!("{}\n Defects:\n", bar));
            for d in &self.defects_detected {
                out.push_str(&format!("  • {}\n", d));
            }
        }

        if !self.classified_failures.is_empty() {
            out.push_str(&format!("{}\n Failure Classes:\n", bar));
            for f in &self.classified_failures {
                out.push_str(&format!("  [{}] {} — {}\n", f.class, f.assertion_id, f.message));
            }
        }

        out.push_str(&format!("{}\n", bar));
        out
    }
}

// ── Runtime metrics ───────────────────────────────────────────────────────────

/// Aggregated numeric metrics extracted from a replay run.
/// Written to `<scenario>.metrics.json` alongside the full report.
#[derive(serde::Serialize)]
pub struct RuntimeMetrics {
    pub wav_path: String,
    pub run_duration_ms: u64,
    pub total_events: usize,
    pub wake_sessions: u32,
    pub command_sessions: u32,
    pub dirty_session_closes: u32,
    pub illegal_transitions: u32,
    pub speech_reco_contaminations: u32,
    pub duplicate_commands: u32,
    /// Whether frames were delivered without inter-frame sleep (CI mode).
    pub accelerated: bool,
    /// command_sessions / wake_sessions, or 0.0 if no wakes.
    pub avg_commands_per_wake: f64,
    /// (wake_sessions - dirty_closes) / wake_sessions, or 1.0 if no wakes.
    pub clean_session_rate: f64,
    /// assertions_passed / total_assertions.
    pub assertion_pass_rate: f64,
    pub assertions_passed: usize,
    pub assertions_failed: usize,
    pub passed: bool,
    pub defects: Vec<String>,
    /// Pipeline latency statistics derived from event timestamps.
    pub latency: LatencyStats,
}

impl ReplayReport {
    pub fn to_metrics(&self, journal: &SessionJournal) -> RuntimeMetrics {
        let total_assertions = self.assertions_passed + self.assertions_failed;
        let pass_rate = if total_assertions > 0 {
            self.assertions_passed as f64 / total_assertions as f64
        } else {
            1.0
        };
        let avg_cmd = if self.wake_sessions > 0 {
            self.command_sessions as f64 / self.wake_sessions as f64
        } else {
            0.0
        };
        let clean_rate = if self.wake_sessions > 0 {
            let clean = self.wake_sessions.saturating_sub(self.dirty_session_closes);
            clean as f64 / self.wake_sessions as f64
        } else {
            1.0
        };
        RuntimeMetrics {
            wav_path: self.wav_path.clone(),
            run_duration_ms: self.run_duration_ms,
            total_events: self.total_events,
            wake_sessions: self.wake_sessions,
            command_sessions: self.command_sessions,
            dirty_session_closes: self.dirty_session_closes,
            illegal_transitions: self.illegal_transitions,
            speech_reco_contaminations: self.speech_reco_contaminations,
            duplicate_commands: journal.duplicate_commands,
            accelerated: jarvis_core::recorder::is_accelerated(),
            avg_commands_per_wake: avg_cmd,
            clean_session_rate: clean_rate,
            assertion_pass_rate: pass_rate,
            assertions_passed: self.assertions_passed,
            assertions_failed: self.assertions_failed,
            passed: self.passed,
            defects: self.defects_detected.clone(),
            latency: journal.compute_latency(),
        }
    }
}

// ── Batch summary ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct BatchReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub runs: Vec<ReplayReport>,
}

impl BatchReport {
    pub fn new(runs: Vec<ReplayReport>) -> Self {
        let passed = runs.iter().filter(|r| r.passed).count();
        let failed = runs.len() - passed;
        Self { total: runs.len(), passed, failed, runs }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
    }

    pub fn to_summary(&self) -> String {
        let bar = "═".repeat(60);
        let status = if self.failed == 0 { "ALL PASS" } else { "FAILURES" };
        let mut out = format!("\n{}\n BATCH RESULT — {}  ({}/{} passed)\n{}\n",
            bar, status, self.passed, self.total, bar);
        for r in &self.runs {
            let icon = if r.passed { "✓" } else { "✗" };
            out.push_str(&format!(" {} {}\n", icon, r.wav_path));
            if !r.passed {
                for a in r.assertions.iter().filter(|a| !a.passed) {
                    out.push_str(&format!("    [{}] {}\n", a.id, a.description));
                    for f in &a.failures {
                        out.push_str(&format!("      → {}\n", f));
                    }
                }
            }
        }
        out.push_str(&format!("{}\n", bar));
        out
    }

    pub fn write_to_file(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_json())
            .map_err(|e| format!("Failed to write batch report: {}", e))
    }
}
