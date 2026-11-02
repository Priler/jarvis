use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use rand::prelude::*;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
// use chrono::Timelike;

use crate::{audio, config, time, DB, SOUND_DIR};

mod structs;
pub use structs::*;

static VOICES: OnceCell<Vec<structs::VoiceConfig>> = OnceCell::new();
static CURRENT_VOICE_ID: OnceCell<RwLock<String>> = OnceCell::new();

pub fn init(default_voice: &str, language: &str) -> Result<(), String> {
    let voices = scan_voices()?;

    if voices.is_empty() {
        return Err("No voices found".into());
    }

    info!(
        "Loaded {} voice(s): {:?}",
        voices.len(),
        voices.iter().map(|v| &v.voice.id).collect::<Vec<_>>()
    );

    // resolve which voice to use
    let voice_id =
        if !default_voice.is_empty() && voices.iter().any(|v| v.voice.id == default_voice) {
            default_voice.to_string()
        } else {
            // auto-detect: pick the first voice that supports the active language
            let auto = voices
                .iter()
                .find(|v| v.voice.languages.contains(&language.to_string()))
                .or_else(|| voices.first());

            match auto {
                Some(v) => {
                    if default_voice.is_empty() {
                        info!(
                            "No voice configured, auto-selected '{}' for language '{}'",
                            v.voice.id, language
                        );
                    } else {
                        warn!(
                            "Voice '{}' not found, auto-selected '{}'",
                            default_voice, v.voice.id
                        );
                    }
                    v.voice.id.clone()
                }
                None => return Err("No compatible voice found".into()),
            }
        };

    CURRENT_VOICE_ID.get_or_init(|| RwLock::new(voice_id));
    VOICES
        .set(voices)
        .map_err(|_| "Voices already initialized")?;

    Ok(())
}

pub fn scan_voices() -> Result<Vec<structs::VoiceConfig>, String> {
    let voices_dir = SOUND_DIR.join(&config::VOICES_PATH);

    if !voices_dir.exists() {
        return Err(format!("Voices directory not found: {:?}", voices_dir));
    }

    let mut voices = Vec::new();

    let entries =
        fs::read_dir(&voices_dir).map_err(|e| format!("Failed to read voices directory: {}", e))?;

    for entry in entries.flatten() {
        let voice_path = entry.path();
        if !voice_path.is_dir() {
            continue;
        }

        let toml_path = voice_path.join("voice.toml");
        if !toml_path.exists() {
            warn!("Voice folder {:?} missing voice.toml, skipping", voice_path);
            continue;
        }

        match load_voice_config(&toml_path, &voice_path) {
            Ok(config) => voices.push(config),
            Err(e) => warn!("Failed to load voice {:?}: {}", voice_path, e),
        }
    }

    Ok(voices)
}

fn load_voice_config(toml_path: &Path, voice_path: &Path) -> Result<structs::VoiceConfig, String> {
    let content =
        fs::read_to_string(toml_path).map_err(|e| format!("Failed to read voice.toml: {}", e))?;

    let mut config: structs::VoiceConfig =
        toml::from_str(&content).map_err(|e| format!("Failed to parse voice.toml: {}", e))?;

    config.path = voice_path.to_path_buf();

    Ok(config)
}

pub fn list_voices() -> &'static [structs::VoiceConfig] {
    VOICES.get().map(|v| v.as_slice()).unwrap_or(&[])
}

pub fn get_voice(voice_id: &str) -> Option<&'static structs::VoiceConfig> {
    VOICES.get()?.iter().find(|v| v.voice.id == voice_id)
}

pub fn get_current_voice() -> Option<&'static structs::VoiceConfig> {
    let current_id = CURRENT_VOICE_ID.get()?.read().clone();
    get_voice(&current_id)
}

pub fn set_current_voice(voice_id: &str) {
    if let Some(lock) = CURRENT_VOICE_ID.get() {
        *lock.write() = voice_id.to_string();
    }
}

fn get_current_language() -> String {
    DB.get()
        .map(|db| db.read().language.clone())
        .unwrap_or_else(|| "ru".to_string())
}

fn find_sound_file(voice_path: &Path, lang: &str, sound_name: &str) -> Option<PathBuf> {
    const EXTENSIONS: &[&str] = &["mp3", "wav", "ogg"];
    let lang_path = voice_path.join(lang);

    // try language subfolder first (/en, /ua, /ru, etc)
    for ext in EXTENSIONS {
        let file_path = lang_path.join(format!("{}.{}", sound_name, ext));
        if file_path.exists() {
            return Some(file_path);
        }
    }

    // fallback to root voice folder
    for ext in EXTENSIONS {
        let file_path = voice_path.join(format!("{}.{}", sound_name, ext));
        if file_path.exists() {
            return Some(file_path);
        }
    }

    None
}

fn play_random_from_list(voice_path: &Path, lang: &str, sounds: &[String]) {
    if sounds.is_empty() {
        return;
    }

    let sound_name = sounds.choose(&mut rand::thread_rng()).unwrap();

    match find_sound_file(voice_path, lang, sound_name) {
        Some(path) => {
            debug!("Playing: {:?}", path);
            audio::play_sound(&path);
        }
        None => {
            warn!("Sound not found: {} (lang: {})", sound_name, lang);
        }
    }
}

pub fn speak_text(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }

    if speak_with_piper(text) {
        return true;
    }

    let lang = get_current_language();
    let voice = match lang.as_str() {
        "ru" => "ru-cl",
        "ua" => "uk",
        "en" => "en",
        _ => "en",
    };

    let speed = match lang.as_str() {
        "ru" | "ua" => "125",
        _ => "140",
    };
    let pitch = match lang.as_str() {
        "ru" | "ua" => "38",
        _ => "45",
    };
    let pitch_range = "35";
    let word_gap = "6";

    let try_spawn = |program: &str, args: &[&str]| -> bool {
        match Command::new(program).args(args).spawn() {
            Ok(_) => true,
            Err(e) => {
                debug!("TTS fallback '{}' failed: {}", program, e);
                false
            }
        }
    };

    if try_spawn(
        "espeak-ng",
        &[
            "-v",
            voice,
            "-s",
            speed,
            "-p",
            pitch,
            "-P",
            pitch_range,
            "-g",
            word_gap,
            text,
        ],
    ) {
        return true;
    }

    if try_spawn(
        "espeak",
        &["-v", voice, "-s", speed, "-p", pitch, "-g", word_gap, text],
    ) {
        return true;
    }

    if try_spawn("spd-say", &[text]) {
        return true;
    }

    warn!("No TTS fallback available for text: {}", text);
    false
}

fn speak_with_piper(text: &str) -> bool {
    let piper_bin = find_piper_binary();
    let model_path = find_piper_model();

    let (piper_bin, model_path) = match (piper_bin, model_path) {
        (Some(bin), Some(model)) => (bin, model),
        _ => return false,
    };

    let output_path = temp_piper_output_path();
    let mut child = match Command::new(&piper_bin)
        .arg("--model")
        .arg(&model_path)
        .arg("--output_file")
        .arg(&output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            debug!("Piper TTS spawn failed ({}): {}", piper_bin.display(), e);
            return false;
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(text.as_bytes()) {
            debug!("Piper TTS stdin write failed: {}", e);
            let _ = child.kill();
            let _ = fs::remove_file(&output_path);
            return false;
        }
    }

    let output = match child.wait_with_output() {
        Ok(output) => output,
        Err(e) => {
            debug!("Piper TTS wait failed: {}", e);
            let _ = fs::remove_file(&output_path);
            return false;
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("Piper TTS failed: {}", stderr.trim());
        let _ = fs::remove_file(&output_path);
        return false;
    }

    match fs::metadata(&output_path) {
        Ok(metadata) if metadata.len() > 0 => {
            info!("Speaking with Piper: {}", output_path.display());
            audio::play_sound(&output_path);
            let _ = fs::remove_file(&output_path);
            true
        }
        _ => {
            warn!(
                "Piper TTS did not create a valid audio file: {}",
                output_path.display()
            );
            let _ = fs::remove_file(&output_path);
            false
        }
    }
}

fn find_piper_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PIPER_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let bundled = PathBuf::from("/home/ivan/piper/.venv/bin/piper");
    if bundled.is_file() {
        return Some(bundled);
    }

    Some(PathBuf::from("piper"))
}

fn find_piper_model() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("JARVIS_PIPER_MODEL") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
        warn!(
            "JARVIS_PIPER_MODEL does not point to a file: {}",
            path.display()
        );
    }

    let models_dir = PathBuf::from("/home/ivan/piper");
    let entries = fs::read_dir(&models_dir).ok()?;

    entries.flatten().map(|entry| entry.path()).find(|path| {
        path.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "onnx")
                .unwrap_or(false)
            && path.with_extension("onnx.json").is_file()
    })
}

fn temp_piper_output_path() -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("jarvis-piper-{}-{}.wav", std::process::id(), now))
}

pub fn play(reaction: structs::Reaction) {
    let voice = match get_current_voice() {
        Some(v) => v,
        None => {
            warn!("No current voice set");
            return;
        }
    };

    let lang = get_current_language();

    let reactions = match voice.reactions.get(&lang) {
        Some(r) => r,
        None => {
            warn!("No reactions for language: {}", lang);
            return;
        }
    };

    let sounds = match reaction {
        structs::Reaction::Greet => {
            // try time-specific first
            let time_specific = match time::TimeOfDay::now() {
                time::TimeOfDay::Morning => &reactions.greet_morning,
                time::TimeOfDay::Day => &reactions.greet_day,
                time::TimeOfDay::Evening => &reactions.greet_evening,
                time::TimeOfDay::Night => &reactions.greet_night,
            };

            if time_specific.is_empty() {
                &reactions.greet
            } else {
                time_specific
            }
        }
        structs::Reaction::Reply => &reactions.reply,
        structs::Reaction::Ok => &reactions.ok,
        structs::Reaction::NotFound => &reactions.not_found,
        structs::Reaction::Thanks => &reactions.thanks,
        structs::Reaction::Error => &reactions.error,
        structs::Reaction::Goodbye => &reactions.goodbye,
    };

    play_random_from_list(&voice.path, &lang, sounds);
}

pub fn play_random_from(sounds: &[String]) {
    let voice = match get_current_voice() {
        Some(v) => v,
        None => {
            warn!("No current voice set");
            return;
        }
    };

    let lang = get_current_language();
    play_random_from_list(&voice.path, &lang, sounds);
}

// Play a preview sound for a specific voice
pub fn play_preview(voice_id: &str) {
    let voice = match get_voice(voice_id) {
        Some(v) => v,
        None => {
            warn!("Voice not found for preview: {}", voice_id);
            return;
        }
    };

    let lang = get_current_language();

    let reactions = match voice.reactions.get(&lang) {
        Some(r) => r,
        None => {
            warn!("No reactions for language {} in voice {}", lang, voice_id);
            return;
        }
    };

    // pick from reply, ok, or greet sounds for preview
    let sounds: Vec<&String> = reactions
        .reply
        .iter()
        .chain(reactions.ok.iter())
        .chain(reactions.greet.iter())
        .collect();

    if sounds.is_empty() {
        warn!("No preview sounds for voice: {}", voice_id);
        return;
    }

    let sound_name = sounds.choose(&mut rand::thread_rng()).unwrap();

    if let Some(path) = find_sound_file(&voice.path, &lang, sound_name) {
        debug!("Playing preview: {:?}", path);
        audio::play_sound(&path);
    }
}

// shortcuts
pub fn play_greet() {
    play(structs::Reaction::Greet);
} // app startup
pub fn play_reply() {
    play(structs::Reaction::Reply);
} // wake word detected
pub fn play_ok() {
    play(structs::Reaction::Ok);
} // command executed
pub fn play_not_found() {
    play(structs::Reaction::NotFound);
}
pub fn play_thanks() {
    play(structs::Reaction::Thanks);
}
pub fn play_error() {
    play(structs::Reaction::Error);
}
pub fn play_goodbye() {
    play(structs::Reaction::Goodbye);
}
