//! `--supersedes P` when P already has a successor names the current tip.
//!
//! The old line told the seat to run `--supersedes P` again, which is the
//! command that just failed.

use std::path::Path;
use std::process::{Command, Output};

fn deedar(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deedar"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("DEEDAR_HOST_SIGNING_KEY", "off")
        .env_remove("DEEDAR_HOST_KEY")
        .env_remove("DEEDAR_URL")
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
    String::from_utf8(out.stdout).expect("stdout is utf8")
}

fn quote(home: &Path, url: &str, id: &str, excerpt: &str, supersedes: Option<&str>) {
    let mut args = vec![
        "--url",
        url,
        "create",
        "quote",
        "--id",
        id,
        "--excerpt",
        excerpt,
        "--src-url",
        "https://example.invalid/edition",
        "--agent",
        "me",
    ];
    if let Some(prior) = supersedes {
        args.push("--supersedes");
        args.push(prior);
    }
    ok(home, &args);
}

/// Three creates: P, a revision of P, then another `--supersedes P`.
/// The third exits 1 on one stderr line that names the tip. Revising
/// that tip then stores the new deed.
#[test]
fn revising_a_revised_deed_names_the_tip_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = format!("file://{}", home.join("store").display());
    quote(home, &url, "deed-quote-p", "x", None);
    quote(home, &url, "deed-quote-c1", "y", Some("deed-quote-p"));

    let out = deedar(
        home,
        &[
            "--url",
            &url,
            "create",
            "quote",
            "--id",
            "deed-quote-c2",
            "--excerpt",
            "z",
            "--src-url",
            "https://example.invalid/edition",
            "--agent",
            "me",
            "--supersedes",
            "deed-quote-p",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{:?}", out.status);
    assert_eq!(String::from_utf8_lossy(&out.stdout).as_ref(), "");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("already revised by deed-quote-c1"), "{err}");
    assert!(err.contains("--supersedes deed-quote-c1"), "{err}");
    assert_eq!(
        err.as_ref(),
        "deedar: deed frozen: deed-quote-p; deed-quote-p is already revised by \
         deed-quote-c1; revise that: --supersedes deed-quote-c1\n"
    );

    let revised = ok(
        home,
        &[
            "--url",
            &url,
            "create",
            "quote",
            "--id",
            "deed-quote-c2",
            "--excerpt",
            "z",
            "--src-url",
            "https://example.invalid/edition",
            "--agent",
            "me",
            "--supersedes",
            "deed-quote-c1",
        ],
    );
    assert!(revised.contains("accession=deed-quote-c2\n"), "{revised}");
}

/// A second live edge off P is a fork. `current` lists both tips, and the
/// hint does too, in that same accession order.
#[test]
fn a_forked_revision_lists_every_tip_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = format!("file://{}", home.join("store").display());
    quote(home, &url, "deed-quote-root", "root", None);
    quote(home, &url, "deed-quote-m", "em", Some("deed-quote-root"));
    quote(home, &url, "deed-quote-a", "ay", None);
    std::fs::write(
        home.join("store/deeds/deed-quote-a.supersedes"),
        "deed-quote-root\n",
    )
    .unwrap();

    let out = deedar(
        home,
        &[
            "--url",
            &url,
            "create",
            "quote",
            "--id",
            "deed-quote-next",
            "--excerpt",
            "next",
            "--src-url",
            "https://example.invalid/edition",
            "--agent",
            "me",
            "--supersedes",
            "deed-quote-root",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{:?}", out.status);
    assert_eq!(String::from_utf8_lossy(&out.stdout).as_ref(), "");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).as_ref(),
        "deedar: deed frozen: deed-quote-root; deed-quote-root is already revised by \
         deed-quote-a deed-quote-m; revise one of those\n"
    );
}
