//! RFC 3161 TimeStampReq over evidence bytes. Offline path only.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use deed::{Body, DeedId};
use deeder::{imprint_hash, timestamp_req, Client, CreateRequest};

const SHA256_OID: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];

fn tmp_url() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("deeder-ts-{n}"));
    fs::create_dir_all(&dir).unwrap();
    format!("file://{}", dir.display())
}

fn write_blob(dir: &std::path::Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, bytes).unwrap();
    p
}

fn file(id: &str, path: PathBuf) -> CreateRequest {
    CreateRequest {
        id: Some(DeedId::parse(id).unwrap()),
        name: id.into(),
        sources: Vec::new(),
        agent_id: "reader".into(),
        activity_id: None,
        grants: Vec::new(),
        body: Body::File {
            path,
            media_type: Some("text/plain".into()),
        },
        supersedes: None,
    }
}

fn offline() {
    assert!(
        std::env::var("DEEDER_TSA")
            .ok()
            .filter(|s| !s.is_empty())
            .is_none(),
        "tests must not call a timestamp authority; unset DEEDER_TSA"
    );
}

#[test]
fn timestamp_req_contains_sha256_oid_and_hash() {
    let mut hash = [0u8; 32];
    for (i, b) in hash.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(7).wrapping_add(3);
    }
    let der = timestamp_req(&hash);
    assert!(
        der.windows(SHA256_OID.len()).any(|w| w == SHA256_OID),
        "{der:02x?}"
    );
    assert!(der.windows(32).any(|w| w == hash.as_slice()), "{der:02x?}");
}

#[test]
fn timestamp_without_tsa_writes_tsq_and_evidence_still_succeeds() {
    offline();
    let url = tmp_url();
    let root = deeder::store_dir(&url).unwrap();
    let blob = write_blob(&root, "note.txt", b"lane timestamp");
    let mut client = Client::open(&url).unwrap();
    let id = DeedId::parse("deed-file-note").unwrap();
    client.create(file("deed-file-note", blob)).unwrap();
    let dest = client.timestamp(&id).expect("timestamp");
    assert_eq!(dest, root.join("deeds").join("deed-file-note.tsq"));
    let tsq = fs::read(&dest).unwrap();
    let ev = fs::read(root.join("deeds").join("deed-file-note.evidence")).unwrap();
    assert_eq!(tsq, timestamp_req(&imprint_hash(&ev)));
    client.evidence(&id).expect("evidence");
}

#[test]
fn get_after_timestamp_still_works() {
    offline();
    let url = tmp_url();
    let root = deeder::store_dir(&url).unwrap();
    let blob = write_blob(&root, "note.txt", b"lane timestamp");
    let mut client = Client::open(&url).unwrap();
    let id = DeedId::parse("deed-file-note").unwrap();
    client.create(file("deed-file-note", blob)).unwrap();
    client.timestamp(&id).expect("timestamp");
    let got = client.get(&id).expect("get");
    assert_eq!(got.id, id);
    assert_eq!(got.name, "deed-file-note");
}
