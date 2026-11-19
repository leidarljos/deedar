//! Host-issued evidence sidecar, in two constructions: a keyed hash attests
//! integrity, an Ed25519 signature over the same canonical bytes attests
//! authorisation. A sidecar says which it is.

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
    // Same width as the store's writer key: one HMAC-SHA256 tag (FIPS 180-4).
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
    // A zero octet ends the domain, so the deed cannot be read as a longer tag.
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

/// Where a host signing key lives, when one is configured as a file.
///
/// `DEEDAR_HOST_SIGNING_KEY` names it. Unset, the key at
/// `$XDG_CONFIG_HOME/deedar/host.key` (`~/.config/deedar/host.key`) is used
/// when that file exists, so a seat that made one once signs from then on
/// with nothing set. `DEEDAR_HOST_SIGNING_KEY=off` signs nothing.
/// A value of 32 bytes that is not a file is the seed itself, and this
/// returns nothing because there is no path.
///
/// The private half never belongs beside the store: the point of the signature
/// is that whoever reads the store cannot produce one.
#[must_use]
pub fn signing_key_path() -> Option<PathBuf> {
    match env::var_os("DEEDAR_HOST_SIGNING_KEY") {
        Some(raw) if raw.is_empty() || raw == "off" => None,
        Some(raw) => {
            let path = PathBuf::from(&raw);
            path.is_file().then_some(path)
        }
        None => default_signing_key_path().filter(|path| path.is_file()),
    }
}

/// The path the seat's host key is looked for at when nothing names one.
#[must_use]
pub fn default_signing_key_path() -> Option<PathBuf> {
    let config = env::var_os("XDG_CONFIG_HOME")
        .filter(|raw| !raw.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("deedar").join("host.key"))
}

/// Load the 32-byte Ed25519 seed the host signs with, if it is configured.
///
/// RFC 8032 defines that seed as 32 octets. `DEEDAR_HOST_SIGNING_KEY` is read
/// by the first of these rules that fits:
///
/// 1. `off` signs nothing, and an empty value is unset.
/// 2. A value that names a file is that file, which has to be the 32 bytes.
/// 3. `seed:` then 32 bytes is that seed, whatever the bytes are, `/` and `\`
///    included.
/// 4. `hex:` then 64 hex digits is that seed.
/// 5. A bare value of exactly 32 bytes with no `/` and no `\` is that seed.
///
/// Anything else is a path: one that exists and is not a file, or one that is
/// not there, is refused. A seed that contains `/` or `\` therefore has to be
/// written with `seed:` or `hex:`, because it cannot be told from a path that
/// is missing.
///
/// Unset, [`default_signing_key_path`] is used when that file exists.
///
/// # Errors
///
/// Fails when the named path is not a file of 32 bytes, when `seed:` or `hex:`
/// does not carry a seed, and when the value is none of the forms above.
pub fn load_signing_key() -> Result<Option<SigningKey>> {
    match env::var_os("DEEDAR_HOST_SIGNING_KEY") {
        Some(raw) if raw.is_empty() => load_default_signing_key(),
        Some(raw) if raw == "off" => Ok(None),
        Some(raw) => load_configured_signing_key(&raw),
        None => load_default_signing_key(),
    }
}

fn load_default_signing_key() -> Result<Option<SigningKey>> {
    match default_signing_key_path() {
        Some(path) if path.is_file() => Ok(Some(read_seed_file(&path)?)),
        _ => Ok(None),
    }
}

fn load_configured_signing_key(raw: &std::ffi::OsStr) -> Result<Option<SigningKey>> {
    let path = PathBuf::from(raw);
    if path.is_file() {
        // `signing_key_path` is the file this variable names.
        let path = signing_key_path().unwrap_or(path);
        return Ok(Some(read_seed_file(&path)?));
    }
    if let Some(seed) = tagged_seed(raw)? {
        return Ok(Some(SigningKey::from_bytes(&seed)));
    }
    // A bare value with a separator is a path, even at 32 bytes: a missing
    // path must not be read as the seed. `seed:` and `hex:` are how a seed
    // that contains a separator is written.
    if !looks_like_path(raw) {
        if let Some(seed) = seed_bytes(raw) {
            return Ok(Some(SigningKey::from_bytes(&seed)));
        }
    }
    let shown = path.display();
    if looks_like_path(raw) || path.exists() {
        let seed_form = if raw.as_encoded_bytes().len() == 32 {
            "; if those 32 bytes are the seed, write it `seed:` then the bytes, or `hex:` then 64 hex digits"
        } else {
            ""
        };
        return Err(Error::Io(format!(
            "DEEDAR_HOST_SIGNING_KEY names {shown}, which is not a file{seed_form}; \
             write 32 bytes there (`head -c 32 /dev/urandom > {shown}`) or set DEEDAR_HOST_SIGNING_KEY=off"
        )));
    }
    Err(Error::Io(format!(
        "DEEDAR_HOST_SIGNING_KEY names {shown}, which is not a file and is not a 32-byte seed; \
         write 32 bytes to a file (`head -c 32 /dev/urandom > host.key`) and set DEEDAR_HOST_SIGNING_KEY to that path, \
         or set DEEDAR_HOST_SIGNING_KEY=off"
    )))
}

/// The `seed:` and `hex:` forms. `None` when the value has neither prefix.
fn tagged_seed(raw: &std::ffi::OsStr) -> Result<Option<[u8; 32]>> {
    let bytes = raw.as_encoded_bytes();
    if let Some(rest) = bytes.strip_prefix(b"seed:") {
        let seed: [u8; 32] = rest.try_into().map_err(|_| {
            Error::Io(format!(
                "DEEDAR_HOST_SIGNING_KEY=seed: takes exactly 32 bytes, and this is {}; \
                 `hex:` and 64 hex digits write the same seed",
                rest.len()
            ))
        })?;
        return Ok(Some(seed));
    }
    if let Some(rest) = bytes.strip_prefix(b"hex:") {
        let seed = std::str::from_utf8(rest)
            .ok()
            .and_then(crate::attest::from_hex)
            .ok_or_else(|| {
                Error::Io(
                    "DEEDAR_HOST_SIGNING_KEY=hex: takes exactly 64 hex digits, \
                     the 32 bytes of the seed"
                        .into(),
                )
            })?;
        return Ok(Some(seed));
    }
    Ok(None)
}

/// `/` or `\` means a bare value is a path, including a 32-byte one.
fn looks_like_path(raw: &std::ffi::OsStr) -> bool {
    let bytes = raw.as_encoded_bytes();
    bytes.contains(&b'/') || bytes.contains(&b'\\')
}

/// The value is a seed when it is exactly 32 bytes and no file has that name.
fn seed_bytes(raw: &std::ffi::OsStr) -> Option<[u8; 32]> {
    let bytes = raw.as_encoded_bytes();
    if bytes.len() != 32 {
        return None;
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(bytes);
    Some(seed)
}

fn read_seed_file(path: &Path) -> Result<SigningKey> {
    let bytes = fs::read(path).map_err(|e| Error::Io(e.to_string()))?;
    let seed: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
        Error::Io(format!(
            "DEEDAR_HOST_SIGNING_KEY must be a 32-byte seed; {} is {} bytes. \
             Write one with `head -c 32 /dev/urandom > {}`",
            path.display(),
            bytes.len(),
            path.display()
        ))
    })?;
    Ok(SigningKey::from_bytes(&seed))
}

/// What a sidecar file holds.
pub enum Sidecar {
    /// `ed25519:<128 hex>`. An Ed25519 signature is 64 octets (RFC 8032), and
    /// hex doubles that.
    Signature(Vec<u8>),
    /// Raw keyed-hash bytes, from before the signature existed.
    KeyedHash(Vec<u8>),
}

/// Read a sidecar and say which construction wrote it.
///
/// The signature form is text and self-describing; anything else is a raw hash.
#[must_use]
pub fn read_sidecar(bytes: &[u8]) -> Option<Sidecar> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if let Some(hex) = text.trim().strip_prefix(&format!("{ED25519}:")) {
            // 64 signature octets, two hex characters each.
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
/// [`Error::Evidence`] when no accepted key produced it.
pub fn verify_ed25519(
    signers: &BTreeSet<[u8; 32]>,
    deed_bytes: &[u8],
    unix_time: u64,
    signature: &[u8],
) -> Result<()> {
    // Ed25519 signatures are 64 octets (RFC 8032).
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
    // Domain, one zero octet so the tag cannot run into the deed, the deed,
    // and the time as a little-endian `u64` (8 octets, the schema's `UInt64`).
    let mut message = Vec::with_capacity(HOST_DOMAIN.len() + 1 + deed_bytes.len() + 8);
    message.extend_from_slice(HOST_DOMAIN);
    message.push(0);
    message.extend_from_slice(deed_bytes);
    message.extend_from_slice(&unix_time.to_le_bytes());
    message
}
