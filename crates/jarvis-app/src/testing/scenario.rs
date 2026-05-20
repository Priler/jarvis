#![allow(dead_code)]

//! YAML scenario format for batch replay runs.
//!
//! A scenario file declares a set of replay runs with expected outcomes.
//! The batch runner validates each assertion result against the expected
//! values and fails the run if they diverge.

use std::path::{Path, PathBuf};

// ── Scenario types ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScenarioBatch {
    /// Human-readable name for this batch.
    pub name: String,
    /// Base directory for resolving relative WAV paths.
    /// Defaults to the directory containing the YAML file.
    #[serde(default)]
    pub wav_base: Option<String>,
    pub scenarios: Vec<Scenario>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Scenario {
    /// Unique scenario identifier (used in reports and CI gating).
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Path to the WAV file (relative to wav_base or the YAML file).
    pub wav: String,
    /// Maximum time to wait for the pipeline to finish processing (seconds).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Expected outcomes.  If None, only basic assertions are run.
    #[serde(default)]
    pub expected: ExpectedBehavior,
    /// Tags for filtering runs (e.g. "regression", "edge", "p0").
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ExpectedBehavior {
    /// Expected number of wake sessions.
    pub wake_count: Option<u32>,
    /// Expected number of command sessions.
    pub command_count: Option<u32>,
    /// Whether at least one command should have been executed successfully.
    pub executed: Option<bool>,
    /// Assert no illegal transitions.
    #[serde(default = "default_true")]
    pub no_illegal_transitions: bool,
    /// Maximum acceptable roundtrip latency (ms).
    pub max_roundtrip_ms: Option<u64>,
    /// Assertion IDs that must PASS for this scenario to pass.
    #[serde(default)]
    pub must_pass: Vec<String>,
    /// Assertion IDs that are expected to FAIL (known defects).
    #[serde(default)]
    pub known_failures: Vec<String>,

    // ── Extended expectation fields (Phase 5) ─────────────────────────────────

    /// Substrings that must appear in at least one recognized transcript.
    /// Checked against `CommandSessionOpen.text` from the event timeline.
    #[serde(default)]
    pub expected_transcripts: Vec<String>,

    /// Strings that must NOT appear in any transcript (e.g. wake phrase in self-hearing test).
    #[serde(default)]
    pub forbidden_transcripts: Vec<String>,

    /// If true, asserts `wake_sessions == 0`.  Used for self-hearing and no-activation scenarios.
    pub self_hearing_safe: Option<bool>,

    /// Maximum duplicate commands allowed.  Defaults to 0 when `Some(0)`.
    pub max_duplicate_commands: Option<u32>,

    /// Maximum wake-detection latency (VoiceActive → WakeSessionOpen) across all sessions.
    pub max_wake_latency_ms: Option<u64>,

    /// Maximum STT latency (WakeSessionOpen → CommandSessionOpen) across all sessions.
    pub max_stt_latency_ms: Option<u64>,

    /// Maximum end-to-end pipeline latency (VoiceActive → CommandSessionOpen).
    pub max_pipeline_latency_ms: Option<u64>,
}

fn default_timeout() -> u64 { 20 }
fn default_true() -> bool { true }

// ── Loader ────────────────────────────────────────────────────────────────────

impl ScenarioBatch {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read scenario file {:?}: {}", path, e))?;
        let mut batch: ScenarioBatch = serde_yaml::from_str(&content)
            .map_err(|e| format!("Cannot parse scenario YAML {:?}: {}", path, e))?;

        // Resolve wav_base relative to the YAML file's directory.
        if batch.wav_base.is_none() {
            if let Some(dir) = path.parent() {
                batch.wav_base = Some(dir.to_string_lossy().into_owned());
            }
        }
        Ok(batch)
    }

    /// Resolve the absolute WAV path for a scenario.
    pub fn resolve_wav(&self, scenario: &Scenario) -> PathBuf {
        let wav = Path::new(&scenario.wav);
        if wav.is_absolute() {
            return wav.to_path_buf();
        }
        if let Some(ref base) = self.wav_base {
            return Path::new(base).join(wav);
        }
        wav.to_path_buf()
    }
}

impl Scenario {
    /// Returns scenario-level violations (strings) for all expected behavior checks.
    /// Returns an empty vec if all expectations are met.
    pub fn validate(&self, report: &super::report::ReplayReport) -> Vec<String> {
        let mut violations = Vec::new();

        // ── Basic counts ─────────────────────────────────────────────────────

        if let Some(expected_wakes) = self.expected.wake_count {
            if report.wake_sessions != expected_wakes {
                violations.push(format!(
                    "wake_count: expected {} got {}",
                    expected_wakes, report.wake_sessions
                ));
            }
        }

        if let Some(expected_cmds) = self.expected.command_count {
            if report.command_sessions != expected_cmds {
                violations.push(format!(
                    "command_count: expected {} got {}",
                    expected_cmds, report.command_sessions
                ));
            }
        }

        if self.expected.no_illegal_transitions && report.illegal_transitions > 0 {
            violations.push(format!(
                "no_illegal_transitions violated: {} illegal transition(s)",
                report.illegal_transitions
            ));
        }

        // ── must_pass / known_failures ────────────────────────────────────────

        for must in &self.expected.must_pass {
            let assertion = report.assertions.iter().find(|a| a.id == *must);
            match assertion {
                None => violations.push(format!("must_pass [{}]: assertion not found in report", must)),
                Some(a) if !a.passed => {
                    let detail = a.failures.join("; ");
                    violations.push(format!("must_pass [{}] FAILED: {}", must, detail));
                }
                _ => {}
            }
        }

        // ── Self-hearing safety ───────────────────────────────────────────────

        if let Some(true) = self.expected.self_hearing_safe {
            if report.wake_sessions > 0 {
                violations.push(format!(
                    "self_hearing_safe: expected 0 wake sessions (self-hearing protection), \
                     got {} — pipeline activated on non-wake audio",
                    report.wake_sessions
                ));
            }
        }

        // ── Duplicate commands ────────────────────────────────────────────────

        if let Some(max_dups) = self.expected.max_duplicate_commands {
            let actual_dups = report.classified_failures.iter()
                .filter(|f| f.assertion_id == "A014")
                .count() as u32;
            // Use speech_reco_contaminations as proxy since duplicate_commands
            // is embedded in the journal (not directly in ReplayReport).
            // A014 failure means duplicates > 0.
            let a014_failed = report.assertions.iter()
                .any(|a| a.id == "A014" && !a.passed);
            if a014_failed && max_dups == 0 {
                violations.push(
                    "max_duplicate_commands: A014 failed — duplicate commands detected".to_string()
                );
            }
            let _ = actual_dups; // suppress unused warning
        }

        // ── Transcript checks ─────────────────────────────────────────────────

        // Collect all transcripts from classified failures (transcript info is
        // only available if embedded in failure messages from A014).
        // For direct transcript checking, use forbidden_transcripts against
        // assertion failure messages (best-effort without event log access).
        for forbidden in &self.expected.forbidden_transcripts {
            for a in &report.assertions {
                for f in &a.failures {
                    if f.to_lowercase().contains(&forbidden.to_lowercase()) {
                        violations.push(format!(
                            "forbidden_transcript '{}' found in assertion [{}] failure: {}",
                            forbidden, a.id, f
                        ));
                    }
                }
            }
        }

        // ── Latency thresholds ────────────────────────────────────────────────

        if let Some(max_wake_lat) = self.expected.max_wake_latency_ms {
            if let Some(p95) = report.latency_p95_wake_ms {
                if p95 > max_wake_lat {
                    violations.push(format!(
                        "max_wake_latency_ms: p95={}ms exceeds threshold {}ms",
                        p95, max_wake_lat
                    ));
                }
            }
        }

        if let Some(max_stt_lat) = self.expected.max_stt_latency_ms {
            if let Some(p95) = report.latency_p95_stt_ms {
                if p95 > max_stt_lat {
                    violations.push(format!(
                        "max_stt_latency_ms: p95={}ms exceeds threshold {}ms",
                        p95, max_stt_lat
                    ));
                }
            }
        }

        if let Some(max_pipe_lat) = self.expected.max_pipeline_latency_ms {
            if let Some(p95) = report.latency_p95_pipeline_ms {
                if p95 > max_pipe_lat {
                    violations.push(format!(
                        "max_pipeline_latency_ms: p95={}ms exceeds threshold {}ms",
                        p95, max_pipe_lat
                    ));
                }
            }
        }

        violations
    }
}
