//! RFC 3161 TimeStampReq over evidence bytes. Offline path only.

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use deed::{Body, DeedId};
use deedar::{imprint_hash, timestamp_req, Client, CreateRequest};

use common::tmp_url;

const SHA256_OID: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];

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
        std::env::var("DEEDAR_TSA")
            .ok()
            .filter(|s| !s.is_empty())
            .is_none(),
        "tests must not call a timestamp authority; unset DEEDAR_TSA"
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
    let root = deedar::store_dir(&url).unwrap();
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

/// The operator binary. This suite lives in the library crate, and the
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

/// Kills a still-running `deedar` if the test fails before it is reaped.
struct Reap(Option<Child>);

impl Drop for Reap {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn timestamp_when_the_tsa_accepts_and_stays_silent_saves_the_tsq_and_exits() {
    offline();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("port").port();
    let tsa = format!("http://127.0.0.1:{port}");
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        // Accept and write nothing. Hold the socket so the client blocks in
        // read instead of seeing the connection close.
        if let Ok((stream, _)) = listener.accept() {
            let _hold = stream;
            let _ = release_rx.recv();
        }
    });

    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let blob = write_blob(&root, "note.txt", b"lane timestamp");
    let mut client = Client::open(&url).unwrap();
    let id = DeedId::parse("deed-file-note").unwrap();
    client.create(file("deed-file-note", blob)).unwrap();
    let deeds = root.join("deeds");
    let cap_before = fs::read(deeds.join("deed-file-note.cap")).unwrap();
    let ev = fs::read(deeds.join("deed-file-note.evidence")).unwrap();
    let tsq = deeds.join("deed-file-note.tsq");

    // READ_TIMEOUT in src/timestamp.rs. This listener accepts and writes
    // nothing, so the command hits that read deadline, not the connect one.
    let read_timeout = Duration::from_secs(10);
    // Starting the binary and the accept thread. A missing deadline hangs
    // past this, which is the failure this test exists to catch.
    let margin = Duration::from_secs(5);
    // A socket timeout wakes a little after the deadline. Two seconds is
    // that slack, and still nowhere near an immediate error.
    let early = Duration::from_secs(2);

    let bin = deedar_bin();
    let child = Command::new(&bin)
        .arg("--url")
        .arg(&url)
        .args(["timestamp", "deed-file-note"])
        .env("DEEDAR_TSA", &tsa)
        .env("DEEDAR_HOST_SIGNING_KEY", "off")
        .env_remove("DEEDAR_URL")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("run {}: {e}", bin.display()));
    let mut reap = Reap(Some(child));
    let started = Instant::now();
    loop {
        let child = reap.0.as_mut().expect("child");
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > read_timeout + margin => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "deedar timestamp still running after {:?}",
                    read_timeout + margin
                );
            }
            Ok(None) => {
                // See a return inside the margin without spinning the core.
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("wait: {e}"),
        }
    }
    let elapsed = started.elapsed();
    let child = reap.0.take().expect("child");
    let output = child.wait_with_output().expect("output");
    drop(release_tx);

    assert!(
        elapsed <= read_timeout + margin,
        "took {elapsed:?}; bound is {:?}",
        read_timeout + margin
    );
    assert!(
        elapsed + early >= read_timeout,
        "took {elapsed:?}; expected to wait about {read_timeout:?}"
    );
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stdout, "");
    let err_line = stderr.trim_end();
    assert!(!err_line.contains('\n'), "{stderr}");
    assert!(err_line.contains("DEEDAR_TSA"), "{stderr}");
    assert!(err_line.contains(&tsa), "{stderr}");
    assert!(
        err_line.contains(&format!("{}s", read_timeout.as_secs())),
        "{stderr}"
    );
    assert!(err_line.contains(&tsq.display().to_string()), "{stderr}");
    let saved = fs::read(&tsq).expect("tsq");
    assert_eq!(saved, timestamp_req(&imprint_hash(&ev)));
    assert!(!deeds.join("deed-file-note.tsr").exists());
    assert_eq!(
        fs::read(deeds.join("deed-file-note.cap")).unwrap(),
        cap_before
    );
    assert_eq!(fs::read(deeds.join("deed-file-note.evidence")).unwrap(), ev);
    client.evidence(&id).expect("evidence");
    client.get(&id).expect("get");
}

#[test]
fn get_after_timestamp_still_works() {
    offline();
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let blob = write_blob(&root, "note.txt", b"lane timestamp");
    let mut client = Client::open(&url).unwrap();
    let id = DeedId::parse("deed-file-note").unwrap();
    client.create(file("deed-file-note", blob)).unwrap();
    client.timestamp(&id).expect("timestamp");
    let got = client.get(&id).expect("get");
    assert_eq!(got.id, id);
    assert_eq!(got.name, "deed-file-note");
}
