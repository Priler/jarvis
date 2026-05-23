#![allow(dead_code)]

//! Replay mode detection and configuration.
//!
//! Parses the `replay`, `replay-dir`, and `replay-batch` CLI sub-commands
//! and sets up the environment for each run.

use std::path::{Path, PathBuf};

// ── Replay command ────────────────────────────────────────────────────────────

/// Parsed replay subcommand extracted from argv.
#[derive(Debug)]
pub enum ReplayCommand {
    /// `replay <wav_path> [--out <dir>] [--accelerated]`
    Single {
        wav_path: PathBuf,
        output_path: Option<PathBuf>,
        accelerated: bool,
    },
    /// `replay-dir <dir> [--out <dir>] [--accelerated]`
    Dir {
        dir: PathBuf,
        output_dir: PathBuf,
        accelerated: bool,
    },
    /// `replay-batch <yaml> [--out <dir>] [--accelerated]`
    Batch {
        yaml_path: PathBuf,
        output_dir: PathBuf,
        accelerated: bool,
    },
    /// `stress-replay <dir|wav> [--n <N>] [--out <dir>] [--accelerated]`
    ///
    /// Runs each WAV file (or a single WAV) N times and reports aggregate pass/fail.
    StressReplay {
        path: PathBuf,
        iterations: u32,
        output_dir: PathBuf,
        accelerated: bool,
    },
    /// `background-validation <dir> [--out <dir>] [--accelerated]`
    ///
    /// Runs all WAVs in `dir` as background (expected_wake=false) audio and
    /// reports false-wakes-per-hour and related FP statistics.
    BackgroundValidation {
        dir: PathBuf,
        output_dir: PathBuf,
        accelerated: bool,
    },
}

/// Returns true if `--accelerated` appears in argv.
pub fn parse_accelerated_flag() -> bool {
    std::env::args().any(|a| a == "--accelerated")
}

/// Parse argv for replay subcommands.  Returns `None` if this is a normal run.
pub fn parse_replay_command() -> Option<ReplayCommand> {
    let args: Vec<String> = std::env::args().collect();

    // Need at least: exe <subcommand> <arg>
    if args.len() < 3 {
        return None;
    }

    let out_flag = find_flag_value(&args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("validation_results"));

    let accelerated = args.iter().any(|a| a == "--accelerated");

    match args[1].as_str() {
        "replay" => {
            let wav = PathBuf::from(&args[2]);
            // --out overrides --validation-out for this path.
            let output_path = find_flag_value(&args, "--validation-out")
                .map(PathBuf::from)
                .or_else(|| {
                    let stem = wav.file_stem().unwrap_or_default().to_string_lossy();
                    Some(out_flag.join(format!("{}.result.json", stem)))
                });
            Some(ReplayCommand::Single { wav_path: wav, output_path, accelerated })
        }
        "replay-dir" => Some(ReplayCommand::Dir {
            dir: PathBuf::from(&args[2]),
            output_dir: out_flag,
            accelerated,
        }),
        "replay-batch" => Some(ReplayCommand::Batch {
            yaml_path: PathBuf::from(&args[2]),
            output_dir: out_flag,
            accelerated,
        }),
        "stress-replay" => {
            let n: u32 = find_flag_value(&args, "--n")
                .and_then(|s| s.parse().ok())
                .unwrap_or(100);
            Some(ReplayCommand::StressReplay {
                path: PathBuf::from(&args[2]),
                iterations: n,
                output_dir: out_flag,
                accelerated,
            })
        }
        "background-validation" => Some(ReplayCommand::BackgroundValidation {
            dir: PathBuf::from(&args[2]),
            output_dir: out_flag,
            accelerated,
        }),
        _ => None,
    }
}

/// Parse `--validation-out <path>` when the binary is invoked as a subprocess
/// by the batch runner (i.e., with `--audio-test`).
pub fn parse_inline_validation_output() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    find_flag_value(&args, "--validation-out").map(PathBuf::from)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
}

// ── Inline single-WAV replay ──────────────────────────────────────────────────

/// Set up a single-WAV inline replay run.
///
/// Must be called from `main` before `recorder::init()`.
/// Initialises the test harness and configures the recorder to WAV mode.
/// The normal pipeline then runs to completion; the harness is finalised
/// by `testing::harness::TestHarness::on_replay_complete()` just before
/// `process::exit()`.
pub fn setup_single(wav_path: &Path, output_path: Option<PathBuf>) -> Result<(), String> {
    // Validate WAV exists.
    if !wav_path.exists() {
        return Err(format!("WAV file not found: {:?}", wav_path));
    }

    let wav_str = wav_path.to_string_lossy().into_owned();

    // Create output directory if specified.
    if let Some(ref out) = output_path {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
    }

    // Register harness BEFORE the recorder is initialised so that any events
    // emitted during init are captured.
    super::harness::TestHarness::init(wav_str.clone(), output_path);

    // Inject the WAV path so recorder::init_wav is called by the normal
    // startup path in main.rs via parse_audio_test_arg() / JARVIS_AUDIO_TEST_FILE.
    // We use the env-var approach to avoid duplicating the init call.
    std::env::set_var("JARVIS_AUDIO_TEST_FILE", &wav_str);

    info!("[REPLAY] Single WAV replay: {}", wav_str);
    Ok(())
}

/// Print a summary of the replay run stats extracted from a result JSON.
pub fn print_dir_summary(report: &super::report::BatchReport) {
    println!("{}", report.to_summary());
}
