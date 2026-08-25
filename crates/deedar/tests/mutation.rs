//! Mutate on-disk store files. Evidence and get must fail closed.

mod common;

use std::fs;

use deed::{DeedId, Error};
use deedar::{encode_response, Response};

use common::{file, open, quote, tmp_url, write_blob};

fn deeds_dir(url: &str) -> std::path::PathBuf {
    deedar::store_dir(url).unwrap().join("deeds")
}

fn bytes_dir(url: &str) -> std::path::PathBuf {
    deedar::store_dir(url).unwrap().join("bytes")
}

fn hex_of(deed_paths: &[std::path::PathBuf]) -> String {
    deed_paths[0]
        .to_str()
        .and_then(|s| s.strip_prefix("sha256:"))
        .expect("write-once")
        .to_string()
}

#[test]
fn flipped_deed_record_fails_evidence() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-item", "excerpt", "https://example.com/a"))
        .unwrap();
    let id = DeedId::parse("deed-quote-item").unwrap();
    let mut deed = client.get(&id).unwrap();
    deed.name = "mutated".into();
    fs::write(
        deeds_dir(&url).join("deed-quote-item.cap"),
        encode_response(&Response::Deed(deed)).unwrap(),
    )
    .unwrap();
    let err = client.evidence(&id).expect_err("mutated deed");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

#[test]
fn flipped_evidence_bytes_fail_evidence() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-item", "excerpt", "https://example.com/a"))
        .unwrap();
    let path = deeds_dir(&url).join("deed-quote-item.evidence");
    let mut raw = fs::read(&path).unwrap();
    let i = raw.len() - 1;
    raw[i] ^= 0xff;
    fs::write(&path, raw).unwrap();
    let err = client
        .evidence(&DeedId::parse("deed-quote-item").unwrap())
        .expect_err("mutated evidence");
    assert!(
        matches!(err, Error::Evidence(_) | Error::Decode(_)),
        "{err}"
    );
}

#[test]
fn flipped_product_bytes_fail_evidence() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let blob = write_blob(&root, "note.md", b"a short note\n");
    let mut client = open(&url);
    let (deed, _) = client.create(file("deed-file-note", blob)).unwrap();
    let hex = hex_of(&deed.paths);
    fs::write(bytes_dir(&url).join(&hex), b"changed").unwrap();
    let err = client
        .evidence(&DeedId::parse("deed-file-note").unwrap())
        .expect_err("mutated bytes");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

#[test]
fn missing_product_bytes_fail_evidence() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let blob = write_blob(&root, "note.md", b"a short note\n");
    let mut client = open(&url);
    let (deed, _) = client.create(file("deed-file-note", blob)).unwrap();
    let hex = hex_of(&deed.paths);
    fs::remove_file(bytes_dir(&url).join(&hex)).unwrap();
    let err = client
        .evidence(&DeedId::parse("deed-file-note").unwrap())
        .expect_err("missing bytes");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

#[test]
fn truncated_deed_file_fails_get() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-item", "excerpt", "https://example.com/a"))
        .unwrap();
    fs::write(deeds_dir(&url).join("deed-quote-item.cap"), b"xx").unwrap();
    let err = client
        .get(&DeedId::parse("deed-quote-item").unwrap())
        .expect_err("truncated");
    assert!(matches!(err, Error::Decode(_)), "{err}");
}

#[test]
fn copied_evidence_onto_another_deed_fails() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-one", "one", "https://example.com/a"))
        .unwrap();
    client
        .create(quote("deed-quote-two", "two", "https://example.com/b"))
        .unwrap();
    let src = deeds_dir(&url).join("deed-quote-one.evidence");
    let dst = deeds_dir(&url).join("deed-quote-two.evidence");
    fs::copy(&src, &dst).unwrap();
    let err = client
        .evidence(&DeedId::parse("deed-quote-two").unwrap())
        .expect_err("copied evidence");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

#[test]
fn swapped_evidence_files_fail_both() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-one", "one", "https://example.com/a"))
        .unwrap();
    client
        .create(quote("deed-quote-two", "two", "https://example.com/b"))
        .unwrap();
    let a = deeds_dir(&url).join("deed-quote-one.evidence");
    let b = deeds_dir(&url).join("deed-quote-two.evidence");
    let tmp = deeds_dir(&url).join("swap.tmp");
    fs::rename(&a, &tmp).unwrap();
    fs::rename(&b, &a).unwrap();
    fs::rename(&tmp, &b).unwrap();
    let e1 = client
        .evidence(&DeedId::parse("deed-quote-one").unwrap())
        .expect_err("a");
    let e2 = client
        .evidence(&DeedId::parse("deed-quote-two").unwrap())
        .expect_err("b");
    assert!(matches!(e1, Error::Evidence(_)), "{e1}");
    assert!(matches!(e2, Error::Evidence(_)), "{e2}");
}

#[test]
fn swapped_cap_and_evidence_pairs_must_not_verify_under_the_filename() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-one", "one", "https://example.com/a"))
        .unwrap();
    client
        .create(quote("deed-quote-two", "two", "https://example.com/b"))
        .unwrap();
    let dir = deeds_dir(&url);
    for ext in ["cap", "evidence"] {
        let a = dir.join(format!("deed-quote-one.{ext}"));
        let b = dir.join(format!("deed-quote-two.{ext}"));
        let tmp = dir.join(format!("swap.{ext}"));
        fs::rename(&a, &tmp).unwrap();
        fs::rename(&b, &a).unwrap();
        fs::rename(&tmp, &b).unwrap();
    }
    let id = DeedId::parse("deed-quote-one").unwrap();
    if let Ok(d) = client.get(&id) {
        assert_eq!(
            d.id, id,
            "get must not return a deed whose id is not the asked accession"
        );
    }
    assert!(
        client.evidence(&id).is_err(),
        "swapped pair must not pass evidence"
    );
}

#[test]
fn missing_evidence_file_fails_evidence() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-item", "excerpt", "https://example.com/a"))
        .unwrap();
    fs::remove_file(deeds_dir(&url).join("deed-quote-item.evidence")).unwrap();
    let err = client
        .evidence(&DeedId::parse("deed-quote-item").unwrap())
        .expect_err("missing evidence");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

#[test]
fn extra_tomb_without_delete_hides_the_deed() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-item", "excerpt", "https://example.com/a"))
        .unwrap();
    fs::write(deeds_dir(&url).join("deed-quote-item.tomb"), b"tomb").unwrap();
    let id = DeedId::parse("deed-quote-item").unwrap();
    assert!(matches!(client.get(&id), Err(Error::Tombstoned(_))));
    assert!(matches!(client.evidence(&id), Err(Error::Tombstoned(_))));
}
