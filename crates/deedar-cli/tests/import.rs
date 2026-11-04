//! `deedar import` end to end: a bag from one store into another, then the
//! receiver's own evidence, trail, current and log audit on what it took.

use std::path::Path;
use std::process::{Command, Output};

use deedar::EXIT_UNTRUSTED_SIGNER;

fn deedar(home: &Path, signing: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deedar"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("DEEDAR_HOST_SIGNING_KEY", signing)
        .env_remove("DEEDAR_HOST_KEY")
        .env_remove("DEEDAR_URL")
        .current_dir(home)
        .output()
        .expect("deedar runs")
}

fn ok(home: &Path, signing: &str, args: &[&str]) -> String {
    let out = deedar(home, signing, args);
    assert!(
        out.status.success(),
        "deedar {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn fails(home: &Path, signing: &str, args: &[&str]) -> String {
    let out = deedar(home, signing, args);
    assert!(!out.status.success(), "deedar {args:?} passed");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// `check` and `vouch check` finished, and the signer was not accepted.
fn untrusted(out: &Output) {
    assert_eq!(
        out.status.code(),
        Some(i32::from(EXIT_UNTRUSTED_SIGNER)),
        "stdout {}\nstderr {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn url(home: &Path, name: &str) -> String {
    format!("file://{}", home.join(name).display())
}

fn quote(home: &Path, store: &str, signing: &str, id: &str, extra: &[&str]) {
    let mut args = vec![
        "--url",
        store,
        "create",
        "quote",
        "--id",
        id,
        "--excerpt",
        "a range in a frozen edition",
        "--src-url",
        "https://example.invalid/edition",
        "--agent",
        "seat",
    ];
    args.extend_from_slice(extra);
    ok(home, signing, &args);
}

/// An unsigned bag needs the flag. Taken, every deed in it answers on the
/// receiver: evidence, the trail to its source, current to its revision, and
/// a log that answers for the shelves. A second import adds nothing.
#[test]
fn an_unsigned_bag_needs_the_flag_and_then_answers_on_the_receiver() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let sender = url(home, "sender");
    let receiver = url(home, "receiver");
    quote(home, &sender, "off", "deed-quote-src", &[]);
    quote(
        home,
        &sender,
        "off",
        "deed-quote-made",
        &["--input", "deed-quote-src"],
    );
    quote(home, &sender, "off", "deed-quote-v1", &[]);
    quote(
        home,
        &sender,
        "off",
        "deed-quote-v2",
        &["--excerpt", "a later take", "--supersedes", "deed-quote-v1"],
    );
    std::fs::write(home.join("letter.txt"), "the letter itself").unwrap();
    ok(
        home,
        "off",
        &[
            "--url",
            &sender,
            "create",
            "file",
            "--id",
            "deed-file-letter",
            "--path",
            "letter.txt",
            "--agent",
            "seat",
        ],
    );
    let ids = [
        "deed-quote-src",
        "deed-quote-made",
        "deed-quote-v1",
        "deed-quote-v2",
        "deed-file-letter",
    ];
    let mut export = vec!["--url", &sender, "export", "--into", "bag/data/deeds"];
    export.extend_from_slice(&ids);
    ok(home, "off", &export);

    let err = fails(home, "off", &["--url", &receiver, "import", "bag"]);
    assert!(err.contains("head is unsigned"), "{err}");
    assert!(err.contains("--allow-unsigned-head"), "{err}");
    assert_eq!(ok(home, "off", &["--url", &receiver, "log", "list"]), "");
    assert_eq!(ok(home, "off", &["--url", &receiver, "list"]), "");

    let first = ok(
        home,
        "off",
        &["--url", &receiver, "import", "bag", "--allow-unsigned-head"],
    );
    assert!(first.contains("5 deeds proven"), "{first}");
    assert!(
        first.contains("imported 5 deeds, appended 5 log entries"),
        "{first}"
    );

    let mut evidence = vec!["--url", &receiver, "evidence"];
    evidence.extend_from_slice(&ids);
    let said = ok(home, "off", &evidence);
    assert!(said.contains("5 of 5 verified"), "{said}");
    let trail = ok(
        home,
        "off",
        &["--url", &receiver, "trail", "deed-quote-made"],
    );
    assert!(trail.contains("id=deed-quote-src"), "{trail}");
    let current = ok(
        home,
        "off",
        &["--url", &receiver, "current", "deed-quote-v1"],
    );
    assert!(current.contains("id=deed-quote-v2"), "{current}");
    let audit = ok(home, "off", &["--url", &receiver, "log", "audit"]);
    assert_eq!(audit, "ok 5 logged and served\n");

    let second = ok(
        home,
        "off",
        &["--url", &receiver, "import", "bag", "--allow-unsigned-head"],
    );
    assert!(
        second.contains("imported 0 deeds, appended 0 log entries"),
        "{second}"
    );
}

/// A head signed by a key the receiver lists is the bar, and needs no flag.
#[test]
fn a_head_signed_by_a_listed_key_needs_no_flag() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let key = home.join("sender.key");
    std::fs::write(&key, [7u8; 32]).unwrap();
    let key = key.display().to_string();
    let sender = url(home, "sender");
    let receiver = url(home, "receiver");
    quote(home, &sender, &key, "deed-quote-signed", &[]);
    let host = ok(home, &key, &["--url", &sender, "host"]);
    let public = host
        .strip_prefix("host key: ")
        .and_then(|rest| rest.split(',').next())
        .unwrap_or_else(|| panic!("{host}"));
    ok(
        home,
        &key,
        &[
            "--url",
            &sender,
            "export",
            "--into",
            "bag/data/deeds",
            "deed-quote-signed",
        ],
    );

    ok(home, "off", &["--url", &receiver, "list"]);
    std::fs::write(
        home.join("receiver/layout"),
        format!("1\nattestation = required\nsigner = {public}\n"),
    )
    .unwrap();
    let said = ok(home, "off", &["--url", &receiver, "import", "bag"]);
    assert!(said.contains("(accepted)"), "{said}");
    assert!(said.contains("appended 1 log entries"), "{said}");
    let ev = ok(
        home,
        "off",
        &["--url", &receiver, "evidence", "deed-quote-signed"],
    );
    assert!(ev.contains("id=deed-quote-signed ok"), "{ev}");
    ok(home, "off", &["--url", &receiver, "log", "audit"]);
    assert!(home
        .join("receiver/deeds/deed-quote-signed.proof")
        .is_file());
    assert!(home.join("receiver/deeds/deed-quote-signed.head").is_file());
}

/// `--allow-unsigned-head` takes a signature by a key the layout does not
/// list. The deed was created with signing off, so there is no host sidecar
/// for that key to fail afterwards. `check` reports the signer, warns, and
/// exits `EXIT_UNTRUSTED_SIGNER`.
#[test]
fn allow_unsigned_head_takes_an_unlisted_signature() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let key = home.join("sender.key");
    std::fs::write(&key, [7u8; 32]).unwrap();
    let other = home.join("other.key");
    std::fs::write(&other, [9u8; 32]).unwrap();
    let key = key.display().to_string();
    let other = other.display().to_string();
    let sender = url(home, "sender");
    let receiver = url(home, "receiver");
    quote(home, &sender, "off", "deed-quote-signed", &[]);
    ok(
        home,
        &key,
        &[
            "--url",
            &sender,
            "export",
            "--into",
            "bag/data/deeds",
            "deed-quote-signed",
        ],
    );

    ok(home, "off", &["--url", &receiver, "list"]);
    let host = fails(home, &other, &["--url", &receiver, "host"]);
    let public = host
        .split_whitespace()
        .find(|word| word.starts_with("ed25519:"))
        .unwrap_or_else(|| panic!("{host}"));
    std::fs::write(
        home.join("receiver/layout"),
        format!("1\nsigner = {public}\n"),
    )
    .unwrap();

    let checked = deedar(home, "off", &["--url", &receiver, "check", "bag"]);
    untrusted(&checked);
    let stdout = String::from_utf8_lossy(&checked.stdout);
    let stderr = String::from_utf8_lossy(&checked.stderr);
    assert!(stdout.contains("not a key this reader accepts"), "{stdout}");
    assert!(
        stderr.contains("deedar: warning:") && stderr.contains("not a key this reader accepts"),
        "{stderr}"
    );
    let err = fails(home, "off", &["--url", &receiver, "import", "bag"]);
    assert!(err.contains("does not list"), "{err}");
    assert!(err.contains("--allow-unsigned-head"), "{err}");
    let said = ok(
        home,
        "off",
        &["--url", &receiver, "import", "bag", "--allow-unsigned-head"],
    );
    assert!(said.contains("appended 1 log entries"), "{said}");
    let ev = ok(
        home,
        "off",
        &["--url", &receiver, "evidence", "deed-quote-signed"],
    );
    assert!(ev.contains("id=deed-quote-signed ok"), "{ev}");
}

/// A layout that demands attestation still imports an unsigned head when
/// the flag is set, and warns. Without the flag nothing is written.
#[test]
fn attestation_required_imports_an_unsigned_head_with_the_flag_and_warns() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let key_path = home.join("host.key");
    std::fs::write(&key_path, [7u8; 32]).unwrap();
    let key = key_path.display().to_string();
    let sender = url(home, "sender");
    let receiver = url(home, "receiver");
    // The deed is attested. Export with signing off leaves the head unsigned
    // and keeps the sidecar, which is what a required store can still hold.
    quote(home, &sender, &key, "deed-quote-plain", &[]);
    let host = ok(home, &key, &["--url", &sender, "host"]);
    let public = host
        .strip_prefix("host key: ")
        .and_then(|rest| rest.split(',').next())
        .unwrap_or_else(|| panic!("{host}"));
    ok(
        home,
        "off",
        &[
            "--url",
            &sender,
            "export",
            "--into",
            "bag/data/deeds",
            "deed-quote-plain",
        ],
    );
    ok(home, "off", &["--url", &receiver, "list"]);
    std::fs::write(
        home.join("receiver/layout"),
        format!("1\nattestation = required\nsigner = {public}\n"),
    )
    .unwrap();

    let err = fails(home, "off", &["--url", &receiver, "import", "bag"]);
    assert!(err.contains("--allow-unsigned-head"), "{err}");
    assert_eq!(ok(home, "off", &["--url", &receiver, "log", "list"]), "");

    let out = deedar(
        home,
        "off",
        &["--url", &receiver, "import", "bag", "--allow-unsigned-head"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stdout.contains("appended 1 log entries"), "{stdout}");
    assert!(
        stderr.contains("deedar: warning:") && stderr.contains("attestation = required"),
        "{stderr}"
    );
    assert!(
        ok(home, "off", &["--url", &receiver, "log", "list"]).contains("deed-quote-plain"),
        "the deed was not logged"
    );

    let err = fails(
        home,
        "off",
        &["--url", &receiver, "import", "bag", "--allow-unsigned"],
    );
    assert!(
        err.contains("unknown import flag --allow-unsigned"),
        "{err}"
    );
}

/// `host accept` on a store that demands attestation refuses, names the
/// line to add by hand, and leaves the layout as it was.
#[test]
fn host_accept_on_a_required_store_refuses() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let key_path = home.join("host.key");
    std::fs::write(&key_path, [7u8; 32]).unwrap();
    let key = key_path.display().to_string();
    let store = url(home, "store");
    ok(home, "off", &["--url", &store, "list"]);
    let layout_path = home.join("store/layout");
    let layout_text = "1\nattestation = required\n";
    std::fs::write(&layout_path, layout_text).unwrap();

    let out = deedar(home, &key, &["--url", &store, "host", "accept"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("attestation = required"), "{stderr}");
    assert!(stderr.contains("signer = ed25519:"), "{stderr}");
    assert!(stderr.contains("by hand"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&layout_path).unwrap(), layout_text);
}

/// `vouch check` reports a key the layout does not list, warns, and exits
/// `EXIT_UNTRUSTED_SIGNER`. With no list it says so, does not warn, and
/// exits 0. A signature that does not cover the file still exits 1.
#[test]
fn vouch_check_reports_an_unlisted_key_and_warns() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let key_path = home.join("host.key");
    std::fs::write(&key_path, [7u8; 32]).unwrap();
    let other_path = home.join("other.key");
    std::fs::write(&other_path, [9u8; 32]).unwrap();
    let key = key_path.display().to_string();
    let other = other_path.display().to_string();
    let store = url(home, "store");
    ok(home, "off", &["--url", &store, "list"]);
    std::fs::write(home.join("manifest-sha256.txt"), "abc  data/one\n").unwrap();
    ok(
        home,
        &key,
        &["--url", &store, "vouch", "sign", "manifest-sha256.txt"],
    );

    let open = deedar(
        home,
        "off",
        &["--url", &store, "vouch", "check", "manifest-sha256.txt"],
    );
    assert!(
        open.status.success(),
        "{}",
        String::from_utf8_lossy(&open.stderr)
    );
    assert!(String::from_utf8_lossy(&open.stdout).contains("no signer list was given"));
    assert!(
        !String::from_utf8_lossy(&open.stderr).contains("deedar: warning:"),
        "{}",
        String::from_utf8_lossy(&open.stderr)
    );

    let host = fails(home, &other, &["--url", &store, "host"]);
    let public = host
        .split_whitespace()
        .find(|word| word.starts_with("ed25519:"))
        .unwrap_or_else(|| panic!("{host}"));
    std::fs::write(home.join("store/layout"), format!("1\nsigner = {public}\n")).unwrap();
    let out = deedar(
        home,
        "off",
        &["--url", &store, "vouch", "check", "manifest-sha256.txt"],
    );
    untrusted(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stdout.contains("not a key this reader accepts"), "{stdout}");
    assert!(
        stderr.contains("deedar: warning:") && stderr.contains("not a key this reader accepts"),
        "{stderr}"
    );

    std::fs::write(home.join("manifest-sha256.txt"), "changed\n").unwrap();
    let broken = deedar(
        home,
        "off",
        &["--url", &store, "vouch", "check", "manifest-sha256.txt"],
    );
    assert_eq!(broken.status.code(), Some(1), "{broken:?}");
    assert!(
        !String::from_utf8_lossy(&broken.stderr).contains("deedar: warning:"),
        "{}",
        String::from_utf8_lossy(&broken.stderr)
    );
}
