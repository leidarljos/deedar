//! `deedar check` on a bag whose product bytes were rewritten after export.
//!
//! Import hashes each `bytes/<sha256>` file and refuses one that does not
//! hash to its name. Check only required the file to be regular, so the
//! same bag passed `deedar check` and failed `import`.

use std::path::Path;
use std::process::{Command, Output};

fn deedar(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deedar"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("DEEDAR_HOST_SIGNING_KEY", "off")
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
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Swapping the contents of `bytes/<sha256>` after export has to make
/// `deedar check` exit nonzero. The name is the digest the bag claims.
#[test]
fn tampered_product_bytes_fail_check() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let url = format!("file://{}", home.join("store").display());
    std::fs::write(home.join("letter.txt"), b"the letter itself").unwrap();
    ok(
        home,
        &[
            "--url",
            &url,
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
    ok(
        home,
        &[
            "--url",
            &url,
            "export",
            "--into",
            "bag/data/deeds",
            "deed-file-letter",
        ],
    );
    ok(home, &["--url", &url, "check", "bag"]);

    let bytes = home.join("bag/data/deeds/deed-file-letter/bytes");
    let mut files: Vec<_> = std::fs::read_dir(&bytes)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "export wrote one product file");
    let file = files.pop().unwrap();
    let name = file.file_name().unwrap().to_string_lossy().into_owned();
    std::fs::write(&file, b"not the letter").unwrap();

    let out = deedar(home, &["--url", &url, "check", "bag"]);
    assert!(!out.status.success(), "tampered bytes passed check");
    let err = String::from_utf8_lossy(&out.stderr);
    let want = format!("deed-file-letter: bytes/{name} hashes to sha256:");
    assert!(err.contains(&want), "{err}");
}
