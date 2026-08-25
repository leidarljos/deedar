use sha2::{Digest, Sha256};

use deed::{Deed, DeedId, Error, Result};

use crate::wire;
use crate::FsStore;

/// Content address of the canonical deed bytes.
pub fn deed_digest(deed: &Deed) -> String {
    let hash = Sha256::digest(wire::deed_bytes(deed));
    format!("sha256:{}", hex_encode(&hash))
}

/// Slug accession, or `sha256:` of deed bytes or a product path on a listed deed.
pub fn resolve(store: &FsStore, raw: &str) -> Result<DeedId> {
    if let Ok(id) = DeedId::parse(raw) {
        return Ok(id);
    }
    if !is_sha256_addr(raw) {
        return Err(Error::InvalidId(raw.into()));
    }
    let mut digest_hit = None;
    let mut path_hits = Vec::new();
    for deed in store.list()? {
        if deed_digest(&deed) == raw {
            digest_hit = Some(deed.id.clone());
        }
        if deed
            .paths
            .iter()
            .any(|p| p.to_str().is_some_and(|s| s == raw))
        {
            path_hits.push(deed.id);
        }
    }
    if let Some(id) = digest_hit {
        return Ok(id);
    }
    match path_hits.as_slice() {
        [id] => Ok(id.clone()),
        [] => Err(Error::NotFound(raw.into())),
        _ => Err(Error::InvalidId(format!(
            "{raw} names {} deeds",
            path_hits.len()
        ))),
    }
}

fn is_sha256_addr(raw: &str) -> bool {
    let Some(hex) = raw.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}
