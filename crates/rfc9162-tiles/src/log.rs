//! The log on a store: one [`Writer`] appends entries, seals bundles, writes
//! tiles and then the tree state; any number of [`Reader`]s read entries and
//! check them against the tree.
//!
//! Write order follows Tessera's POSIX storage: entry bundles, hash tiles,
//! index sidecars, the manifest, then `.state/treeState` last, every file
//! whole or not at all. On open the writer moves anything the tree state does
//! not cover to `.state/torn-<utc>` and never adopts it silently, and rebuilds
//! a sidecar or manifest row a sealed bundle lacks.

use std::ops::Range;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::bundle::{decode_bundle, encode_bundle};
use crate::error::{Error, Result};
use crate::index::{identity, KeyFn, Manifest, Sidecar};
use crate::layout::{
    entries_path, index_path, parse_entries_path, parse_index_path, Layout, Tile, MAX_ENTRY_LEN,
    TILE_WIDTH, TREE_STATE, TREE_STATE_LOCK,
};
use crate::store::TileStore;
use crate::tree::{encode_tile, Frontier, Hash, TileHashReader};

/// What `.state/treeState` holds: the size and the root the tiles are
/// complete to. Two lines, the decimal size and the lower-case hex root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeState {
    /// Entries the tiles are complete to.
    pub size: u64,
    /// The root at that size.
    pub root: Hash,
}

impl TreeState {
    /// The state the store holds, or nothing for a store without one.
    pub fn read(store: &impl TileStore) -> Result<Option<Self>> {
        let Some(bytes) = store.get(TREE_STATE)? else {
            return Ok(None);
        };
        let text = String::from_utf8(bytes).map_err(|_| Error::Corrupt {
            path: TREE_STATE.to_string(),
            reason: "not UTF-8".to_string(),
        })?;
        Self::parse(&text).map(Some)
    }

    /// The state in its two-line text.
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        let size = lines
            .next()
            .and_then(|l| l.parse::<u64>().ok())
            .ok_or_else(|| Error::Corrupt {
                path: TREE_STATE.to_string(),
                reason: "first line is not a size".to_string(),
            })?;
        let root = lines
            .next()
            .and_then(rfc9162::from_hex)
            .ok_or_else(|| Error::Corrupt {
                path: TREE_STATE.to_string(),
                reason: "second line is not a hex root".to_string(),
            })?;
        Ok(Self { size, root })
    }

    /// The two-line text.
    #[must_use]
    pub fn to_text(&self) -> String {
        format!("{}\n{}\n", self.size, rfc9162::hex(&self.root))
    }

    /// Writes the state, whole or not at all.
    pub fn write(&self, store: &impl TileStore) -> Result<()> {
        store.put(TREE_STATE, self.to_text().as_bytes())?;
        Ok(())
    }
}

/// The one writer of a log. Holds `.state/treeState.lock` from open to drop.
pub struct Writer<S: TileStore> {
    store: S,
    keys: KeyFn,
    frontier: Frontier,
    partial: Vec<Vec<u8>>,
    manifest: Manifest,
    quarantined: Vec<String>,
    torn_dir: Option<String>,
}

impl<S: TileStore> Writer<S> {
    /// Opens the log on `store`, takes the writer lock, quarantines a torn
    /// tail, loads the frontier and completes the sidecars and manifest of
    /// every sealed bundle. `keys` derives the lookup keys of an entry from
    /// its bytes ([`crate::index::no_keys`] for the identity hash alone); a
    /// reader has to use the same function. `Error::Locked` when another
    /// writer holds the lock; a lock left by a crashed writer has to be
    /// removed by hand.
    pub fn open(store: S, keys: KeyFn) -> Result<Self> {
        let body = format!("{}\n", std::process::id());
        if !store.create_new(TREE_STATE_LOCK, body.as_bytes())? {
            return Err(Error::Locked(TREE_STATE_LOCK.to_string()));
        }
        match Self::load(&store, &keys) {
            Ok(loaded) => Ok(Self {
                store,
                keys,
                frontier: loaded.frontier,
                partial: loaded.partial,
                manifest: loaded.manifest,
                quarantined: loaded.quarantined,
                torn_dir: loaded.torn_dir,
            }),
            Err(e) => {
                let _ = store.delete(TREE_STATE_LOCK);
                Err(e)
            }
        }
    }

    fn load(store: &S, keys: &KeyFn) -> Result<Loaded> {
        let state = TreeState::read(store)?.unwrap_or(TreeState {
            size: 0,
            root: rfc9162::root(&[]),
        });
        let (quarantined, torn_dir) = quarantine(store, state.size)?;
        let frontier = Frontier::load(store, state.size)?;
        if frontier.root() != state.root {
            return Err(Error::Corrupt {
                path: TREE_STATE.to_string(),
                reason: format!(
                    "root {} at size {}, the partial tiles fold to {}",
                    rfc9162::hex(&state.root),
                    state.size,
                    rfc9162::hex(&frontier.root())
                ),
            });
        }
        let partial = read_partial_bundle(store, &frontier)?;
        let manifest = repair_index(store, keys, &frontier)?;
        Ok(Loaded {
            frontier,
            partial,
            manifest,
            quarantined,
            torn_dir,
        })
    }

    /// The store the writer writes.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Entries in the log.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.frontier.size()
    }

    /// The root over every entry.
    #[must_use]
    pub fn root(&self) -> Hash {
        self.frontier.root()
    }

    /// The tree state the tiles are complete to.
    #[must_use]
    pub fn tree_state(&self) -> TreeState {
        TreeState {
            size: self.size(),
            root: self.root(),
        }
    }

    /// Store paths open moved aside because the tree state did not cover
    /// them, and the `.state/torn-<utc>` directory they went to.
    #[must_use]
    pub fn quarantined(&self) -> (&[String], Option<&str>) {
        (&self.quarantined, self.torn_dir.as_deref())
    }

    /// Appends one entry and returns its index.
    pub fn append(&mut self, entry: &[u8]) -> Result<u64> {
        Ok(self.append_batch([entry.to_vec()])?.start)
    }

    /// Appends entries in order and returns their index range. Nothing is
    /// written when any entry is over 65535 bytes. Bundles, tiles and the
    /// tree state are each written once for the whole batch.
    pub fn append_batch<I>(&mut self, entries: I) -> Result<Range<u64>>
    where
        I: IntoIterator<Item = Vec<u8>>,
    {
        let entries: Vec<Vec<u8>> = entries.into_iter().collect();
        if let Some(long) = entries.iter().find(|e| e.len() > MAX_ENTRY_LEN) {
            return Err(Error::EntryTooLong(long.len()));
        }
        let start = self.frontier.size();
        if entries.is_empty() {
            return Ok(start..start);
        }
        let mut completed: Vec<(Tile, Vec<Hash>)> = Vec::new();
        let mut sealed: Vec<(u64, Vec<Vec<u8>>)> = Vec::new();
        for entry in entries {
            self.frontier
                .push(rfc9162::leaf_hash(&entry), &mut completed);
            self.partial.push(entry);
            if self.partial.len() as u64 == TILE_WIDTH {
                let index = self.frontier.size() / TILE_WIDTH - 1;
                sealed.push((index, std::mem::take(&mut self.partial)));
            }
        }
        let end = self.frontier.size();
        let layout = Layout::new(end);

        for (index, bundle) in &sealed {
            self.store.put(
                &entries_path(*index, TILE_WIDTH as u16),
                &encode_bundle(bundle)?,
            )?;
        }
        if !self.partial.is_empty() {
            self.store.put(
                &entries_path(layout.full_bundles(), layout.partial_bundle_width()),
                &encode_bundle(&self.partial)?,
            )?;
        }
        for (tile, hashes) in &completed {
            self.store.put(&tile.path(), &encode_tile(hashes))?;
        }
        for (tile, hashes) in self.frontier.partial_tiles() {
            self.store.put(&tile.path(), &encode_tile(hashes))?;
        }
        self.seal(&sealed)?;
        self.tree_state().write(&self.store)?;

        // Older partials of a tile that is full are covered by the full tile and
        // the tree state, so nothing needs them.
        for (index, _) in &sealed {
            self.remove_partials(&entries_path(*index, TILE_WIDTH as u16))?;
        }
        for (tile, _) in &completed {
            self.remove_partials(&tile.path())?;
        }
        Ok(start..end)
    }

    /// Writes the sidecar of each sealed bundle, then the manifest with their
    /// rows.
    fn seal(&mut self, sealed: &[(u64, Vec<Vec<u8>>)]) -> Result<()> {
        if sealed.is_empty() {
            return Ok(());
        }
        for (index, bundle) in sealed {
            let sidecar = Sidecar::build(bundle, &self.keys);
            let bytes = sidecar.to_bytes();
            self.store.put(&index_path(*index), &bytes)?;
            self.manifest
                .bundles
                .push(Manifest::row(*index, &sidecar, &bytes));
        }
        self.manifest.write(&self.store)
    }

    /// The manifest over the sealed bundles.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn remove_partials(&self, full_path: &str) -> Result<()> {
        for path in self.store.list(&format!("{full_path}.p"))? {
            self.store.delete(&path)?;
        }
        Ok(())
    }
}

impl<S: TileStore> Drop for Writer<S> {
    fn drop(&mut self) {
        let _ = self.store.delete(TREE_STATE_LOCK);
    }
}

/// What open loads before the writer exists.
struct Loaded {
    frontier: Frontier,
    partial: Vec<Vec<u8>>,
    manifest: Manifest,
    quarantined: Vec<String>,
    torn_dir: Option<String>,
}

/// The manifest with a row and a sidecar for every sealed bundle: rows past
/// the sealed bundles are dropped, missing or differing sidecars rebuilt from
/// the bundle, and the manifest rewritten when anything changed.
fn repair_index(store: &impl TileStore, keys: &KeyFn, frontier: &Frontier) -> Result<Manifest> {
    let full = Layout::new(frontier.size()).full_bundles();
    let mut manifest = Manifest::read(store)?;
    let mut changed = false;
    if manifest.bundles.len() as u64 > full {
        manifest.bundles.truncate(full as usize);
        changed = true;
    }
    for index in manifest.bundles.len() as u64..full {
        let path = entries_path(index, TILE_WIDTH as u16);
        let bytes = store.get(&path)?.ok_or_else(|| Error::Corrupt {
            path: path.clone(),
            reason: "missing".to_string(),
        })?;
        let entries = decode_bundle(&path, &bytes)?;
        let sidecar = Sidecar::build(&entries, keys);
        let bytes = sidecar.to_bytes();
        if store.get(&index_path(index))?.as_deref() != Some(bytes.as_slice()) {
            store.put(&index_path(index), &bytes)?;
        }
        manifest
            .bundles
            .push(Manifest::row(index, &sidecar, &bytes));
        changed = true;
    }
    if changed {
        manifest.write(store)?;
    }
    Ok(manifest)
}

fn read_partial_bundle(store: &impl TileStore, frontier: &Frontier) -> Result<Vec<Vec<u8>>> {
    let layout = Layout::new(frontier.size());
    let width = layout.partial_bundle_width();
    if width == 0 {
        return Ok(Vec::new());
    }
    let path = entries_path(layout.full_bundles(), width);
    let bytes = store.get(&path)?.ok_or_else(|| Error::Corrupt {
        path: path.clone(),
        reason: "missing".to_string(),
    })?;
    let entries = decode_bundle(&path, &bytes)?;
    if entries.len() != usize::from(width) {
        return Err(Error::Corrupt {
            path,
            reason: format!("{} entries, width {width}", entries.len()),
        });
    }
    let (_, level0) = frontier
        .partial_tiles()
        .into_iter()
        .find(|(tile, _)| tile.level == 0)
        .expect("a partial bundle has a partial level-0 tile");
    for (i, (entry, hash)) in entries.iter().zip(level0).enumerate() {
        if rfc9162::leaf_hash(entry) != *hash {
            return Err(Error::Corrupt {
                path,
                reason: format!("entry {i} does not hash to its leaf"),
            });
        }
    }
    Ok(entries)
}

/// Whether a stored path is one the tree of `size` has.
fn covered(path: &str, layout: &Layout) -> bool {
    if let Some((index, width)) = parse_entries_path(path) {
        return covered_indexed(
            index,
            width,
            layout.full_bundles(),
            layout.partial_bundle_width(),
        );
    }
    if let Some(index) = parse_index_path(path) {
        return index < layout.full_bundles();
    }
    if let Some(tile) = Tile::parse(path) {
        return covered_indexed(
            tile.index,
            tile.width,
            layout.full_tiles_at(tile.level),
            layout.width_at(tile.level),
        );
    }
    false
}

fn covered_indexed(index: u64, width: u16, full: u64, partial_width: u16) -> bool {
    if u64::from(width) == TILE_WIDTH {
        index < full
    } else {
        index == full && width <= partial_width
    }
}

/// Moves every path under `tile/` that the tree of `size` does not cover to
/// `.state/torn-<utc>/`, returning the paths moved and that directory.
fn quarantine(store: &impl TileStore, size: u64) -> Result<(Vec<String>, Option<String>)> {
    let layout = Layout::new(size);
    let torn: Vec<String> = store
        .list("tile")?
        .into_iter()
        .filter(|path| !covered(path, &layout))
        .collect();
    if torn.is_empty() {
        return Ok((Vec::new(), None));
    }
    let dir = format!(".state/torn-{}", utc_stamp());
    for path in &torn {
        store.rename(path, &format!("{dir}/{path}"))?;
    }
    Ok((torn, Some(dir)))
}

/// Now as `YYYYMMDDTHHMMSSZ`.
fn utc_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (date, time) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = date as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

/// Reads a log at one tree state. Every entry it returns hashes to the leaf
/// the checked tiles hold for its index.
pub struct Reader<S: TileStore> {
    hashes: TileHashReader<S>,
    keys: KeyFn,
    manifest: Manifest,
}

impl<S: TileStore> Reader<S> {
    /// A reader at the tree state the store holds, with the log's key
    /// function; the empty tree for a store without a state.
    pub fn open(store: S, keys: KeyFn) -> Result<Self> {
        let state = TreeState::read(&store)?.unwrap_or(TreeState {
            size: 0,
            root: rfc9162::root(&[]),
        });
        Self::at(store, state.size, state.root, keys)
    }

    /// A reader that trusts `root` to be the root at `size`, which is how a
    /// verified checkpoint opens a log.
    pub fn at(store: S, size: u64, root: Hash, keys: KeyFn) -> Result<Self> {
        let mut manifest = Manifest::read(&store)?;
        let full = Layout::new(size).full_bundles();
        manifest.bundles.truncate(full as usize);
        Ok(Self {
            hashes: TileHashReader::new(store, size, root),
            keys,
            manifest,
        })
    }

    /// The manifest rows for the sealed bundles the root covers.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Indices of every entry carrying `key`, ascending: through the
    /// manifest ranges, the filters and the sidecars for sealed bundles, then
    /// a scan of the unsealed one. A key is an identity hash or a value the
    /// key function derives; each hit is checked on the entry itself.
    pub fn find(&self, key: &[u8]) -> Result<Vec<u64>> {
        let mut found = Vec::new();
        for row in self.manifest.candidates(key) {
            let path = index_path(row.index);
            let bytes = self.store().get(&path)?.ok_or_else(|| Error::Corrupt {
                path: path.clone(),
                reason: "missing".to_string(),
            })?;
            if rfc9162::hex(&Sha256::digest(&bytes)) != row.sidecar {
                return Err(Error::Corrupt {
                    path,
                    reason: "does not hash to its manifest row".to_string(),
                });
            }
            let sidecar = Sidecar::from_bytes(&path, &bytes)?;
            let offsets = sidecar.lookup(key);
            if offsets.is_empty() {
                continue;
            }
            let entries = self.bundle(row.index)?;
            for offset in offsets {
                let entry = &entries[usize::from(offset)];
                if self.carries(entry, key) {
                    found.push(row.index * TILE_WIDTH + u64::from(offset));
                }
            }
        }
        let layout = Layout::new(self.size());
        if layout.partial_bundle_width() > 0 {
            let first = layout.full_bundles() * TILE_WIDTH;
            for (offset, entry) in self.bundle(layout.full_bundles())?.iter().enumerate() {
                if self.carries(entry, key) {
                    found.push(first + offset as u64);
                }
            }
        }
        Ok(found)
    }

    fn carries(&self, entry: &[u8], key: &[u8]) -> bool {
        identity(entry) == key || (self.keys)(entry).iter().any(|k| k == key)
    }

    /// The store behind the reader.
    pub fn store(&self) -> &S {
        self.hashes.store()
    }

    /// Entries the reader's root covers.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.hashes.size()
    }

    /// The trusted root.
    #[must_use]
    pub fn root(&self) -> Hash {
        self.hashes.trusted_root()
    }

    /// The checked tile reader.
    pub fn hashes(&self) -> &TileHashReader<S> {
        &self.hashes
    }

    /// Entry `index`, checked against its leaf hash; nothing past the size.
    pub fn get(&self, index: u64) -> Result<Option<Vec<u8>>> {
        if index >= self.size() {
            return Ok(None);
        }
        let (bundle, offset) = Layout::bundle_of(index);
        let entries = self.bundle(bundle)?;
        Ok(Some(entries[usize::from(offset)].clone()))
    }

    /// The inclusion proof for entry `index`, from checked tiles; the bytes
    /// `rfc9162::inclusion_proof` gives over the flat leaf slice.
    pub fn inclusion(&self, index: u64) -> Result<Option<Vec<Hash>>> {
        self.hashes.inclusion_proof(index)
    }

    /// The consistency proof from a tree of `old` entries to this one, from
    /// checked tiles; the bytes `rfc9162::consistency_proof` gives.
    pub fn consistency(&self, old: u64) -> Result<Option<Vec<Hash>>> {
        self.hashes.consistency_proof(old)
    }

    /// Every entry of bundle `index`, each checked against its leaf hash.
    pub fn bundle(&self, index: u64) -> Result<Vec<Vec<u8>>> {
        let layout = Layout::new(self.size());
        let width = if index < layout.full_bundles() {
            TILE_WIDTH as u16
        } else if index == layout.full_bundles() && layout.partial_bundle_width() > 0 {
            layout.partial_bundle_width()
        } else {
            return Err(Error::OutOfRange(format!(
                "bundle {index} at size {}",
                self.size()
            )));
        };
        let path = entries_path(index, width);
        let bytes = self.store().get(&path)?.ok_or_else(|| Error::Corrupt {
            path: path.clone(),
            reason: "missing".to_string(),
        })?;
        let entries = decode_bundle(&path, &bytes)?;
        if entries.len() != usize::from(width) {
            return Err(Error::Corrupt {
                path,
                reason: format!("{} entries, width {width}", entries.len()),
            });
        }
        let tile = self.hashes.tile(0, index)?;
        for (i, (entry, hash)) in entries.iter().zip(tile.iter()).enumerate() {
            if rfc9162::leaf_hash(entry) != *hash {
                return Err(Error::Corrupt {
                    path,
                    reason: format!("entry {i} does not hash to its leaf"),
                });
            }
        }
        Ok(entries)
    }
}
