use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use deed::{Deed, DeedId, Error, Result};

use crate::fs::{atomic_write, FsStore};

impl FsStore {
    /// Walk `{id}.successor` files to the tip. Stops when there is no
    /// successor, or the next id is tombstoned or missing.
    pub fn current(&self, id: &DeedId) -> Result<Deed> {
        let mut tip = id.clone();
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(tip.clone()) {
                break;
            }
            let Some(next) = read_successor(&self.successor_path(&tip))? else {
                break;
            };
            if self.tomb_path(&next).exists() || !self.deed_path(&next).exists() {
                break;
            }
            tip = next;
        }
        self.get(&tip)
    }

    pub(crate) fn record_supersedes(&self, new: &DeedId, prior: &DeedId) -> Result<()> {
        if new == prior {
            return Err(Error::InvalidBody("supersedes self".into()));
        }
        if !self.deed_path(prior).exists() {
            return Err(Error::NotFound(prior.to_string()));
        }
        let successor = self.successor_path(prior);
        if successor.exists() {
            return Err(Error::Frozen(prior.to_string()));
        }
        atomic_write(&self.supersedes_path(new), format!("{prior}\n").as_bytes())?;
        atomic_write(&successor, format!("{new}\n").as_bytes())?;
        Ok(())
    }

    pub(crate) fn successor_path(&self, id: &DeedId) -> PathBuf {
        self.dir().join("deeds").join(format!("{id}.successor"))
    }

    pub(crate) fn supersedes_path(&self, id: &DeedId) -> PathBuf {
        self.dir().join("deeds").join(format!("{id}.supersedes"))
    }
}

fn read_successor(path: &std::path::Path) -> Result<Option<DeedId>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|e| Error::Io(e.to_string()))?;
    let id = raw.trim();
    if id.is_empty() {
        return Ok(None);
    }
    Ok(Some(DeedId::parse(id)?))
}
