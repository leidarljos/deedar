# rfc9162-tiles

Tiled, indexed storage for an [rfc9162](https://crates.io/crates/rfc9162) log, laid out as
[C2SP tlog-tiles](https://c2sp.org/tlog-tiles): hash tiles of 256 hashes, entry bundles of 256
length-prefixed entries, partial tiles for the unsealed tail, and the tree `rfc9162` hashes. Any
tlog-tiles client reads the tree. The root over the tiles is `rfc9162::root` over the same leaves at
every size, and a proof assembled from tiles is the bytes `rfc9162::inclusion_proof` and
`rfc9162::consistency_proof` give over the flat leaf slice.

```rust
use rfc9162_tiles::{Frontier, Layout, MemoryStore, TileHashReader, TileStore};
use rfc9162_tiles::tree::encode_tile;

let leaves: Vec<[u8; 32]> = (0..300u32).map(|i| rfc9162::leaf_hash(&i.to_be_bytes())).collect();
let store = MemoryStore::new();
let mut frontier = Frontier::empty();
let mut completed = Vec::new();
for leaf in &leaves {
    frontier.push(*leaf, &mut completed);
}
for (tile, hashes) in &completed {
    store.put(&tile.path(), &encode_tile(hashes)).unwrap();
}
for (tile, hashes) in frontier.partial_tiles() {
    store.put(&tile.path(), &encode_tile(hashes)).unwrap();
}
assert_eq!(Layout::new(300).tiles().len(), 3); // one full level-0 tile, a partial of 44, a partial level-1 tile of 1
let reader = TileHashReader::new(store, 300, rfc9162::root(&leaves));
assert_eq!(reader.root().unwrap(), rfc9162::root(&leaves));
```

A reader checks every tile against the trusted root before it uses it: the partial tiles together,
by the root they fold to; a full tile by the hash it folds to in the tile one level up.

Part of [deedar](https://github.com/leidarljos/deedar). MIT.
