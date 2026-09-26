//! Host-issued evidence sidecar beside store HMAC.
//!
//! Two constructions with different claims. The keyed hash says the bytes are
//! the bytes the host saw. The signature says who vouched for them, and is the
//! only one a store can demand, because a keyed-hash verifier holds the key it
//! checks with and can therefore mint what it checks.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use deed::{Body, DeedId, Error};
use deedar::{Client, CreateRequest};

static ENV: Mutex<()> = Mutex::new(());

fn isolate() -> (std::sync::MutexGuard<'static, ()>, String, PathBuf) {
    let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: ENV is held for the whole sitting that reads either variable.
    unsafe {
        std::env::remove_var("DEEDAR_HOST_KEY");
        std::env::remove_var("DEEDAR_HOST_SIGNING_KEY");
    }
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let parent = std::env::temp_dir().join(format!("deedar-host-{n}"));
    let store = parent.join("store");
    fs::create_dir_all(&store).unwrap();
    (guard, format!("file://{}", store.display()), parent)
}

fn sidecar(url: &str, id: &str) -> PathBuf {
    deedar::store_dir(url)
        .unwrap()
        .join("deeds")
        .join(format!("{id}.host"))
}

fn write_key(path: &Path, fill: u8) {
    fs::write(path, [fill; 32]).unwrap();
}

fn open(url: &str) -> Client {
    Client::open(url).unwrap()
}

fn quote(id: &str, excerpt: &str, url: &str) -> CreateRequest {
    CreateRequest {
        id: Some(DeedId::parse(id).unwrap()),
        name: id.into(),
        sources: Vec::new(),
        agent_id: "reader".into(),
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

#[test]
fn create_without_host_key_is_hmac_only() {
    let (_guard, url, parent) = isolate();
    assert!(!parent.join("host.key").exists());
    let mut client = open(&url);
    let id = DeedId::parse("deed-quote-host").unwrap();
    let (deed, issued) = client
        .create(quote(
            "deed-quote-host",
            "hmac only",
            "https://example.com/host",
        ))
        .unwrap();
    assert_eq!(deed.id, id);
    let got = client.get(&id).unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.name, "deed-quote-host");
    let ev = client.evidence(&id).unwrap();
    assert_eq!(ev.deed_id, id);
    assert_eq!(ev.signature, issued.signature);
    assert!(!sidecar(&url, "deed-quote-host").exists());
    client.delete(&id).unwrap();
    assert!(matches!(client.get(&id), Err(Error::Tombstoned(_))));
}

#[test]
fn create_with_host_key_env_writes_sidecar() {
    let (_guard, url, parent) = isolate();
    let key = parent.join("lane.host");
    write_key(&key, 9);
    // SAFETY: ENV is held for the whole sitting that reads DEEDAR_HOST_KEY.
    unsafe {
        std::env::set_var("DEEDAR_HOST_KEY", &key);
    }
    let mut client = open(&url);
    let id = DeedId::parse("deed-quote-host").unwrap();
    let (deed, issued) = client
        .create(quote(
            "deed-quote-host",
            "host issued",
            "https://example.com/host",
        ))
        .unwrap();
    assert_eq!(deed.id, id);
    let host = sidecar(&url, "deed-quote-host");
    assert_eq!(fs::read(&host).unwrap().len(), 32);
    let got = client.get(&id).unwrap();
    assert_eq!(got.id, id);
    let ev = client.evidence(&id).unwrap();
    assert_eq!(ev.deed_id, id);
    assert_eq!(ev.signature, issued.signature);
    client.delete(&id).unwrap();
    assert!(matches!(client.get(&id), Err(Error::Tombstoned(_))));
}

#[test]
fn tampered_host_sidecar_fails_evidence() {
    let (_guard, url, parent) = isolate();
    let key = parent.join("lane.host");
    write_key(&key, 3);
    // SAFETY: ENV is held for the whole sitting that reads DEEDAR_HOST_KEY.
    unsafe {
        std::env::set_var("DEEDAR_HOST_KEY", &key);
    }
    let mut client = open(&url);
    let id = DeedId::parse("deed-quote-host").unwrap();
    client
        .create(quote(
            "deed-quote-host",
            "tamper host",
            "https://example.com/host",
        ))
        .unwrap();
    let path = sidecar(&url, "deed-quote-host");
    let mut raw = fs::read(&path).unwrap();
    raw[0] ^= 0xff;
    fs::write(&path, raw).unwrap();
    let err = client.evidence(&id).expect_err("tampered host");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
    assert_eq!(client.get(&id).unwrap().id, id);
}

#[test]
fn hmac_only_deed_survives_later_host_key() {
    let (_guard, url, parent) = isolate();
    let mut client = open(&url);
    let id = DeedId::parse("deed-quote-host").unwrap();
    client
        .create(quote(
            "deed-quote-host",
            "before host",
            "https://example.com/host",
        ))
        .unwrap();
    assert!(!sidecar(&url, "deed-quote-host").exists());
    drop(client);

    write_key(&parent.join("host.key"), 7);
    let key = parent.join("lane.host");
    write_key(&key, 11);
    // SAFETY: ENV is held for the whole sitting that reads DEEDAR_HOST_KEY.
    unsafe {
        std::env::set_var("DEEDAR_HOST_KEY", &key);
    }
    let mut client = open(&url);
    let got = client.get(&id).expect("open and get still work");
    assert_eq!(got.id, id);
    assert!(!sidecar(&url, "deed-quote-host").exists());
    let err = client
        .evidence(&id)
        .expect_err("host key requires a host sidecar");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
    client.delete(&id).unwrap();
    assert!(matches!(client.get(&id), Err(Error::Tombstoned(_))));
}

#[test]
fn missing_host_sidecar_fails_when_host_key_is_configured() {
    let (_guard, url, parent) = isolate();
    let key = parent.join("lane.host");
    write_key(&key, 5);
    // SAFETY: ENV is held for the whole sitting that reads DEEDAR_HOST_KEY.
    unsafe {
        std::env::set_var("DEEDAR_HOST_KEY", &key);
    }
    let mut client = open(&url);
    let id = DeedId::parse("deed-quote-host").unwrap();
    client
        .create(quote(
            "deed-quote-host",
            "strip host",
            "https://example.com/host",
        ))
        .unwrap();
    client.evidence(&id).expect("sidecar present");
    fs::remove_file(sidecar(&url, "deed-quote-host")).unwrap();
    let err = client.evidence(&id).expect_err("deleted sidecar");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
    assert_eq!(client.get(&id).unwrap().id, id);
}

#[test]
fn store_signature_cannot_stand_in_for_host_when_keys_match() {
    let (_guard, url, parent) = isolate();
    let client = open(&url);
    let writer = deedar::store_dir(&url).unwrap().join("writer.key");
    let host_key = parent.join("same-as-writer.key");
    fs::copy(&writer, &host_key).unwrap();
    drop(client);
    // SAFETY: ENV is held for the whole sitting that reads DEEDAR_HOST_KEY.
    unsafe {
        std::env::set_var("DEEDAR_HOST_KEY", &host_key);
    }
    let mut client = open(&url);
    let id = DeedId::parse("deed-quote-host").unwrap();
    let (_, issued) = client
        .create(quote(
            "deed-quote-host",
            "same key",
            "https://example.com/host",
        ))
        .unwrap();
    let ev = client.evidence(&id).expect("host check with matching keys");
    assert_eq!(ev.deed_id, id);
    fs::write(sidecar(&url, "deed-quote-host"), &issued.signature).unwrap();
    let err = client
        .evidence(&id)
        .expect_err("store HMAC is not host evidence");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

/// Write a layout naming what the store accepts.
fn write_layout(url: &str, body: &str) {
    let dir = deedar::store_dir(url).unwrap();
    fs::write(dir.join("layout"), body).unwrap();
}

/// A signing key on disk, and the public half a layout would name.
fn signing_key(path: &Path, fill: u8) -> String {
    fs::write(path, [fill; 32]).unwrap();
    let key = ed25519_dalek::SigningKey::from_bytes(&[fill; 32]);
    key.verifying_key()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn a_signed_sidecar_verifies_against_an_accepted_signer() {
    let (_guard, url, parent) = isolate();
    let key = parent.join("host.signing");
    let public = signing_key(&key, 21);
    // SAFETY: ENV is held for the whole sitting.
    unsafe {
        std::env::set_var("DEEDAR_HOST_SIGNING_KEY", &key);
    }
    // The store has to exist before its layout can be written.
    drop(open(&url));
    write_layout(
        &url,
        &format!("1\nattestation = required\nsigner = ed25519:{public}\n"),
    );

    let mut client = open(&url);
    let id = DeedId::parse("deed-quote-host").unwrap();
    client
        .create(quote(
            "deed-quote-host",
            "signed",
            "https://example.com/host",
        ))
        .unwrap();
    let written = fs::read(sidecar(&url, "deed-quote-host")).unwrap();
    assert!(
        String::from_utf8_lossy(&written).starts_with("ed25519:"),
        "the sidecar says which construction wrote it"
    );
    let ev = client.evidence(&id).expect("an accepted signer");
    assert_eq!(ev.deed_id, id);
}

#[test]
fn a_signature_from_a_key_the_layout_does_not_accept_is_refused() {
    let (_guard, url, parent) = isolate();
    let key = parent.join("host.signing");
    signing_key(&key, 22);
    let stranger = signing_key(&parent.join("other.signing"), 23);
    // SAFETY: ENV is held for the whole sitting.
    unsafe {
        std::env::set_var("DEEDAR_HOST_SIGNING_KEY", &key);
    }
    drop(open(&url));
    write_layout(
        &url,
        &format!("1\nattestation = required\nsigner = ed25519:{stranger}\n"),
    );

    let mut client = open(&url);
    let id = DeedId::parse("deed-quote-host").unwrap();
    client
        .create(quote(
            "deed-quote-host",
            "wrong signer",
            "https://example.com/host",
        ))
        .unwrap();
    let err = client.evidence(&id).expect_err("a signer nobody accepts");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
    // The refusal says where the accepted signers are listed and how many.
    let said = err.to_string();
    assert!(said.contains("/layout lists"), "{said}");
    assert!(said.contains("(1 listed)"), "{said}");
}

/// A host that signs with a key its store does not list is told so before
/// its deeds fail evidence; a listed key is not.
#[test]
fn the_client_names_a_signing_key_the_layout_does_not_list() {
    let (_guard, url, parent) = isolate();
    let key = parent.join("host.signing");
    let public = signing_key(&key, 24);
    // SAFETY: ENV is held for the whole sitting.
    unsafe {
        std::env::set_var("DEEDAR_HOST_SIGNING_KEY", &key);
    }
    drop(open(&url));
    write_layout(&url, "1\n");
    assert_eq!(
        open(&url).unaccepted_signer(),
        Some(format!("ed25519:{public}"))
    );
    write_layout(&url, &format!("1\nsigner = ed25519:{public}\n"));
    assert_eq!(open(&url).unaccepted_signer(), None);
    // SAFETY: ENV is held for the whole sitting.
    unsafe {
        std::env::set_var("DEEDAR_HOST_SIGNING_KEY", "off");
    }
    assert_eq!(open(&url).unaccepted_signer(), None);
}

/// The reason the construction changed. A keyed hash cannot answer "who was
/// entitled to make this", because whoever can check one can mint one.
#[test]
fn a_required_attestation_is_not_satisfied_by_a_keyed_hash() {
    let (_guard, url, parent) = isolate();
    let key = parent.join("lane.host");
    write_key(&key, 31);
    // SAFETY: ENV is held for the whole sitting.
    unsafe {
        std::env::set_var("DEEDAR_HOST_KEY", &key);
    }
    let mut client = open(&url);
    let id = DeedId::parse("deed-quote-host").unwrap();
    client
        .create(quote(
            "deed-quote-host",
            "hash not signature",
            "https://example.com/host",
        ))
        .unwrap();
    // The hash verifies while the store asks for nothing more.
    client.evidence(&id).expect("hash checks out on its own");
    drop(client);

    write_layout(&url, "1\nattestation = required\n");
    let mut client = open(&url);
    let err = client
        .evidence(&id)
        .expect_err("a hash is not an attestation");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
    assert_eq!(client.get(&id).unwrap().id, id, "the deed still opens");
}

#[test]
fn a_required_attestation_refuses_a_deed_with_no_sidecar_at_all() {
    let (_guard, url, _parent) = isolate();
    let mut client = open(&url);
    let id = DeedId::parse("deed-quote-host").unwrap();
    client
        .create(quote(
            "deed-quote-host",
            "nothing vouched",
            "https://example.com/host",
        ))
        .unwrap();
    client.evidence(&id).expect("no demand, no sidecar, fine");
    drop(client);

    write_layout(&url, "1\nattestation = required\n");
    let mut client = open(&url);
    let err = client.evidence(&id).expect_err("nothing vouched for it");
    assert!(matches!(err, Error::Evidence(_)), "{err}");
}

/// A layout demanding something nobody can read would open the store wide
/// while claiming to be closed.
#[test]
fn a_layout_nobody_can_read_refuses_to_open_the_store() {
    let (_guard, url, _parent) = isolate();
    drop(open(&url));
    write_layout(&url, "1\nattestation = sometimes\n");
    assert!(Client::open(&url).is_err());
}
