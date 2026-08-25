//! Tip evidence walks sources. Trail does not skip a broken edge.

mod common;

use std::fs;

use deed::{DeedId, Error, Source};
use deeder::{encode_response, Response};

use common::{file, open, quote, tmp_url, write_blob};

#[test]
fn evidence_succeeds_when_sources_are_clean() {
    let mut client = open(&tmp_url());
    let (first, _) = client
        .create(quote("deed-quote-first", "first", "https://example.com/a"))
        .unwrap();
    let mut later = quote("deed-quote-later", "later", "https://example.com/b");
    later.sources.push(Source::deed(first.id.clone()));
    let (second, _) = client.create(later).unwrap();
    let ev = client.evidence(&second.id).expect("clean trail");
    assert_eq!(ev.deed_id, second.id);
    client.evidence(&first.id).expect("source still evidences");
}

#[test]
fn evidence_fails_when_a_source_record_is_tampered() {
    let url = tmp_url();
    let mut client = open(&url);
    let (first, _) = client
        .create(quote("deed-quote-first", "first", "https://example.com/a"))
        .unwrap();
    let mut later = quote("deed-quote-later", "later", "https://example.com/b");
    later.sources.push(Source::deed(first.id.clone()));
    let (second, _) = client.create(later).unwrap();

    let mut mutated = client.get(&first.id).unwrap();
    mutated.name = "mutated".into();
    fs::write(
        deeder::store_dir(&url)
            .unwrap()
            .join("deeds")
            .join("deed-quote-first.cap"),
        encode_response(&Response::Deed(mutated)).unwrap(),
    )
    .unwrap();

    let err = client.evidence(&second.id).expect_err("tampered source");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

#[test]
fn evidence_fails_when_a_source_is_tombstoned() {
    let mut client = open(&tmp_url());
    let (first, _) = client
        .create(quote("deed-quote-first", "first", "https://example.com/a"))
        .unwrap();
    let mut later = quote("deed-quote-later", "later", "https://example.com/b");
    later.sources.push(Source::deed(first.id.clone()));
    let (second, _) = client.create(later).unwrap();
    client.delete(&first.id).unwrap();
    let err = client.evidence(&second.id).expect_err("tombstoned source");
    assert!(
        matches!(err, Error::Tombstoned(_) | Error::Evidence(_)),
        "{err}"
    );
}

#[test]
fn evidence_fails_when_a_source_is_missing() {
    let mut client = open(&tmp_url());
    let ghost = DeedId::parse("deed-quote-ghost").unwrap();
    let mut later = quote("deed-quote-later", "later", "https://example.com/b");
    later.sources.push(Source::deed(ghost));
    let (second, _) = client.create(later).unwrap();
    let err = client.evidence(&second.id).expect_err("missing source");
    assert!(
        matches!(err, Error::NotFound(_) | Error::Evidence(_)),
        "{err}"
    );
}

#[test]
fn evidence_fails_when_a_source_product_is_tampered() {
    let url = tmp_url();
    let root = deeder::store_dir(&url).unwrap();
    let blob = write_blob(&root, "note.md", b"a short note\n");
    let mut client = open(&url);
    let (first, _) = client.create(file("deed-file-note", blob)).unwrap();
    let mut later = quote("deed-quote-later", "later", "https://example.com/b");
    later.sources.push(Source::deed(first.id.clone()));
    let (second, _) = client.create(later).unwrap();
    let hex = first.paths[0]
        .to_str()
        .and_then(|s| s.strip_prefix("sha256:"))
        .expect("write-once");
    fs::write(
        deeder::store_dir(&url).unwrap().join("bytes").join(hex),
        b"changed",
    )
    .unwrap();
    let err = client
        .evidence(&second.id)
        .expect_err("tampered source bytes");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

#[test]
fn trail_fails_when_a_source_is_missing() {
    let mut client = open(&tmp_url());
    let ghost = DeedId::parse("deed-quote-ghost").unwrap();
    let mut later = quote("deed-quote-later", "later", "https://example.com/b");
    later.sources.push(Source::deed(ghost));
    let (second, _) = client.create(later).unwrap();
    let err = client.trail(&second.id).expect_err("missing source");
    assert!(matches!(err, Error::NotFound(_)), "{err}");
}
