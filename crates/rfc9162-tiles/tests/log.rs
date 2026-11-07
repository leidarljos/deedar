//! A log written through the writer reads back entry for entry, survives a
//! reopen, refuses a second writer, and quarantines a torn tail on open.

use rfc9162::leaf_hash;
use rfc9162_tiles::layout::{TREE_STATE, TREE_STATE_LOCK};
use rfc9162_tiles::{Error, MemoryStore, PosixStore, Reader, TileStore, Writer};

fn entry(i: u64) -> Vec<u8> {
    format!("record {i}").into_bytes()
}

fn entries(range: std::ops::Range<u64>) -> Vec<Vec<u8>> {
    range.map(entry).collect()
}

fn leaves(n: u64) -> Vec<[u8; 32]> {
    (0..n).map(|i| leaf_hash(&entry(i))).collect()
}

#[test]
fn a_posix_log_reads_back_after_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = PosixStore::open(dir.path()).unwrap();
    {
        let mut writer = Writer::open(store.clone()).unwrap();
        assert_eq!(writer.size(), 0);
        assert_eq!(writer.root(), rfc9162::root(&[]));
        assert_eq!(writer.append_batch(entries(0..300)).unwrap(), 0..300);
        assert_eq!(writer.append(&entry(300)).unwrap(), 300);
        assert!(store.exists(TREE_STATE_LOCK).unwrap());
        // A second writer is refused while the first holds the lock.
        assert!(matches!(Writer::open(store.clone()), Err(Error::Locked(_))));
    }
    assert!(!store.exists(TREE_STATE_LOCK).unwrap());
    let mut writer = Writer::open(store.clone()).unwrap();
    assert_eq!(writer.size(), 301);
    assert_eq!(writer.root(), rfc9162::root(&leaves(301)));
    assert_eq!(
        (writer.quarantined().0.len(), writer.quarantined().1),
        (0, None)
    );
    writer.append_batch(entries(301..600)).unwrap();
    drop(writer);

    let reader = Reader::open(store.clone()).unwrap();
    assert_eq!(reader.size(), 600);
    assert_eq!(reader.root(), rfc9162::root(&leaves(600)));
    for i in [0u64, 1, 255, 256, 300, 301, 511, 512, 599] {
        assert_eq!(reader.get(i).unwrap(), Some(entry(i)), "entry {i}");
    }
    assert_eq!(reader.get(600).unwrap(), None);
    // Layout on disk: two full bundles, a partial of 88, and the partials of
    // the sealed bundles are gone.
    let files = store.list("tile/entries").unwrap();
    assert_eq!(
        files,
        vec![
            "tile/entries/000".to_string(),
            "tile/entries/001".to_string(),
            "tile/entries/002.p/88".to_string(),
        ]
    );
    let tiles = store.list("tile/0").unwrap();
    assert_eq!(
        tiles,
        vec![
            "tile/0/000".to_string(),
            "tile/0/001".to_string(),
            "tile/0/002.p/88".to_string(),
        ]
    );
    // The level-1 partial of size 301 stays beside the one of size 600:
    // partials go only when their full tile exists.
    assert_eq!(
        store.list("tile/1").unwrap(),
        vec!["tile/1/000.p/1".to_string(), "tile/1/000.p/2".to_string()]
    );
    assert_eq!(
        String::from_utf8(store.get(TREE_STATE).unwrap().unwrap()).unwrap(),
        format!("600\n{}\n", rfc9162::hex(&rfc9162::root(&leaves(600))))
    );
}

#[test]
fn a_torn_tail_is_quarantined_not_adopted() {
    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone()).unwrap();
    writer.append_batch(entries(0..300)).unwrap();
    let root = writer.root();
    drop(writer);

    // A crash after the bundles and tiles of a bigger append, before the
    // tree state: a wider partial bundle, a sealed bundle the state does not
    // know, its hash tile, and a wider partial tile.
    store.put("tile/entries/001.p/60", b"junk").unwrap();
    store.put("tile/entries/002", b"junk").unwrap();
    store.put("tile/0/002", b"junk").unwrap();
    store.put("tile/0/003.p/9", b"junk").unwrap();
    store.put("tile/1/000.p/3", b"junk").unwrap();
    store.put("tile/index/002", b"junk").unwrap();
    store.put("tile/odd", b"junk").unwrap();

    let mut writer = Writer::open(store.clone()).unwrap();
    assert_eq!(writer.size(), 300);
    assert_eq!(writer.root(), root);
    let (moved, dir) = writer.quarantined();
    let dir = dir.expect("a torn directory").to_string();
    assert!(dir.starts_with(".state/torn-"), "{dir}");
    assert_eq!(
        moved,
        &[
            "tile/0/002",
            "tile/0/003.p/9",
            "tile/1/000.p/3",
            "tile/entries/001.p/60",
            "tile/entries/002",
            "tile/index/002",
            "tile/odd",
        ]
    );
    for path in moved {
        assert!(!store.exists(path).unwrap(), "{path} still live");
        assert_eq!(
            store.get(&format!("{dir}/{path}")).unwrap(),
            Some(b"junk".to_vec()),
            "{path} not under {dir}"
        );
    }
    // The live partials of size 300 stay.
    assert!(store.exists("tile/entries/001.p/44").unwrap());
    assert!(store.exists("tile/0/001.p/44").unwrap());
    assert!(store.exists("tile/1/000.p/1").unwrap());

    // The log goes on from 300.
    assert_eq!(writer.append(&entry(300)).unwrap(), 300);
    drop(writer);
    let reader = Reader::open(store.clone()).unwrap();
    assert_eq!(reader.get(300).unwrap(), Some(entry(300)));
    assert_eq!(reader.root(), rfc9162::root(&leaves(301)));
}

#[test]
fn tiles_without_a_tree_state_are_all_torn() {
    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone()).unwrap();
    writer.append_batch(entries(0..10)).unwrap();
    drop(writer);
    store.delete(TREE_STATE).unwrap();
    let writer = Writer::open(store.clone()).unwrap();
    assert_eq!(writer.size(), 0);
    assert_eq!(writer.quarantined().0.len(), 2);
    assert!(store.list("tile").unwrap().is_empty());
}

#[test]
fn an_entry_past_65535_bytes_changes_nothing() {
    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone()).unwrap();
    writer.append_batch(entries(0..3)).unwrap();
    let before = store.list("").unwrap();
    let result = writer.append_batch(vec![entry(3), vec![0u8; 65536]]);
    assert!(matches!(result, Err(Error::EntryTooLong(65536))));
    assert_eq!(writer.size(), 3);
    assert_eq!(store.list("").unwrap(), before);
    assert_eq!(writer.append(&vec![1u8; 65535]).unwrap(), 3);
}

#[test]
fn a_reader_refuses_an_entry_that_does_not_hash_to_its_leaf() {
    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone()).unwrap();
    writer.append_batch(entries(0..5)).unwrap();
    drop(writer);
    let mut bytes = store.get("tile/entries/000.p/5").unwrap().unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    store.put("tile/entries/000.p/5", &bytes).unwrap();
    let reader = Reader::open(store.clone()).unwrap();
    assert!(matches!(reader.get(4), Err(Error::Corrupt { .. })));
    // The writer refuses to adopt it too.
    assert!(matches!(
        Writer::open(store.clone()),
        Err(Error::Corrupt { .. })
    ));
    assert!(!store.exists(TREE_STATE_LOCK).unwrap());
}
