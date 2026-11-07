//! The tree over tiles is the tree over the flat slice: the same root at
//! every size, and the tile counts tlog-tiles gives for its worked example.

use rfc9162::leaf_hash;
use rfc9162_tiles::layout::TILE_WIDTH;
use rfc9162_tiles::tree::encode_tile;
use rfc9162_tiles::{Frontier, Layout, MemoryStore, TileHashReader, TileStore};

fn leaves(n: u64) -> Vec<[u8; 32]> {
    (0..n)
        .map(|i| leaf_hash(format!("entry-{i}").as_bytes()))
        .collect()
}

/// Appends `leaves` into `store`, writing every full and partial tile, and
/// returns the frontier.
fn write_tiles(store: &MemoryStore, leaves: &[[u8; 32]]) -> Frontier {
    let mut frontier = Frontier::empty();
    let mut completed = Vec::new();
    for leaf in leaves {
        frontier.push(*leaf, &mut completed);
    }
    for (tile, hashes) in &completed {
        store.put(&tile.path(), &encode_tile(hashes)).unwrap();
    }
    for (tile, hashes) in frontier.partial_tiles() {
        store.put(&tile.path(), &encode_tile(hashes)).unwrap();
    }
    frontier
}

#[test]
fn the_frontier_root_is_the_flat_root_for_every_size_to_2000() {
    let all = leaves(2000);
    let mut frontier = Frontier::empty();
    let mut completed = Vec::new();
    assert_eq!(frontier.root(), rfc9162::root(&[]));
    for size in 1..=2000usize {
        frontier.push(all[size - 1], &mut completed);
        assert_eq!(frontier.size() as usize, size);
        assert_eq!(
            frontier.root(),
            rfc9162::root(&all[..size]),
            "root at size {size}"
        );
    }
    // 2000 leaves complete seven level-0 tiles and nothing above.
    assert_eq!(completed.len(), 7);
    assert!(completed.iter().all(|(tile, _)| tile.level == 0));
}

#[test]
fn a_reader_recomputes_the_root_from_stored_partial_tiles() {
    let all = leaves(2000);
    for size in [0u64, 1, 2, 255, 256, 257, 511, 512, 513, 1000, 1999, 2000] {
        let store = MemoryStore::new();
        let frontier = write_tiles(&store, &all[..size as usize]);
        let root = rfc9162::root(&all[..size as usize]);
        assert_eq!(frontier.root(), root);
        let loaded = Frontier::load(&store, size).unwrap();
        assert_eq!(loaded, frontier);
        let reader = TileHashReader::new(store.clone(), size, root);
        assert_eq!(reader.root().unwrap(), root, "size {size}");
        // Every stored hash reads back as the root over the leaves it covers.
        let layout = Layout::new(size);
        for level in 0..layout.levels() {
            let span = 1u64 << (8 * u32::from(level));
            for index in 0..layout.hashes_at(level) {
                let lo = (index * span) as usize;
                let hi = ((index + 1) * span) as usize;
                assert_eq!(
                    reader.hash(level, index).unwrap(),
                    rfc9162::root(&all[lo..hi]),
                    "size {size} level {level} hash {index}"
                );
            }
        }
    }
}

#[test]
fn a_wrong_root_or_a_changed_tile_is_refused() {
    let all = leaves(600);
    let store = MemoryStore::new();
    write_tiles(&store, &all);
    let root = rfc9162::root(&all);
    let wrong = TileHashReader::new(store.clone(), 600, [0u8; 32]);
    assert!(wrong.root().is_err());
    assert!(wrong.hash(0, 0).is_err());

    // A full level-0 tile with one hash changed folds to a hash the level
    // above does not hold.
    let tile = Layout::new(600).tile(0, 1).unwrap();
    let mut bytes = store.get(&tile.path()).unwrap().unwrap();
    bytes[40] ^= 1;
    store.put(&tile.path(), &bytes).unwrap();
    let reader = TileHashReader::new(store.clone(), 600, root);
    assert!(reader.hash(0, 0).is_ok());
    assert!(reader.hash(0, 300).is_err());
}

#[test]
fn the_tlog_tiles_worked_example_at_70000() {
    let layout = Layout::new(70_000);
    assert_eq!(layout.levels(), 3);
    assert_eq!(layout.full_tiles_at(0), 273);
    assert_eq!(layout.width_at(0), 112);
    assert_eq!(layout.full_tiles_at(1), 1);
    assert_eq!(layout.width_at(1), 17);
    assert_eq!(layout.full_tiles_at(2), 0);
    assert_eq!(layout.width_at(2), 1);
    assert_eq!(layout.tiles().len(), 273 + 1 + 1 + 1 + 1);

    let all = leaves(70_000);
    let store = MemoryStore::new();
    let frontier = write_tiles(&store, &all);
    let root = rfc9162::root(&all);
    assert_eq!(frontier.root(), root);

    let full = |level: &str| {
        store
            .list(&format!("tile/{level}"))
            .unwrap()
            .into_iter()
            .filter(|p| !p.contains(".p/"))
            .count()
    };
    let partial = |level: &str| -> Vec<String> {
        store
            .list(&format!("tile/{level}"))
            .unwrap()
            .into_iter()
            .filter(|p| p.contains(".p/"))
            .collect()
    };
    assert_eq!(full("0"), 273);
    assert_eq!(partial("0"), vec!["tile/0/273.p/112".to_string()]);
    assert_eq!(full("1"), 1);
    assert_eq!(partial("1"), vec!["tile/1/001.p/17".to_string()]);
    assert_eq!(full("2"), 0);
    assert_eq!(partial("2"), vec!["tile/2/000.p/1".to_string()]);
    assert!(store.list("tile/3").unwrap().is_empty());

    let reader = TileHashReader::new(store.clone(), 70_000, root);
    assert_eq!(reader.root().unwrap(), root);
    assert_eq!(reader.hash(0, 69_999).unwrap(), all[69_999]);
    assert_eq!(
        reader.hash(1, 272).unwrap(),
        rfc9162::root(&all[272 * 256..273 * 256])
    );
    assert_eq!(reader.hash(2, 0).unwrap(), rfc9162::root(&all[..65_536]));
    assert!(reader.hash(0, 70_000).is_err());
    assert!(reader.tile(0, 274).is_err());
    let hash_bytes =
        store.bytes_under("tile/0") + store.bytes_under("tile/1") + store.bytes_under("tile/2");
    // Dense stored hashes: 2n - popcount(n) of them; tiles hold the same
    // number of hashes plus nothing, since every stored hash is in exactly
    // one tile.
    let hashes_in_tiles = layout
        .tiles()
        .iter()
        .map(|t| u64::from(t.width))
        .sum::<u64>();
    assert_eq!(hash_bytes as u64, hashes_in_tiles * 32);
    assert_eq!(hashes_in_tiles, 70_000 + 273 + 1);
    eprintln!(
        "70000 entries: hash tiles {hash_bytes} bytes, {:.3} bytes per entry",
        hash_bytes as f64 / 70_000.0
    );
    assert_eq!(TILE_WIDTH, 256);
}
