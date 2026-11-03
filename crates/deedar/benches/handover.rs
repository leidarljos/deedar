//! What a handover costs as the log grows.
//!
//! Every receipt re-reads the log and re-hashes every leaf, so exporting `m`
//! deeds from a log of `n` entries is `m` full reads and `m * n` hashes. That
//! is visible from the code; this is the number, so the fix is measured
//! against something rather than argued from a reading.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use deed::{Body, DeedId};
use deedar::{Client, CreateRequest, FsStore};

/// Samples in the export group.
///
/// Criterion's default measurement is 5 s. Auto mode keeps a fast iteration on
/// linear sampling, and that sampler warns when its first sample holds one
/// iteration. The count it prints is the one that fits the measurement, rounded
/// down to a multiple of ten.
///
/// Sized from the slow export alone, 70 samples (flat mode, a 64.2 ms
/// iteration), the faster exports still warned: both said to reduce the count
/// to 40, or to raise the measurement to the 5.4 s and 6.8 s the one-iteration
/// schedule had already taken, which leaves the warning in place. At 40 the
/// linear schedule is 820 iterations, so an iteration under 6.1 ms gets at
/// least two of them on the first sample. Past that, and under 125 ms, auto
/// mode measures flat, at least two iterations a sample. The exports in the
/// run sit between 2.2 ms and 64.1 ms.
///
/// The runs are `handover-noise.txt`, on commit
/// 0bca3d786c37ff497b923eddc6bd55a1a0f6afee, Intel Xeon family 6 model 207,
/// 8 cores. Intervals on that run were inside criterion's 0.01 noise threshold,
/// which is why the measurement stays at the default 5 s.
const EXPORT_SAMPLES: usize = 40;

fn store_with(n: usize) -> (tempfile::TempDir, FsStore, Vec<DeedId>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("file://{}", dir.path().display());
    let mut client = Client::open(&url).expect("open");
    let ids: Vec<DeedId> = (0..n)
        .map(|at| {
            let id = format!("deed-quote-bench{at}");
            client
                .create(CreateRequest {
                    id: Some(DeedId::parse(&id).expect("id")),
                    name: id.clone(),
                    sources: Vec::new(),
                    agent_id: "bench".into(),
                    activity_id: None,
                    grants: Vec::new(),
                    body: Body::Quote {
                        edition: "https://example.invalid/a".into(),
                        start: 0,
                        end: 4,
                        excerpt: format!("take {at}"),
                        urls: vec!["https://example.invalid/a".into()],
                    },
                    supersedes: None,
                })
                .expect("create");
            DeedId::parse(&id).expect("id")
        })
        .collect();
    let store = FsStore::open(deedar::store_dir(&url).expect("dir")).expect("store");
    (dir, store, ids)
}

fn receipts(c: &mut Criterion) {
    let mut group = c.benchmark_group("receipt over a log of n");
    // Criterion's defaults, 5 s and 100 samples. An 8 s budget warned for a
    // log of 5000: the mean receipt was 2.8669 ms, and a linear sample of 100
    // needs 14.5 s or else 50 samples. At 5 s, auto mode measures that receipt
    // flat. The run of these settings collected 1800 iterations and did not
    // warn. Its 95% interval was 0.09% of the mean, inside the 0.01 noise
    // threshold. The 8 s run reported 12% severe outliers on that receipt;
    // this one reported one.
    for n in [100usize, 1_000, 5_000] {
        let (_dir, store, ids) = store_with(n);
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| store.receipt(&ids[n / 2]).expect("receipt"))
        });
    }
    group.finish();
}

fn export_many(c: &mut Criterion) {
    let mut group = c.benchmark_group("export m deeds from a log of n");
    // Default 5 s measurement. The sample count is `EXPORT_SAMPLES`.
    group.sample_size(EXPORT_SAMPLES);
    for (n, m) in [(1_000usize, 10usize), (1_000, 100), (5_000, 100)] {
        let (_dir, store, ids) = store_with(n);
        group.throughput(Throughput::Elements(m as u64));
        // Through the store one deed at a time, which is the library's loop.
        group.bench_with_input(
            BenchmarkId::new(format!("export_into each, n={n}"), m),
            &m,
            |b, _| {
                b.iter_batched(
                    || tempfile::tempdir().expect("bag"),
                    |bag| {
                        for id in &ids[..m] {
                            store.export_into(id, bag.path()).expect("export");
                        }
                    },
                    criterion::BatchSize::PerIteration,
                )
            },
        );
        // Through one exporter, which is the command line's loop.
        group.bench_with_input(
            BenchmarkId::new(format!("one exporter, n={n}"), m),
            &m,
            |b, _| {
                b.iter_batched(
                    || tempfile::tempdir().expect("bag"),
                    |bag| {
                        let exporter = store.exporter().expect("exporter");
                        for id in &ids[..m] {
                            exporter.export(id, bag.path()).expect("export");
                        }
                    },
                    criterion::BatchSize::PerIteration,
                )
            },
        );
    }
    group.finish();
}

criterion_group!(benches, receipts, export_many);
criterion_main!(benches);
