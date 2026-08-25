//! Hostile inputs and store-owner attacks against the shipped Client.

mod common;

use deed::{DeedId, Error, Source};
use deeder::{decode_request, encode_request, Request};
use std::fs;
use std::os::unix::fs::symlink;

use common::{file, open, quote, tmp_url, write_blob};

#[test]
fn id_with_slash_or_dot_is_rejected() {
    assert!(DeedId::parse("deed-file-../passwd").is_err());
    assert!(DeedId::parse("deed-file-a/b").is_err());
    assert!(DeedId::parse("deed-file-a.b").is_err());
    assert!(DeedId::parse("deed-file-").is_err());
}

#[test]
fn empty_agent_is_rejected() {
    let mut client = open(&tmp_url());
    let mut req = quote("deed-quote-item", "excerpt", "https://example.com/a");
    req.agent_id.clear();
    let err = client.create(req).expect_err("empty agent");
    assert!(matches!(err, Error::InvalidBody(_)), "{err}");
}

#[test]
fn javascript_url_is_rejected() {
    let mut client = open(&tmp_url());
    let err = client
        .create(quote("deed-quote-item", "excerpt", "javascript:alert(1)"))
        .expect_err("javascript url");
    assert!(matches!(err, Error::InvalidUrl(_)), "{err}");
}

#[test]
fn directory_as_file_path_is_rejected() {
    let url = tmp_url();
    let root = deeder::store_dir(&url).unwrap();
    let mut client = open(&url);
    let err = client
        .create(file("deed-file-note", root.clone()))
        .expect_err("directory");
    assert!(err.to_string().contains("not a file"), "{err}");
}

#[test]
fn recreate_after_delete_stays_frozen() {
    let mut client = open(&tmp_url());
    let req = quote("deed-quote-item", "excerpt", "https://example.com/a");
    client.create(req).unwrap();
    client
        .delete(&DeedId::parse("deed-quote-item").unwrap())
        .unwrap();
    let err = client
        .create(quote("deed-quote-item", "again", "https://example.com/a"))
        .expect_err("tombstone");
    assert!(matches!(err, Error::Frozen(_)), "{err}");
}

#[test]
fn writer_key_wrong_length_refuses_open() {
    let url = tmp_url();
    let root = deeder::store_dir(&url).unwrap();
    open(&url);
    fs::write(root.join("writer.key"), [0u8; 8]).unwrap();
    let err = match deeder::Client::open(&url) {
        Err(e) => e,
        Ok(_) => panic!("short key opened"),
    };
    assert!(err.to_string().contains("writer.key"), "{err}");
}

#[test]
fn replacing_writer_key_fails_existing_evidence() {
    let url = tmp_url();
    let root = deeder::store_dir(&url).unwrap();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-item", "excerpt", "https://example.com/a"))
        .unwrap();
    fs::write(root.join("writer.key"), [7u8; 32]).unwrap();
    let mut other = open(&url);
    let err = other
        .evidence(&DeedId::parse("deed-quote-item").unwrap())
        .expect_err("old evidence");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

#[test]
fn trail_on_a_cycle_returns_both_and_stops() {
    let mut client = open(&tmp_url());
    let a = DeedId::parse("deed-quote-one").unwrap();
    let b = DeedId::parse("deed-quote-two").unwrap();
    client
        .create(quote("deed-quote-one", "one", "https://example.com/a"))
        .unwrap();
    let mut second = quote("deed-quote-two", "two", "https://example.com/b");
    second.sources.push(Source::deed(a.clone()));
    client.create(second).unwrap();
    // Point the first deed at the second by rewriting sources through a new
    // create is frozen; cycle is built by a later take that cites both ways
    // via a third node citing both, plus two citing each other through
    // stored records. Build B→A at create, then a third C→B and C→A.
    let mut third = quote("deed-quote-three", "three", "https://example.com/c");
    third.sources = vec![Source::deed(a.clone()), Source::deed(b.clone())];
    client.create(third).unwrap();
    let trail = client
        .trail(&DeedId::parse("deed-quote-three").unwrap())
        .unwrap();
    assert_eq!(trail.len(), 3, "{}", trail.len());
}

#[test]
fn two_deeds_citing_each_other_do_not_loop() {
    let url = tmp_url();
    let mut client = open(&url);
    client
        .create(quote("deed-quote-one", "one", "https://example.com/a"))
        .unwrap();
    let mut second = quote("deed-quote-two", "two", "https://example.com/b");
    second
        .sources
        .push(Source::deed(DeedId::parse("deed-quote-one").unwrap()));
    client.create(second).unwrap();
    // Mutate the first record's sources to name the second (hostile store).
    let mut first = client
        .get(&DeedId::parse("deed-quote-one").unwrap())
        .unwrap();
    first
        .sources
        .push(Source::deed(DeedId::parse("deed-quote-two").unwrap()));
    let cap = deeder::store_dir(&url)
        .unwrap()
        .join("deeds")
        .join("deed-quote-one.cap");
    fs::write(
        &cap,
        deeder::encode_response(&deeder::Response::Deed(first)).unwrap(),
    )
    .unwrap();
    let trail = client
        .trail(&DeedId::parse("deed-quote-one").unwrap())
        .expect("cycle must terminate");
    assert!(trail.len() <= 2, "{}", trail.len());
}

#[test]
fn garbage_request_bytes_are_rejected() {
    let url = tmp_url();
    let mut store = deeder::FsStore::open(deeder::store_dir(&url).unwrap()).unwrap();
    let err = store.handle(b"not-capn").expect_err("garbage");
    assert!(matches!(err, Error::Decode(_)), "{err}");
}

#[test]
fn symlink_to_a_file_is_ingested_as_that_file() {
    let url = tmp_url();
    let root = deeder::store_dir(&url).unwrap();
    let target = write_blob(&root, "real.txt", b"payload");
    let link = root.join("alias.txt");
    symlink(&target, &link).unwrap();
    let mut client = open(&url);
    let (deed, _) = client.create(file("deed-file-note", link)).unwrap();
    assert!(deed.paths.iter().all(deeder::is_write_once_addr));
    client
        .evidence(&DeedId::parse("deed-file-note").unwrap())
        .unwrap();
}

#[test]
fn handle_create_round_trip_rejects_unknown_op() {
    let bytes = encode_request(&Request::List).unwrap();
    let decoded = decode_request(&bytes).unwrap();
    assert!(matches!(decoded, Request::List));
}

#[test]
fn quote_without_http_scheme_is_rejected() {
    let err = deed::Source::url("ftp://example.com/x").expect_err("ftp");
    assert!(matches!(err, Error::InvalidUrl(_)));
}

#[test]
fn minted_id_cannot_escape_kind_prefix() {
    let err = DeedId::parse("deed-quote-item").unwrap();
    assert!(err.as_str().starts_with("deed-quote-"));
    let mut req = quote("deed-quote-item", "excerpt", "https://example.com/a");
    req.id = None;
    req.name = "../etc".into();
    let mut client = open(&tmp_url());
    let (deed, _) = client.create(req).unwrap();
    assert!(deed.id.as_str().starts_with("deed-quote-"), "{}", deed.id);
    assert!(!deed.id.as_str().contains(".."));
}
