//! An append-only Merkle log over the deeds a store has issued, hashed as
//! RFC 9162 (doi:10.17487/RFC9162): leaf `SHA-256(0x00 || entry)`, node
//! `SHA-256(0x01 || left || right)`. Inclusion proofs catch a dropped deed;
//! consistency proofs catch a rewritten log. Signatures alone bind neither.
//! Gossip lives outside this file. Two consistent logs shown to two readers
//! both verify here.
//!
//! The hash, root and proof functions call the `rfc9162` crate. The entry
//! line and the signed head stay in this file.

use deed::{Error, Result};

/// One entry: the deed's content address and the instant it was logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The deed's accession.
    pub id: String,
    /// `sha256:` of the canonical deed bytes.
    pub digest: String,
    /// Seconds since the epoch, as the evidence records them.
    pub unix_time: u64,
}

impl Entry {
    /// The bytes a leaf hashes over: tab-separated, fixed order.
    #[must_use]
    pub fn material(&self) -> Vec<u8> {
        format!("{}\t{}\t{}", self.id, self.digest, self.unix_time).into_bytes()
    }

    /// The line this entry is stored as.
    #[must_use]
    pub fn line(&self) -> String {
        format!("{}\t{}\t{}\n", self.id, self.digest, self.unix_time)
    }

    /// Parse one stored line.
    ///
    /// # Errors
    ///
    /// Fails when the line is not three tab-separated fields with a numeric time.
    pub fn parse(line: &str) -> Result<Self> {
        let mut parts = line.trim_end_matches('\n').split('\t');
        let (Some(id), Some(digest), Some(time), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(Error::Io(format!("log: not an entry: {line:?}")));
        };
        let unix_time = time
            .parse()
            .map_err(|_| Error::Io(format!("log: not a time: {time:?}")))?;
        Ok(Self {
            id: id.to_string(),
            digest: digest.to_string(),
            unix_time,
        })
    }
}

/// Leaf hash, `SHA-256(0x00 || entry)`.
#[must_use]
pub fn leaf_hash(material: &[u8]) -> [u8; 32] {
    rfc9162::leaf_hash(material)
}

/// Interior hash, `SHA-256(0x01 || left || right)`.
#[must_use]
pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    rfc9162::node_hash(left, right)
}

/// The Merkle root over `leaves`, RFC 9162 section 2.1: the tree splits at
/// the largest power of two below the width, which is the shape the proofs
/// below are sound over.
#[must_use]
pub fn root(leaves: &[[u8; 32]]) -> [u8; 32] {
    rfc9162::root(leaves)
}

/// What a store publishes: how many entries it has, and the root over them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// Number of entries the root covers.
    pub size: usize,
    /// Merkle root, hex.
    pub root: String,
}

impl Head {
    /// The head over a set of leaves.
    #[must_use]
    pub fn over(leaves: &[[u8; 32]]) -> Self {
        Self {
            size: leaves.len(),
            root: hex(&root(leaves)),
        }
    }

    /// The bytes a signature covers.
    #[must_use]
    pub fn material(&self) -> Vec<u8> {
        format!("deedar-log-v1\t{}\t{}", self.size, self.root).into_bytes()
    }
}

/// The audit path for the leaf at `index`, RFC 9162 section 2.1.3: sibling
/// subtree roots, innermost first.
#[must_use]
pub fn inclusion_proof(leaves: &[[u8; 32]], index: usize) -> Option<Vec<[u8; 32]>> {
    rfc9162::inclusion_proof(leaves, index)
}

/// Whether `leaf` sits at `index` of a tree of `size` leaves with that root.
#[must_use]
pub fn verify_inclusion(
    leaf: &[u8; 32],
    index: usize,
    size: usize,
    path: &[[u8; 32]],
    root_hex: &str,
) -> bool {
    rfc9162::verify_inclusion(leaf, index, size, path, root_hex)
}

/// The consistency proof that a tree of `old` leaves is a prefix of these,
/// RFC 9162 section 2.1.4. The nodes recompute both roots; a rewritten log
/// cannot produce a set that yields both. `None` when `old` is zero or wider
/// than the tree.
#[must_use]
pub fn consistency_proof(leaves: &[[u8; 32]], old: usize) -> Option<Vec<[u8; 32]>> {
    rfc9162::consistency_proof(leaves, old)
}

/// Whether the tree of `old` leaves with `old_root` is a prefix of the tree of
/// `new` leaves with `new_root`; both roots are recomputed from `nodes`.
#[must_use]
pub fn verify_consistency(
    old: usize,
    old_root: &str,
    new: usize,
    new_root: &str,
    path: &[[u8; 32]],
) -> bool {
    rfc9162::verify_consistency(old, old_root, new, new_root, path)
}

/// A 32 byte hash from the lower-case hex [`hex`] writes. Any other text
/// yields nothing.
#[must_use]
pub fn from_hex(text: &str) -> Option<[u8; 32]> {
    rfc9162::from_hex(text)
}

/// Hex, lower case, fixed width.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    rfc9162::hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An entry round-trips through the line the log stores it as, and a line
    /// that is not an entry is refused rather than skipped.
    #[test]
    fn an_entry_round_trips_and_a_bad_line_is_refused() {
        let entry = Entry {
            id: "deed-file-note".into(),
            digest: "sha256:abc".into(),
            unix_time: 1_757_000_000,
        };
        let back = Entry::parse(&entry.line()).expect("parses");
        assert_eq!(back, entry);
        assert!(Entry::parse("two\tfields\n").is_err());
        assert!(Entry::parse("a\tb\tnot-a-time\n").is_err());
        assert!(Entry::parse("a\tb\t1\tc\n").is_err());
    }

    fn proof(text: &str) -> Vec<[u8; 32]> {
        text.split_whitespace()
            .map(|part| from_hex(part).expect("a vector hash"))
            .collect()
    }

    fn ct_leaves() -> Vec<[u8; 32]> {
        let inputs: [&[u8]; 8] = [
            b"",
            &[0x00],
            &[0x10],
            &[0x20, 0x21],
            &[0x30, 0x31],
            &[0x40, 0x41, 0x42, 0x43],
            &[0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57],
            &[
                0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d,
                0x6e, 0x6f,
            ],
        ];
        inputs.iter().copied().map(leaf_hash).collect()
    }

    const EMPTY_ROOT: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

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

    const INCLUSION: [&[&str]; 8] = [
        &[
            "",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7",
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7",
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d 0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7",
            "fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e",
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e",
            "07506a85fd9dd2f120eb694f86011e5bb4662e5c415a62917033d4a9624487e7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
            "07506a85fd9dd2f120eb694f86011e5bb4662e5c415a62917033d4a9624487e7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
            "d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a",
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a",
            "07506a85fd9dd2f120eb694f86011e5bb4662e5c415a62917033d4a9624487e7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a",
            "4271a26be0d8a84f0bd54c8c302e7cb3a3b5d1fa6780a40bcce2873477dab658 d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 837dbb152e9b079010717e84e865da4ebc0fa198a806d59d31bf15accef22d0e",
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 837dbb152e9b079010717e84e865da4ebc0fa198a806d59d31bf15accef22d0e",
            "07506a85fd9dd2f120eb694f86011e5bb4662e5c415a62917033d4a9624487e7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 837dbb152e9b079010717e84e865da4ebc0fa198a806d59d31bf15accef22d0e",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 837dbb152e9b079010717e84e865da4ebc0fa198a806d59d31bf15accef22d0e",
            "4271a26be0d8a84f0bd54c8c302e7cb3a3b5d1fa6780a40bcce2873477dab658 b08693ec2e721597130641e8211e7eedccb4c26413963eee6c1e2ed16ffb1a5f d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b b08693ec2e721597130641e8211e7eedccb4c26413963eee6c1e2ed16ffb1a5f d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4",
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4",
            "07506a85fd9dd2f120eb694f86011e5bb4662e5c415a62917033d4a9624487e7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4",
            "4271a26be0d8a84f0bd54c8c302e7cb3a3b5d1fa6780a40bcce2873477dab658 ca854ea128ed050b41b35ffc1b87b8eb2bde461e9e3b5596ece6b9d5975a0ae0 d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b ca854ea128ed050b41b35ffc1b87b8eb2bde461e9e3b5596ece6b9d5975a0ae0 d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "46f6ffadd3d06a09ff3c5860d2755c8b9819db7df44251788c7d8e3180de8eb1 0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "b08693ec2e721597130641e8211e7eedccb4c26413963eee6c1e2ed16ffb1a5f 0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
        ],
    ];

    const CONSISTENCY: [&[&str]; 8] = [
        &[
            "",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7",
            "",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7",
            "",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e",
            "5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7 07506a85fd9dd2f120eb694f86011e5bb4662e5c415a62917033d4a9624487e7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
            "",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
            "5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7 07506a85fd9dd2f120eb694f86011e5bb4662e5c415a62917033d4a9624487e7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
            "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
            "",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a",
            "5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7 07506a85fd9dd2f120eb694f86011e5bb4662e5c415a62917033d4a9624487e7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a",
            "0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a",
            "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b 4271a26be0d8a84f0bd54c8c302e7cb3a3b5d1fa6780a40bcce2873477dab658 d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 837dbb152e9b079010717e84e865da4ebc0fa198a806d59d31bf15accef22d0e",
            "5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 837dbb152e9b079010717e84e865da4ebc0fa198a806d59d31bf15accef22d0e",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7 07506a85fd9dd2f120eb694f86011e5bb4662e5c415a62917033d4a9624487e7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 837dbb152e9b079010717e84e865da4ebc0fa198a806d59d31bf15accef22d0e",
            "837dbb152e9b079010717e84e865da4ebc0fa198a806d59d31bf15accef22d0e",
            "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b 4271a26be0d8a84f0bd54c8c302e7cb3a3b5d1fa6780a40bcce2873477dab658 b08693ec2e721597130641e8211e7eedccb4c26413963eee6c1e2ed16ffb1a5f d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a b08693ec2e721597130641e8211e7eedccb4c26413963eee6c1e2ed16ffb1a5f d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "",
        ],
        &[
            "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7 5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4",
            "5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e 6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4",
            "0298d122906dcfc10892cb53a73992fc5b9f493ea4c9badb27b791b4127a7fe7 07506a85fd9dd2f120eb694f86011e5bb4662e5c415a62917033d4a9624487e7 fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125 6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4",
            "6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4",
            "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b 4271a26be0d8a84f0bd54c8c302e7cb3a3b5d1fa6780a40bcce2873477dab658 ca854ea128ed050b41b35ffc1b87b8eb2bde461e9e3b5596ece6b9d5975a0ae0 d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a ca854ea128ed050b41b35ffc1b87b8eb2bde461e9e3b5596ece6b9d5975a0ae0 d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "b08693ec2e721597130641e8211e7eedccb4c26413963eee6c1e2ed16ffb1a5f 46f6ffadd3d06a09ff3c5860d2755c8b9819db7df44251788c7d8e3180de8eb1 0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "",
        ],
    ];

    /// The eight Certificate Transparency leaves. RFC 9162 keeps that
    /// hashing. The empty root, each prefix root, and every inclusion and
    /// consistency proof are the values this log returns for them.
    #[test]
    fn the_ct_vectors_keep_their_roots_and_proofs() {
        let all = ct_leaves();
        assert_eq!(hex(&root(&[])), EMPTY_ROOT);
        for size in 1..=8 {
            let leaves = &all[..size];
            assert_eq!(hex(&root(leaves)), ROOTS[size - 1], "root of {size}");
            for (index, want) in INCLUSION[size - 1].iter().enumerate() {
                let path = inclusion_proof(leaves, index).expect("an inclusion proof");
                assert_eq!(path, proof(want), "inclusion of {index} in {size}");
            }
            for (before, want) in CONSISTENCY[size - 1].iter().enumerate() {
                let old = before + 1;
                let path = consistency_proof(leaves, old).expect("a consistency proof");
                assert_eq!(path, proof(want), "consistency of {old} into {size}");
            }
        }
    }

    /// The empty tree has a defined root rather than a panic, because a store
    /// publishes a head before it holds anything.
    #[test]
    fn an_empty_log_still_has_a_head() {
        let head = Head::over(&[]);
        assert_eq!(head.size, 0);
        assert_eq!(head.root.len(), 64);
        assert!(inclusion_proof(&[], 0).is_none());
    }
}
