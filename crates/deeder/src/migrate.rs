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

impl FsStore {
    pub fn migrate(&self) -> Result<()> {
        ensure_layout(self.dir())
    }
}
