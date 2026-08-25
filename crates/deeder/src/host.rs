//! Host-issued evidence sidecar. Domain-tagged keyed hash, not the store HMAC.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use deed::{DeedId, Error, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Domain tag so a store HMAC is never a valid host check.
const HOST_DOMAIN: &[u8] = b"deeder.host.v1";

/// `DEEDER_HOST_KEY` if set, else `{store}/../host.key`.
pub fn host_key_path(store_dir: &Path) -> PathBuf {
    match env::var_os("DEEDER_HOST_KEY") {
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
