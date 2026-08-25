use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::kind::Kind;

/// Accession: `deed-<kind>-<slug>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeedId(String);

impl DeedId {
    pub fn parse(raw: &str) -> Result<Self> {
        let rest = raw
            .strip_prefix("deed-")
            .ok_or_else(|| Error::InvalidId(raw.into()))?;
        let (kind, slug) = rest
            .split_once('-')
            .ok_or_else(|| Error::InvalidId(raw.into()))?;
        Kind::parse(kind)?;
        if slug.is_empty()
            || !slug
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(Error::InvalidId(raw.into()));
        }
        Ok(Self(raw.to_string()))
    }

    pub fn mint(kind: &Kind, slug: &str) -> Result<Self> {
        Self::parse(&format!("deed-{}-{slug}", kind.token()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DeedId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
