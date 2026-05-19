use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Cursor;
use rustpotter::{WakewordRef, WakewordRefBuildFromBuffers, WakewordSave};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: build-rpw <samples_dir> <output.rpw> [max_secs]");
        eprintln!("  Builds a Rustpotter wake word model from all .wav files in <samples_dir>");
        eprintln!("  max_secs: trim each sample to this duration (default: 0.8s)");
        std::process::exit(1);
    }

    let samples_dir = &args[1];
    let output_path = &args[2];
    let max_secs: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.8);

    let mut sample_paths: Vec<String> = fs::read_dir(samples_dir)
        .expect("Cannot read samples directory")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x.eq_ignore_ascii_case("wav"))
                .unwrap_or(false)
        })
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();

    sample_paths.sort();

    if sample_paths.is_empty() {
        eprintln!("No .wav files found in {}", samples_dir);
        std::process::exit(1);
    }

    println!("Building model from {} samples:", sample_paths.len());

    let mut samples: HashMap<String, Vec<u8>> = HashMap::new();

    for path in &sample_paths {
        let name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        println!("  Processing: {}", name);

        let wav_bytes = convert_to_16khz_mono_16bit(path, max_secs)
            .unwrap_or_else(|e| panic!("Failed to convert {}: {}", name, e));

        samples.insert(name, wav_bytes);
    }

    let wakeword_ref = WakewordRef::new_from_sample_buffers(
        "jarvis-ru".to_string(),
        Some(0.5),  // threshold
        None,       // avg_threshold
        samples,
        13,         // standard MFCC coefficient count
    )
    .expect("Failed to build WakewordRef");

    wakeword_ref
        .save_to_file(output_path)
        .expect("Failed to save RPW file");

    println!("Saved model to: {}", output_path);
}

// Convert any WAV (any rate, channels, bit depth) → 16kHz mono 16-bit WAV bytes in memory.
// max_secs: only keep the first N seconds (trims surrounding silence from training samples).
fn convert_to_16khz_mono_16bit(path: &str, max_secs: f32) -> Result<Vec<u8>, String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| format!("Cannot open WAV: {}", e))?;
    let spec = reader.spec();

    let raw = read_as_i16(&mut reader)?;

    // Down-mix to mono
    let mono = if spec.channels > 1 {
        raw.chunks(spec.channels as usize)
            .map(|ch| {
                let sum: i32 = ch.iter().map(|&s| s as i32).sum();
                (sum / spec.channels as i32) as i16
            })
            .collect()
    } else {
        raw
    };

    // Resample to 16000 Hz if needed
    let mut resampled = if spec.sample_rate != 16000 {
        resample_linear(&mono, spec.sample_rate, 16000)
    } else {
        mono
    };

    // Trim to max_secs to remove surrounding silence
    let max_samples = (max_secs * 16000.0) as usize;
    resampled.truncate(max_samples);

    // Encode as in-memory 16kHz mono 16-bit WAV
    let mut wav_bytes: Vec<u8> = Vec::new();
    {
        let wav_spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(Cursor::new(&mut wav_bytes), wav_spec)
            .map_err(|e| format!("Cannot create WAV writer: {}", e))?;
        for &sample in &resampled {
            writer
                .write_sample(sample)
                .map_err(|e| format!("WAV write error: {}", e))?;
        }
        writer
            .finalize()
            .map_err(|e| format!("WAV finalize error: {}", e))?;
    }

    Ok(wav_bytes)
}

fn read_as_i16(reader: &mut hound::WavReader<std::io::BufReader<std::fs::File>>) -> Result<Vec<i16>, String> {
    let spec = reader.spec();
    match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| {
                s.map(|v| (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("WAV read error: {}", e)),
        hound::SampleFormat::Int => {
            if spec.bits_per_sample <= 16 {
                reader
                    .samples::<i16>()
                    .map(|s| s.map_err(|e| e.to_string()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("WAV read error: {}", e))
            } else {
                let shift = (spec.bits_per_sample - 16) as i32;
                reader
                    .samples::<i32>()
                    .map(|s| {
                        s.map(|v| (v >> shift) as i16)
                            .map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("WAV read error: {}", e))
            }
        }
    }
}

fn resample_linear(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    if samples.is_empty() {
        return Vec::new();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((samples.len() as f64) / ratio).ceil() as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let left = pos.floor() as usize;
            let right = (left + 1).min(samples.len() - 1);
            let frac = pos - left as f64;
            (samples[left] as f64 * (1.0 - frac) + samples[right] as f64 * frac).round() as i16
        })
        .collect()
}
