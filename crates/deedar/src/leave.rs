//! Content Credentials sidecar when a file, set, or clip leaves.

use std::fs;
use std::path::{Path, PathBuf};

use deed::{DeedId, Error, Kind, Result};
use sha2::{Digest, Sha256};

use crate::FsStore;

/// Copy write-once product bytes to `dest/{id}/` and write `manifest.json`.
/// Then request an RFC 3161 timestamp over the evidence bytes (same sitting).
pub fn leave(store: &mut FsStore, id: &DeedId, dest: &Path) -> Result<PathBuf> {
    let deed = store.get(id)?;
    match deed.kind {
        Kind::File | Kind::Set | Kind::Clip => {}
        other => {
            return Err(Error::InvalidBody(format!(
                "leave is for file, set, and clip, not {}",
                other.token()
            )));
        }
    }
    let out = dest.join(id.as_str());
    fs::create_dir_all(&out).map_err(|e| Error::Io(e.to_string()))?;
    let mut concat = Vec::new();
    for path in &deed.paths {
        let Some(hex) = path.to_str().and_then(|s| s.strip_prefix("sha256:")) else {
            return Err(Error::InvalidBody(format!(
                "path {} is not write-once",
                path.display()
            )));
        };
        let bytes =
            fs::read(store.dir().join("bytes").join(hex)).map_err(|e| Error::Io(e.to_string()))?;
        fs::write(out.join(hex), &bytes).map_err(|e| Error::Io(e.to_string()))?;
        concat.extend_from_slice(&bytes);
    }
    let hash = hex_encode(&Sha256::digest(&concat));
    let manifest = format!(
        "{{\n  \"claim_generator\": \"deedar\",\n  \"deed\": \"{}\",\n  \"kind\": \"{}\",\n  \"hash\": \"{hash}\"\n}}\n",
        deed.id,
        deed.kind.token(),
    );
    fs::write(out.join("manifest.json"), manifest).map_err(|e| Error::Io(e.to_string()))?;
    store.timestamp(id)?;
    Ok(out)
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
