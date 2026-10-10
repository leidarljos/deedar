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

impl FsStore {
    pub fn migrate(&self) -> Result<()> {
        ensure_layout(self.dir())
    }
}
