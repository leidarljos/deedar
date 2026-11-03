//! Refusals name the input and the next command.
//!
//! An unknown accession, a prefix of several accessions, an unset
//! `DEEDAR_URL`, and a path that is not a store used to end in a short
//! line. Each of these is one stderr line and exit 1.

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

fn refused(out: &Output, stderr: &str) {
    assert_eq!(out.status.code(), Some(1), "{:?}", out.status);
    assert_eq!(String::from_utf8_lossy(&out.stdout).as_ref(), "");
    assert_eq!(String::from_utf8_lossy(&out.stderr).as_ref(), stderr);
}

fn store_url(home: &Path) -> String {
    format!("file://{}", home.join("store").display())
}

fn quote(home: &Path, url: &str, id: &str) {
    ok(
        home,
        &[
            "--url",
            url,
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
        ],
    );
}

fn next_list(url: &str) -> String {
    format!("`deedar --url {url} list`")
}

fn next_get(url: &str, id: &str) -> String {
    format!("`deedar --url {url} get {id}`")
}

#[test]
fn an_unknown_accession_names_the_input_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = store_url(home);
    quote(home, &url, "deed-quote-survey");
    let out = deedar(home, &["--url", &url, "get", "deed-quote-missing"]);
    refused(
        &out,
        &format!(
            "deedar: unknown accession deed-quote-missing; {}\n",
            next_list(&url)
        ),
    );
}

#[test]
fn a_prefix_of_one_accession_names_that_accession_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = store_url(home);
    quote(home, &url, "deed-quote-survey");
    let out = deedar(home, &["--url", &url, "get", "deed-quote-sur"]);
    refused(
        &out,
        &format!(
            "deedar: prefix deed-quote-sur matches deed-quote-survey; {}\n",
            next_get(&url, "deed-quote-survey")
        ),
    );
}

#[test]
fn an_ambiguous_prefix_lists_matches_in_byte_order_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = store_url(home);
    quote(home, &url, "deed-quote-survey");
    quote(home, &url, "deed-quote-survey-v2");
    let got = ok(home, &["--url", &url, "get", "deed-quote-survey"]);
    assert!(got.starts_with("id=deed-quote-survey "), "{got}");

    let out = deedar(home, &["--url", &url, "get", "deed-quote-sur"]);
    refused(
        &out,
        &format!(
            "deedar: ambiguous prefix deed-quote-sur matches 2 deeds, at most 8 in byte order: \
             deed-quote-survey, deed-quote-survey-v2; {}\n",
            next_list(&url)
        ),
    );
}

/// Nine deeds share a prefix. The line names eight, in byte order, and the
/// count says the ninth is there.
#[test]
fn an_ambiguous_prefix_lists_at_most_eight_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = store_url(home);
    for n in 1..=9 {
        quote(home, &url, &format!("deed-quote-p{n:02}"));
    }
    let listed = (1..=8)
        .map(|n| format!("deed-quote-p{n:02}"))
        .collect::<Vec<_>>()
        .join(", ");
    let out = deedar(home, &["--url", &url, "get", "deed-quote-p"]);
    refused(
        &out,
        &format!(
            "deedar: ambiguous prefix deed-quote-p matches 9 deeds, at most 8 in byte order: \
             {listed}; {}\n",
            next_list(&url)
        ),
    );
}

#[test]
fn a_shared_product_path_lists_both_accessions_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = store_url(home);
    for (id, name) in [("deed-file-first", "a.txt"), ("deed-file-second", "b.txt")] {
        std::fs::write(home.join(name), b"same bytes\n").unwrap();
        ok(
            home,
            &[
                "--url", &url, "create", "file", "--id", id, "--path", name, "--agent", "seat",
            ],
        );
    }
    let got = ok(home, &["--url", &url, "get", "deed-file-first"]);
    let path = got
        .lines()
        .find_map(|line| line.strip_prefix("paths="))
        .expect("paths line");
    let out = deedar(home, &["--url", &url, "get", path]);
    refused(
        &out,
        &format!(
            "deedar: ambiguous {path} matches 2 deeds, at most 8 in byte order: \
             deed-file-first, deed-file-second; {}\n",
            next_list(&url)
        ),
    );
}

#[test]
fn an_unset_deedar_url_names_the_seat_path_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let seat = home.join(".local/share/deedar/store");
    let out = Command::new(env!("CARGO_BIN_EXE_deedar"))
        .arg("list")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env_remove("XDG_DATA_HOME")
        .env("DEEDAR_HOST_SIGNING_KEY", "off")
        .env_remove("DEEDAR_HOST_KEY")
        .env_remove("DEEDAR_URL")
        .current_dir(home)
        .output()
        .expect("deedar runs");
    let path = seat.display();
    refused(
        &out,
        &format!(
            "deedar: DEEDAR_URL is unset and there is no store at {path}; \
             `export DEEDAR_URL=file://{path}` or `deedar --url file://{path}`\n"
        ),
    );
}

#[test]
fn a_file_path_is_not_a_store_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let file = home.join("notes.txt");
    std::fs::write(&file, b"not a store\n").unwrap();
    let url = format!("file://{}", file.display());
    let out = deedar(home, &["--url", &url, "list"]);
    refused(
        &out,
        &format!(
            "deedar: {} is not a store; `export DEEDAR_URL=file://PATH` or `deedar --url file://PATH`\n",
            file.display()
        ),
    );
}

#[test]
fn a_url_that_is_not_a_path_names_the_input_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let out = deedar(home, &["--url", "notapath", "list"]);
    refused(
        &out,
        "deedar: notapath is not a store path; `export DEEDAR_URL=file://PATH` or `deedar --url file://PATH`\n",
    );
}
