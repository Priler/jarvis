# Jarvis Offline — Production Acoustic Certification Corpus

This directory contains the real-world acoustic corpus for production wake-word certification.

## Structure

```
corpus/
├── wake_positive/       — Real recordings expected to trigger wake (TP measurement)
│   ├── clean/           — Clean speech at ~1 m, no background noise
│   ├── quiet/           — Quiet speech, low volume
│   ├── fast_speech/     — Above-average speech rate
│   ├── far_field/       — 2–4 m distance from microphone
│   └── noisy/           — With background noise (TV, music, conversation)
├── wake_negative/       — Real recordings NOT expected to trigger wake (FP measurement)
│   ├── random_speech/   — General Russian/mixed speech
│   ├── podcasts/        — Podcast audio clips
│   ├── music/           — Music including speech-heavy (rap, interviews)
│   ├── similar_phonetics/ — Words similar to "jarvis": дарвис, джервис, джон, etc.
│   └── movies/          — Movie audio (dialogue + music)
├── background_longrun/  — 8–20 hours of background audio for FP/hour measurement
├── noisy/               — Recordings in noisy environments
├── far_field/           — Far-field recordings
├── quiet_speech/        — Quiet speech edge cases
├── fast_speech/         — Fast speech edge cases
├── multi_speaker/       — Per-speaker directories (speaker_01 … speaker_10+)
├── self_hearing/        — Recordings of the assistant's own TTS voice
├── microphones/         — Per-microphone certification directories
│   ├── intel_smart_sound/
│   ├── motu/
│   ├── usb/
│   └── headset/
├── rooms/               — Per-environment recordings
│   ├── quiet_room/
│   ├── bedroom/
│   ├── tv_background/
│   ├── fan_noise/
│   ├── street_noise/
│   └── music_background/
└── metadata/            — Global metadata schemas and documentation
```

## Recording Requirements

### Format
- Sample rate: 16000 Hz (mono) — matches Rustpotter / Vosk input
- Bit depth: 16-bit PCM
- Format: WAV (RIFF)
- Duration: 3–30 seconds per file (positive), 10–120 seconds (negative), up to hours (background)

### Metadata
Each WAV file **must** have a companion `.meta.json` file.
See `metadata/meta_schema.json` for the schema.

### Minimum corpus size for certification
| Category | Min count |
|----------|-----------|
| wake_positive total | 250 |
| wake_negative total | 500 |
| multi_speaker speakers | 10 |
| background_longrun hours | 8 |
| microphone variants | 4 |
| room variants | 6 |

## Running Corpus Validation

```bash
# From the project root:
cargo run --bin jarvis-app -- corpus-validation tests/corpus \
    --out validation_results/corpus \
    --accelerated
```

## Certification Targets

| Metric | Production target |
|--------|------------------|
| FAR | < 0.05 |
| FRR | < 0.10 |
| False wakes / hour | < 1.0 |

**IMPORTANT:** No fake WAVs. No synthetic data. All recordings must be real.
