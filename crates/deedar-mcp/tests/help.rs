//! `deedar-mcp --help` is a page, not a protocol error.

use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_deedar-mcp"))
        .args(args)
        .env("DEEDAR_HOST_SIGNING_KEY", "off")
        .output()
        .expect("deedar-mcp runs")
}

#[test]
fn help_and_version_print_and_an_argument_is_refused() {
    let help = run(&["--help"]);
    assert!(
        help.status.success(),
        "{}",
        String::from_utf8_lossy(&help.stderr)
    );
    let page = String::from_utf8(help.stdout).expect("stdout is utf8");
    assert!(page.contains("DEEDAR_URL"), "{page}");
    assert!(page.contains("deedar-mcp --version"), "{page}");
    assert!(help.stderr.is_empty());

    let short = run(&["-h"]);
    assert_eq!(
        String::from_utf8(short.stdout).expect("stdout is utf8"),
        page
    );

    let version = run(&["--version"]);
    assert!(version.status.success());
    let text = String::from_utf8(version.stdout).expect("stdout is utf8");
    assert_eq!(text, format!("deedar-mcp {}\n", env!("CARGO_PKG_VERSION")));

    let extra = run(&["foo"]);
    assert_eq!(extra.status.code(), Some(1));
    assert!(extra.stdout.is_empty());
    let err = String::from_utf8(extra.stderr).expect("stderr is utf8");
    assert_eq!(
        err,
        "deedar-mcp takes no arguments and speaks on standard input; `deedar-mcp --help`\n"
    );
    assert!(!err.contains("initialize"));
}

/// With no client on standard input the server says what it expects, on one
/// line, and exits 1 instead of ending on a protocol error.
#[test]
fn standard_input_with_no_client_names_what_the_server_expects() {
    let out = Command::new(env!("CARGO_BIN_EXE_deedar-mcp"))
        .env("DEEDAR_HOST_SIGNING_KEY", "off")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("deedar-mcp runs");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8(out.stderr).expect("stderr is utf8");
    assert_eq!(
        err,
        "deedar-mcp: expects an MCP client on standard input, \
         and it closed before one sent initialize; `deedar-mcp --help`\n"
    );
    assert!(!err.contains("connection closed"), "{err}");
}
