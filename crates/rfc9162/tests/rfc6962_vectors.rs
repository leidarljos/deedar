//! The roots of the eight-leaf tree used across Certificate Transparency
//! implementations (RFC 6962 hashing, which RFC 9162 keeps), recomputed here
//! from the leaf inputs, and every proof over them.

use rfc9162::{
    consistency_proof, hex, inclusion_proof, leaf_hash, root, verify_consistency, verify_inclusion,
    Head,
};

fn inputs() -> Vec<Vec<u8>> {
    vec![
        vec![],
        vec![0x00],
        vec![0x10],
        vec![0x20, 0x21],
        vec![0x30, 0x31],
        vec![0x40, 0x41, 0x42, 0x43],
        (0x50..0x58).collect(),
        (0x60..0x70).collect(),
    ]
}

const ROOTS: [&str; 8] = [
    "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
    "fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
    "aeb6bcfe274b70a14fb067a5e5578264db0fa9b51af5e0ba159158f329e06e77",
    "d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
    "4e3bbb1f7b478dcfe71fb631631519a3bca12c9aefca1612bfce4c13a86264d4",
    "76e67dadbcdf1e10e1b74ddc608abd2f98dfb16fbce75277b5232a127f2087ef",
    "ddb89be403809e325750d3d263cd78929c2942b7942a34b77e122c9594a74c8c",
    "5dc9da79a70659a9ad559cb701ded9a2ab9d823aad2f4960cfe370eff4604328",
];

fn leaves() -> Vec<[u8; 32]> {
    inputs().iter().map(|d| leaf_hash(d)).collect()
}

#[test]
fn the_empty_tree_is_the_hash_of_nothing() {
    assert_eq!(
        hex(&root(&[])),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn every_prefix_has_the_published_root() {
    let all = leaves();
    for (n, want) in ROOTS.iter().enumerate() {
        assert_eq!(hex(&root(&all[..=n])), *want, "tree of {} leaves", n + 1);
    }
}

#[test]
fn every_leaf_and_every_prefix_proves_against_the_published_roots() {
    let all = leaves();
    for size in 1..=8 {
        let head = Head::over(&all[..size]);
        assert_eq!(head.root, ROOTS[size - 1]);
        for index in 0..size {
            let path = inclusion_proof(&all[..size], index).unwrap();
            assert!(verify_inclusion(
                &all[index],
                index,
                size,
                &path,
                ROOTS[size - 1]
            ));
        }
        for old in 1..=size {
            let path = consistency_proof(&all[..size], old).unwrap();
            assert!(verify_consistency(
                old,
                ROOTS[old - 1],
                size,
                ROOTS[size - 1],
                &path
            ));
        }
    }
}

#[test]
fn the_domain_names_the_log_in_the_signed_bytes() {
    let head = Head::over(&leaves());
    assert_eq!(
        head.material("a-log"),
        format!("a-log\t8\t{}", ROOTS[7]).into_bytes()
    );
    assert_ne!(head.material("a-log"), head.material("another-log"));
}
