//! An append-only log over the deeds a store has issued.
//!
//! Every deed is signed, and a signature says who wrote a deed. It does not
//! say what else the store holds, and that is a different question. A holder
//! can hand one reader a store and another reader the same store with a deed
//! removed, and both sets verify: every signature in them is good. `delete` is
//! a supported verb here, so this is not hypothetical.
//!
//! The log binds the set. Each deed appends a leaf; the leaves hash into a
//! Merkle tree; the root plus the number of leaves is a tree head, and the
//! store signs that. Two things follow that a pile of signatures cannot give:
//!
//! - an *inclusion* proof, which shows a deed is in the tree a head names, so
//!   a store that dropped it cannot produce a head that still covers it; and
//! - a *consistency* proof, which shows a later head extends an earlier one
//!   rather than replacing it, so a store cannot rewrite what it already
//!   published without every reader who kept a head noticing.
//!
//! The hashing is the one in RFC 6962: a leaf is `SHA-256(0x00 || entry)` and
//! a node is `SHA-256(0x01 || left || right)`, with the odd node at each level
//! carried up. The prefixes are what stop a leaf being passed off as a node.
//! Certificate Transparency uses this shape for the same reason a provenance
//! store wants it: the party you are auditing is the party serving the data.
//!
//! What this does not do is gossip. One store signing its own heads catches a
//! holder who rewrites history between two readings by the same reader, and
//! catches deletion outright. It does not catch a holder who keeps two
//! consistent logs and shows one to each reader; that needs the heads to be
//! compared somewhere neither controls, which is a network protocol rather
//! than a file format.

use sha2::{Digest, Sha256};

use deed::{Error, Result};

/// One entry: the deed's content address and the instant it was logged.
///
/// The address rather than the deed, because the log is about which deeds
/// exist and the bytes are already addressed by their hash. A log that
/// repeated the deed would be a second copy to keep in step.
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
    /// The bytes a leaf hashes over.
    ///
    /// Tab-separated and in a fixed order, because the leaf hash has to be the
    /// same on every machine that recomputes it from the same entry.
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
    /// Fails when the line is not three tab-separated fields with a numeric
    /// time, because a log that skipped what it could not read would report a
    /// tree the store never signed.
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
    let mut hasher = Sha256::new();
    hasher.update([0x00]);
    hasher.update(material);
    hasher.finalize().into()
}

/// Interior hash, `SHA-256(0x01 || left || right)`.
#[must_use]
pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([0x01]);
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// The Merkle root over `leaves`, or the hash of the empty string when there
/// are none, which is what RFC 6962 defines an empty tree's root to be.
#[must_use]
pub fn root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return Sha256::digest([]).into();
    }
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut pair = level.chunks_exact(2);
        for two in &mut pair {
            next.push(node_hash(&two[0], &two[1]));
        }
        // An odd leaf at this level is carried up rather than paired with
        // itself, which is what keeps a tree of n leaves distinct from one of
        // n + 1 where the last is a duplicate.
        if let [odd] = pair.remainder() {
            next.push(*odd);
        }
        level = next;
    }
    level[0]
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

/// The audit path proving the leaf at `index` is in a tree of `size` leaves.
///
/// The path is the sibling at each level, bottom up. A verifier that has the
/// leaf recomputes the root from it and compares against a head it already
/// trusts, which is what makes the proof worth more than the store's word.
#[must_use]
pub fn inclusion_proof(leaves: &[[u8; 32]], index: usize) -> Option<Vec<[u8; 32]>> {
    if index >= leaves.len() {
        return None;
    }
    let mut path = Vec::new();
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    let mut at = index;
    while level.len() > 1 {
        let sibling = if at % 2 == 0 { at + 1 } else { at - 1 };
        // The carried-up odd node has no sibling at this level and so
        // contributes nothing to the path.
        if sibling < level.len() {
            path.push(level[sibling]);
        }
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut pair = level.chunks_exact(2);
        for two in &mut pair {
            next.push(node_hash(&two[0], &two[1]));
        }
        if let [odd] = pair.remainder() {
            next.push(*odd);
        }
        level = next;
        at /= 2;
    }
    Some(path)
}

/// Whether `leaf` sits at `index` of a tree of `size` leaves with root `root`.
#[must_use]
pub fn verify_inclusion(
    leaf: &[u8; 32],
    index: usize,
    size: usize,
    path: &[[u8; 32]],
    root_hex: &str,
) -> bool {
    if index >= size {
        return false;
    }
    let mut hash = *leaf;
    let mut at = index;
    let mut width = size;
    let mut step = 0usize;
    while width > 1 {
        let sibling = if at % 2 == 0 { at + 1 } else { at - 1 };
        if sibling < width {
            let Some(next) = path.get(step) else {
                return false;
            };
            step += 1;
            hash = if at % 2 == 0 {
                node_hash(&hash, next)
            } else {
                node_hash(next, &hash)
            };
        }
        at /= 2;
        width = width.div_ceil(2);
    }
    step == path.len() && hex(&hash) == root_hex
}

/// Hex, lower case, fixed width.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaves(n: usize) -> Vec<[u8; 32]> {
        (0..n)
            .map(|i| leaf_hash(format!("deed-{i}").as_bytes()))
            .collect()
    }

    /// A leaf and a node hash differently over the same bytes, which is what
    /// stops a leaf being presented as an interior node.
    #[test]
    fn a_leaf_is_not_a_node() {
        let left = leaf_hash(b"a");
        let right = leaf_hash(b"b");
        let parent = node_hash(&left, &right);
        let mut joined = Vec::new();
        joined.extend_from_slice(&left);
        joined.extend_from_slice(&right);
        assert_ne!(parent, leaf_hash(&joined));
    }

    /// Every leaf proves in, at every tree size, including the odd ones where
    /// a node is carried up a level with no sibling.
    #[test]
    fn every_leaf_proves_in() {
        for size in 1..=33usize {
            let set = leaves(size);
            let head = Head::over(&set);
            assert_eq!(head.size, size);
            for index in 0..size {
                let path = inclusion_proof(&set, index).expect("a path");
                assert!(
                    verify_inclusion(&set[index], index, size, &path, &head.root),
                    "size {size} index {index}"
                );
            }
        }
    }

    /// The proof is for one leaf at one place. A verifier that accepted it for
    /// another would be checking nothing.
    #[test]
    fn a_proof_does_not_travel() {
        let set = leaves(8);
        let head = Head::over(&set);
        let path = inclusion_proof(&set, 3).expect("a path");
        assert!(verify_inclusion(&set[3], 3, 8, &path, &head.root));
        // Same path, different leaf.
        assert!(!verify_inclusion(&set[4], 3, 8, &path, &head.root));
        // Same leaf, different place.
        assert!(!verify_inclusion(&set[3], 4, 8, &path, &head.root));
        // Same everything, a root the store did not publish.
        assert!(!verify_inclusion(&set[3], 3, 8, &path, &"0".repeat(64)));
        // A truncated path leaves the hash short of the root.
        assert!(!verify_inclusion(&set[3], 3, 8, &path[..2], &head.root));
    }

    /// Dropping a deed changes the root, which is the whole point: a store
    /// that removes one cannot publish a head that still covers it.
    #[test]
    fn removing_an_entry_moves_the_root() {
        let set = leaves(6);
        let full = Head::over(&set);
        let mut short = set.clone();
        short.remove(2);
        let missing = Head::over(&short);
        assert_ne!(full.root, missing.root);
        assert_ne!(full.size, missing.size);
        // And the proof the store issued before the removal no longer checks.
        let path = inclusion_proof(&set, 2).expect("a path");
        assert!(!verify_inclusion(
            &set[2],
            2,
            short.len(),
            &path,
            &missing.root
        ));
    }

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
