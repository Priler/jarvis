# wake_positive/ — Wake-Word Positive Corpus

Real recordings that are expected to trigger a wake-word detection.

## Subcategories

| Directory | Content | Minimum count |
|-----------|---------|---------------|
| `clean/` | Clear speech, 1 m, quiet room | 50 |
| `quiet/` | Low volume, speaker far or soft | 50 |
| `fast_speech/` | Above-average speech rate | 50 |
| `far_field/` | 2–4 m distance from microphone | 50 |
| `noisy/` | With background noise (TV, music) | 50 |

## Wake Phrases

Record variations of these phrases:
- "джарвис открой калькулятор"
- "джарвис включи музыку"
- "джарвис стоп"
- "джарвис открой браузер"
- "джарвис какая погода"

Include natural variations (different stress, speed, tone).

## Each file must have a .meta.json sidecar with `expected_wake: true`.
