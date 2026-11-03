//! Tip evidence walks sources. Trail does not skip a broken edge.
//! `current` walks supersede edges: one tip, a fork, a cycle, a missing deed.

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use deed::{DeedId, Error, Source};
use deedar::{encode_response, Response};

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
        deedar::store_dir(&url)
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
    let root = deedar::store_dir(&url).unwrap();
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
        deedar::store_dir(&url).unwrap().join("bytes").join(hex),
        b"changed",
    )
    .unwrap();
    let err = client
        .evidence(&second.id)
        .expect_err("tampered source bytes");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

/// The operator binary. These tests live in the library crate, and the
/// command's exit and streams are what a seat sees.
fn deedar_bin() -> PathBuf {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let target = match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"),
    };
    target.join(profile).join("deedar")
}

fn deedar(url: &str, args: &[&str]) -> (i32, String, String) {
    let bin = deedar_bin();
    let out = Command::new(&bin)
        .arg("--url")
        .arg(url)
        .args(args)
        .env("DEEDAR_HOST_SIGNING_KEY", "off")
        .env_remove("DEEDAR_URL")
        .output()
        .unwrap_or_else(|e| panic!("run {}: {e}", bin.display()));
    (
        out.status.code().unwrap_or(1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn current_follows_a_straight_chain_to_its_tip() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-a", "start", "https://example.com/a"))
        .unwrap();
    let mut mid = quote("deed-quote-z", "mid", "https://example.com/z");
    mid.supersedes = Some(DeedId::parse("deed-quote-a").unwrap());
    client.create(mid).unwrap();
    let mut tip = quote("deed-quote-m", "tip", "https://example.com/m");
    tip.supersedes = Some(DeedId::parse("deed-quote-z").unwrap());
    client.create(tip).unwrap();

    let (code, out, err) = deedar(&url, &["current", "deed-quote-a"]);
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out.lines().next(),
        Some("id=deed-quote-m kind=quote name=deed-quote-m")
    );
    let (code, out, err) = deedar(&url, &["current", "deed-quote-z"]);
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out.lines().next(),
        Some("id=deed-quote-m kind=quote name=deed-quote-m")
    );
}

#[test]
fn current_lists_both_tips_of_a_fork_in_accession_order() {
    // `m` is the successor file. `a` also supersedes the root, and `a`
    // sorts before `m`, which is neither creation order nor that file.
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-root", "root", "https://example.com/root"))
        .unwrap();
    let mut em = quote("deed-quote-m", "em", "https://example.com/m");
    em.supersedes = Some(DeedId::parse("deed-quote-root").unwrap());
    client.create(em).unwrap();
    client
        .create(quote("deed-quote-a", "ay", "https://example.com/a"))
        .unwrap();
    fs::write(
        deedar::store_dir(&url)
            .unwrap()
            .join("deeds")
            .join("deed-quote-a.supersedes"),
        "deed-quote-root\n",
    )
    .unwrap();

    let (code, out, err) = deedar(&url, &["current", "deed-quote-root"]);
    assert_eq!(code, 0);
    assert_eq!(out, "id=deed-quote-a\nid=deed-quote-m\n");
    assert_eq!(err, "deedar: warning: fork deed-quote-a deed-quote-m\n");
    let (code, out, err) = deedar(&url, &["current", "deed-quote-a"]);
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out.lines().next(),
        Some("id=deed-quote-a kind=quote name=deed-quote-a")
    );
}

#[test]
fn current_reports_each_id_on_a_cycle_once() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-z", "tail", "https://example.com/z"))
        .unwrap();
    let mut ay = quote("deed-quote-a", "ay", "https://example.com/a");
    ay.supersedes = Some(DeedId::parse("deed-quote-z").unwrap());
    client.create(ay).unwrap();
    client
        .create(quote("deed-quote-m", "em", "https://example.com/m"))
        .unwrap();
    let deeds = deedar::store_dir(&url).unwrap().join("deeds");
    fs::write(deeds.join("deed-quote-a.successor"), "deed-quote-m\n").unwrap();
    fs::write(deeds.join("deed-quote-m.successor"), "deed-quote-a\n").unwrap();

    let (code, out, err) = deedar(&url, &["current", "deed-quote-z"]);
    assert_eq!(code, 0);
    assert_eq!(out, "id=deed-quote-a\nid=deed-quote-m\n");
    assert_eq!(err, "deedar: warning: cycle deed-quote-a deed-quote-m\n");
    let again = deedar(&url, &["current", "deed-quote-a"]);
    assert_eq!(again.0, 0);
    assert_eq!(again.1, out);
    assert_eq!(again.2, err);
}

#[test]
fn current_stops_at_the_last_deed_when_the_next_is_missing() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-a", "start", "https://example.com/a"))
        .unwrap();
    let mut mid = quote("deed-quote-m", "mid", "https://example.com/m");
    mid.supersedes = Some(DeedId::parse("deed-quote-a").unwrap());
    client.create(mid).unwrap();
    let mut tip = quote("deed-quote-z", "tip", "https://example.com/z");
    tip.supersedes = Some(DeedId::parse("deed-quote-m").unwrap());
    client.create(tip).unwrap();
    fs::remove_file(
        deedar::store_dir(&url)
            .unwrap()
            .join("deeds")
            .join("deed-quote-m.cap"),
    )
    .unwrap();

    let (code, out, err) = deedar(&url, &["current", "deed-quote-a"]);
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out.lines().next(),
        Some("id=deed-quote-a kind=quote name=deed-quote-a")
    );
    let (code, out, err) = deedar(&url, &["current", "deed-quote-z"]);
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out.lines().next(),
        Some("id=deed-quote-z kind=quote name=deed-quote-z")
    );
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
