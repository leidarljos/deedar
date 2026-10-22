//! `deedar import` end to end: a bag from one store into another, then the
//! receiver's own evidence, trail, current and log audit on what it took.

use std::path::Path;
use std::process::{Command, Output};

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

/// `--allow-unsigned-head` waives a missing signature only, not one by an
/// unlisted key.
#[test]
fn allow_unsigned_head_does_not_waive_an_unlisted_signature() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let key = home.join("sender.key");
    std::fs::write(&key, [7u8; 32]).unwrap();
    let key = key.display().to_string();
    let sender = url(home, "sender");
    let receiver = url(home, "receiver");
    quote(home, &sender, &key, "deed-quote-signed", &[]);
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
    std::fs::write(home.join("receiver/layout"), "1\n").unwrap();
    let err = fails(
        home,
        "off",
        &["--url", &receiver, "import", "bag", "--allow-unsigned-head"],
    );
    assert!(err.contains("does not list"), "{err}");
}

/// A layout that demands attestation refuses an unsigned head, flag or not.
#[test]
fn attestation_required_refuses_an_unsigned_head_even_with_the_flag() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let sender = url(home, "sender");
    let receiver = url(home, "receiver");
    quote(home, &sender, "off", "deed-quote-plain", &[]);
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
    std::fs::write(home.join("receiver/layout"), "1\nattestation = required\n").unwrap();

    let err = fails(
        home,
        "off",
        &["--url", &receiver, "import", "bag", "--allow-unsigned-head"],
    );
    assert!(err.contains("attestation = required"), "{err}");
    assert_eq!(ok(home, "off", &["--url", &receiver, "log", "list"]), "");

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
