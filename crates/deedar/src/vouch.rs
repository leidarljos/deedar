//! Signing a file that somebody else will open.
//!
//! A satchel arrives with a manifest, and checking it against that manifest
//! catches a payload that was corrupted or truncated on the way. It does not
//! catch a manifest that was rewritten, because a receiver who recomputes the
//! digests from the bag they were handed is checking the bag against itself.
//! Integrity and authenticity are different questions and a hash only answers
//! the first.
//!
//! So the manifest gets a signature. It is the right thing to sign because it
//! already covers the whole payload by construction: signing it is signing
//! everything it lists, and the checker that objects to an unlisted file is
//! what stops that being a loophole.
//!
//! The signature lives beside the file rather than inside it, so the file it
//! covers is byte-for-byte the one a reader would have had anyway, and a
//! reader who does not care about signatures is not handed a format they have
//! to strip.
//!
//! This is deedar's because the keys are. The private half is named by
//! `DEEDAR_HOST_SIGNING_KEY` and belongs nowhere near the store it vouches
//! for: the point of a signature is that whoever reads the thing cannot
//! produce one.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use deed::{Error, Result};

use crate::attest::{from_hex, to_hex, ED25519};

/// Domain separation, so a signature over a manifest cannot be replayed as a
/// signature over anything else this key signs.
const DOMAIN: &[u8] = b"deedar-vouch-v1";

/// What a detached signature file holds.
///
/// The public key travels with the signature. That does not make the
/// signature trustworthy on its own, which is the point of a signer list: a
/// receiver checks that the key is one they accept, and the file only saves
/// them guessing which key to try.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vouch {
    /// The verifying key, as 32 bytes.
    pub signer: [u8; 32],
    /// The signature over the covered bytes.
    pub signature: [u8; 64],
}

impl Vouch {
    /// The text a `.sig` file holds: scheme, key, signature.
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{ED25519} {} {}\n",
            to_hex(&self.signer),
            to_hex(&self.signature)
        )
    }

    /// Parse a `.sig` file.
    ///
    /// # Errors
    ///
    /// Fails when the line is not this scheme, a 32-byte key and a 64-byte
    /// signature. A signature that is nearly right is one nobody can check.
    pub fn parse(text: &str) -> Result<Self> {
        let mut parts = text.split_whitespace();
        let (Some(scheme), Some(key), Some(sig), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(Error::Evidence("vouch: not a signature line".into()));
        };
        if scheme != ED25519 {
            return Err(Error::Evidence(format!("vouch: scheme {scheme:?}")));
        }
        let signer = from_hex(key).ok_or_else(|| Error::Evidence("vouch: key".into()))?;
        if sig.len() != 128 {
            return Err(Error::Evidence("vouch: signature length".into()));
        }
        let mut signature = [0u8; 64];
        for (slot, pair) in signature.iter_mut().zip(sig.as_bytes().chunks_exact(2)) {
            let pair =
                std::str::from_utf8(pair).map_err(|_| Error::Evidence("vouch: hex".into()))?;
            *slot =
                u8::from_str_radix(pair, 16).map_err(|_| Error::Evidence("vouch: hex".into()))?;
        }
        Ok(Self { signer, signature })
    }
}

/// The manifest inside a satchel, which is the file worth signing.
///
/// A receiver is handed a directory, not a filename. Making them know which
/// file carries the digests before they can check who signed them is making
/// them learn the format in order to verify it.
#[must_use]
pub fn manifest_in(dir: &Path) -> PathBuf {
    dir.join("manifest-sha256.txt")
}

/// Where the signature for `path` lives.
#[must_use]
pub fn beside(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".sig");
    PathBuf::from(name)
}

/// The bytes a signature actually covers.
fn covered(bytes: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(DOMAIN.len() + 1 + bytes.len());
    message.extend_from_slice(DOMAIN);
    message.push(0);
    message.extend_from_slice(bytes);
    message
}

/// Sign a file, writing the signature beside it.
///
/// # Errors
///
/// Fails when no signing key is configured, or the file cannot be read or the
/// signature written.
pub fn sign(path: &Path) -> Result<PathBuf> {
    let key: SigningKey = crate::host::load_signing_key()?.ok_or_else(|| {
        Error::Evidence(
            "no signing key: set DEEDAR_HOST_SIGNING_KEY to a 32 byte seed outside the store"
                .into(),
        )
    })?;
    let bytes = std::fs::read(path).map_err(|e| Error::Io(e.to_string()))?;
    let signature = key.sign(&covered(&bytes));
    let vouch = Vouch {
        signer: key.verifying_key().to_bytes(),
        signature: signature.to_bytes(),
    };
    let out = beside(path);
    std::fs::write(&out, vouch.render()).map_err(|e| Error::Io(e.to_string()))?;
    Ok(out)
}

/// Check the signature beside a file against the keys a reader accepts.
///
/// An empty `accept` checks the signature against the key the file names,
/// which proves the bytes and the signature go together and nothing about who
/// made them. That is worth telling apart from a real check, so it is reported
/// rather than silently treated as success.
///
/// # Errors
///
/// Fails when the signature is absent, malformed, does not verify, or names a
/// key the reader does not accept.
pub fn check(path: &Path, accept: &BTreeSet<[u8; 32]>) -> Result<Checked> {
    let sig_path = beside(path);
    let text = std::fs::read_to_string(&sig_path)
        .map_err(|_| Error::Evidence(format!("no signature at {}", sig_path.display())))?;
    let vouch = Vouch::parse(&text)?;
    let bytes = std::fs::read(path).map_err(|e| Error::Io(e.to_string()))?;

    let key = VerifyingKey::from_bytes(&vouch.signer)
        .map_err(|_| Error::Evidence("vouch: not a verifying key".into()))?;
    key.verify(&covered(&bytes), &Signature::from_bytes(&vouch.signature))
        .map_err(|_| Error::Evidence("vouch: the signature does not cover these bytes".into()))?;

    if accept.is_empty() {
        return Ok(Checked {
            signer: vouch.signer,
            accepted: false,
        });
    }
    if !accept.contains(&vouch.signer) {
        return Err(Error::Evidence(format!(
            "vouch: signed by {}, which is not a key this reader accepts",
            to_hex(&vouch.signer)
        )));
    }
    Ok(Checked {
        signer: vouch.signer,
        accepted: true,
    })
}

/// What a check established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The key that signed.
    pub signer: [u8; 32],
    /// Whether that key was on the reader's list, as opposed to merely being
    /// the one the file named.
    pub accepted: bool,
}

impl Checked {
    /// One line, which says which of the two questions was answered.
    #[must_use]
    pub fn render(&self) -> String {
        if self.accepted {
            format!("signed by {} (accepted)\n", to_hex(&self.signer))
        } else {
            format!(
                "signed by {}, and no signer list was given, so this says the bytes and the \
                 signature go together and nothing about who made them\n",
                to_hex(&self.signer)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV: Mutex<()> = Mutex::new(());

    fn with_key<T>(seed: [u8; 32], body: impl FnOnce(&Path) -> T) -> T {
        let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let key = dir.path().join("signing.key");
        std::fs::write(&key, seed).expect("write key");
        // SAFETY: ENV is held for the whole sitting that reads the variable.
        unsafe {
            std::env::set_var("DEEDAR_HOST_SIGNING_KEY", &key);
        }
        let out = body(dir.path());
        unsafe {
            std::env::remove_var("DEEDAR_HOST_SIGNING_KEY");
        }
        drop(guard);
        out
    }

    /// A signature covers the bytes it was made over, and stops covering them
    /// the moment they change.
    #[test]
    fn a_signature_stops_covering_bytes_that_moved() {
        with_key([7u8; 32], |dir| {
            let manifest = dir.join("manifest-sha256.txt");
            std::fs::write(&manifest, "abc  data/one\n").expect("write");
            sign(&manifest).expect("signs");

            let checked = check(&manifest, &BTreeSet::new()).expect("checks");
            // No signer list means the weaker of the two answers, and it says so.
            assert!(!checked.accepted);
            assert!(checked.render().contains("nothing about who made them"));

            std::fs::write(&manifest, "abc  data/one\ndef  data/two\n").expect("rewrite");
            let err = check(&manifest, &BTreeSet::new()).expect_err("a rewrite passed");
            assert!(format!("{err}").contains("does not cover"), "{err}");
        });
    }

    /// A reader who says which keys they accept gets the stronger answer, and
    /// a signature from anyone else is refused rather than reported.
    #[test]
    fn a_key_nobody_accepts_is_refused() {
        with_key([9u8; 32], |dir| {
            let manifest = dir.join("manifest-sha256.txt");
            std::fs::write(&manifest, "abc  data/one\n").expect("write");
            sign(&manifest).expect("signs");

            let mine = SigningKey::from_bytes(&[9u8; 32])
                .verifying_key()
                .to_bytes();
            let stranger = SigningKey::from_bytes(&[1u8; 32])
                .verifying_key()
                .to_bytes();

            let checked = check(&manifest, &BTreeSet::from([mine])).expect("checks");
            assert!(checked.accepted);
            assert_eq!(checked.signer, mine);

            let err = check(&manifest, &BTreeSet::from([stranger]))
                .expect_err("a stranger's signature was accepted");
            assert!(
                format!("{err}").contains("not a key this reader accepts"),
                "{err}"
            );
        });
    }

    /// The signature is domain separated, so one over a manifest cannot be
    /// replayed as one over a deed.
    #[test]
    fn a_manifest_signature_is_not_a_deed_signature() {
        let bytes = b"the same bytes";
        assert_ne!(covered(bytes), crate::host::attested_for_test(bytes, 0));
    }

    /// A signature file that is not one is refused rather than half read.
    #[test]
    fn a_malformed_signature_is_refused() {
        assert!(Vouch::parse("").is_err());
        assert!(Vouch::parse("ed25519 short sig").is_err());
        assert!(Vouch::parse(&format!("rsa {} {}", "a".repeat(64), "b".repeat(128))).is_err());
        assert!(Vouch::parse(&format!("ed25519 {} {}", "a".repeat(64), "b".repeat(64))).is_err());
        let good = Vouch {
            signer: [3u8; 32],
            signature: [4u8; 64],
        };
        assert_eq!(Vouch::parse(&good.render()).expect("round trips"), good);
    }
}
