//! Leave copies product bytes and a Content Credentials sidecar.

mod common;

use deed::{DeedId, Error};
use deedar::{imprint_hash, timestamp_req};
use sha2::{Digest, Sha256};

use common::{file, open, quote, tmp_url, write_blob};

fn offline() {
    assert!(
        std::env::var("DEEDAR_TSA")
            .ok()
            .filter(|s| !s.is_empty())
            .is_none(),
        "tests must not call a timestamp authority; unset DEEDAR_TSA"
    );
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn hash_field(manifest: &str) -> &str {
    let key = "\"hash\": \"";
    let start = manifest.find(key).expect("hash field") + key.len();
    let end = start + manifest[start..].find('"').expect("hash close");
    &manifest[start..end]
}

#[test]
fn leave_copies_bytes_and_writes_a_matching_manifest() {
    offline();
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let sitting = write_blob(&root, "sitting.txt", b"credentials sitting\n");
    let mut client = open(&url);
    let id = DeedId::parse("deed-file-note").unwrap();
    let (deed, _) = client.create(file("deed-file-note", sitting)).unwrap();

    let dest = root.join("left");
    let out = client.leave(&id, &dest).expect("leave");
    assert_eq!(out, dest.join(id.as_str()));
    assert!(out.join("manifest.json").is_file());

    let mut concat = Vec::new();
    for path in &deed.paths {
        let hex = path
            .to_str()
            .and_then(|s| s.strip_prefix("sha256:"))
            .expect("write-once path");
        let copied = std::fs::read(out.join(hex)).expect("copied bytes");
        concat.extend_from_slice(&copied);
    }
    let want = hex_encode(&Sha256::digest(&concat));
    let manifest = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    assert_eq!(hash_field(&manifest), want);
    assert!(manifest.contains("\"claim_generator\": \"deedar\""));
    assert!(manifest.contains(&format!("\"deed\": \"{id}\"")));
    assert!(manifest.contains("\"kind\": \"file\""));

    let got = client.get(&id).expect("get after leave");
    assert_eq!(got.id, id);
    client.evidence(&id).expect("evidence after leave");

    let tsq = root.join("deeds").join(format!("{id}.tsq"));
    assert!(tsq.is_file(), "leave must write {tsq:?}");
    let ev = std::fs::read(root.join("deeds").join(format!("{id}.evidence"))).unwrap();
    assert_eq!(
        std::fs::read(&tsq).unwrap(),
        timestamp_req(&imprint_hash(&ev))
    );
}

#[test]
fn leave_requests_timestamp_over_evidence_bytes() {
    offline();
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let sitting = write_blob(&root, "sitting.txt", b"leave timestamp sitting\n");
    let mut client = open(&url);
    let id = DeedId::parse("deed-file-note").unwrap();
    client.create(file("deed-file-note", sitting)).unwrap();

    client.leave(&id, &root.join("left")).expect("leave");

    let tsq = root.join("deeds").join("deed-file-note.tsq");
    let tsr = root.join("deeds").join("deed-file-note.tsr");
    assert!(tsq.is_file(), "offline leave writes .tsq");
    assert!(!tsr.exists(), "offline leave does not write .tsr");
    let ev = std::fs::read(root.join("deeds").join("deed-file-note.evidence")).unwrap();
    assert_eq!(
        std::fs::read(&tsq).unwrap(),
        timestamp_req(&imprint_hash(&ev))
    );
    client
        .evidence(&id)
        .expect("evidence after leave timestamp");
}

#[test]
fn leave_of_a_quote_is_invalid_body() {
    let url = tmp_url();
    let dest = deedar::store_dir(&url).unwrap().join("left");
    let mut client = open(&url);
    let (deed, _) = client
        .create(quote("deed-quote-item", "excerpt", "https://example.com/a"))
        .unwrap();
    let err = client.leave(&deed.id, &dest).expect_err("quote leave");
    assert!(matches!(err, Error::InvalidBody(_)), "{err}");
}

#[test]
fn get_and_evidence_succeed_without_leave() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let sitting = write_blob(&root, "sitting.txt", b"credentials sitting\n");
    let mut client = open(&url);
    let id = DeedId::parse("deed-file-note").unwrap();
    client.create(file("deed-file-note", sitting)).unwrap();
    let got = client.get(&id).expect("get");
    assert_eq!(got.id, id);
    client.evidence(&id).expect("evidence");
    assert!(!root.join("deeds").join("manifest.json").exists());
}
