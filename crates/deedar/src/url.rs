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

pub fn open(raw: &str) -> Result<FsStore> {
    let url = StoreUrl::parse(raw)?;
    FsStore::open(url.dir())
}
