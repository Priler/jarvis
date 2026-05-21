# wake_negative/ — Wake-Word Negative Corpus

Real recordings that must NOT trigger a wake-word detection.
Critical for FAR measurement and false positive certification.

## Subcategories

| Directory | Content | Minimum count |
|-----------|---------|---------------|
| `random_speech/` | General Russian speech, unrelated to wake phrase | 100 |
| `podcasts/` | Podcast clips (Russian podcasts preferred) | 100 |
| `music/` | Speech-heavy music: rap, phonk, interviews | 100 |
| `similar_phonetics/` | Words phonetically similar to "джарвис" | 100 |
| `movies/` | Movie dialogue + soundtrack | 100 |

## Similar Phonetics to Target

These are the highest-risk false positive triggers:
- дарвис, джервис, джон, джей, джар
- дар, парвис, харвис, мартин
- Any word starting with "дж" or "дар"

## Each file must have a .meta.json sidecar with:
- `expected_wake: false`
- `expected_false_positive: true`

## Rules
- NO recordings containing "джарвис" — use wake_positive/ for those
- Minimum 10 seconds per file
- Prefer real-world sources (downloaded/recorded, not synthesised)
