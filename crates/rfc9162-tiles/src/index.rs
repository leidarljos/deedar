//! The index beside each sealed bundle, `tile/index/<N>`, and the manifest
//! over them. Both derive from the bundles: a rebuild from the entries gives
//! the same bytes, so losing them loses nothing.
//!
//! A sidecar holds a [`Bloom`] filter over every key of the bundle and the
//! sorted (key, offset) pairs. The keys of an entry are its identity hash,
//! `SHA-256(entry)`, and whatever the log's [`KeyFn`] derives from it.
//!
//! Sidecar bytes: `0x01`, big-endian u16 pair count, big-endian u32 filter
//! length, the filter, then each pair as big-endian u16 key length, the key,
//! and the one-byte offset of the entry in the bundle.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::bloom::{Bloom, BITS_PER_KEY};
use crate::error::{Error, Result};
use crate::layout::MANIFEST;
use crate::store::TileStore;

/// Derives the lookup keys of an entry from its bytes. The identity hash is
/// always a key and need not be returned.
pub type KeyFn = Box<dyn Fn(&[u8]) -> Vec<Vec<u8>> + Send + Sync>;

/// A key function that adds no keys beyond the identity hash.
#[must_use]
pub fn no_keys() -> KeyFn {
    Box::new(|_| Vec::new())
}

/// `SHA-256(entry)`, the key every entry has.
#[must_use]
pub fn identity(entry: &[u8]) -> [u8; 32] {
    Sha256::digest(entry).into()
}

const VERSION: u8 = 1;

/// The sidecar of one sealed bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    /// (key, offset in the bundle), sorted, without duplicates.
    pub pairs: Vec<(Vec<u8>, u8)>,
    /// The filter over every key in `pairs`.
    pub filter: Bloom,
}

impl Sidecar {
    /// The sidecar over a bundle's entries with the log's key function.
    #[must_use]
    pub fn build<E: AsRef<[u8]>>(entries: &[E], keys: &KeyFn) -> Self {
        let mut set: BTreeSet<(Vec<u8>, u8)> = BTreeSet::new();
        for (offset, entry) in entries.iter().enumerate() {
            let entry = entry.as_ref();
            set.insert((identity(entry).to_vec(), offset as u8));
            for key in keys(entry) {
                set.insert((key, offset as u8));
            }
        }
        let pairs: Vec<(Vec<u8>, u8)> = set.into_iter().collect();
        let distinct: BTreeSet<&[u8]> = pairs.iter().map(|(k, _)| k.as_slice()).collect();
        let distinct: Vec<&[u8]> = distinct.into_iter().collect();
        let filter = Bloom::build(&distinct, BITS_PER_KEY);
        Self { pairs, filter }
    }

    /// Offsets of the entries carrying `key`, ascending.
    #[must_use]
    pub fn lookup(&self, key: &[u8]) -> Vec<u8> {
        if !self.filter.contains(key) {
            return Vec::new();
        }
        let start = self.pairs.partition_point(|(k, _)| k.as_slice() < key);
        self.pairs[start..]
            .iter()
            .take_while(|(k, _)| k.as_slice() == key)
            .map(|(_, offset)| *offset)
            .collect()
    }

    /// The smallest and largest key, or nothing for an empty bundle.
    #[must_use]
    pub fn key_range(&self) -> Option<(&[u8], &[u8])> {
        Some((
            self.pairs.first()?.0.as_slice(),
            self.pairs.last()?.0.as_slice(),
        ))
    }

    /// The sidecar as bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let filter = self.filter.to_bytes();
        let mut out = vec![VERSION];
        out.extend_from_slice(&(self.pairs.len() as u16).to_be_bytes());
        out.extend_from_slice(&(filter.len() as u32).to_be_bytes());
        out.extend_from_slice(&filter);
        for (key, offset) in &self.pairs {
            out.extend_from_slice(&(key.len() as u16).to_be_bytes());
            out.extend_from_slice(key);
            out.push(*offset);
        }
        out
    }

    /// The sidecar `to_bytes` wrote.
    pub fn from_bytes(path: &str, bytes: &[u8]) -> Result<Self> {
        let corrupt = |reason: &str| Error::Corrupt {
            path: path.to_string(),
            reason: reason.to_string(),
        };
        if bytes.first() != Some(&VERSION) {
            return Err(corrupt("not a version 1 sidecar"));
        }
        let count = usize::from(u16::from_be_bytes([
            *bytes.get(1).ok_or_else(|| corrupt("cut"))?,
            *bytes.get(2).ok_or_else(|| corrupt("cut"))?,
        ]));
        let filter_len = u32::from_be_bytes(
            bytes
                .get(3..7)
                .ok_or_else(|| corrupt("cut"))?
                .try_into()
                .expect("four bytes"),
        ) as usize;
        let filter = Bloom::from_bytes(bytes.get(7..7 + filter_len).ok_or_else(|| corrupt("cut"))?)
            .map_err(|e| corrupt(&e.to_string()))?;
        let mut at = 7 + filter_len;
        let mut pairs = Vec::with_capacity(count);
        for _ in 0..count {
            let len = usize::from(u16::from_be_bytes([
                *bytes.get(at).ok_or_else(|| corrupt("cut"))?,
                *bytes.get(at + 1).ok_or_else(|| corrupt("cut"))?,
            ]));
            at += 2;
            let key = bytes
                .get(at..at + len)
                .ok_or_else(|| corrupt("cut"))?
                .to_vec();
            at += len;
            let offset = *bytes.get(at).ok_or_else(|| corrupt("cut"))?;
            at += 1;
            pairs.push((key, offset));
        }
        if at != bytes.len() {
            return Err(corrupt("bytes trail the last pair"));
        }
        if pairs.windows(2).any(|w| w[0] >= w[1]) {
            return Err(corrupt("pairs are not sorted"));
        }
        Ok(Self { pairs, filter })
    }
}

/// One manifest row: a sealed bundle, its key range and its sidecar's hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestRow {
    /// The bundle index.
    pub index: u64,
    /// The smallest key, hex.
    pub min: String,
    /// The largest key, hex.
    pub max: String,
    /// `SHA-256` of the sidecar bytes, hex.
    pub sidecar: String,
    /// Pairs in the sidecar.
    pub keys: usize,
}

/// The manifest, `manifest`: every sealed bundle's row, in order. JSON,
/// rewritten whole on every change.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// Format version, 1.
    pub version: u8,
    /// One row per sealed bundle, by index.
    pub bundles: Vec<ManifestRow>,
}

impl Manifest {
    /// The manifest the store holds; empty for a store without one.
    pub fn read(store: &impl TileStore) -> Result<Self> {
        match store.get(MANIFEST)? {
            None => Ok(Self {
                version: 1,
                bundles: Vec::new(),
            }),
            Some(bytes) => {
                let manifest: Self =
                    serde_json::from_slice(&bytes).map_err(|e| Error::Corrupt {
                        path: MANIFEST.to_string(),
                        reason: e.to_string(),
                    })?;
                if manifest.version != 1 {
                    return Err(Error::Corrupt {
                        path: MANIFEST.to_string(),
                        reason: format!("version {}", manifest.version),
                    });
                }
                if manifest
                    .bundles
                    .iter()
                    .enumerate()
                    .any(|(i, row)| row.index != i as u64)
                {
                    return Err(Error::Corrupt {
                        path: MANIFEST.to_string(),
                        reason: "rows are not the bundles in order".to_string(),
                    });
                }
                Ok(manifest)
            }
        }
    }

    /// Writes the manifest, whole or not at all.
    pub fn write(&self, store: &impl TileStore) -> Result<()> {
        let bytes = serde_json::to_vec(self).map_err(|e| Error::Format(e.to_string()))?;
        store.put(MANIFEST, &bytes)?;
        Ok(())
    }

    /// The row for bundle `index` built from its sidecar bytes.
    #[must_use]
    pub fn row(index: u64, sidecar: &Sidecar, bytes: &[u8]) -> ManifestRow {
        let (min, max) = sidecar
            .key_range()
            .map(|(min, max)| (rfc9162::hex(min), rfc9162::hex(max)))
            .unwrap_or_default();
        ManifestRow {
            index,
            min,
            max,
            sidecar: rfc9162::hex(&Sha256::digest(bytes)),
            keys: sidecar.pairs.len(),
        }
    }

    /// Rows whose key range holds `key`.
    pub fn candidates<'a>(&'a self, key: &[u8]) -> impl Iterator<Item = &'a ManifestRow> + 'a {
        let key = rfc9162::hex(key);
        self.bundles
            .iter()
            .filter(move |row| row.min <= key && key <= row.max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sidecar_round_trips_and_looks_up_every_key() {
        let entries: Vec<Vec<u8>> = (0..256u32)
            .map(|i| format!("{}:{i}", i % 7).into_bytes())
            .collect();
        let keys: KeyFn = Box::new(|e| vec![e.split(|b| *b == b':').next().unwrap().to_vec()]);
        let sidecar = Sidecar::build(&entries, &keys);
        // 256 identity keys and 256 caller keys, each pair once.
        assert_eq!(sidecar.pairs.len(), 512);
        let bytes = sidecar.to_bytes();
        assert_eq!(Sidecar::from_bytes("s", &bytes).unwrap(), sidecar);
        for (offset, entry) in entries.iter().enumerate() {
            assert_eq!(sidecar.lookup(&identity(entry)), vec![offset as u8]);
        }
        // Entries 3, 10, 17, ... share the caller key "3".
        let threes: Vec<u8> = (0..256u32)
            .filter(|i| i % 7 == 3)
            .map(|i| i as u8)
            .collect();
        assert_eq!(sidecar.lookup(b"3"), threes);
        assert!(sidecar.lookup(b"9").is_empty());
        assert!(Sidecar::from_bytes("s", &bytes[..bytes.len() - 1]).is_err());
        assert!(Sidecar::from_bytes("s", &[2]).is_err());
    }

    #[test]
    fn a_manifest_names_the_rows_holding_a_key() {
        let sidecar = Sidecar::build(&[b"a".to_vec(), b"b".to_vec()], &no_keys());
        let row = Manifest::row(0, &sidecar, &sidecar.to_bytes());
        assert_eq!(row.keys, 2);
        let manifest = Manifest {
            version: 1,
            bundles: vec![row],
        };
        let (min, max) = sidecar.key_range().unwrap();
        assert_eq!(manifest.candidates(min).count(), 1);
        assert_eq!(manifest.candidates(max).count(), 1);
        assert_eq!(manifest.candidates(&[0u8; 32]).count(), 0);
        assert_eq!(manifest.candidates(&[0xffu8; 32]).count(), 0);
    }
}
