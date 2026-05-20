#![allow(dead_code)]

//! Acoustic corpus metadata.
//!
//! Each WAV in the acoustic corpus may have a companion `<wav>.meta.json` file
//! that describes the expected behavior and recording conditions.
//! The batch runner reads these files to drive expectation validation and
//! precision/recall analysis.

use std::path::Path;

// ── Corpus metadata ───────────────────────────────────────────────────────────

/// Per-WAV metadata describing recording conditions and expected behavior.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CorpusMetadata {
    /// Whether a wake activation is expected for this WAV.
    pub expected_wake: bool,
    /// ID of the expected command (e.g. "calculator_open").  None for no-command scenarios.
    #[serde(default)]
    pub expected_command: Option<String>,
    /// True if this WAV is specifically testing false-positive resistance.
    #[serde(default)]
    pub expected_false_positive: bool,
    /// Subjective noise level: "quiet", "low", "medium", "high", "very_high".
    #[serde(default = "default_noise_level")]
    pub noise_level: String,
    /// Distance from speaker to microphone in meters.
    #[serde(default)]
    pub distance_meters: Option<f32>,
    /// Speaker identifier (e.g. "speaker_01", "speaker_02").
    #[serde(default)]
    pub speaker_id: Option<String>,
    /// Microphone used (e.g. "intel_smart_sound", "motu", "loopback", "windows_default").
    #[serde(default)]
    pub microphone: Option<String>,
    /// Room type (e.g. "bedroom", "kitchen", "office", "outdoor").
    #[serde(default)]
    pub room_type: Option<String>,
    /// Acoustic category: A1, A2, A3, A4, A5, A6, A7, A8.
    #[serde(default)]
    pub category: Option<String>,
    /// Human-readable note about this recording.
    #[serde(default)]
    pub note: Option<String>,
}

fn default_noise_level() -> String { "quiet".to_string() }

impl CorpusMetadata {
    /// Load metadata from `<wav_path>.meta.json`.
    /// Returns None if the file does not exist or cannot be parsed.
    pub fn load_for_wav(wav_path: &Path) -> Option<Self> {
        let meta_path = wav_path.with_extension("meta.json");
        let content = std::fs::read_to_string(&meta_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save metadata to `<wav_path>.meta.json`.
    pub fn save_for_wav(&self, wav_path: &Path) -> Result<(), String> {
        let meta_path = wav_path.with_extension("meta.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Serialize error: {}", e))?;
        std::fs::write(&meta_path, json)
            .map_err(|e| format!("Write error for {:?}: {}", meta_path, e))
    }

    /// Load all metadata files from a directory.
    /// Returns a Vec of (wav_path, metadata) pairs for WAVs that have metadata.
    pub fn load_all_in_dir(dir: &Path) -> Vec<(std::path::PathBuf, Self)> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        let mut result = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e.eq_ignore_ascii_case("wav")) {
                if let Some(meta) = Self::load_for_wav(&path) {
                    result.push((path, meta));
                }
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }
}

// ── Precision / Recall analysis ───────────────────────────────────────────────

/// Classification counts for a batch of replay results vs corpus metadata.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct PrecisionRecallStats {
    /// Threshold used for this computation.
    pub threshold: f32,
    /// True positives: expected_wake=true AND wake_sessions > 0.
    pub tp: u32,
    /// False positives: expected_wake=false AND wake_sessions > 0.
    pub fp: u32,
    /// True negatives: expected_wake=false AND wake_sessions == 0.
    pub tn: u32,
    /// False negatives: expected_wake=true AND wake_sessions == 0.
    pub fn_count: u32,
    /// TP / (TP + FP), or 1.0 if no positives predicted.
    pub precision: f64,
    /// TP / (TP + FN), or 1.0 if no actual positives.
    pub recall: f64,
    /// 2 * precision * recall / (precision + recall), or 0.0 if both are 0.
    pub f1: f64,
    /// FP / (FP + TN) — false positive rate (1 - specificity).
    pub fpr: f64,
    /// FN / (TP + FN) — false rejection rate.
    pub frr: f64,
}

impl PrecisionRecallStats {
    /// Compute precision/recall from a batch of (ReplayReport, CorpusMetadata) pairs.
    pub fn compute(
        results: &[(super::report::ReplayReport, CorpusMetadata)],
        threshold: f32,
    ) -> Self {
        let mut tp = 0u32;
        let mut fp = 0u32;
        let mut tn = 0u32;
        let mut fn_count = 0u32;

        for (report, meta) in results {
            let woke = report.wake_sessions > 0;
            match (meta.expected_wake, woke) {
                (true, true) => tp += 1,
                (false, true) => fp += 1,
                (false, false) => tn += 1,
                (true, false) => fn_count += 1,
            }
        }

        let precision = if tp + fp > 0 {
            tp as f64 / (tp + fp) as f64
        } else {
            1.0
        };
        let recall = if tp + fn_count > 0 {
            tp as f64 / (tp + fn_count) as f64
        } else {
            1.0
        };
        let f1 = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        };
        let fpr = if fp + tn > 0 {
            fp as f64 / (fp + tn) as f64
        } else {
            0.0
        };
        let frr = if tp + fn_count > 0 {
            fn_count as f64 / (tp + fn_count) as f64
        } else {
            0.0
        };

        Self { threshold, tp, fp, tn, fn_count, precision, recall, f1, fpr, frr }
    }

    /// Format as a one-line table row.
    pub fn to_table_row(&self) -> String {
        format!(
            "| {:.2} | {:>3} | {:>3} | {:>3} | {:>3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
            self.threshold,
            self.tp, self.fp, self.tn, self.fn_count,
            self.precision, self.recall, self.f1,
            self.fpr, self.frr
        )
    }
}

/// Sweep `thresholds` over the results and return a curve.
pub fn threshold_sweep(
    results: &[(super::report::ReplayReport, CorpusMetadata)],
    thresholds: &[f32],
) -> Vec<PrecisionRecallStats> {
    thresholds.iter()
        .map(|&t| PrecisionRecallStats::compute(results, t))
        .collect()
}

/// Find the threshold with the highest F1 score.
pub fn best_f1_point(curve: &[PrecisionRecallStats]) -> Option<&PrecisionRecallStats> {
    curve.iter().max_by(|a, b| a.f1.partial_cmp(&b.f1).unwrap_or(std::cmp::Ordering::Equal))
}
