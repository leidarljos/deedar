//! Host-issued evidence sidecar.
//!
//! Two constructions, and they claim different things.
//!
//! The keyed hash came first and attests integrity: these bytes are the bytes
//! the host saw. It cannot attest authorisation, because a verifier holding the
//! issuer's key can mint what it checks, and the key sits beside the store where
//! the agent being vouched for can read it.
//!
//! The signature is the one a store can demand. Ed25519 over the same canonical
//! bytes, so nothing about the canonical form changes; the verifier holds only
//! a public key and cannot mint. Which keys count lives in the layout, so a
//! reader who did not set the store up can see what it accepts.
//!
//! A sidecar written by either construction stays readable, and a sidecar says
//! which one it is rather than leaving a reader to guess by length.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use std::collections::BTreeSet;

use deed::{DeedId, Error, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::attest::ED25519;

type HmacSha256 = Hmac<Sha256>;

/// Domain tag so a store HMAC is never a valid host check.
const HOST_DOMAIN: &[u8] = b"deedar.host.v1";

/// `DEEDAR_HOST_KEY` if set, else `{store}/../host.key`.
pub fn host_key_path(store_dir: &Path) -> PathBuf {
    match env::var_os("DEEDAR_HOST_KEY") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => store_dir.join("..").join("host.key"),
    }
}

pub fn sidecar(store_dir: &Path, id: &DeedId) -> PathBuf {
    store_dir.join("deeds").join(format!("{id}.host"))
}

pub fn load(store_dir: &Path) -> Result<Option<[u8; 32]>> {
    let path = host_key_path(store_dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|e| Error::Io(e.to_string()))?;
    if bytes.len() != 32 {
        return Err(Error::Io("host.key must be 32 bytes".into()));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(Some(key))
}

fn host_mac(key: &[u8; 32], deed_bytes: &[u8], unix_time: u64) -> Result<HmacSha256> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|e| Error::Io(e.to_string()))?;
    mac.update(HOST_DOMAIN);
    mac.update(&[0]);
    mac.update(deed_bytes);
    mac.update(&unix_time.to_le_bytes());
    Ok(mac)
}

pub fn sign(key: &[u8; 32], deed_bytes: &[u8], unix_time: u64) -> Result<Vec<u8>> {
    Ok(host_mac(key, deed_bytes, unix_time)?
        .finalize()
        .into_bytes()
        .to_vec())
}

pub fn verify(key: &[u8; 32], deed_bytes: &[u8], unix_time: u64, signature: &[u8]) -> Result<()> {
    host_mac(key, deed_bytes, unix_time)?
        .verify_slice(signature)
        .map_err(|_| Error::Evidence("host keyed hash".into()))
}

/// Where a host signing key lives, when one is configured.
///
/// The private half never belongs beside the store: the point of the signature
/// is that whoever reads the store cannot produce one.
#[must_use]
pub fn signing_key_path() -> Option<PathBuf> {
    env::var_os("DEEDAR_HOST_SIGNING_KEY")
        .filter(|raw| !raw.is_empty())
        .map(PathBuf::from)
}

/// Load the 32-byte Ed25519 seed the host signs with, if it is configured.
///
/// # Errors
///
/// Fails when the file exists and is not a 32-byte seed, because a signing key
/// that is nearly right is a sidecar nobody can verify.
pub fn load_signing_key() -> Result<Option<SigningKey>> {
    let Some(path) = signing_key_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|e| Error::Io(e.to_string()))?;
    let seed: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| Error::Io("DEEDAR_HOST_SIGNING_KEY must be a 32 byte seed".into()))?;
    Ok(Some(SigningKey::from_bytes(&seed)))
}

/// What a sidecar file holds.
pub enum Sidecar {
    /// `ed25519:<128 hex>`, verifiable against a public key.
    Signature(Vec<u8>),
    /// Raw keyed-hash bytes, from before the signature existed.
    KeyedHash(Vec<u8>),
}

/// Read a sidecar and say which construction wrote it.
///
/// The signature form is text and self-describing; anything else is the older
/// raw hash, which is how a store written before this keeps opening.
#[must_use]
pub fn read_sidecar(bytes: &[u8]) -> Option<Sidecar> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if let Some(hex) = text.trim().strip_prefix(&format!("{ED25519}:")) {
            if hex.len() == 128 {
                let mut raw = Vec::with_capacity(64);
                for pair in hex.as_bytes().chunks_exact(2) {
                    let pair = std::str::from_utf8(pair).ok()?;
                    raw.push(u8::from_str_radix(pair, 16).ok()?);
                }
                return Some(Sidecar::Signature(raw));
            }
            return None;
        }
    }
    (!bytes.is_empty()).then(|| Sidecar::KeyedHash(bytes.to_vec()))
}

/// Sign the canonical bytes, in the form a sidecar file holds.
#[must_use]
pub fn sign_ed25519(key: &SigningKey, deed_bytes: &[u8], unix_time: u64) -> Vec<u8> {
    let signature = key.sign(&attested(deed_bytes, unix_time));
    format!("{ED25519}:{}", crate::attest::to_hex(&signature.to_bytes())).into_bytes()
}

/// Check a signature against every key the store accepts.
///
/// # Errors
///
/// [`Error::Evidence`] when no accepted key produced it, which is the same
/// answer for a wrong key and an absent one: the store cannot tell them apart
/// and neither should a caller.
pub fn verify_ed25519(
    signers: &BTreeSet<[u8; 32]>,
    deed_bytes: &[u8],
    unix_time: u64,
    signature: &[u8],
) -> Result<()> {
    let raw: [u8; 64] = signature
        .try_into()
        .map_err(|_| Error::Evidence("host signature length".into()))?;
    let signature = Signature::from_bytes(&raw);
    let message = attested(deed_bytes, unix_time);
    for signer in signers {
        if let Ok(key) = VerifyingKey::from_bytes(signer) {
            if key.verify_strict(&message, &signature).is_ok() {
                return Ok(());
            }
        }
    }
    Err(Error::Evidence("host signature".into()))
}

/// The bytes both constructions cover, so the canonical form is one thing.
/// The attested message, for a test that checks two domains stay apart.
#[cfg(test)]
#[must_use]
pub fn attested_for_test(deed_bytes: &[u8], unix_time: u64) -> Vec<u8> {
    attested(deed_bytes, unix_time)
}

fn attested(deed_bytes: &[u8], unix_time: u64) -> Vec<u8> {
    let mut message = Vec::with_capacity(HOST_DOMAIN.len() + 1 + deed_bytes.len() + 8);
    message.extend_from_slice(HOST_DOMAIN);
    message.push(0);
    message.extend_from_slice(deed_bytes);
    message.extend_from_slice(&unix_time.to_le_bytes());
    message
}
