//! `deedar check --since` end to end: the receiver keeps a head, and the next
//! bag has to bridge from exactly that head.

use std::path::Path;
use std::process::{Command, Output};

fn deedar(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deedar"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("DEEDAR_HOST_SIGNING_KEY", "off")
        .env(
            "DEEDAR_URL",
            format!("file://{}", home.join("store").display()),
        )
        .current_dir(home)
        .output()
        .expect("deedar runs")
}

fn ok(home: &Path, args: &[&str]) -> String {
    let out = deedar(home, args);
    assert!(
        out.status.success(),
        "deedar {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn mint(home: &Path, name: &str) {
    std::fs::write(home.join(format!("{name}.txt")), name).unwrap();
    ok(
        home,
        &[
            "create",
            "file",
            "--name",
            name,
            "--path",
            &format!("{name}.txt"),
            "--agent",
            "test",
        ],
    );
}

#[test]
fn an_honest_sender_bridges_from_the_kept_head() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    mint(home, "one");
    ok(
        home,
        &["export", "--into", "bag1/data/deeds", "deed-file-one"],
    );
    let said = ok(home, &["check", "bag1", "--keep", "kept.head"]);
    assert!(said.contains("kept this head"), "{said}");

    mint(home, "two");
    ok(
        home,
        &["export", "--into", "bag2/data/deeds", "deed-file-two"],
    );
    let bridge = ok(home, &["log", "bridge", "1"]);
    std::fs::write(home.join("bag2/bridge.txt"), bridge).unwrap();
    let said = ok(home, &["check", "bag2", "--since", "kept.head"]);
    assert!(
        said.contains("grew from the 1 entries you kept to 2"),
        "{said}"
    );
}

#[test]
fn a_rewritten_log_fails_against_the_kept_head() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    mint(home, "one");
    ok(
        home,
        &["export", "--into", "bag1/data/deeds", "deed-file-one"],
    );
    ok(home, &["check", "bag1", "--keep", "kept.head"]);

    // Rewrite history: empty the log, mint, and log everything again in a
    // different order. The new log has its own size-1 head.
    std::fs::write(home.join("store/log"), "").unwrap();
    mint(home, "zero");
    ok(home, &["log", "backfill"]);
    ok(
        home,
        &["export", "--into", "bag2/data/deeds", "deed-file-zero"],
    );
    let bridge = ok(home, &["log", "bridge", "1"]);
    std::fs::write(home.join("bag2/bridge.txt"), bridge).unwrap();

    let out = deedar(home, &["check", "bag2", "--since", "kept.head"]);
    assert!(!out.status.success(), "a rewritten log passed");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("not the log you saw before"), "{err}");
}

#[test]
fn a_bridge_passed_as_the_kept_head_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    mint(home, "one");
    mint(home, "two");
    ok(
        home,
        &["export", "--into", "bag/data/deeds", "deed-file-two"],
    );
    let bridge = ok(home, &["log", "bridge", "1"]);
    std::fs::write(home.join("bag/bridge.txt"), bridge).unwrap();
    let out = deedar(home, &["check", "bag", "--since", "bag/bridge.txt"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("this is a bridge"), "{err}");
}
