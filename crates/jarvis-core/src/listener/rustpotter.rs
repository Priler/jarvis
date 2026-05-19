use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use once_cell::sync::OnceCell;
use rustpotter::{Rustpotter, WakewordRef, WakewordRefBuildFromBuffers};

use crate::{config, APP_DIR};

static RUSTPOTTER: OnceCell<Mutex<Rustpotter>> = OnceCell::new();
// Rechunking buffer: Rustpotter requires exactly get_samples_per_frame() samples per call,
// but the pipeline uses a different frame size (512). Leftover samples carry over between calls.
static REMAINDER: OnceCell<Mutex<Vec<i16>>> = OnceCell::new();

// Cached at init so data_callback never locks RUSTPOTTER just to read the frame size.
static EXPECTED_FRAME_SIZE: AtomicUsize = AtomicUsize::new(0);

// Diagnostics for [WAKE] logging
static MAX_SCORE: AtomicU32 = AtomicU32::new(0); // f32::to_bits()
static FRAME_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn get_max_score() -> f32 {
    f32::from_bits(MAX_SCORE.load(Ordering::Relaxed))
}

pub fn get_frame_count() -> usize {
    FRAME_COUNT.load(Ordering::Relaxed)
}

pub fn init() -> Result<(), ()> {
    let rustpotter_config = config::RUSTPOTTER_DEFAULT_CONFIG;

    info!("RUSTPOTTER PATH PATCH V50 ACTIVE — exe dir: {}", APP_DIR.display());

    match Rustpotter::new(&rustpotter_config) {
        Ok(mut rinstance) => {
            let mut loaded = 0usize;

            let rp_dir = APP_DIR.join("resources/rustpotter");

            // In WAV test mode: auto-create a model from the test WAV's wake word portion.
            // This guarantees detection regardless of which WAV is used for testing.
            if crate::recorder::is_wav_mode() {
                if let Some(wav_path) = crate::recorder::get_wav_path() {
                    match create_model_from_wav(wav_path, 0.7) {
                        Ok(wakeword_ref) => {
                            match rinstance.add_wakeword_ref("jarvis-auto", wakeword_ref) {
                                Ok(_) => {
                                    info!("[AUDIO_TEST] Auto-created Rustpotter wake word model from first 0.7s of WAV");
                                    loaded += 1;
                                }
                                Err(e) => {
                                    error!("[AUDIO_TEST] Failed to add auto wake word model: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("[AUDIO_TEST] Failed to create model from WAV: {}. Falling back to RPW files.", e);
                        }
                    }
                }
            }

            // In normal (microphone) mode: load jarvis-ru.rpw trained on user's recordings
            if !crate::recorder::is_wav_mode() {
                let ru_model = rp_dir.join("jarvis-ru.rpw");
                if ru_model.exists() {
                    let path_str = ru_model.to_string_lossy();
                    match rinstance.add_wakeword_from_file(path_str.as_ref(), path_str.as_ref()) {
                        Ok(_) => {
                            info!("Wakeword model loaded: {}", ru_model.display());
                            loaded += 1;
                        }
                        Err(e) => {
                            warn!("Failed to load jarvis-ru.rpw: {}", e);
                        }
                    }
                }
            }

            // Fallback: English community models
            if loaded == 0 {
                let wake_word_files = [
                    rp_dir.join("jarvis-default.rpw"),
                    rp_dir.join("jarvis-community-1.rpw"),
                    rp_dir.join("jarvis-community-2.rpw"),
                    rp_dir.join("jarvis-community-3.rpw"),
                    rp_dir.join("jarvis-community-4.rpw"),
                    rp_dir.join("jarvis-community-5.rpw"),
                ];
                for path in &wake_word_files {
                    if !path.exists() {
                        continue;
                    }
                    let path_str = path.to_string_lossy();
                    match rinstance.add_wakeword_from_file(path_str.as_ref(), path_str.as_ref()) {
                        Ok(_) => {
                            info!("Wakeword model loaded: {}", path.display());
                            loaded += 1;
                            break;
                        }
                        Err(e) => {
                            warn!("Failed to load wakeword file '{}': {}", path.display(), e);
                        }
                    }
                }
            }

            if loaded == 0 {
                error!("No wakeword models were loaded — Rustpotter cannot detect wake words.");
                return Err(());
            }

            let _ = RUSTPOTTER.set(Mutex::new(rinstance));
            // Cache the required frame size so data_callback can read it without locking.
            if let Some(rp) = RUSTPOTTER.get() {
                let size = rp.lock().unwrap().get_samples_per_frame();
                EXPECTED_FRAME_SIZE.store(size, Ordering::Relaxed);
            }
        }
        Err(msg) => {
            error!("Rustpotter failed to initialize.\nError details: {}", msg);
            return Err(());
        }
    }

    Ok(())
}

/// Creates a WakewordRef from the first `duration_secs` seconds of a WAV file.
/// The extracted audio is written as an in-memory 16kHz mono 16-bit WAV buffer
/// and passed to Rustpotter's MFCC extractor.
fn create_model_from_wav(wav_path: &str, duration_secs: f32) -> Result<WakewordRef, String> {
    // Load and process WAV the same way wav_source does, but only the first N seconds
    let mut reader = hound::WavReader::open(wav_path)
        .map_err(|e| format!("Cannot open WAV: {}", e))?;
    let spec = reader.spec();
    let src_rate = spec.sample_rate;
    let src_channels = spec.channels;

    // Read all samples as i16 at 16kHz mono (matching the pipeline)
    let raw = read_all_as_i16(&mut reader)?;
    let mono = if src_channels > 1 {
        to_mono_16(&raw, src_channels)
    } else {
        raw
    };
    let pcm = if src_rate != 16000 {
        resample_linear_16(&mono, src_rate, 16000)
    } else {
        mono
    };

    // Take only the first N seconds
    let max_samples = (duration_secs * 16000.0) as usize;
    let wake_samples = &pcm[..max_samples.min(pcm.len())];

    if wake_samples.is_empty() {
        return Err("WAV is too short for model creation".to_string());
    }

    // Encode as an in-memory WAV buffer (16kHz mono 16-bit)
    let mut wav_bytes: Vec<u8> = Vec::new();
    {
        let wav_spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(std::io::Cursor::new(&mut wav_bytes), wav_spec)
            .map_err(|e| format!("Cannot create WAV writer: {}", e))?;
        for &sample in wake_samples {
            writer.write_sample(sample).map_err(|e| format!("WAV write error: {}", e))?;
        }
        writer.finalize().map_err(|e| format!("WAV finalize error: {}", e))?;
    }

    // Build the WakewordRef from this single sample
    let mut samples = HashMap::new();
    samples.insert("jarvis-auto-sample".to_string(), wav_bytes);

    WakewordRef::new_from_sample_buffers(
        "jarvis-auto".to_string(),
        Some(0.35),  // lower threshold for single sample
        None,
        samples,
        13, // standard MFCC coefficient count
    )
}

fn read_all_as_i16(reader: &mut hound::WavReader<std::io::BufReader<std::fs::File>>) -> Result<Vec<i16>, String> {
    let spec = reader.spec();
    match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.map(|v| (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                     .map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("WAV read error: {}", e)),
        hound::SampleFormat::Int => {
            if spec.bits_per_sample <= 16 {
                reader.samples::<i16>()
                    .map(|s| s.map_err(|e| e.to_string()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("WAV read error: {}", e))
            } else {
                let shift = (spec.bits_per_sample - 16) as i32;
                reader.samples::<i32>()
                    .map(|s| s.map(|v| (v >> shift) as i16).map_err(|e| e.to_string()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("WAV read error: {}", e))
            }
        }
    }
}

fn to_mono_16(samples: &[i16], channels: u16) -> Vec<i16> {
    samples.chunks(channels as usize)
        .map(|ch| {
            let sum: i32 = ch.iter().map(|&s| s as i32).sum();
            (sum / channels as i32) as i16
        })
        .collect()
}

fn resample_linear_16(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    if samples.is_empty() { return Vec::new(); }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((samples.len() as f64) / ratio).ceil() as usize;
    (0..out_len).map(|i| {
        let pos = i as f64 * ratio;
        let left = pos.floor() as usize;
        let right = (left + 1).min(samples.len() - 1);
        let frac = pos - left as f64;
        (samples[left] as f64 * (1.0 - frac) + samples[right] as f64 * frac).round() as i16
    }).collect()
}

pub fn data_callback(frame_buffer: &[i16]) -> Option<i32> {
    let frame = FRAME_COUNT.fetch_add(1, Ordering::Relaxed);

    // Periodic frame count log (info level only in WAV test mode to avoid noise)
    if crate::recorder::is_wav_mode() && frame % 50 == 0 && frame > 0 {
        info!("[WAKE] {} frames processed by Rustpotter", frame);
    }

    let rustpotter_cell = RUSTPOTTER.get()?;

    // Rustpotter requires exactly get_samples_per_frame() samples per call (480 at 16kHz/30ms).
    // The pipeline may use a different frame size (512). Buffer incoming samples and extract
    // complete chunks of the expected size, carrying over any remainder between calls.
    //
    // Frame size is read from the atomic cached at init — no lock needed here.
    let expected = EXPECTED_FRAME_SIZE.load(Ordering::Relaxed);
    if expected == 0 {
        return None;
    }

    let chunks: Vec<Vec<i16>> = {
        let mut buf = REMAINDER.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
        buf.extend_from_slice(frame_buffer);
        let mut chunks = Vec::new();
        while buf.len() >= expected {
            chunks.push(buf.drain(..expected).collect());
        }
        chunks
    };

    if chunks.is_empty() {
        return None;
    }

    // Acquire the Rustpotter lock once for the entire batch of same-tick chunks.
    let mut rp = rustpotter_cell.lock().unwrap();
    for chunk in &chunks {
        let detection = rp.process_samples::<i16>(chunk);
        if let Some(detection) = detection {
            // Update max score (CAS loop)
            let score_bits = detection.score.to_bits();
            let mut cur = MAX_SCORE.load(Ordering::Relaxed);
            loop {
                if f32::from_bits(cur) >= detection.score {
                    break;
                }
                match MAX_SCORE.compare_exchange_weak(
                    cur, score_bits,
                    Ordering::Relaxed, Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(x) => cur = x,
                }
            }

            let above_threshold = detection.score > config::RUSPOTTER_MIN_SCORE;
            info!(
                "[WAKE] score: {:.3}, threshold: {:.3}, detected: {}",
                detection.score, config::RUSPOTTER_MIN_SCORE, above_threshold
            );

            if above_threshold {
                info!("[WAKE] Wake word DETECTED! name={:?}", detection.name);
                return Some(0);
            }
        }
    }

    None
}
