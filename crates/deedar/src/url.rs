use std::path::{Path, PathBuf};

use deed::{Error, Result};

use crate::fs::FsStore;

/// `DEEDAR_URL` contract: `file:///abs/path` is the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreUrl {
    dir: PathBuf,
}

impl StoreUrl {
    pub fn parse(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        let path = if let Some(rest) = raw.strip_prefix("file://") {
            PathBuf::from(rest)
        } else if raw.starts_with('/') {
            PathBuf::from(raw)
        } else {
            return Err(Error::InvalidUrl(raw.into()));
        };
        if !path.is_absolute() {
            return Err(Error::InvalidUrl(raw.into()));
        }
        Ok(Self { dir: path })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn as_url(&self) -> String {
        format!("file://{}", self.dir.display())
    }
}

/// Where a seat keeps its store when nobody says otherwise.
///
/// `$XDG_DATA_HOME/deedar/store`, or `~/.local/share/deedar/store`. The
/// command line and the MCP server both ask here, so they open one store.
#[must_use]
pub fn default_store() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })?;
    Some(base.join("deedar").join("store"))
}

pub fn open(raw: &str) -> Result<FsStore> {
    let url = StoreUrl::parse(raw)?;
    FsStore::open(url.dir())
}
