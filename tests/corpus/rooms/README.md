# rooms/ — Room Acoustic Validation Corpus

Per-environment recordings for acoustic room validation.

## Required Environments

| Directory | Environment | Background noise |
|-----------|-------------|-----------------|
| `quiet_room/` | Silent room | None |
| `bedroom/` | Bedroom | Fan, low ambient |
| `tv_background/` | Living room with TV | TV at normal volume |
| `fan_noise/` | Room with fan | Fan white noise |
| `street_noise/` | Near window / outdoor | Street / traffic |
| `music_background/` | Music playing | Music at normal volume |

## Per-Environment Requirements

For each environment:
- 20+ positive wake recordings
- 20+ negative recordings
- Include noise reference recording (10+ sec, no speech)
- Set `room_type` in `.meta.json`

## Purpose

Identifies which acoustic environment degrades wake accuracy most.
Informs adaptive threshold calibration per environment type.
