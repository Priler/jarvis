pub mod structs;
use structs::WakeWordEngine;
use structs::SpeechToTextEngine;
use structs::RecorderType;
use structs::AudioType;

use std::fs;
use std::env;
use std::path::PathBuf;
use once_cell::sync::Lazy;

use platform_dirs::{AppDirs};
use rustpotter::{RustpotterConfig, WavFmt, DetectorConfig, FiltersConfig, ScoreMode, GainNormalizationConfig, BandPassConfig};

use crate::{config, APP_DIRS, APP_CONFIG_DIR, APP_LOG_DIR};

#[allow(dead_code)]

pub fn init_dirs() -> Result<(), String> {
    // infer app dirs
    if APP_DIRS.get().is_some() {
        return Ok(());
    }

    // cache_dir, config_dir, data_dir, state_dir
    APP_DIRS.set(AppDirs::new(Some(config::BUNDLE_IDENTIFIER), false).unwrap()).unwrap();

    // setup directories
    let mut config_dir = PathBuf::from(&APP_DIRS.get().unwrap().config_dir);
    let mut log_dir = PathBuf::from(&APP_DIRS.get().unwrap().config_dir);

    // create dirs, if required
    if !config_dir.exists() {
        if fs::create_dir_all(&config_dir).is_err() {
            config_dir = env::current_dir().expect("Cannot infer the config directory");
            fs::create_dir_all(&config_dir).expect("Cannot create config directory, access denied?");
        }
    }

    if !log_dir.exists() {
        if fs::create_dir_all(&log_dir).is_err() {
            log_dir = env::current_dir().expect("Cannot infer the log directory");
            fs::create_dir_all(&log_dir).expect("Cannot create log directory, access denied?");
        }
    }

    // store inferred paths
    APP_CONFIG_DIR.set(config_dir).unwrap();
    APP_LOG_DIR.set(log_dir).unwrap();

    Ok(())
}


/*
    Defaults.
 */
pub const DEFAULT_AUDIO_TYPE: AudioType = AudioType::Kira;
pub const DEFAULT_RECORDER_TYPE: RecorderType = RecorderType::PvRecorder;
pub const DEFAULT_WAKE_WORD_ENGINE: WakeWordEngine = WakeWordEngine::Rustpotter;
pub const DEFAULT_SPEECH_TO_TEXT_ENGINE: SpeechToTextEngine = SpeechToTextEngine::Vosk;

pub const DEFAULT_VOICE: &str = "jarvis-og";

pub const BUNDLE_IDENTIFIER: &str = "com.priler.jarvis";
pub const DB_FILE_NAME: &str = "app.db";
pub const LOG_FILE_NAME: &str = "log.txt";
pub const APP_VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");
pub const AUTHOR_NAME: Option<&str> = option_env!("CARGO_PKG_AUTHORS");
pub const REPOSITORY_LINK: Option<&str> = option_env!("CARGO_PKG_REPOSITORY");
pub const TG_OFFICIAL_LINK: Option<&str> = Some("https://t.me/howdyho_official");
pub const FEEDBACK_LINK: Option<&str> = Some("https://t.me/jarvis_feedback_bot");

/*
    Tray.
 */
pub const TRAY_ICON: &str = "32x32.png";
pub const TRAY_TOOLTIP: &str = "Jarvis Voice Assistant";

// RUSPOTTER
pub const RUSPOTTER_MIN_SCORE: f32 = 0.62;
pub const RUSTPOTTER_DEFAULT_CONFIG: Lazy<RustpotterConfig> = Lazy::new(|| {
    RustpotterConfig {
        fmt: WavFmt::default(),
        detector: DetectorConfig {
            avg_threshold: 0.,
            threshold: 0.5,
            min_scores: 15,
            score_mode: ScoreMode::Average,
            comparator_band_size: 5,
            comparator_ref: 0.22
        },
        filters: FiltersConfig {
            gain_normalizer: GainNormalizationConfig {
                enabled: true,
                gain_ref: None,
                min_gain: 0.7,
                max_gain: 1.0,
            },
            band_pass: BandPassConfig {
                enabled: true,
                low_cutoff: 80.,
                high_cutoff: 400.,
            }
        }
    }
});

// PICOVOICE
pub const COMMANDS_PATH: &str = "commands/";
pub const KEYWORDS_PATH: &str = "picovoice/keywords/";
pub const DEFAULT_KEYWORD: &str = "jarvis_windows.ppn";
pub const DEFAULT_SENSITIVITY: f32 = 1.0;

// VOSK
// pub const VOSK_MODEL_PATH: &str = const_concat!(PUBLIC_PATH, "/vosk/model_small");
pub const VOSK_FETCH_PHRASE: &str = "джарвис";
pub const VOSK_MODEL_PATH: &str = "vosk/model_small";
pub const VOSK_MIN_RATIO: f64 = 70.0;

// ETC
pub const CMD_RATIO_THRESHOLD: f64 = 65f64;
pub const CMS_WAIT_DELAY: std::time::Duration = std::time::Duration::from_secs(15);

pub const ASSISTANT_GREET_PHRASES: [&str; 3] = ["greet1", "greet2", "greet3"];
pub const ASSISTANT_PHRASES_TBR: [&str; 17] = [
    "джарвис",
    "сэр",
    "слушаю сэр",
    "всегда к услугам",
    "произнеси",
    "ответь",
    "покажи",
    "скажи",
    "давай",
    "да сэр",
    "к вашим услугам сэр",
    "всегда к вашим услугам сэр",
    "запрос выполнен сэр",
    "выполнен сэр",
    "есть",
    "загружаю сэр",
    "очень тонкое замечание сэр",
];
