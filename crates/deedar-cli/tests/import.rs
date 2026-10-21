//! `deedar import` end to end: check a bag, then append its receipts.

use std::path::Path;
use std::process::{Command, Output};

fn deedar(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deedar"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("DEEDAR_HOST_SIGNING_KEY", "off")
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

#[test]
fn import_checks_the_bag_and_appends_receipts_once() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let sender = format!("file://{}", home.join("sender").display());
    let receiver = format!("file://{}", home.join("receiver").display());

    ok(
        home,
        &[
            "--url",
            &sender,
            "create",
            "quote",
            "--id",
            "deed-quote-shared",
            "--excerpt",
            "shared excerpt",
            "--src-url",
            "https://example.invalid/edition",
            "--agent",
            "seat",
        ],
    );
    ok(
        home,
        &[
            "--url",
            &sender,
            "export",
            "--into",
            "bag/data/deeds",
            "deed-quote-shared",
        ],
    );

    let first = ok(home, &["--url", &receiver, "import", "bag"]);
    assert!(first.contains("1 deeds proven"), "{first}");
    assert!(
        first.contains("imported 1 deeds, appended 1 log entries"),
        "{first}"
    );
    ok(home, &["--url", &receiver, "get", "deed-quote-shared"]);

    let second = ok(home, &["--url", &receiver, "import", "bag"]);
    assert!(
        second.contains("imported 0 deeds, appended 0 log entries"),
        "{second}"
    );
}
