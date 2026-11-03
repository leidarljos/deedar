mod common;

use deed::{Body, DeedId, Error};
use deedar::{deed_digest, Client, CreateRequest};

fn tmp_url() -> String {
    format!("file://{}", common::scratch("deedar-digest").display())
}

fn file_req(id: &str, path: std::path::PathBuf) -> CreateRequest {
    CreateRequest {
        id: Some(DeedId::parse(id).unwrap()),
        name: "digest sitting".into(),
        sources: Vec::new(),
        agent_id: "digest".into(),
        activity_id: None,
        grants: Vec::new(),
        body: Body::File {
            path,
            media_type: Some("text/plain".into()),
        },
        supersedes: None,
    }
}

#[test]
fn get_and_resolve_accept_slug_deed_digest_and_product_path() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let sitting = root.join("sitting.txt");
    std::fs::write(&sitting, b"digest sitting\n").unwrap();

    let mut client = Client::open(&url).expect("open");
    let id = DeedId::parse("deed-file-lane-digest").unwrap();
    let (deed, _) = client
        .create(file_req("deed-file-lane-digest", sitting))
        .expect("create");
    assert_eq!(deed.id, id);

    let by_slug = client.get(&id).expect("get slug");
    assert_eq!(by_slug.id, id);
    assert_eq!(by_slug.name, "digest sitting");

    let digest = deed_digest(&deed);
    assert_eq!(client.resolve(&digest).expect("resolve digest"), id);

    let product = deed
        .paths
        .iter()
        .find_map(|p| p.to_str().filter(|s| s.starts_with("sha256:")))
        .expect("product path")
        .to_string();
    assert_eq!(client.resolve(&product).expect("resolve product"), id);

    let resolved = client.resolve(&digest).expect("resolve digest again");
    let got = client.get(&resolved).expect("get digest");
    assert_eq!(got.id, id);

    let missing = format!("sha256:{}", "ab".repeat(32));
    let err = client.resolve(&missing).expect_err("random digest");
    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
}

#[test]
fn product_path_shared_by_two_deeds_does_not_pick_one() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let a_path = root.join("a.txt");
    let b_path = root.join("b.txt");
    std::fs::write(&a_path, b"same bytes\n").unwrap();
    std::fs::write(&b_path, b"same bytes\n").unwrap();

    let mut client = Client::open(&url).expect("open");
    let (a, _) = client
        .create(file_req("deed-file-first", a_path))
        .expect("create a");
    let (b, _) = client
        .create(file_req("deed-file-second", b_path))
        .expect("create b");
    assert_eq!(a.paths, b.paths);
    let product = a.paths[0].to_str().expect("path").to_string();

    let err = client.resolve(&product).expect_err("shared product path");
    match err {
        Error::InvalidId(msg) => assert!(msg.contains("names 2 deeds"), "{msg}"),
        other => panic!("expected InvalidId, got {other:?}"),
    }
    assert_eq!(client.resolve(&deed_digest(&a)).expect("digest a"), a.id);
    assert_eq!(client.resolve(&deed_digest(&b)).expect("digest b"), b.id);
}

#[test]
fn tombstoned_digest_stays_not_found_while_the_log_still_names_it() {
    let url = tmp_url();
    let mut client = Client::open(&url).expect("open");
    let (deed, _) = client
        .create(quote_req(
            "deed-quote-tomb",
            "gone after delete",
            "https://example.invalid/a",
        ))
        .expect("create");
    let digest = deed_digest(&deed);
    client.delete(&deed.id).expect("delete");

    let still_logged = client
        .log_entries()
        .expect("log")
        .into_iter()
        .any(|entry| entry.id == deed.id.to_string() && entry.digest == digest);
    assert!(still_logged, "delete dropped the log line");

    let err = client.resolve(&digest).expect_err("tombstoned digest");
    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
}

#[test]
fn unlogged_digest_resolves_and_a_logged_one_still_does() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let mut client = Client::open(&url).expect("open");
    let (older, _) = client
        .create(quote_req(
            "deed-quote-older",
            "written before the log was dropped",
            "https://example.invalid/a",
        ))
        .expect("create older");
    let older_digest = deed_digest(&older);
    std::fs::remove_file(root.join("log")).expect("remove the log");

    let (newer, _) = client
        .create(quote_req(
            "deed-quote-newer",
            "written once the log existed again",
            "https://example.invalid/a",
        ))
        .expect("create newer");
    let newer_digest = deed_digest(&newer);

    let audit = client.log_audit().expect("audit");
    assert!(
        audit.unlogged.iter().any(|id| id == "deed-quote-older"),
        "{audit:?}"
    );
    assert!(
        !audit.unlogged.iter().any(|id| id == "deed-quote-newer"),
        "{audit:?}"
    );

    assert_eq!(
        client.resolve(&older_digest).expect("unlogged digest"),
        older.id
    );
    assert_eq!(
        client.resolve(&newer_digest).expect("logged digest"),
        newer.id
    );
}

fn quote_req(id: &str, excerpt: &str, url: &str) -> CreateRequest {
    CreateRequest {
        id: Some(DeedId::parse(id).unwrap()),
        name: id.into(),
        sources: Vec::new(),
        agent_id: "digest".into(),
        activity_id: None,
        grants: Vec::new(),
        body: Body::Quote {
            edition: url.into(),
            start: 0,
            end: excerpt.len() as u64,
            excerpt: excerpt.into(),
            urls: vec![url.into()],
        },
        supersedes: None,
    }
}
