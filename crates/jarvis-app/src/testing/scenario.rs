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
    /// Returns true if the report satisfies this scenario's expected behavior.
    pub fn validate(&self, report: &super::report::ReplayReport) -> Vec<String> {
        let mut violations = Vec::new();

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

        violations
    }
}
