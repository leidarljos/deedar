use std::fs;
use std::path::Path;

use deed::{Error, Result};

use crate::fs::FsStore;

/// Write `{store}/layout` as `1\n` when the file is missing.
pub fn ensure_layout(dir: &Path) -> Result<()> {
    let path = dir.join("layout");
    if !path.exists() {
        fs::write(&path, b"1\n").map_err(|e| Error::Io(e.to_string()))?;
    }
    Ok(())
}

/// The layout of a store this host has just created: version 1, and the
/// host's own key as its one signer.
pub fn write_fresh_layout(dir: &Path, public: &[u8; 32]) -> Result<()> {
    let text = format!(
        "1\nsigner = {}:{}\n",
        crate::attest::ED25519,
        crate::attest::to_hex(public)
    );
    fs::write(dir.join("layout"), text).map_err(|e| Error::Io(e.to_string()))
}

/// Add `signer = ed25519:<hex>` to `{store}/layout` unless the layout
/// already lists that key. Every other line is kept as it was. Returns
/// whether the file changed.
///
/// # Errors
///
/// Fails when the layout cannot be read or written.
pub fn add_signer(dir: &Path, public: &[u8; 32]) -> Result<bool> {
    ensure_layout(dir)?;
    if crate::attest::Policy::read(dir)?.signers.contains(public) {
        return Ok(false);
    }
    let path = dir.join("layout");
    let mut text = fs::read_to_string(&path).map_err(|e| Error::Io(e.to_string()))?;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "signer = {}:{}\n",
        crate::attest::ED25519,
        crate::attest::to_hex(public)
    ));
    // Write beside it and rename, so a reader never sees half a layout. The
    // name carries the pid, so two writers never share one temporary file,
    // and the new file keeps the old one's mode.
    let tmp = dir.join(format!("layout.tmp.{}", std::process::id()));
    let mode = fs::metadata(&path)
        .map_err(|e| Error::Io(e.to_string()))?
        .permissions();
    let written = fs::write(&tmp, text)
        .and_then(|()| fs::set_permissions(&tmp, mode))
        .and_then(|()| fs::rename(&tmp, &path));
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(Error::Io(e.to_string()));
    }
    Ok(true)
}

impl FsStore {
    pub fn migrate(&self) -> Result<()> {
        ensure_layout(self.dir())
    }
}
