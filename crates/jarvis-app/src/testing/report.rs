#![allow(dead_code)]

//! Report generation — JSON machine-readable + human-readable text summary.

use std::path::Path;

use super::assertions::{AssertionEngine, AssertionResult};
use super::session_log::SessionJournal;

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
                    _ => {}
                }
            }
        }
        defects.dedup();

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

        out.push_str(&format!("{}\n", bar));
        out
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
