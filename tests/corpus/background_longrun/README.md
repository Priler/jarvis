# background_longrun/ — Long-Run False Positive Corpus

8–20 hours of continuous background audio for `false_wakes_per_hour` measurement.

## Content Requirements

- Podcasts (Russian preferred)
- Streams / YouTube commentary
- TV audio (news, shows)
- Gaming content (voice commentary)
- Ambient room audio
- Office noise
- City / outdoor noise

## Rules

- ALL files must have `expected_wake: false` in their `.meta.json`
- Files WITH the wake phrase must NOT be placed here (use wake_positive/)
- Prefer long files (30 min – 2 hours) to reduce subprocess overhead
- Total duration >= 8 hours required for valid FP/hour measurement

## Target metric

```
false_wakes_per_hour < 1.0   (production target)
false_wakes_per_hour < 3.0   (limited-ready target)
```
