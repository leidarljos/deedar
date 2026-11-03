//! A value is stored as the seat wrote it, even when it is spelled like a flag.
//!
//! `--help`, `-h`, `--version`, `-V` and `--url` are flags only in flag
//! position: before the verb, or among that verb's own flags. The word after
//! `--excerpt`, `--name` or `--step` is the value.

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
        "deedar {args:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stderr).as_ref(), "");
    String::from_utf8(out.stdout).expect("stdout is utf8")
}

fn store_url(home: &Path) -> String {
    format!("file://{}", home.join("store").display())
}

fn create_quote(home: &Path, url: &str, id: &str, flags: &[&str]) -> String {
    let mut args = vec!["--url", url, "create", "quote", "--id", id];
    args.extend_from_slice(flags);
    args.extend_from_slice(&["--src-url", "https://e.invalid", "--agent", "me"]);
    ok(home, &args)
}

fn line(text: &str, expected: &str) {
    assert!(text.lines().any(|line| line == expected), "{text}");
}

const USAGE: &str = "\
usage: deedar [--url FILE] create|get|list|trail|evidence|delete|leave|timestamp|current|log|export|import|check|vouch|migrate|host
deedar --help            this page
deedar create --help     kinds and flags, including --path
evidence, current and export take one id, several ids, or - to read them from stdin
log takes head, list, audit, backfill, prove ID, or bridge SIZE
import takes a satchel directory, and --allow-unsigned-head for a missing or unlisted signature
check takes a satchel directory, --since KEPT_HEAD, --bridge FILE and --keep FILE; vouch takes sign FILE or check FILE
host says whether the store's layout lists the host signing key, and exits 1 when it does not
host accept adds the host signing key to the layout as a signer, and leaves every other line
";

const CREATE_HELP: &str = "\
deedar create KIND --agent WHO [flags]
kinds: file set quote patch maildraft clip page form table procedure event
create file wants --path FILE and --agent WHO
create set wants --member PATH (repeatable) and --agent WHO
--name NAME     defaults to the kind
--path FILE     the bytes for kind file (not --field path=)
--supersedes ID the deed this one revises
the same create again prints the deed it froze and exits 0; other bytes
under the same name are refused: revise with --name NAME-v2 --supersedes ID
the last stdout line is accession=ID: cite that ID, not the file= line
";

#[test]
fn an_excerpt_equal_to_help_is_stored() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = store_url(home);
    let made = create_quote(home, &url, "deed-quote-h", &["--excerpt", "--help"]);
    line(&made, "excerpt=--help");
    line(&made, "accession=deed-quote-h");
    let got = ok(home, &["--url", &url, "get", "deed-quote-h"]);
    line(&got, "excerpt=--help");
}

#[test]
fn an_excerpt_equal_to_short_version_is_stored() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = store_url(home);
    let made = create_quote(home, &url, "deed-quote-v", &["--excerpt", "-V"]);
    line(&made, "excerpt=-V");
    line(&made, "accession=deed-quote-v");
    let got = ok(home, &["--url", &url, "get", "deed-quote-v"]);
    line(&got, "excerpt=-V");
}

#[test]
fn a_name_equal_to_url_is_stored() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = store_url(home);
    let made = create_quote(
        home,
        &url,
        "deed-quote-u",
        &["--name", "--url", "--excerpt", "x"],
    );
    assert!(made.contains("name=--url"), "{made}");
    line(&made, "accession=deed-quote-u");
    let got = ok(home, &["--url", &url, "get", "deed-quote-u"]);
    assert!(
        got.lines()
            .any(|line| line.starts_with("id=deed-quote-u ") && line.contains("name=--url")),
        "{got}"
    );
    line(&got, "excerpt=x");
}

#[test]
fn a_step_equal_to_version_is_stored() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = store_url(home);
    // `get` prints the step count. The misread prints `deedar <version>` and
    // stores nothing, so the accession line is what shows the step was kept.
    let made = ok(
        home,
        &[
            "--url",
            &url,
            "create",
            "procedure",
            "--id",
            "deed-procedure-version",
            "--step",
            "--version",
            "--agent",
            "me",
        ],
    );
    line(&made, "steps=1");
    line(&made, "accession=deed-procedure-version");
    assert!(!made.starts_with("deedar "), "{made}");
    let got = ok(home, &["--url", &url, "get", "deed-procedure-version"]);
    line(
        &got,
        "id=deed-procedure-version kind=procedure name=procedure",
    );
    line(&got, "steps=1");
}

#[test]
fn help_and_version_print_the_same_pages() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    assert_eq!(ok(home, &["--help"]), USAGE);
    assert_eq!(ok(home, &["-h"]), USAGE);
    assert_eq!(ok(home, &["create", "--help"]), CREATE_HELP);
    assert_eq!(ok(home, &["create", "file", "--help"]), CREATE_HELP);
    let nowhere = home.join("no-such-store");
    let url = format!("file://{}", nowhere.display());
    assert_eq!(ok(home, &["--url", &url, "create", "--help"]), CREATE_HELP);
    let version = format!("deedar {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(ok(home, &["-V"]), version);
    assert_eq!(ok(home, &["--version"]), version);
}

#[test]
fn list_url_after_the_verb_selects_that_store() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = store_url(home);
    let other = format!("file://{}", home.join("other").display());
    create_quote(
        home,
        &url,
        "deed-quote-listed",
        &["--excerpt", "kept in this store"],
    );
    let listed = ok(home, &["list", "--url", &url]);
    assert!(listed.contains("deed-quote-listed"), "{listed}");
    line(&listed, "excerpt=kept in this store");
    let empty = ok(home, &["list", "--url", &other]);
    assert_eq!(empty, "");
}
