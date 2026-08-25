use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::id::DeedId;

/// Prior deed, a path, or a URL. `trail` walks deed sources.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Deed(DeedId),
    Path(PathBuf),
    Url(String),
}

impl Source {
    pub fn url(raw: &str) -> Result<Self> {
        if !(raw.starts_with("http://")
            || raw.starts_with("https://")
            || raw.starts_with("file://"))
        {
            return Err(Error::InvalidUrl(raw.into()));
        }
        if raw.len() < 8 {
            return Err(Error::InvalidUrl(raw.into()));
        }
        Ok(Self::Url(raw.to_string()))
    }

    pub fn path(raw: impl Into<PathBuf>) -> Self {
        Self::Path(raw.into())
    }

    pub fn deed(id: DeedId) -> Self {
        Self::Deed(id)
    }
}
