use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use jarvis_core::audio_buffer::AudioRingBuffer;
use jarvis_core::audio_processing;
use jarvis_core::config::structs::NoiseSuppressionBackend;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Realistic frame sizes at 16 kHz.
/// 160 = 10 ms (Rustpotter native), 512 ≈ 32 ms, 1600 = 100 ms.
const FRAME_SIZES: [usize; 3] = [160, 512, 1600];

fn voice_frame(n: usize) -> Vec<i16> {
    // RMS ≈ 1 000 — clearly above the 100.0 energy threshold
    vec![1_000i16; n]
}

fn silent_frame(n: usize) -> Vec<i16> {
    vec![0i16; n]
}

// ── VAD / energy detect ───────────────────────────────────────────────────────

fn bench_vad_detect(c: &mut Criterion) {
    // init() reads DB; without a DB it falls back to the "energy" backend and
    // the default threshold from config::VAD_ENERGY_THRESHOLD.
    audio_processing::vad::init();

    let mut group = c.benchmark_group("vad/detect");

    for &n in &FRAME_SIZES {
        let frame = voice_frame(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("voice", n), &frame, |b, f| {
            b.iter(|| audio_processing::vad::detect(f))
        });

        let frame = silent_frame(n);
        group.bench_with_input(BenchmarkId::new("silence", n), &frame, |b, f| {
            b.iter(|| audio_processing::vad::detect(f))
        });
    }

    group.finish();
}

// ── Gain normalizer ───────────────────────────────────────────────────────────

fn bench_gain_normalizer(c: &mut Criterion) {
    audio_processing::gain_normalizer::init();

    let mut group = c.benchmark_group("gain_normalizer/normalize");

    for &n in &FRAME_SIZES {
        let frame = voice_frame(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &frame, |b, f| {
            b.iter(|| audio_processing::gain_normalizer::normalize(f))
        });
    }

    group.finish();
}

// ── Noise suppression (None / passthrough) ────────────────────────────────────

fn bench_noise_suppression_none(c: &mut Criterion) {
    audio_processing::noise_suppression::init(NoiseSuppressionBackend::None);

    let mut group = c.benchmark_group("noise_suppression/none");

    for &n in &FRAME_SIZES {
        let frame = voice_frame(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &frame, |b, f| {
            b.iter(|| audio_processing::noise_suppression::process(f))
        });
    }

    group.finish();
}

// ── AudioRingBuffer ───────────────────────────────────────────────────────────

fn bench_audio_ring_buffer(c: &mut Criterion) {
    let mut group = c.benchmark_group("audio_ring_buffer");

    // push — steady state: buffer always full, each push evicts the oldest frame
    for &n in &FRAME_SIZES {
        let frame = voice_frame(n);
        group.bench_with_input(BenchmarkId::new("push_full", n), &frame, |b, f| {
            // pre-fill so every iteration exercises the eviction path
            let mut buf = AudioRingBuffer::new(2.0, n, 16_000);
            let cap = 2 * (16_000 / n); // ≈ max_frames
            for _ in 0..cap {
                buf.push(f);
            }
            b.iter(|| buf.push(f))
        });
    }

    // drain — realistic pre-speech buffer: 0.5 s worth of 512-sample frames
    group.bench_function("drain_0.5s_512", |b| {
        b.iter_batched(
            || {
                let mut buf = AudioRingBuffer::new(0.5, 512, 16_000);
                let frame = voice_frame(512);
                for _ in 0..15 {
                    buf.push(&frame);
                }
                buf
            },
            |mut buf| buf.drain_all(),
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ── Full mini-pipeline: VAD → gain → NS ──────────────────────────────────────

fn bench_full_pipeline(c: &mut Criterion) {
    // All three are OnceCell-based; init() is idempotent.
    audio_processing::vad::init();
    audio_processing::gain_normalizer::init();
    audio_processing::noise_suppression::init(NoiseSuppressionBackend::None);

    let frame = voice_frame(512);

    c.bench_function("pipeline/vad+gain+ns/512", |b| {
        b.iter(|| {
            let (is_voice, _) = audio_processing::vad::detect(&frame);
            if is_voice {
                let gained = audio_processing::gain_normalizer::normalize(&frame);
                audio_processing::noise_suppression::process(&gained)
            } else {
                frame.clone()
            }
        })
    });
}

criterion_group!(
    benches,
    bench_vad_detect,
    bench_gain_normalizer,
    bench_noise_suppression_none,
    bench_audio_ring_buffer,
    bench_full_pipeline,
);
criterion_main!(benches);
