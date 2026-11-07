//! Proofs assembled from tiles are the proofs over the flat slice, byte for
//! byte, at every size to 2000 and at the tlog-tiles worked example.

use rfc9162::{
    consistency_proof, hex, inclusion_proof, leaf_hash, verify_consistency, verify_inclusion,
};
use rfc9162_tiles::index::no_keys;
use rfc9162_tiles::{MemoryStore, Reader, Writer};

fn entry(i: u64) -> Vec<u8> {
    format!("record {i}").into_bytes()
}

/// xorshift64*, so the indices are the same on every run and need no crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn check_size(store: &MemoryStore, leaves: &[[u8; 32]], size: u64, rng: &mut Rng, probes: usize) {
    let flat = &leaves[..size as usize];
    let reader = Reader::open(store.clone(), no_keys()).unwrap();
    assert_eq!(reader.size(), size);
    assert_eq!(reader.root(), rfc9162::root(flat));
    let root_hex = hex(&reader.root());
    let mut indices: Vec<u64> = (0..probes).map(|_| rng.below(size)).collect();
    indices.extend([0, size - 1, size / 2]);
    for index in indices {
        let from_tiles = reader.inclusion(index).unwrap().unwrap();
        let from_flat = inclusion_proof(flat, index as usize).unwrap();
        assert_eq!(
            from_tiles, from_flat,
            "inclusion at size {size} index {index}"
        );
        assert!(verify_inclusion(
            &flat[index as usize],
            index as usize,
            size as usize,
            &from_tiles,
            &root_hex
        ));
    }
    let mut olds: Vec<u64> = (0..probes).map(|_| 1 + rng.below(size)).collect();
    olds.extend([1, size, size / 2 + 1]);
    for old in olds {
        let from_tiles = reader.consistency(old).unwrap().unwrap();
        let from_flat = consistency_proof(flat, old as usize).unwrap();
        assert_eq!(
            from_tiles, from_flat,
            "consistency at size {size} old {old}"
        );
        assert!(verify_consistency(
            old as usize,
            &hex(&rfc9162::root(&flat[..old as usize])),
            size as usize,
            &root_hex,
            &from_tiles
        ));
    }
    assert_eq!(reader.inclusion(size).unwrap(), None);
    assert_eq!(reader.consistency(0).unwrap(), None);
    assert_eq!(reader.consistency(size + 1).unwrap(), None);
}

#[test]
fn proofs_from_tiles_are_the_flat_proofs_at_every_size_to_2000() {
    let leaves: Vec<[u8; 32]> = (0..2000).map(|i| leaf_hash(&entry(i))).collect();
    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone(), no_keys()).unwrap();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    {
        let reader = Reader::open(store.clone(), no_keys()).unwrap();
        assert_eq!(reader.root(), rfc9162::root(&[]));
        assert_eq!(reader.inclusion(0).unwrap(), None);
        assert_eq!(reader.consistency(1).unwrap(), None);
    }
    for size in 1..=2000u64 {
        writer.append(&entry(size - 1)).unwrap();
        check_size(&store, &leaves, size, &mut rng, 3);
    }
}

#[test]
fn proofs_from_tiles_are_the_flat_proofs_at_70000() {
    let leaves: Vec<[u8; 32]> = (0..70_000).map(|i| leaf_hash(&entry(i))).collect();
    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone(), no_keys()).unwrap();
    writer.append_batch((0..70_000).map(entry)).unwrap();
    drop(writer);
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    check_size(&store, &leaves, 70_000, &mut rng, 200);
    // Between sizes that are themselves tile boundaries.
    let reader = Reader::open(store.clone(), no_keys()).unwrap();
    for old in [256u64, 65_536, 69_888, 69_889, 69_999] {
        assert_eq!(
            reader.consistency(old).unwrap().unwrap(),
            consistency_proof(&leaves, old as usize).unwrap()
        );
    }
}
