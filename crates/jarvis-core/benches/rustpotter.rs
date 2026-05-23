use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use rustpotter::{
    AudioFmt, BandPassConfig, DetectorConfig, FiltersConfig, GainNormalizationConfig,
    Rustpotter, RustpotterConfig, ScoreMode,
};

// ── Setup ─────────────────────────────────────────────────────────────────────

const RPW_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/rustpotter/jarvis-default.rpw"
);

fn default_config() -> RustpotterConfig {
    RustpotterConfig {
        fmt: AudioFmt::default(),
        detector: DetectorConfig {
            avg_threshold: 0.,
            threshold: 0.5,
            min_scores: 5,
            score_ref: 0.22,
            band_size: 5,
            vad_mode: None,
            score_mode: ScoreMode::Max,
            eager: false,
        },
        filters: FiltersConfig {
            gain_normalizer: GainNormalizationConfig {
                enabled: false,
                gain_ref: None,
                min_gain: 0.7,
                max_gain: 1.0,
            },
            band_pass: BandPassConfig {
                enabled: false,
                low_cutoff: 80.,
                high_cutoff: 8000.,
            },
        },
    }
}

/// Build a ready-to-use Rustpotter instance with one .rpw model loaded.
fn make_rustpotter() -> Rustpotter {
    let mut rp = Rustpotter::new(&default_config()).expect("Rustpotter::new failed");
    rp.add_wakeword_from_file(RPW_PATH, RPW_PATH)
        .expect("Failed to load .rpw model — ensure resources/rustpotter/jarvis-default.rpw exists");
    rp
}

// ── 1. process_samples: raw hot-path cost per native frame ───────────────────

fn bench_process_samples(c: &mut Criterion) {
    let mut rp = make_rustpotter();
    let frame_size = rp.get_samples_per_frame(); // 480 at 16kHz/30ms

    let mut group = c.benchmark_group("rustpotter/process_samples");
    group.throughput(Throughput::Elements(frame_size as u64));

    // Silence — no detection expected; isolates pure MFCC compute cost.
    let silence = vec![0i16; frame_size];
    group.bench_function("silence", |b| {
        b.iter(|| rp.process_samples::<i16>(&silence))
    });

    // Low-energy voice-like signal (RMS ~1000, clearly above silence).
    let voice = vec![1_000i16; frame_size];
    group.bench_function("voice", |b| {
        b.iter(|| rp.process_samples::<i16>(&voice))
    });

    group.finish();
}

// ── 2. Rechunking: simulate data_callback with a mismatched pipeline frame ───
//
// The live pipeline feeds 512-sample frames; Rustpotter needs 480.
// data_callback rechunks them and calls process_samples once per complete chunk.
// This bench measures the full roundtrip including the Vec bookkeeping.

fn bench_data_callback_simulation(c: &mut Criterion) {
    let mut rp = make_rustpotter();
    let rp_frame = rp.get_samples_per_frame(); // 480

    let mut group = c.benchmark_group("rustpotter/data_callback_sim");

    for pipeline_frame in [160usize, 512, 1600] {
        group.throughput(Throughput::Elements(pipeline_frame as u64));
        group.bench_with_input(
            BenchmarkId::new("pipeline_frame", pipeline_frame),
            &pipeline_frame,
            |b, &n| {
                let input = vec![1_000i16; n];
                let mut remainder: Vec<i16> = Vec::new();
                b.iter(|| {
                    remainder.extend_from_slice(&input);
                    while remainder.len() >= rp_frame {
                        let chunk: Vec<i16> = remainder.drain(..rp_frame).collect();
                        let _ = rp.process_samples::<i16>(&chunk);
                    }
                })
            },
        );
    }

    group.finish();
}

// ── 3. Model loading: one-time startup cost of loading a .rpw file ────────────

fn bench_model_loading(c: &mut Criterion) {
    c.bench_function("rustpotter/load_rpw_model", |b| {
        b.iter_batched(
            || Rustpotter::new(&default_config()).expect("Rustpotter::new failed"),
            |mut rp| {
                rp.add_wakeword_from_file(RPW_PATH, RPW_PATH)
                    .expect("load failed");
                rp
            },
            BatchSize::SmallInput,
        )
    });
}

// ── 4. Initialisation: full Rustpotter::new() cost ───────────────────────────

fn bench_init(c: &mut Criterion) {
    c.bench_function("rustpotter/new", |b| {
        b.iter(|| Rustpotter::new(&default_config()).expect("Rustpotter::new failed"))
    });
}

criterion_group!(
    benches,
    bench_process_samples,
    bench_data_callback_simulation,
    bench_model_loading,
    bench_init,
);
criterion_main!(benches);
