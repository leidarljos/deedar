//! Key lookup through manifest, filter and sidecar across sealed bundles and
//! the unsealed tail; sidecars that rebuild to the bytes written; the sizes
//! the layout costs at the tlog-tiles worked example.

use rfc9162_tiles::index::{identity, no_keys, KeyFn};
use rfc9162_tiles::layout::{MANIFEST, TILE_WIDTH};
use rfc9162_tiles::{Error, MemoryStore, PosixStore, Reader, Sidecar, TileStore, Writer};
use sha2::{Digest, Sha256};

/// `<package>:<record>`; the package is the caller key.
fn entry(i: u64) -> Vec<u8> {
    format!("pkg-{}:record {i}", i % 13).into_bytes()
}

fn keys() -> KeyFn {
    Box::new(|entry: &[u8]| {
        let colon = entry.iter().position(|b| *b == b':').unwrap();
        vec![entry[..colon].to_vec()]
    })
}

#[test]
fn keys_are_found_across_bundles_and_in_the_tail() {
    let dir = tempfile::tempdir().unwrap();
    let store = PosixStore::open(dir.path()).unwrap();
    let mut writer = Writer::open(store.clone(), keys()).unwrap();
    writer.append_batch((0..700).map(entry)).unwrap();
    assert_eq!(writer.manifest().bundles.len(), 2);
    drop(writer);

    assert_eq!(
        store.list("tile/index").unwrap(),
        vec!["tile/index/000".to_string(), "tile/index/001".to_string()]
    );
    assert!(store.exists(MANIFEST).unwrap());

    let reader = Reader::open(store.clone(), keys()).unwrap();
    // A caller key: every record of the package, in both sealed bundles and
    // the 188-entry tail.
    let expected: Vec<u64> = (0..700u64).filter(|i| i % 13 == 5).collect();
    assert_eq!(reader.find(b"pkg-5").unwrap(), expected);
    // Identity hashes, in a sealed bundle and in the tail.
    for i in [0u64, 255, 256, 511, 512, 699] {
        assert_eq!(
            reader.find(&identity(&entry(i))).unwrap(),
            vec![i],
            "entry {i}"
        );
    }
    assert!(reader.find(b"pkg-13").unwrap().is_empty());
    assert!(reader.find(&[0u8; 32]).unwrap().is_empty());

    // Manifest rows carry the sidecar hash and the key range.
    for row in &reader.manifest().bundles {
        let bytes = store
            .get(&format!("tile/index/{:03}", row.index))
            .unwrap()
            .unwrap();
        assert_eq!(row.sidecar, rfc9162::hex(&Sha256::digest(&bytes)));
        assert_eq!(row.keys, 2 * TILE_WIDTH as usize);
        assert!(row.min < row.max);
    }
}

#[test]
fn a_rebuilt_sidecar_is_the_written_one() {
    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone(), keys()).unwrap();
    writer.append_batch((0..600).map(entry)).unwrap();
    drop(writer);
    let written: Vec<Vec<u8>> = (0..2)
        .map(|n| store.get(&format!("tile/index/{n:03}")).unwrap().unwrap())
        .collect();
    let manifest = store.get(MANIFEST).unwrap().unwrap();

    let reader = Reader::open(store.clone(), keys()).unwrap();
    for n in 0..2u64 {
        let rebuilt = Sidecar::build(&reader.bundle(n).unwrap(), &keys()).to_bytes();
        assert_eq!(rebuilt, written[n as usize], "sidecar {n}");
    }

    // Lose both sidecars and the manifest: open restores the same bytes.
    store.delete("tile/index/000").unwrap();
    store.put("tile/index/001", b"stale").unwrap();
    store.delete(MANIFEST).unwrap();
    let writer = Writer::open(store.clone(), keys()).unwrap();
    assert_eq!(writer.manifest().bundles.len(), 2);
    for (n, bytes) in written.iter().enumerate() {
        assert_eq!(
            &store.get(&format!("tile/index/{n:03}")).unwrap().unwrap(),
            bytes
        );
    }
    assert_eq!(store.get(MANIFEST).unwrap().unwrap(), manifest);
}

#[test]
fn a_sidecar_that_does_not_match_its_row_is_refused() {
    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone(), keys()).unwrap();
    writer.append_batch((0..300).map(entry)).unwrap();
    drop(writer);
    let mut bytes = store.get("tile/index/000").unwrap().unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    store.put("tile/index/000", &bytes).unwrap();
    let reader = Reader::open(store.clone(), keys()).unwrap();
    assert!(matches!(reader.find(b"pkg-0"), Err(Error::Corrupt { .. })));
}

#[test]
fn a_reader_with_no_key_function_still_finds_identity_hashes() {
    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone(), no_keys()).unwrap();
    writer.append_batch((0..300).map(entry)).unwrap();
    drop(writer);
    let reader = Reader::open(store.clone(), no_keys()).unwrap();
    assert_eq!(reader.find(&identity(&entry(7))).unwrap(), vec![7]);
    assert_eq!(reader.find(&identity(&entry(299))).unwrap(), vec![299]);
    assert!(reader.find(b"pkg-0").unwrap().is_empty());
    assert_eq!(reader.manifest().bundles[0].keys, TILE_WIDTH as usize);
}

#[test]
fn sizes_at_the_70000_entry_worked_example() {
    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone(), keys()).unwrap();
    writer.append_batch((0..70_000).map(entry)).unwrap();
    assert_eq!(writer.manifest().bundles.len(), 273);
    drop(writer);

    let reader = Reader::open(store.clone(), keys()).unwrap();
    let expected: Vec<u64> = (0..70_000u64).filter(|i| i % 13 == 11).collect();
    assert_eq!(reader.find(b"pkg-11").unwrap(), expected);
    for i in [0u64, 34_567, 69_887, 69_888, 69_999] {
        assert_eq!(reader.find(&identity(&entry(i))).unwrap(), vec![i]);
    }

    let entries = store.bytes_under("tile/entries");
    let tiles =
        store.bytes_under("tile/0") + store.bytes_under("tile/1") + store.bytes_under("tile/2");
    let sidecars = store.bytes_under("tile/index");
    let manifest = store.get(MANIFEST).unwrap().unwrap().len();
    let payload: usize = (0..70_000).map(|i| entry(i).len()).sum();
    assert_eq!(entries, payload + 2 * 70_000);
    assert_eq!(tiles, 32 * (70_000 + 273 + 1));
    eprintln!(
        "70000 entries: payload {payload} B; bundles {entries} B ({:.3} B/entry, 2 B/entry framing); \
         hash tiles {tiles} B ({:.3} B/entry); sidecars {sidecars} B ({:.3} B/entry); \
         manifest {manifest} B ({:.3} B/entry)",
        entries as f64 / 70_000.0,
        tiles as f64 / 70_000.0,
        sidecars as f64 / 70_000.0,
        manifest as f64 / 70_000.0
    );
}
