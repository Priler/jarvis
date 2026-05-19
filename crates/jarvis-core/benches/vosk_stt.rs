/// Vosk STT latency benchmarks.
///
/// Run with:
///   cargo bench -p jarvis-core --features vosk --bench vosk_stt
///
/// These benchmarks answer the P2-1 question: does accept_waveform() block
/// the wake-word loop long enough to justify moving STT to a separate thread?
use criterion::{
    criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use vosk::{Model, Recognizer};

// ── Model / recognizer paths ─────────────────────────────────────────────────

const MODEL_RU: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/vosk/vosk-model-small-ru-0.22"
);

const MODEL_EN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/vosk/vosk-model-en-us-0.22-lgraph"
);

// Same grammar Jarvis uses for Russian wake-word detection
const WAKE_GRAMMAR: &[&str] = &["джарвис", "jarvis", "[unk]"];

// ── Helpers ──────────────────────────────────────────────────────────────────

fn load_model(path: &str) -> Model {
    Model::new(path).unwrap_or_else(|| panic!("Failed to load model: {path}"))
}

fn speech_recognizer(model: &Model) -> Recognizer {
    let mut r = Recognizer::new(model, 16000.0)
        .expect("Failed to create speech recognizer");
    r.set_max_alternatives(3);
    r.set_words(false);
    r
}

fn wake_recognizer(model: &Model) -> Recognizer {
    Recognizer::new_with_grammar(model, 16000.0, WAKE_GRAMMAR)
        .expect("Failed to create wake recognizer")
}

fn silence(n: usize) -> Vec<i16> {
    vec![0i16; n]
}

fn noise(n: usize) -> Vec<i16> {
    // Pseudo-random white noise — exercises the full MFCC + Viterbi path.
    // LCG: cheap, no rand dependency.
    let mut state: u32 = 0xDEAD_BEEF;
    (0..n).map(|_| {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        ((state >> 16) as i16).wrapping_shr(2) // RMS ≈ 4096, clearly audible
    }).collect()
}

// ── 1. accept_waveform per pipeline frame ─────────────────────────────────────
//
// This is the hot path: the production code feeds 512-sample chunks one by one.
// If this blocks, wake-word misses accumulate.

fn bench_accept_waveform(c: &mut Criterion) {
    let model_ru = load_model(MODEL_RU);
    let model_en = load_model(MODEL_EN);

    let mut speech_ru = speech_recognizer(&model_ru);
    let mut wake_ru   = wake_recognizer(&model_ru);
    let mut speech_en = speech_recognizer(&model_en);

    let mut group = c.benchmark_group("vosk/accept_waveform");

    for &n in &[512usize, 4096] {
        group.throughput(Throughput::Elements(n as u64));

        let sil = silence(n);
        let nse = noise(n);

        group.bench_with_input(BenchmarkId::new("ru_speech/silence", n),
            &sil, |b, f| b.iter(|| { let _ = speech_ru.accept_waveform(f); }));

        group.bench_with_input(BenchmarkId::new("ru_speech/noise", n),
            &nse, |b, f| b.iter(|| { let _ = speech_ru.accept_waveform(f); }));

        group.bench_with_input(BenchmarkId::new("ru_wake_grammar/silence", n),
            &sil, |b, f| b.iter(|| { let _ = wake_ru.accept_waveform(f); }));

        group.bench_with_input(BenchmarkId::new("ru_wake_grammar/noise", n),
            &nse, |b, f| b.iter(|| { let _ = wake_ru.accept_waveform(f); }));

        group.bench_with_input(BenchmarkId::new("en_speech/noise", n),
            &nse, |b, f| b.iter(|| { let _ = speech_en.accept_waveform(f); }));
    }

    group.finish();
}

// ── 2. Full utterance decode ──────────────────────────────────────────────────
//
// Simulates recognizing a complete command: feed N seconds of audio in one
// call, then force final_result().  This measures worst-case blocking time
// if the pipeline ever feeds large chunks (e.g. pre-roll + utterance at once).

fn bench_full_utterance(c: &mut Criterion) {
    let model_ru = load_model(MODEL_RU);

    let mut group = c.benchmark_group("vosk/full_utterance");
    group.sample_size(50);

    for secs in [0.5f32, 1.0, 2.0] {
        let audio = noise((secs * 16000.0) as usize);
        let label = format!("{secs:.1}s");

        group.bench_with_input(
            BenchmarkId::new("ru_speech", &label),
            &audio,
            |b, a| {
                b.iter_batched(
                    || speech_recognizer(&model_ru),
                    |mut rec| {
                        let _ = rec.accept_waveform(a);
                        let _ = rec.final_result();
                        rec
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_with_input(
            BenchmarkId::new("ru_wake_grammar", &label),
            &audio,
            |b, a| {
                b.iter_batched(
                    || wake_recognizer(&model_ru),
                    |mut rec| {
                        let _ = rec.accept_waveform(a);
                        let _ = rec.final_result();
                        rec
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

// ── 3. Recognizer creation ────────────────────────────────────────────────────
//
// Measures how long it takes to spin up a new Recognizer once the model is
// already loaded.  Relevant for understanding reset() cost vs. allocating fresh.

fn bench_recognizer_creation(c: &mut Criterion) {
    let model = load_model(MODEL_RU);
    let mut group = c.benchmark_group("vosk/new_recognizer");
    group.sample_size(50);

    group.bench_function("speech", |b| {
        b.iter_batched(|| (), |_| speech_recognizer(&model), BatchSize::SmallInput)
    });

    group.bench_function("wake_grammar", |b| {
        b.iter_batched(|| (), |_| wake_recognizer(&model), BatchSize::SmallInput)
    });

    group.finish();
}

// ── 4. Model loading ──────────────────────────────────────────────────────────
//
// One-time startup cost.  Low sample count to avoid multi-minute runs.

fn bench_model_loading(c: &mut Criterion) {
    let mut group = c.benchmark_group("vosk/load_model");
    group.sample_size(10);

    group.bench_function("ru_small", |b| b.iter(|| load_model(MODEL_RU)));
    group.bench_function("en_lgraph", |b| b.iter(|| load_model(MODEL_EN)));

    group.finish();
}

criterion_group!(
    benches,
    bench_accept_waveform,
    bench_full_utterance,
    bench_recognizer_creation,
    bench_model_loading,
);
criterion_main!(benches);
