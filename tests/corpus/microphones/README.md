# microphones/ — Per-Microphone Certification Corpus

Each subdirectory contains recordings from a specific microphone for comparative analysis.

## Required Microphones

| Directory | Device | Notes |
|-----------|--------|-------|
| `intel_smart_sound/` | Intel Smart Sound Technology (built-in laptop mic) | Primary target device |
| `motu/` | MOTU audio interface | High-quality reference mic |
| `usb/` | Generic USB microphone | Common consumer device |
| `headset/` | Headset microphone | Close-mic condition |

## Per-Microphone Requirements

For each microphone, include:
- 25+ positive wake recordings (`expected_wake: true`)
- 25+ negative recordings (`expected_wake: false`)
- Recordings at 1 m, 2 m, and far-field if applicable
- Include `.meta.json` with `microphone` field set to the device name

## Purpose

Identifies which microphone produces the most FP/FN events.
Informs whether microphone-specific threshold tuning is needed.
