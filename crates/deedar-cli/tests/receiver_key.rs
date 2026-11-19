//! The getting-started page says how a receiver lists the sender's key.
//!
//! `deedar host accept` on the receiver adds this machine's key.
//! The sender's public half is a `signer` line copied from `deedar host`.

use std::path::{Path, PathBuf};

fn repo_file(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name)
}

#[test]
fn getting_started_names_the_receiver_signer_line() {
    for name in [
        "docs/orgmode/getting-started.org",
        "docs/source/getting-started.rst",
    ] {
        let text = std::fs::read_to_string(repo_file(name)).expect(name);
        for needle in [
            "head -c 32 /dev/urandom > host.key",
            "ed25519:<hex>",
            "signer = ed25519:<hex>",
            "adds the key this machine signs with",
        ] {
            assert!(text.contains(needle), "{name} is missing {needle}");
        }
    }
}
