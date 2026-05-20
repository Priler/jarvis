#![allow(dead_code)]

//! Test harness — owns the validation channel receiver and orchestrates
//! a single inline replay run or a batch of subprocess runs.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use once_cell::sync::Lazy;
use parking_lot::Mutex;

use super::assertions::AssertionEngine;
use super::report::{BatchReport, ReplayReport};
use super::scenario::ScenarioBatch;
use super::session_log::SessionJournal;
use super::ValidationEvent;

// ── Global harness ────────────────────────────────────────────────────────────

static HARNESS: Lazy<Mutex<Option<TestHarness>>> = Lazy::new(|| Mutex::new(None));

pub struct TestHarness {
    rx: mpsc::Receiver<ValidationEvent>,
    journal: SessionJournal,
    start: Instant,
    wav_path: String,
    output_path: Option<PathBuf>,
    /// IPC subscription for tracking IpcEvent delivery.
    ipc_rx: Option<tokio::sync::broadcast::Receiver<jarvis_core::ipc::IpcEvent>>,
}

impl TestHarness {
    /// Initialize the harness and register the ValidationBus sender.
    /// Must be called BEFORE the pipeline starts (before recorder::init).
    pub fn init(wav_path: String, output_path: Option<PathBuf>) {
        let (tx, rx) = mpsc::sync_channel::<ValidationEvent>(8192);
        super::register(tx);

        // Subscribe to IPC events so A010 can verify ordering.
        let ipc_rx = jarvis_core::ipc::subscribe();

        *HARNESS.lock() = Some(TestHarness {
            rx,
            journal: SessionJournal::new(),
            start: Instant::now(),
            wav_path,
            output_path,
            ipc_rx,
        });
    }

    /// Called just before process::exit(0) in the WAV path.
    /// Drains the channel, runs assertions, prints + optionally writes report.
    /// Returns exit code: 0 = all pass, 1 = failures.
    pub fn on_replay_complete() -> i32 {
        let mut guard = HARNESS.lock();
        let harness = match guard.take() {
            Some(h) => h,
            None => return 0,
        };
        drop(guard);

        let duration_ms = harness.start.elapsed().as_millis() as u64;
        let mut journal = harness.journal;

        // Drain validation events.
        while let Ok(ev) = harness.rx.try_recv() {
            journal.record(ev);
        }

        // Drain IPC events (non-blocking).
        if let Some(mut ipc_rx) = harness.ipc_rx {
            loop {
                match ipc_rx.try_recv() {
                    Ok(event) => {
                        let tag = ipc_event_tag(&event);
                        journal.record(ValidationEvent::IpcEvent { tag, ts: super::now_ms() });
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(_) => break,
                }
            }
        }

        let engine = AssertionEngine::run_all(&journal);
        let report = ReplayReport::build(&harness.wav_path, duration_ms, &journal, &engine);

        println!("{}", report.to_summary());

        if let Some(ref out) = harness.output_path {
            if let Err(e) = report.write_to_file(out) {
                eprintln!("[HARNESS] Failed to write report: {}", e);
            } else {
                println!("[HARNESS] Report written to {:?}", out);
            }
        }

        // Also write event journal to <wav>.events.json for post-mortem.
        if let Some(ref out) = harness.output_path {
            let events_path = out.with_extension("events.json");
            let _ = std::fs::write(&events_path, journal.to_json());
        }

        if report.passed { 0 } else { 1 }
    }
}

// ── IPC event tag mapping ─────────────────────────────────────────────────────

fn ipc_event_tag(event: &jarvis_core::ipc::IpcEvent) -> &'static str {
    use jarvis_core::ipc::IpcEvent;
    match event {
        IpcEvent::WakeWordDetected => "wake_word_detected",
        IpcEvent::Listening => "listening",
        IpcEvent::SpeechRecognized { .. } => "speech_recognized",
        IpcEvent::CommandExecuted { .. } => "command_executed",
        IpcEvent::Idle => "idle",
        IpcEvent::Error { .. } => "error",
        IpcEvent::Started => "started",
        IpcEvent::Stopping => "stopping",
        IpcEvent::Pong => "pong",
        IpcEvent::RevealWindow => "reveal_window",
        IpcEvent::ConfirmationRequired { .. } => "confirmation_required",
        IpcEvent::SandboxWarning { .. } => "sandbox_warning",
        IpcEvent::Loading { .. } => "loading",
        IpcEvent::CognitionState { .. } => "cognition_state",
        IpcEvent::ClarificationNeeded { .. } => "clarification_needed",
        IpcEvent::PlanStarted { .. } => "plan_started",
        IpcEvent::PlanProgress { .. } => "plan_progress",
        IpcEvent::MemoryRecalled { .. } => "memory_recalled",
        IpcEvent::AgentEvent { .. } => "agent_event",
        IpcEvent::WorkflowStarted { .. } => "workflow_started",
        IpcEvent::WorkflowStepCompleted { .. } => "workflow_step_completed",
        IpcEvent::WorkflowCompleted { .. } => "workflow_completed",
        IpcEvent::GovernanceAlert { .. } => "governance_alert",
        IpcEvent::ScreenContext { .. } => "screen_context",
    }
}

// ── Batch / directory runner ──────────────────────────────────────────────────

/// Run all WAV files in a directory.  Each run is spawned as a subprocess so
/// state is fully isolated between runs.
pub fn run_dir(dir: &Path, output_dir: &Path) -> BatchReport {
    let wavs: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|_| panic!("Cannot read dir {:?}", dir))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e.eq_ignore_ascii_case("wav")))
        .collect();

    let reports: Vec<ReplayReport> = wavs
        .iter()
        .map(|wav| run_wav_subprocess(wav, output_dir))
        .collect();

    BatchReport::new(reports)
}

/// Run all scenarios from a YAML batch file.  Each run is spawned as a
/// subprocess.  Scenario-level expected behavior is validated after the run.
pub fn run_batch(yaml_path: &Path, output_dir: &Path) -> BatchReport {
    let batch = ScenarioBatch::load(yaml_path).unwrap_or_else(|e| {
        eprintln!("[HARNESS] {}", e);
        std::process::exit(1);
    });

    println!("[HARNESS] Running batch '{}' — {} scenario(s)", batch.name, batch.scenarios.len());

    let mut reports = Vec::new();
    for scenario in &batch.scenarios {
        let wav = batch.resolve_wav(scenario);
        let mut report = run_wav_subprocess(&wav, output_dir);

        // Validate scenario-level expected behavior.
        let violations = scenario.validate(&report);
        if !violations.is_empty() {
            report.passed = false;
            for v in violations {
                eprintln!("[HARNESS][{}] VIOLATION: {}", scenario.name, v);
            }
        }
        reports.push(report);
    }

    BatchReport::new(reports)
}

/// Spawn the current executable with `--audio-test <wav> --validation-out <out>`
/// and wait for it to complete.  Returns the parsed JSON report.
fn run_wav_subprocess(wav: &Path, output_dir: &Path) -> ReplayReport {
    let exe = std::env::current_exe().expect("Cannot find current executable");

    std::fs::create_dir_all(output_dir).ok();

    let wav_stem = wav.file_stem().unwrap_or_default().to_string_lossy();
    let result_path = output_dir.join(format!("{}.result.json", wav_stem));

    println!("[HARNESS] → {}", wav.display());

    let status = std::process::Command::new(&exe)
        .arg("--audio-test")
        .arg(wav)
        .arg("--validation-out")
        .arg(&result_path)
        .status();

    match status {
        Ok(s) => {
            let exit_ok = s.success();
            if let Ok(content) = std::fs::read_to_string(&result_path) {
                if let Ok(mut report) = serde_json::from_str::<ReplayReport>(&content) {
                    // Ensure passed flag matches exit code.
                    if !exit_ok {
                        report.passed = false;
                    }
                    return report;
                }
            }
            // Could not parse result — fabricate a failure report.
            failed_report(wav, "Subprocess failed to produce a result file")
        }
        Err(e) => failed_report(wav, &format!("Subprocess spawn error: {}", e)),
    }
}

fn failed_report(wav: &Path, reason: &str) -> ReplayReport {
    ReplayReport {
        wav_path: wav.to_string_lossy().into_owned(),
        run_duration_ms: 0,
        total_events: 0,
        wake_sessions: 0,
        command_sessions: 0,
        dirty_session_closes: 0,
        illegal_transitions: 0,
        speech_reco_contaminations: 0,
        assertions_passed: 0,
        assertions_failed: 1,
        passed: false,
        assertions: vec![super::assertions::AssertionResult {
            id: "SUBPROCESS".to_string(),
            description: "Subprocess completed successfully".to_string(),
            passed: false,
            failures: vec![reason.to_string()],
        }],
        defects_detected: vec![format!("Subprocess failure: {}", reason)],
    }
}
