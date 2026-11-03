//! `deedar list` prints the deeds a filter names, and no others.
//!
//! The shelf holds a quote whose excerpt says RFC 9162, a quote that does
//! not, and a file whose name says RFC. `--kind quote --text rfc` keeps the
//! first quote.

use std::path::Path;
use std::process::Command;

fn deedar(home: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_deedar"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("DEEDAR_HOST_SIGNING_KEY", "off")
        .env_remove("DEEDAR_HOST_KEY")
        .env_remove("DEEDAR_URL")
        .current_dir(home)
        .output()
        .expect("deedar runs");
    assert!(
        out.status.success(),
        "deedar {args:?}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stderr).as_ref(), "");
    String::from_utf8(out.stdout).expect("stdout is utf8")
}

/// Accessions, in the order `list` prints them. One `id=` line per deed.
fn accessions(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("id=")?;
            Some(rest.split_once(' ').map(|(id, _)| id).unwrap_or(rest))
        })
        .collect()
}

#[test]
fn list_kind_and_text_prints_only_the_matching_deeds() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = format!("file://{}", home.join("store").display());
    let notes = home.join("notes.txt");
    std::fs::write(&notes, b"notes").unwrap();

    deedar(
        home,
        &[
            "--url",
            &url,
            "create",
            "quote",
            "--id",
            "deed-quote-ctlog",
            "--name",
            "ct log",
            "--excerpt",
            "the quote taken from RFC 9162",
            "--src-url",
            "https://example.invalid/ct",
            "--agent",
            "seat",
        ],
    );
    deedar(
        home,
        &[
            "--url",
            &url,
            "create",
            "quote",
            "--id",
            "deed-quote-plain",
            "--name",
            "heat pump",
            "--excerpt",
            "a heat pump moves heat",
            "--src-url",
            "https://example.invalid/heat",
            "--agent",
            "seat",
        ],
    );
    deedar(
        home,
        &[
            "--url",
            &url,
            "create",
            "file",
            "--id",
            "deed-file-notes",
            "--name",
            "RFC notes",
            "--path",
            notes.to_str().unwrap(),
            "--agent",
            "other",
        ],
    );

    let whole = deedar(home, &["--url", &url, "list"]);
    assert_eq!(
        accessions(&whole),
        ["deed-file-notes", "deed-quote-ctlog", "deed-quote-plain"]
    );

    let by_text = deedar(home, &["--url", &url, "list", "--text", "rfc"]);
    assert_eq!(
        accessions(&by_text),
        ["deed-file-notes", "deed-quote-ctlog"]
    );

    let matched = deedar(
        home,
        &["--url", &url, "list", "--kind", "quote", "--text", "rfc"],
    );
    assert_eq!(accessions(&matched), ["deed-quote-ctlog"]);
    assert!(
        matched.contains("excerpt=the quote taken from RFC 9162\n"),
        "{matched}"
    );
    assert!(!matched.contains("deed-quote-plain"), "{matched}");
    assert!(!matched.contains("deed-file-notes"), "{matched}");

    let by_agent = deedar(home, &["--url", &url, "list", "--agent", "other"]);
    assert_eq!(accessions(&by_agent), ["deed-file-notes"]);
}
