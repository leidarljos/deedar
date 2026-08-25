use deed::{Body, DeedId, Error};
use deeder::{deed_digest, Client, CreateRequest};

fn tmp_url() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("deeder-digest-{n}"));
    std::fs::create_dir_all(&dir).unwrap();
    format!("file://{}", dir.display())
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
    let root = deeder::store_dir(&url).unwrap();
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
    assert!(
        matches!(err, Error::NotFound(_) | Error::InvalidId(_)),
        "{err:?}"
    );
}

#[test]
fn product_path_shared_by_two_deeds_does_not_pick_one() {
    let url = tmp_url();
    let root = deeder::store_dir(&url).unwrap();
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
    assert!(
        matches!(err, Error::InvalidId(_)) || err.to_string().contains("deeds"),
        "{err}"
    );
    assert_eq!(client.resolve(&deed_digest(&a)).expect("digest a"), a.id);
    assert_eq!(client.resolve(&deed_digest(&b)).expect("digest b"), b.id);
}
