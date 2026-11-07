//! Hash tiles: how the writer grows them, how a reader checks them, and the
//! root both agree on.
//!
//! The tree over `size` leaves is a right fold over its perfect subtrees,
//! largest first (RFC 9162 splits at the largest power of two below the
//! width). Those subtree roots are exactly the hashes of the partial tiles,
//! so the frontier of a log is its partial tiles and nothing else, and a
//! reader checks every partial tile at once by recomputing the root from
//! them. A full tile is checked against the hash it folds to in the tile one
//! level up.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::error::{Error, Result};
use crate::layout::{Layout, Tile, HASH_SIZE, TILE_HEIGHT, TILE_WIDTH};
use crate::store::TileStore;

/// A hash as the tree stores it.
pub type Hash = [u8; HASH_SIZE];

/// Checked tiles by (level, index).
type TileCache = RefCell<HashMap<(u8, u64), Rc<Vec<Hash>>>>;

/// The bytes of a tile: its hashes, concatenated.
#[must_use]
pub fn encode_tile(hashes: &[Hash]) -> Vec<u8> {
    let mut out = Vec::with_capacity(hashes.len() * HASH_SIZE);
    for hash in hashes {
        out.extend_from_slice(hash);
    }
    out
}

/// The hashes of a tile's bytes; refused unless 1 to 256 whole hashes.
pub fn decode_tile(path: &str, bytes: &[u8]) -> Result<Vec<Hash>> {
    if bytes.is_empty() || bytes.len() % HASH_SIZE != 0 || bytes.len() > 256 * HASH_SIZE {
        return Err(Error::Corrupt {
            path: path.to_string(),
            reason: format!("{} bytes is not 1 to 256 hashes", bytes.len()),
        });
    }
    Ok(bytes
        .chunks_exact(HASH_SIZE)
        .map(|c| {
            let mut hash = [0u8; HASH_SIZE];
            hash.copy_from_slice(c);
            hash
        })
        .collect())
}

/// The root over a run of hashes that stand for perfect subtrees of equal
/// size, folded with the RFC 9162 shape. Over 256 leaf hashes this is the
/// hash the full tile contributes to the level above.
#[must_use]
pub fn tile_root(hashes: &[Hash]) -> Hash {
    rfc9162::root(hashes)
}

/// The root of the tree whose perfect subtrees have these roots, left to
/// right, largest first.
fn fold_right(subtrees: &[Hash]) -> Hash {
    let mut acc = match subtrees.last() {
        None => return rfc9162::root(&[]),
        Some(last) => *last,
    };
    for subtree in subtrees[..subtrees.len() - 1].iter().rev() {
        acc = rfc9162::node_hash(subtree, &acc);
    }
    acc
}

/// The perfect-subtree roots in a run of `width` (1 to 255) hashes of one
/// level, largest first; each is an aligned chunk whose length is a set bit
/// of the width.
fn subtrees_of(hashes: &[Hash]) -> Vec<Hash> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for bit in (0..TILE_HEIGHT).rev() {
        let chunk = 1usize << bit;
        if hashes.len() & chunk != 0 {
            out.push(rfc9162::root(&hashes[at..at + chunk]));
            at += chunk;
        }
    }
    out
}

/// The growing edge of a log: for each level, the hashes of its partial tile.
/// Pushing a leaf fills level 0; a level that reaches 256 hashes becomes a
/// full tile and contributes one hash to the level above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontier {
    size: u64,
    levels: Vec<Vec<Hash>>,
}

impl Default for Frontier {
    fn default() -> Self {
        Self::empty()
    }
}

impl Frontier {
    /// The frontier of the empty tree.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            size: 0,
            levels: Vec::new(),
        }
    }

    /// The frontier of a tree of `size` leaves, read from the partial tiles
    /// the store holds for that size. Refused when a partial tile is missing
    /// or has another width.
    pub fn load(store: &impl TileStore, size: u64) -> Result<Self> {
        let layout = Layout::new(size);
        let mut levels = Vec::new();
        for level in 0..layout.levels() {
            match layout.partial_at(level) {
                Some(tile) => levels.push(read_tile(store, &tile)?),
                None => levels.push(Vec::new()),
            }
        }
        Ok(Self { size, levels })
    }

    /// Leaves the frontier covers.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The tree root at this size.
    #[must_use]
    pub fn root(&self) -> Hash {
        let mut subtrees = Vec::new();
        for level in self.levels.iter().rev() {
            subtrees.extend(subtrees_of(level));
        }
        fold_right(&subtrees)
    }

    /// Appends one leaf hash. Every full tile the append completes is pushed
    /// to `completed`, level 0 first, with its hashes.
    pub fn push(&mut self, leaf: Hash, completed: &mut Vec<(Tile, Vec<Hash>)>) {
        self.size += 1;
        if self.levels.is_empty() {
            self.levels.push(Vec::with_capacity(TILE_WIDTH as usize));
        }
        self.levels[0].push(leaf);
        let mut level = 0usize;
        while self.levels[level].len() as u64 == TILE_WIDTH {
            let hashes = std::mem::take(&mut self.levels[level]);
            let top = tile_root(&hashes);
            let shift = (level as u32 + 1) * TILE_HEIGHT;
            let index = (self.size >> shift) - 1;
            completed.push((
                Tile {
                    level: level as u8,
                    index,
                    width: TILE_WIDTH as u16,
                },
                hashes,
            ));
            if self.levels.len() == level + 1 {
                self.levels.push(Vec::with_capacity(TILE_WIDTH as usize));
            }
            self.levels[level + 1].push(top);
            level += 1;
        }
    }

    /// The partial tiles at this size, with their hashes, level 0 first.
    #[must_use]
    pub fn partial_tiles(&self) -> Vec<(Tile, &[Hash])> {
        let layout = Layout::new(self.size);
        self.levels
            .iter()
            .enumerate()
            .filter(|(_, hashes)| !hashes.is_empty())
            .map(|(level, hashes)| {
                let tile = layout
                    .partial_at(level as u8)
                    .expect("a non-empty level has a partial tile");
                (tile, hashes.as_slice())
            })
            .collect()
    }
}

fn read_tile(store: &impl TileStore, tile: &Tile) -> Result<Vec<Hash>> {
    let path = tile.path();
    let bytes = store.get(&path)?.ok_or_else(|| Error::Corrupt {
        path: path.clone(),
        reason: "missing".to_string(),
    })?;
    let hashes = decode_tile(&path, &bytes)?;
    if hashes.len() != usize::from(tile.width) {
        return Err(Error::Corrupt {
            path,
            reason: format!("{} hashes, width {}", hashes.len(), tile.width),
        });
    }
    Ok(hashes)
}

/// Reads the tiles a root or a proof needs, and no tile that has not been
/// checked against the trusted root: the partial tiles together through the
/// root they fold to, each full tile through its hash in the level above.
pub struct TileHashReader<S: TileStore> {
    store: S,
    size: u64,
    root: Hash,
    cache: TileCache,
    frontier_checked: Cell<bool>,
}

impl<S: TileStore> TileHashReader<S> {
    /// A reader over `store` that trusts `root` to be the root at `size`.
    pub fn new(store: S, size: u64, root: Hash) -> Self {
        Self {
            store,
            size,
            root,
            cache: RefCell::new(HashMap::new()),
            frontier_checked: Cell::new(false),
        }
    }

    /// The store behind the reader.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Leaves the trusted root covers.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The trusted root.
    #[must_use]
    pub fn trusted_root(&self) -> Hash {
        self.root
    }

    /// The root recomputed from the partial tiles in the store; an error when
    /// it is not the trusted one.
    pub fn root(&self) -> Result<Hash> {
        self.check_frontier()?;
        Ok(self.root)
    }

    fn check_frontier(&self) -> Result<()> {
        if self.frontier_checked.get() {
            return Ok(());
        }
        let frontier = Frontier::load(&self.store, self.size)?;
        let root = frontier.root();
        if root != self.root {
            return Err(Error::Corrupt {
                path: "tile".to_string(),
                reason: format!(
                    "the partial tiles fold to {} at size {}, the trusted root is {}",
                    rfc9162::hex(&root),
                    self.size,
                    rfc9162::hex(&self.root)
                ),
            });
        }
        let mut cache = self.cache.borrow_mut();
        for (tile, hashes) in frontier.partial_tiles() {
            cache.insert((tile.level, tile.index), Rc::new(hashes.to_vec()));
        }
        self.frontier_checked.set(true);
        Ok(())
    }

    /// The hashes of the tile at `level` and `index`, checked against the
    /// tree. A tile the tree of this size does not have is out of range.
    pub fn tile(&self, level: u8, index: u64) -> Result<Rc<Vec<Hash>>> {
        if let Some(hashes) = self.cache.borrow().get(&(level, index)) {
            return Ok(Rc::clone(hashes));
        }
        let layout = Layout::new(self.size);
        let tile = layout.tile(level, index).ok_or_else(|| {
            Error::OutOfRange(format!("tile {level}/{index} at size {}", self.size))
        })?;
        if !tile.is_full() {
            self.check_frontier()?;
            return Ok(Rc::clone(
                self.cache
                    .borrow()
                    .get(&(level, index))
                    .expect("a checked frontier caches every partial tile"),
            ));
        }
        let hashes = read_tile(&self.store, &tile)?;
        let parent = self.tile(level + 1, index / TILE_WIDTH)?;
        let expected = parent[(index % TILE_WIDTH) as usize];
        let top = tile_root(&hashes);
        if top != expected {
            return Err(Error::Corrupt {
                path: tile.path(),
                reason: format!(
                    "folds to {}, the level above holds {}",
                    rfc9162::hex(&top),
                    rfc9162::hex(&expected)
                ),
            });
        }
        let hashes = Rc::new(hashes);
        self.cache
            .borrow_mut()
            .insert((level, index), Rc::clone(&hashes));
        Ok(hashes)
    }

    /// Stored hash `index` at `level`: the root over leaves
    /// `[index * 256^level, (index + 1) * 256^level)`.
    pub fn hash(&self, level: u8, index: u64) -> Result<Hash> {
        let tile = self.tile(level, index / TILE_WIDTH)?;
        let offset = (index % TILE_WIDTH) as usize;
        tile.get(offset).copied().ok_or_else(|| {
            Error::OutOfRange(format!(
                "hash {index} at level {level} at size {}",
                self.size
            ))
        })
    }

    /// The root of the perfect subtree over leaves `[start, start + 2^height)`,
    /// which must be aligned and inside the tree.
    pub fn perfect(&self, start: u64, height: u32) -> Result<Hash> {
        let span = 1u64 << height;
        if start % span != 0 || start.saturating_add(span) > self.size {
            return Err(Error::OutOfRange(format!(
                "perfect subtree at {start} of height {height} at size {}",
                self.size
            )));
        }
        let level = (height / TILE_HEIGHT) as u8;
        let within = height % TILE_HEIGHT;
        let first = start >> (u32::from(level) * TILE_HEIGHT);
        let count = 1usize << within;
        let tile = self.tile(level, first / TILE_WIDTH)?;
        let offset = (first % TILE_WIDTH) as usize;
        Ok(rfc9162::root(&tile[offset..offset + count]))
    }

    /// The RFC 9162 root over leaves `[lo, hi)` of this tree, the same bytes
    /// `rfc9162::root` gives over that slice of leaf hashes.
    pub fn range_root(&self, lo: u64, hi: u64) -> Result<Hash> {
        if lo > hi || hi > self.size {
            return Err(Error::OutOfRange(format!(
                "leaves {lo}..{hi} at size {}",
                self.size
            )));
        }
        let width = hi - lo;
        if width == 0 {
            return Ok(rfc9162::root(&[]));
        }
        if width.is_power_of_two() && lo % width == 0 {
            return self.perfect(lo, width.trailing_zeros());
        }
        let split = split_point(width);
        let left = self.range_root(lo, lo + split)?;
        let right = self.range_root(lo + split, hi)?;
        Ok(rfc9162::node_hash(&left, &right))
    }
    /// The audit path for the leaf at `index`, RFC 9162 section 2.1.3,
    /// assembled from tiles: the bytes `rfc9162::inclusion_proof` gives over
    /// the flat leaf slice. Nothing for an index past the size.
    pub fn inclusion_proof(&self, index: u64) -> Result<Option<Vec<Hash>>> {
        if index >= self.size {
            return Ok(None);
        }
        let mut path = Vec::new();
        self.audit_path(0, self.size, index, &mut path)?;
        Ok(Some(path))
    }

    fn audit_path(&self, lo: u64, hi: u64, index: u64, path: &mut Vec<Hash>) -> Result<()> {
        if hi - lo == 1 {
            return Ok(());
        }
        let split = lo + split_point(hi - lo);
        if index < split {
            self.audit_path(lo, split, index, path)?;
            path.push(self.range_root(split, hi)?);
        } else {
            self.audit_path(split, hi, index, path)?;
            path.push(self.range_root(lo, split)?);
        }
        Ok(())
    }

    /// The consistency proof that the tree of `old` leaves is a prefix of
    /// this one, RFC 9162 section 2.1.4, assembled from tiles: the bytes
    /// `rfc9162::consistency_proof` gives over the flat leaf slice. Nothing
    /// when `old` is zero or past the size.
    pub fn consistency_proof(&self, old: u64) -> Result<Option<Vec<Hash>>> {
        if old == 0 || old > self.size {
            return Ok(None);
        }
        let mut path = Vec::new();
        self.subproof(0, self.size, old, true, &mut path)?;
        Ok(Some(path))
    }

    /// The RFC's SUBPROOF over leaves `[lo, hi)`; `known` is whether the
    /// reader already holds the root of the subtree being described.
    fn subproof(
        &self,
        lo: u64,
        hi: u64,
        old: u64,
        known: bool,
        path: &mut Vec<Hash>,
    ) -> Result<()> {
        if old == hi - lo {
            if !known {
                path.push(self.range_root(lo, hi)?);
            }
            return Ok(());
        }
        let split = split_point(hi - lo);
        if old <= split {
            self.subproof(lo, lo + split, old, known, path)?;
            path.push(self.range_root(lo + split, hi)?);
        } else {
            self.subproof(lo + split, hi, old - split, false, path)?;
            path.push(self.range_root(lo, lo + split)?);
        }
        Ok(())
    }
}

/// The largest power of two strictly below `width`, where RFC 9162 divides a
/// tree of that width.
#[must_use]
pub fn split_point(width: u64) -> u64 {
    let mut split = 1u64;
    while split * 2 < width {
        split *= 2;
    }
    split
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_split_is_the_rfc_one() {
        assert_eq!(split_point(2), 1);
        assert_eq!(split_point(3), 2);
        assert_eq!(split_point(4), 2);
        assert_eq!(split_point(5), 4);
        assert_eq!(split_point(257), 256);
    }

    #[test]
    fn a_tile_round_trips_and_a_ragged_one_is_refused() {
        let hashes: Vec<Hash> = (0..3u8).map(|i| [i; 32]).collect();
        let bytes = encode_tile(&hashes);
        assert_eq!(decode_tile("t", &bytes).unwrap(), hashes);
        assert!(decode_tile("t", &bytes[..40]).is_err());
        assert!(decode_tile("t", &[]).is_err());
    }
}
