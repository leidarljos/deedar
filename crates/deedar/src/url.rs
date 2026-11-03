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
/// `$XDG_DATA_HOME/deedar/store`, or `$HOME/.local/share/deedar/store`.
/// An empty or relative value is ignored for either variable: it would
/// follow the working directory, and two runs would not share one store.
/// The command line and the MCP server both ask here, so they open one store.
#[must_use]
pub fn default_store() -> Option<PathBuf> {
    seat_store(std::env::var_os("XDG_DATA_HOME"), std::env::var_os("HOME"))
}

/// `$XDG_DATA_HOME` when it is absolute, otherwise an absolute `$HOME`.
fn seat_store(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let base = absolute(xdg).or_else(|| absolute(home).map(|home| home.join(".local/share")))?;
    Some(base.join("deedar").join("store"))
}

/// An empty or relative directory is not a data directory.
fn absolute(raw: Option<std::ffi::OsString>) -> Option<PathBuf> {
    raw.map(PathBuf::from).filter(|path| path.is_absolute())
}

pub fn open(raw: &str) -> Result<FsStore> {
    let url = StoreUrl::parse(raw)?;
    FsStore::open(url.dir())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::seat_store;

    fn path(raw: &str) -> OsString {
        OsString::from(raw)
    }

    #[test]
    fn home_is_the_fallback_when_xdg_data_home_is_unset_or_not_absolute() {
        let want = PathBuf::from("/seat/.local/share/deedar/store");
        assert_eq!(seat_store(None, Some(path("/seat"))), Some(want.clone()));
        assert_eq!(
            seat_store(Some(path("")), Some(path("/seat"))),
            Some(want.clone())
        );
        assert_eq!(
            seat_store(Some(path("relative")), Some(path("/seat"))),
            Some(want)
        );
    }

    #[test]
    fn an_empty_or_relative_home_is_ignored() {
        assert_eq!(seat_store(None, Some(path(""))), None);
        assert_eq!(seat_store(None, Some(path("relative"))), None);
        assert_eq!(seat_store(Some(path("")), Some(path(""))), None);
        assert_eq!(
            seat_store(Some(path("relative")), Some(path("relative"))),
            None
        );
        assert_eq!(
            seat_store(Some(path("/xdg")), Some(path("relative"))),
            Some(PathBuf::from("/xdg/deedar/store"))
        );
    }
}
