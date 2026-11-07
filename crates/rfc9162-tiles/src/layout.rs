//! Tile coordinates, store paths and the widths a tree size gives each level,
//! as C2SP tlog-tiles (<https://c2sp.org/tlog-tiles>) fixes them.
//!
//! A tile at level `l` holds up to 256 hashes; hash `i` of tile `n` is the
//! RFC 9162 root over leaves `[(n * 256 + i) * 256^l, (n * 256 + i + 1) * 256^l)`.
//! Level 0 holds leaf hashes. A full tile has 256 hashes; a partial tile has
//! 1 to 255 and is never hashed into the level above.

use crate::error::{Error, Result};

/// Hashes per full tile, and entries per full bundle.
pub const TILE_WIDTH: u64 = 256;
/// `log2(TILE_WIDTH)`: the tree height one tile level covers.
pub const TILE_HEIGHT: u32 = 8;
/// Bytes per hash.
pub const HASH_SIZE: usize = 32;
/// The longest entry a bundle can length-prefix.
pub const MAX_ENTRY_LEN: usize = u16::MAX as usize;
/// Store path of the mutable tree state (size and root), written last.
pub const TREE_STATE: &str = ".state/treeState";
/// Store path of the writer lock.
pub const TREE_STATE_LOCK: &str = ".state/treeState.lock";
/// Store path of the checkpoint lock.
pub const PUBLISH_LOCK: &str = ".state/publish.lock";
/// Store path of the signed checkpoint.
pub const CHECKPOINT: &str = "checkpoint";
/// Store path of the manifest over the sealed bundles.
pub const MANIFEST: &str = "manifest";

/// One tile: its level, its index within the level and how many hashes it
/// holds. `width` is 256 for a full tile and 1 to 255 for a partial one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile {
    /// 0 for leaf hashes; each level up covers 256 times as many leaves.
    pub level: u8,
    /// Position within the level.
    pub index: u64,
    /// Hashes in the tile, 1 to 256.
    pub width: u16,
}

impl Tile {
    /// A tile; `width` outside 1 to 256 is refused.
    pub fn new(level: u8, index: u64, width: u16) -> Result<Self> {
        if width == 0 || u64::from(width) > TILE_WIDTH {
            return Err(Error::OutOfRange(format!("tile width {width}")));
        }
        Ok(Self {
            level,
            index,
            width,
        })
    }

    /// Whether the tile holds 256 hashes.
    #[must_use]
    pub fn is_full(&self) -> bool {
        u64::from(self.width) == TILE_WIDTH
    }

    /// `tile/<L>/<N>` for a full tile, `tile/<L>/<N>.p/<W>` for a partial.
    #[must_use]
    pub fn path(&self) -> String {
        let mut path = format!("tile/{}/{}", self.level, path_elements(self.index));
        if !self.is_full() {
            path.push_str(&format!(".p/{}", self.width));
        }
        path
    }

    /// The tile a store path names, or nothing when the path is not a hash
    /// tile path. `tile/entries/...` and `tile/index/...` are not hash tiles.
    #[must_use]
    pub fn parse(path: &str) -> Option<Self> {
        let rest = path.strip_prefix("tile/")?;
        let (level, rest) = rest.split_once('/')?;
        if level.is_empty() || level.len() > 2 || !level.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if level.len() == 2 && level.starts_with('0') {
            return None;
        }
        let level: u8 = level.parse().ok().filter(|l| *l < 64)?;
        let (index, width) = parse_indexed(rest)?;
        Self::new(level, index, width).ok()
    }
}

/// `<N>` as zero-padded 3-digit elements, all but the last prefixed `x`:
/// 1234067 is `x001/x234/067`.
#[must_use]
pub fn path_elements(index: u64) -> String {
    let digits = format!("{index:03}");
    let mut groups: Vec<String> = Vec::new();
    let bytes = digits.as_bytes();
    let head = bytes.len() % 3;
    let mut start = 0;
    if head != 0 {
        groups.push(format!("{:0>3}", &digits[..head]));
        start = head;
    }
    while start < bytes.len() {
        groups.push(digits[start..start + 3].to_string());
        start += 3;
    }
    let last = groups.len() - 1;
    groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            if i == last {
                g.clone()
            } else {
                format!("x{g}")
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The index `path_elements` wrote, or nothing when the text is not that.
#[must_use]
pub fn parse_path_elements(text: &str) -> Option<u64> {
    let elements: Vec<&str> = text.split('/').collect();
    let last = elements.len() - 1;
    let mut digits = String::new();
    for (i, element) in elements.iter().enumerate() {
        let body = if i == last {
            *element
        } else {
            element.strip_prefix('x')?
        };
        if body.len() != 3 || !body.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.push_str(body);
    }
    let index: u64 = digits.parse().ok()?;
    // The encoding is unique: anything but the canonical spelling is refused.
    (path_elements(index) == text).then_some(index)
}

/// `<N>[.p/<W>]` into (index, width), width 256 when there is no `.p`.
fn parse_indexed(rest: &str) -> Option<(u64, u16)> {
    match rest.split_once(".p/") {
        Some((index, width)) => {
            if width.is_empty() || width.len() > 3 || !width.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            if width.starts_with('0') {
                return None;
            }
            let width: u16 = width.parse().ok()?;
            if width == 0 || u64::from(width) >= TILE_WIDTH {
                return None;
            }
            Some((parse_path_elements(index)?, width))
        }
        None => Some((parse_path_elements(rest)?, TILE_WIDTH as u16)),
    }
}

/// `tile/entries/<N>` for a full bundle, `tile/entries/<N>.p/<W>` for the
/// partial one.
#[must_use]
pub fn entries_path(index: u64, width: u16) -> String {
    let mut path = format!("tile/entries/{}", path_elements(index));
    if u64::from(width) != TILE_WIDTH {
        path.push_str(&format!(".p/{width}"));
    }
    path
}

/// The (bundle index, width) an entries path names.
#[must_use]
pub fn parse_entries_path(path: &str) -> Option<(u64, u16)> {
    parse_indexed(path.strip_prefix("tile/entries/")?)
}

/// `tile/index/<N>`, the key sidecar of sealed bundle `N`.
#[must_use]
pub fn index_path(index: u64) -> String {
    format!("tile/index/{}", path_elements(index))
}

/// The bundle index a sidecar path names.
#[must_use]
pub fn parse_index_path(path: &str) -> Option<u64> {
    parse_path_elements(path.strip_prefix("tile/index/")?)
}

/// The tiles a tree of one size is stored as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Leaves in the tree.
    pub size: u64,
}

impl Layout {
    /// The layout of a tree with `size` leaves.
    #[must_use]
    pub fn new(size: u64) -> Self {
        Self { size }
    }

    /// Hashes the tree has at `level`: `floor(size / 256^level)`.
    #[must_use]
    pub fn hashes_at(&self, level: u8) -> u64 {
        let shift = u32::from(level) * TILE_HEIGHT;
        if shift >= u64::BITS {
            0
        } else {
            self.size >> shift
        }
    }

    /// Levels that hold at least one hash; 0 for the empty tree.
    #[must_use]
    pub fn levels(&self) -> u8 {
        let mut level = 0u8;
        while self.hashes_at(level) > 0 {
            level += 1;
        }
        level
    }

    /// Full tiles at `level`.
    #[must_use]
    pub fn full_tiles_at(&self, level: u8) -> u64 {
        self.hashes_at(level) / TILE_WIDTH
    }

    /// Width of the partial tile at `level`, `floor(size / 256^level) mod 256`;
    /// 0 when the level ends on a full tile.
    #[must_use]
    pub fn width_at(&self, level: u8) -> u16 {
        (self.hashes_at(level) % TILE_WIDTH) as u16
    }

    /// The partial tile at `level`, when there is one.
    #[must_use]
    pub fn partial_at(&self, level: u8) -> Option<Tile> {
        let width = self.width_at(level);
        (width > 0).then(|| Tile {
            level,
            index: self.full_tiles_at(level),
            width,
        })
    }

    /// Every partial tile of the tree, level 0 first. Their hashes are exactly
    /// the perfect-subtree roots the tree root is folded from.
    #[must_use]
    pub fn partial_tiles(&self) -> Vec<Tile> {
        (0..self.levels())
            .filter_map(|level| self.partial_at(level))
            .collect()
    }

    /// Every tile of the tree, full and partial, level 0 first.
    #[must_use]
    pub fn tiles(&self) -> Vec<Tile> {
        let mut out = Vec::new();
        for level in 0..self.levels() {
            for index in 0..self.full_tiles_at(level) {
                out.push(Tile {
                    level,
                    index,
                    width: TILE_WIDTH as u16,
                });
            }
            out.extend(self.partial_at(level));
        }
        out
    }

    /// The tile at `level` and `index` as this tree stores it: full below the
    /// partial, the partial at its width, nothing beyond.
    #[must_use]
    pub fn tile(&self, level: u8, index: u64) -> Option<Tile> {
        let full = self.full_tiles_at(level);
        if index < full {
            Some(Tile {
                level,
                index,
                width: TILE_WIDTH as u16,
            })
        } else if index == full {
            self.partial_at(level)
        } else {
            None
        }
    }

    /// Sealed (full) entry bundles.
    #[must_use]
    pub fn full_bundles(&self) -> u64 {
        self.full_tiles_at(0)
    }

    /// Entries in the unsealed bundle, 0 to 255.
    #[must_use]
    pub fn partial_bundle_width(&self) -> u16 {
        self.width_at(0)
    }

    /// The bundle holding entry `index`, and the entry's offset in it.
    #[must_use]
    pub fn bundle_of(entry: u64) -> (u64, u16) {
        (entry / TILE_WIDTH, (entry % TILE_WIDTH) as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spec_path_example() {
        assert_eq!(path_elements(1_234_067), "x001/x234/067");
        assert_eq!(parse_path_elements("x001/x234/067"), Some(1_234_067));
        assert_eq!(path_elements(0), "000");
        assert_eq!(path_elements(7), "007");
        assert_eq!(path_elements(1000), "x001/000");
        assert_eq!(parse_path_elements("x000/007"), None);
        assert_eq!(parse_path_elements("7"), None);
    }

    #[test]
    fn tile_paths_round_trip() {
        let full = Tile::new(1, 300, 256).unwrap();
        assert_eq!(full.path(), "tile/1/300");
        let partial = Tile::new(0, 273, 112).unwrap();
        assert_eq!(partial.path(), "tile/0/273.p/112");
        assert_eq!(
            Tile::new(0, 1_234_067, 5).unwrap().path(),
            "tile/0/x001/x234/067.p/5"
        );
        for tile in [full, partial] {
            assert_eq!(Tile::parse(&tile.path()), Some(tile));
        }
        assert_eq!(Tile::parse("tile/entries/000"), None);
        assert_eq!(Tile::parse("tile/0/000.p/256"), None);
        assert_eq!(Tile::parse("tile/0/000.p/0"), None);
        assert_eq!(Tile::parse("tile/00/000"), None);
        assert_eq!(entries_path(5, 256), "tile/entries/005");
        assert_eq!(entries_path(5, 12), "tile/entries/005.p/12");
        assert_eq!(parse_entries_path("tile/entries/005.p/12"), Some((5, 12)));
        assert_eq!(index_path(1000), "tile/index/x001/000");
        assert_eq!(parse_index_path("tile/index/x001/000"), Some(1000));
    }

    #[test]
    fn widths_follow_the_size() {
        let layout = Layout::new(256);
        assert_eq!(layout.levels(), 2);
        assert_eq!(layout.full_tiles_at(0), 1);
        assert_eq!(layout.width_at(0), 0);
        assert_eq!(layout.width_at(1), 1);
        assert_eq!(Layout::new(0).levels(), 0);
        assert!(Layout::new(0).tiles().is_empty());
        assert_eq!(Layout::new(1).tiles(), vec![Tile::new(0, 0, 1).unwrap()]);
        assert_eq!(Layout::bundle_of(70_000), (273, 112));
    }
}
