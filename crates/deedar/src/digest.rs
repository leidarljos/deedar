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
///
/// A deed digest is the line the log wrote at accession, so that lookup reads
/// the log instead of every frame. A product path is not a log digest, and
/// neither is a deed the log has not caught up to, so both still walk the
/// shelf. The log also names a tombstone, which `list` skips, and that is not
/// a hit.
pub fn resolve(store: &FsStore, raw: &str) -> Result<DeedId> {
    if let Ok(id) = DeedId::parse(raw) {
        return Ok(id);
    }
    if !is_sha256_addr(raw) {
        return Err(Error::InvalidId(raw.into()));
    }
    if let Some(id) = logged_deed(store, raw) {
        return Ok(id);
    }
    // The log missed. The shelf still has product paths and unlogged deeds.
    // A log that cannot be read takes the same path: the shelf is the answer
    // a store with no log already gave.
    listed(store, raw)
}

/// The live deed whose log line is `raw`.
///
/// `list` walks accessions in order and keeps the last digest match. A
/// tombstone, or a frame that no longer hashes to the line, is left for
/// `listed`.
///
/// The log is read as text. Only a line whose second field is `raw` is parsed.
/// `Entry::line` writes `id`, digest, time, tab-separated, and a miss still
/// walks the shelf, so building an entry for every line would sit on top of
/// that walk.
fn logged_deed(store: &FsStore, raw: &str) -> Option<DeedId> {
    let text = std::fs::read_to_string(store.dir().join("log")).ok()?;
    let mut hits = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let (_, rest) = line.split_once('\t')?;
        let (digest, _) = rest.split_once('\t')?;
        if digest != raw {
            continue;
        }
        let entry = crate::log::Entry::parse(line).ok()?;
        if let Ok(id) = DeedId::parse(&entry.id) {
            hits.push(id);
        }
    }
    hits.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    hits.dedup();
    let mut found = None;
    for id in hits {
        // `get` refuses a tombstone. One frame is hashed so a stale line
        // cannot answer for bytes it no longer describes.
        let Ok(deed) = store.get(&id) else {
            continue;
        };
        if deed_digest(&deed) == raw {
            found = Some(id);
        }
    }
    found
}

/// Every listed deed, which is the walk a `sha256:` used before the log.
fn listed(store: &FsStore, raw: &str) -> Result<DeedId> {
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
    // SHA-256 is 32 octets (FIPS 180-4); the text form is two hex characters each.
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
