//! What a handover costs as the log grows.
//!
//! Every receipt re-reads the log and re-hashes every leaf, so exporting `m`
//! deeds from a log of `n` entries is `m` full reads and `m * n` hashes. That
//! is visible from the code; this is the number, so the fix is measured
//! against something rather than argued from a reading.

use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use deed::{Body, DeedId};
use deedar::{Client, CreateRequest, FsStore};

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
    group.measurement_time(Duration::from_secs(8));
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
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);
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
