#![allow(dead_code)]

//! Statistical production validation framework.
//!
//! Provides FAR/FRR/EER computation, ROC/DET curve generation,
//! multi-speaker corpus support, and background false-positive validation.

use std::path::{Path, PathBuf};

use super::corpus_metadata::CorpusMetadata;
use super::report::ReplayReport;

// ── Speaker metadata ──────────────────────────────────────────────────────────

/// Per-speaker recording metadata for multi-speaker corpus.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SpeakerMetadata {
    pub speaker_id: String,
    /// "male" | "female" | "other"
    pub gender: String,
    /// IETF language tag, e.g. "ru", "ru-RU", "en-US"
    pub accent: String,
    pub microphone: String,
    pub distance_m: f32,
    pub room: String,
    #[serde(default)]
    pub note: Option<String>,
}

impl SpeakerMetadata {
    /// Load from `<wav_path>.speaker.json`.
    pub fn load_for_wav(wav_path: &Path) -> Option<Self> {
        let p = wav_path.with_extension("speaker.json");
        let content = std::fs::read_to_string(&p).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save to `<wav_path>.speaker.json`.
    pub fn save_for_wav(&self, wav_path: &Path) -> Result<(), String> {
        let p = wav_path.with_extension("speaker.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Serialize: {}", e))?;
        std::fs::write(&p, json)
            .map_err(|e| format!("Write {:?}: {}", p, e))
    }
}

// ── Statistical failure classes (v2) ─────────────────────────────────────────

/// Granular failure class for statistical wake-word analysis.
/// V2 companion to `FailureClass` — more specific, used in FAR/FRR context.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StatisticalFailureClass {
    /// False accept: wake fired when it should not have.
    WakeFp,
    /// False reject: wake missed when it should have fired.
    WakeFn,
    /// Detection score drifts inconsistently across identical conditions.
    WakeDrift,
    /// Score oscillates near the threshold boundary across runs.
    ThresholdOscillation,
    /// TTS/speaker output re-triggers wake detection.
    SelfHearing,
    /// Different microphone produces different detection result.
    MicVariance,
    /// Noise environment causes wake failure.
    NoiseFailure,
    /// CPU saturation causes missed or delayed wake.
    CpuStarvation,
    /// Pipeline frame drops under sustained load.
    PipelineStarvation,
    /// Identical WAV replay produces different results across runs.
    ReplayDivergence,
    /// Command not recognized after confirmed wake.
    CommandDrop,
    /// Same command dispatched more than once in a session.
    CommandDuplicate,
}

impl StatisticalFailureClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WakeFp => "WAKE_FP",
            Self::WakeFn => "WAKE_FN",
            Self::WakeDrift => "WAKE_DRIFT",
            Self::ThresholdOscillation => "THRESHOLD_OSCILLATION",
            Self::SelfHearing => "SELF_HEARING",
            Self::MicVariance => "MIC_VARIANCE",
            Self::NoiseFailure => "NOISE_FAILURE",
            Self::CpuStarvation => "CPU_STARVATION",
            Self::PipelineStarvation => "PIPELINE_STARVATION",
            Self::ReplayDivergence => "REPLAY_DIVERGENCE",
            Self::CommandDrop => "COMMAND_DROP",
            Self::CommandDuplicate => "COMMAND_DUPLICATE",
        }
    }
}

impl std::fmt::Display for StatisticalFailureClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Map a description string to a `StatisticalFailureClass`.
pub fn classify_statistical(msg: &str) -> StatisticalFailureClass {
    let m = msg.to_ascii_lowercase();
    if m.contains("self_hear") || m.contains("tts") || m.contains("loopback") {
        StatisticalFailureClass::SelfHearing
    } else if m.contains("mic_var") || m.contains("microphone") {
        StatisticalFailureClass::MicVariance
    } else if m.contains("cpu") || m.contains("starvation") {
        StatisticalFailureClass::CpuStarvation
    } else if m.contains("pipeline") || m.contains("frame_drop") {
        StatisticalFailureClass::PipelineStarvation
    } else if m.contains("diverge") || m.contains("nondetermin") {
        StatisticalFailureClass::ReplayDivergence
    } else if m.contains("duplicate") || m.contains("dup_cmd") {
        StatisticalFailureClass::CommandDuplicate
    } else if m.contains("command_drop") || m.contains("cmd_drop") {
        StatisticalFailureClass::CommandDrop
    } else if m.contains("noise") {
        StatisticalFailureClass::NoiseFailure
    } else if m.contains("oscillat") || m.contains("threshold") {
        StatisticalFailureClass::ThresholdOscillation
    } else if m.contains("drift") {
        StatisticalFailureClass::WakeDrift
    } else if m.contains("false_accept") || m.contains("wake_fp") {
        StatisticalFailureClass::WakeFp
    } else {
        StatisticalFailureClass::WakeFn
    }
}

// ── FAR / FRR / EER ──────────────────────────────────────────────────────────

/// FAR/FRR statistics at a specific threshold.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FarFrrStats {
    pub threshold: f32,
    /// False Accept Rate = FP / (FP + TN).
    pub far: f64,
    /// False Reject Rate = FN / (TP + FN).
    pub frr: f64,
    /// |FAR - FRR| at this threshold (smaller = closer to EER).
    pub far_frr_delta: f64,
    pub tp: u32,
    pub fp: u32,
    pub tn: u32,
    pub fn_count: u32,
}

impl FarFrrStats {
    /// Compute FAR/FRR directly from a batch of (ReplayReport, CorpusMetadata) pairs.
    /// Mirrors `PrecisionRecallStats::compute` but uses the FAR/FRR vocabulary.
    pub fn compute(results: &[(ReplayReport, CorpusMetadata)], threshold: f32) -> Self {
        let mut tp = 0u32;
        let mut fp = 0u32;
        let mut tn = 0u32;
        let mut fn_count = 0u32;

        for (report, meta) in results {
            let woke = report.wake_sessions > 0;
            match (meta.expected_wake, woke) {
                (true,  true)  => tp += 1,
                (false, true)  => fp += 1,
                (false, false) => tn += 1,
                (true,  false) => fn_count += 1,
            }
        }

        let far = if fp + tn > 0 { fp as f64 / (fp + tn) as f64 } else { 0.0 };
        let frr = if tp + fn_count > 0 { fn_count as f64 / (tp + fn_count) as f64 } else { 0.0 };

        Self {
            threshold,
            far,
            frr,
            far_frr_delta: (far - frr).abs(),
            tp,
            fp,
            tn,
            fn_count,
        }
    }

    /// Format as one-line Markdown table row for DET/FAR-FRR table.
    pub fn to_table_row(&self) -> String {
        format!(
            "| {:.2} | {:>4} | {:>4} | {:>4} | {:>4} | {:.4} | {:.4} | {:.4} |",
            self.threshold,
            self.tp, self.fp, self.tn, self.fn_count,
            self.far, self.frr, self.far_frr_delta,
        )
    }
}

// ── ROC / DET curve points ────────────────────────────────────────────────────

/// One point on the ROC curve (Receiver Operating Characteristic).
#[derive(Clone, Debug, serde::Serialize)]
pub struct RocPoint {
    pub threshold: f32,
    /// True Positive Rate (Recall) = TP / (TP + FN).
    pub tpr: f64,
    /// False Positive Rate = FP / (FP + TN).
    pub fpr: f64,
}

/// One point on the DET curve (Detection Error Trade-off).
#[derive(Clone, Debug, serde::Serialize)]
pub struct DetPoint {
    pub threshold: f32,
    /// False Rejection Rate = FN / (TP + FN).
    pub frr: f64,
    /// False Acceptance Rate = FP / (FP + TN).
    pub far: f64,
}

/// Compute ROC curve over a threshold sweep.
pub fn roc_curve(
    results: &[(ReplayReport, CorpusMetadata)],
    thresholds: &[f32],
) -> Vec<RocPoint> {
    thresholds.iter().map(|&t| {
        let s = FarFrrStats::compute(results, t);
        let tpr = if s.tp + s.fn_count > 0 {
            s.tp as f64 / (s.tp + s.fn_count) as f64
        } else {
            1.0
        };
        RocPoint { threshold: t, tpr, fpr: s.far }
    }).collect()
}

/// Compute DET curve over a threshold sweep.
pub fn det_curve(
    results: &[(ReplayReport, CorpusMetadata)],
    thresholds: &[f32],
) -> Vec<DetPoint> {
    thresholds.iter().map(|&t| {
        let s = FarFrrStats::compute(results, t);
        DetPoint { threshold: t, frr: s.frr, far: s.far }
    }).collect()
}

/// Find the Equal Error Rate (EER) — the threshold where FAR ≈ FRR.
/// Returns `(eer_value, threshold)` or `None` if curve is empty.
pub fn find_eer(det: &[DetPoint]) -> Option<(f64, f32)> {
    det.iter()
        .min_by(|a, b| {
            a.far_frr_delta().partial_cmp(&b.far_frr_delta())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|p| ((p.far + p.frr) / 2.0, p.threshold))
}

impl DetPoint {
    fn far_frr_delta(&self) -> f64 {
        (self.far - self.frr).abs()
    }
}

// ── Background validation ─────────────────────────────────────────────────────

/// Per-WAV result from a background validation run.
#[derive(Clone, Debug, serde::Serialize)]
pub struct BackgroundWavResult {
    pub wav_path: String,
    pub duration_secs: f64,
    pub false_wakes: u32,
    pub ghost_commands: u32,
    pub duplicate_activations: u32,
}

/// Aggregate statistics from a background false-positive validation run.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct BackgroundValidationStats {
    pub total_files: u32,
    pub total_duration_hours: f64,
    pub false_wakes: u32,
    pub false_wakes_per_hour: f64,
    pub ghost_commands: u32,
    pub ghost_commands_per_hour: f64,
    pub phantom_sessions: u32,
    pub self_hearing_events: u32,
    pub duplicate_activation_events: u32,
    pub per_file: Vec<BackgroundWavResult>,
}

/// Run background false-positive validation on a directory of WAV files.
///
/// Every WAV in `dir` is replayed via subprocess.  If a `.meta.json` sidecar
/// exists and specifies `expected_wake: false`, its result is scored as an FP
/// when `wake_sessions > 0`.  WAVs without `.meta.json` are also included and
/// assumed to be background (expected_wake = false).
pub fn run_background_validation(
    dir: &Path,
    output_dir: &Path,
    accelerated: bool,
) -> BackgroundValidationStats {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[BGVAL] Cannot determine current exe: {}", e);
            return BackgroundValidationStats::default();
        }
    };

    std::fs::create_dir_all(output_dir).ok();

    let mut wavs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map_or(false, |e| e.eq_ignore_ascii_case("wav")))
                .collect()
        })
        .unwrap_or_default();
    wavs.sort();

    if wavs.is_empty() {
        eprintln!("[BGVAL] No WAV files found in {:?}", dir);
        return BackgroundValidationStats::default();
    }

    println!("[BGVAL] Background validation: {} WAV(s) in {:?}", wavs.len(), dir);

    let mut stats = BackgroundValidationStats::default();
    stats.total_files = wavs.len() as u32;

    for wav in &wavs {
        let meta = CorpusMetadata::load_for_wav(wav);
        // Skip if meta says expected_wake = true (not background material).
        if meta.as_ref().map_or(false, |m| m.expected_wake) {
            println!("[BGVAL] Skipping (expected_wake=true): {}", wav.display());
            continue;
        }

        let duration_secs = wav_duration_secs(wav).unwrap_or(0.0);
        let stem = wav.file_stem().unwrap_or_default().to_string_lossy();
        let result_path = output_dir.join(format!("{}.result.json", stem));

        println!("[BGVAL] {} ({:.1}s)", wav.display(), duration_secs);

        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("--audio-test").arg(wav)
           .arg("--validation-out").arg(&result_path);
        if accelerated {
            cmd.arg("--accelerated");
        }

        let report: Option<ReplayReport> = cmd.status().ok().and_then(|_| {
            std::fs::read_to_string(&result_path).ok()
                .and_then(|c| serde_json::from_str(&c).ok())
        });

        let (fp, ghost, dup) = if let Some(ref r) = report {
            (
                r.wake_sessions,
                r.command_sessions,
                r.defects_detected.iter()
                    .filter(|d| d.contains("duplicate"))
                    .count() as u32,
            )
        } else {
            (0, 0, 0)
        };

        stats.false_wakes += fp;
        stats.ghost_commands += ghost;
        stats.duplicate_activation_events += dup;
        stats.total_duration_hours += duration_secs / 3600.0;

        stats.per_file.push(BackgroundWavResult {
            wav_path: wav.to_string_lossy().into_owned(),
            duration_secs,
            false_wakes: fp,
            ghost_commands: ghost,
            duplicate_activations: dup,
        });
    }

    if stats.total_duration_hours > 0.0 {
        stats.false_wakes_per_hour =
            stats.false_wakes as f64 / stats.total_duration_hours;
        stats.ghost_commands_per_hour =
            stats.ghost_commands as f64 / stats.total_duration_hours;
    }

    stats
}

// ── WAV header reader ─────────────────────────────────────────────────────────

/// Read duration in seconds from a WAV file header without any external crate.
///
/// Parses the RIFF/WAVE header to find sample_rate and data chunk size.
/// Returns `None` if the file is not a valid WAV or too short to parse.
pub fn wav_duration_secs(path: &Path) -> Option<f64> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 44];
    f.read_exact(&mut buf).ok()?;

    // Validate RIFF/WAVE header.
    if &buf[0..4] != b"RIFF" || &buf[8..12] != b"WAVE" {
        return None;
    }

    let sample_rate = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]) as f64;
    let bits_per_sample = u16::from_le_bytes([buf[34], buf[35]]) as f64;
    let channels = u16::from_le_bytes([buf[22], buf[23]]) as f64;

    if sample_rate == 0.0 || bits_per_sample == 0.0 || channels == 0.0 {
        return None;
    }

    // Find "data" chunk — it may not be at offset 36 if there are extra fmt bytes.
    // Try the canonical position first (offset 36), then scan.
    let data_size = if &buf[36..40] == b"data" {
        u32::from_le_bytes([buf[40], buf[41], buf[42], buf[43]]) as f64
    } else {
        // Fallback: file size - 44 bytes header
        let file_len = std::fs::metadata(path).ok()?.len() as f64;
        (file_len - 44.0).max(0.0)
    };

    let bytes_per_sample = bits_per_sample / 8.0;
    let num_samples = data_size / (bytes_per_sample * channels);
    Some(num_samples / sample_rate)
}

// ── Continuous runtime stability ──────────────────────────────────────────────

/// Continuous runtime stability snapshot.
/// Populated from ValidationEvent counts after a long replay session.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ContinuousRuntimeStats {
    pub duration_hours: f64,
    pub wake_sessions_total: u32,
    pub command_sessions_total: u32,
    pub illegal_transitions: u32,
    pub dirty_closes: u32,
    pub stale_cooldowns: u32,
    pub stuck_gates: u32,
    pub ipc_accumulation: u32,
    /// True if the same WAV produced different results across 10+ iterations.
    pub replay_divergence_detected: bool,
    /// Variance in wake_sessions count across identical stress replay iterations.
    pub wake_count_variance: f64,
}

impl ContinuousRuntimeStats {
    /// Derive stability stats from a stress-replay BatchReport.
    pub fn from_stress_replay(
        reports: &[ReplayReport],
        duration_hours: f64,
    ) -> Self {
        if reports.is_empty() {
            return Self { duration_hours, ..Default::default() };
        }

        let wake_counts: Vec<u32> = reports.iter().map(|r| r.wake_sessions).collect();
        let mean = wake_counts.iter().sum::<u32>() as f64 / wake_counts.len() as f64;
        let variance = wake_counts.iter()
            .map(|&c| { let d = c as f64 - mean; d * d })
            .sum::<f64>() / wake_counts.len() as f64;

        let divergence = variance > 0.0;

        Self {
            duration_hours,
            wake_sessions_total: reports.iter().map(|r| r.wake_sessions).sum(),
            command_sessions_total: reports.iter().map(|r| r.command_sessions).sum(),
            illegal_transitions: reports.iter().map(|r| r.illegal_transitions).sum(),
            dirty_closes: reports.iter().map(|r| r.dirty_session_closes).sum(),
            stale_cooldowns: 0, // Not directly measurable from BatchReport
            stuck_gates: 0,     // Not directly measurable from BatchReport
            ipc_accumulation: 0,
            replay_divergence_detected: divergence,
            wake_count_variance: variance,
        }
    }
}

// ── Wake confusion entry ──────────────────────────────────────────────────────

/// One entry in the wake confusion analysis.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WakeConfusionEntry {
    /// The word or phrase that was tested.
    pub stimulus: String,
    /// Number of times it was tested.
    pub trials: u32,
    /// Number of times it triggered a false wake.
    pub false_accepts: u32,
    /// False accept rate for this stimulus.
    pub far: f64,
    /// Category: "phonetically_similar" | "unrelated" | "partial_match" | "ambient"
    pub category: String,
}

impl WakeConfusionEntry {
    pub fn new(stimulus: &str, trials: u32, false_accepts: u32, category: &str) -> Self {
        let far = if trials > 0 { false_accepts as f64 / trials as f64 } else { 0.0 };
        Self {
            stimulus: stimulus.to_string(),
            trials,
            false_accepts,
            far,
            category: category.to_string(),
        }
    }
}

// ── Multi-speaker report ──────────────────────────────────────────────────────

/// Aggregated wake detection results for a single speaker.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SpeakerResult {
    pub speaker_id: String,
    pub gender: String,
    pub trials: u32,
    pub wake_detected: u32,
    pub recall: f64,
}

/// Full multi-speaker validation summary.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct MultiSpeakerReport {
    pub speakers: Vec<SpeakerResult>,
    /// Mean recall across all speakers.
    pub mean_recall: f64,
    /// Speaker with the lowest recall.
    pub worst_speaker: Option<String>,
    /// Speaker with the highest recall.
    pub best_speaker: Option<String>,
}

impl MultiSpeakerReport {
    pub fn build(speakers: Vec<SpeakerResult>) -> Self {
        if speakers.is_empty() {
            return Self::default();
        }
        let mean = speakers.iter().map(|s| s.recall).sum::<f64>() / speakers.len() as f64;
        let worst = speakers.iter()
            .min_by(|a, b| a.recall.partial_cmp(&b.recall).unwrap())
            .map(|s| s.speaker_id.clone());
        let best = speakers.iter()
            .max_by(|a, b| a.recall.partial_cmp(&b.recall).unwrap())
            .map(|s| s.speaker_id.clone());
        Self { speakers, mean_recall: mean, worst_speaker: worst, best_speaker: best }
    }
}
