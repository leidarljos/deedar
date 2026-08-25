//! Host-issued evidence sidecar beside store HMAC.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use deed::{Body, DeedId, Error};
use deeder::{Client, CreateRequest};

static ENV: Mutex<()> = Mutex::new(());

fn isolate() -> (std::sync::MutexGuard<'static, ()>, String, PathBuf) {
    let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: ENV is held for the whole sitting that reads DEEDER_HOST_KEY.
    unsafe {
        std::env::remove_var("DEEDER_HOST_KEY");
    }
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let parent = std::env::temp_dir().join(format!("deeder-host-{n}"));
    let store = parent.join("store");
    fs::create_dir_all(&store).unwrap();
    (guard, format!("file://{}", store.display()), parent)
}

fn sidecar(url: &str, id: &str) -> PathBuf {
    deeder::store_dir(url)
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
    // SAFETY: ENV is held for the whole sitting that reads DEEDER_HOST_KEY.
    unsafe {
        std::env::set_var("DEEDER_HOST_KEY", &key);
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
    // SAFETY: ENV is held for the whole sitting that reads DEEDER_HOST_KEY.
    unsafe {
        std::env::set_var("DEEDER_HOST_KEY", &key);
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
    // SAFETY: ENV is held for the whole sitting that reads DEEDER_HOST_KEY.
    unsafe {
        std::env::set_var("DEEDER_HOST_KEY", &key);
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
    // SAFETY: ENV is held for the whole sitting that reads DEEDER_HOST_KEY.
    unsafe {
        std::env::set_var("DEEDER_HOST_KEY", &key);
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
    let writer = deeder::store_dir(&url).unwrap().join("writer.key");
    let host_key = parent.join("same-as-writer.key");
    fs::copy(&writer, &host_key).unwrap();
    drop(client);
    // SAFETY: ENV is held for the whole sitting that reads DEEDER_HOST_KEY.
    unsafe {
        std::env::set_var("DEEDER_HOST_KEY", &host_key);
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
