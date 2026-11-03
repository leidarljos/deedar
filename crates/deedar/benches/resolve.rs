//! What a `sha256:` lookup costs as the shelf grows.
//!
//! `resolve` used to read and decode every deed, then hash each one. The log
//! already stores the deed digest beside the accession. This bench times the
//! lookup itself: one digest that is present, one that is not, and one product
//! path. The store is built once per size and then only looked up.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use deed::{Body, DeedId};
use deedar::{deed_digest, Client, CreateRequest};

/// A quote has no product path, so the store is `n` quotes plus one file deed.
/// The file is the product-path variant. Its bytes are fixed, so the path
/// digest does not depend on `n`.
const PRODUCT_BYTES: &[u8] = b"resolve bench product\n";

fn store_of(n: usize) -> (tempfile::TempDir, Client, String, String, String) {
    // A host signing key would write a signature beside every deed. The lookup
    // never reads that file, and a machine that has a key would then build a
    // different store from one that does not.
    std::env::set_var("DEEDAR_HOST_SIGNING_KEY", "off");
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("file://{}", dir.path().display());
    let mut client = Client::open(&url).expect("open");
    let mut present = None;
    for at in 0..n {
        let id = format!("deed-quote-r{at:05}");
        let (deed, _) = client
            .create(CreateRequest {
                id: Some(DeedId::parse(&id).expect("id")),
                name: id,
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
        // The middle deed, so the hit is not the first or last line of the log.
        if at == n / 2 {
            present = Some(deed_digest(&deed));
        }
    }
    let product_file = dir.path().join("product.txt");
    std::fs::write(&product_file, PRODUCT_BYTES).expect("product file");
    let (file_deed, _) = client
        .create(CreateRequest {
            id: Some(DeedId::parse("deed-file-product").expect("id")),
            name: "product".into(),
            sources: Vec::new(),
            agent_id: "bench".into(),
            activity_id: None,
            grants: Vec::new(),
            body: Body::File {
                path: product_file,
                media_type: Some("text/plain".into()),
            },
            supersedes: None,
        })
        .expect("create file");
    let product = file_deed
        .paths
        .iter()
        .find_map(|p| p.to_str().map(str::to_string))
        .expect("product path");
    // Not a digest this store wrote. SHA-256 is 32 octets, so the text form
    // is 64 hex characters (FIPS 180-4).
    let absent = format!("sha256:{}", "ab".repeat(32));
    (dir, client, present.expect("middle deed"), absent, product)
}

/// Samples per lookup.
///
/// Criterion's default is 100 samples over 5 s. One release run of a miss
/// over 10 000 deeds took 59.3 ms, and criterion warned that 100 samples of
/// it need 6.0 s or else 80 samples. Eighty samples of that miss are 4.7 s,
/// inside the 5 s measurement, so every lookup uses 80. The machine was an
/// Intel Xeon family 6 model 207, 8 cores. The before and after figures are
/// `measurements/resolve.md`.
const RESOLVE_SAMPLES: usize = 80;

fn lookups(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolve");
    // One iteration is one lookup, so the rate is lookups per second.
    group.throughput(Throughput::Elements(1));
    group.sample_size(RESOLVE_SAMPLES);
    for n in [100usize, 1_000, 10_000] {
        let (_dir, mut client, present, absent, product) = store_of(n);
        group.bench_with_input(BenchmarkId::new("present", n), &present, |b, digest| {
            b.iter(|| client.resolve(digest).expect("present"))
        });
        group.bench_with_input(BenchmarkId::new("absent", n), &absent, |b, digest| {
            b.iter(|| client.resolve(digest).expect_err("absent"))
        });
        group.bench_with_input(BenchmarkId::new("product", n), &product, |b, digest| {
            b.iter(|| client.resolve(digest).expect("product"))
        });
    }
    group.finish();
}

criterion_group!(benches, lookups);
criterion_main!(benches);
