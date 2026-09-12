//! An append-only Merkle log over the deeds a store has issued, hashed as
//! RFC 9162 (doi:10.17487/RFC9162): leaf `SHA-256(0x00 || entry)`, node
//! `SHA-256(0x01 || left || right)`. Inclusion proofs catch a dropped deed;
//! consistency proofs catch a rewritten log. Signatures alone bind neither.
//! No gossip: a holder showing two consistent logs to two readers is not caught.

use sha2::{Digest, Sha256};

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

/// The Merkle root over `leaves`, RFC 9162 section 2.1: the tree splits at
/// the largest power of two below the width, which is the shape the proofs
/// below are sound over.
#[must_use]
pub fn root(leaves: &[[u8; 32]]) -> [u8; 32] {
    match leaves {
        [] => Sha256::digest([]).into(),
        [one] => *one,
        _ => {
            let split = split_point(leaves.len());
            node_hash(&root(&leaves[..split]), &root(&leaves[split..]))
        }
    }
}

/// The largest power of two strictly below `width`, which is where a tree of
/// that width divides.
fn split_point(width: usize) -> usize {
    let mut split = 1usize;
    while split * 2 < width {
        split *= 2;
    }
    split
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
    if index >= leaves.len() {
        return None;
    }
    if leaves.len() == 1 {
        return Some(Vec::new());
    }
    let split = split_point(leaves.len());
    Some(if index < split {
        let mut path = inclusion_proof(&leaves[..split], index)?;
        path.push(root(&leaves[split..]));
        path
    } else {
        let mut path = inclusion_proof(&leaves[split..], index - split)?;
        path.push(root(&leaves[..split]));
        path
    })
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
    if index >= size {
        return false;
    }
    let mut hash = *leaf;
    let (mut at, mut last) = (index, size - 1);
    let mut steps = path.iter();
    while last > 0 {
        let Some(step) = steps.next() else {
            return false;
        };
        // A right-hand node, or the last node at this level, joins from the
        // left; anything else joins from the right.
        if at % 2 == 1 || at == last {
            hash = node_hash(step, &hash);
            while at != 0 && at % 2 == 0 {
                at /= 2;
                last /= 2;
            }
        } else {
            hash = node_hash(&hash, step);
        }
        at /= 2;
        last /= 2;
    }
    steps.next().is_none() && hex(&hash) == root_hex
}

/// The consistency proof that a tree of `old` leaves is a prefix of these,
/// RFC 9162 section 2.1.4. The nodes recompute both roots; a rewritten log
/// cannot produce a set that yields both. `None` when `old` is zero or wider
/// than the tree.
#[must_use]
pub fn consistency_proof(leaves: &[[u8; 32]], old: usize) -> Option<Vec<[u8; 32]>> {
    if old == 0 || old > leaves.len() {
        return None;
    }
    Some(subproof(leaves, old, true))
}

/// The RFC's SUBPROOF. `known` is whether the reader already holds the root of
/// the subtree being described, which is true exactly when it is the old tree
/// itself and so never needs sending.
fn subproof(leaves: &[[u8; 32]], old: usize, known: bool) -> Vec<[u8; 32]> {
    if old == leaves.len() {
        return if known {
            Vec::new()
        } else {
            vec![root(leaves)]
        };
    }
    let split = split_point(leaves.len());
    if old <= split {
        let mut path = subproof(&leaves[..split], old, known);
        path.push(root(&leaves[split..]));
        path
    } else {
        let mut path = subproof(&leaves[split..], old - split, false);
        path.push(root(&leaves[..split]));
        path
    }
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
    if old == 0 || old > new {
        return false;
    }
    if old == new {
        // Nothing was added, so there is nothing to send and the two heads
        // have to be the same head.
        return path.is_empty() && old_root == new_root;
    }
    // When the old tree is a whole subtree its root was never sent, because
    // the reader is the one holding it.
    let (seed, rest) = if old.is_power_of_two() {
        let Some(seed) = from_hex(old_root) else {
            return false;
        };
        (seed, path)
    } else {
        let Some((head, rest)) = path.split_first() else {
            return false;
        };
        (*head, rest)
    };

    let (mut at, mut last) = (old - 1, new - 1);
    while at % 2 == 1 {
        at /= 2;
        last /= 2;
    }
    let (mut from_old, mut from_new) = (seed, seed);
    for step in rest {
        if last == 0 {
            return false;
        }
        if at % 2 == 1 || at == last {
            from_old = node_hash(step, &from_old);
            from_new = node_hash(step, &from_new);
            while at != 0 && at % 2 == 0 {
                at /= 2;
                last /= 2;
            }
        } else {
            from_new = node_hash(&from_new, step);
        }
        at /= 2;
        last /= 2;
    }
    last == 0 && hex(&from_old) == old_root && hex(&from_new) == new_root
}

/// A 32 byte hash from its hex, or nothing when the text is not one.
#[must_use]
pub fn from_hex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (slot, pair) in out.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        let pair = std::str::from_utf8(pair).ok()?;
        *slot = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(out)
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

    /// Every prefix of every tree up to the bound proves it is a prefix.
    #[test]
    fn every_prefix_proves_it_is_one() {
        for new in 1..=33usize {
            let all = leaves(new);
            let later = Head::over(&all);
            for old in 1..=new {
                let earlier = Head::over(&all[..old]);
                let path = consistency_proof(&all, old).expect("a proof");
                assert!(
                    verify_consistency(old, &earlier.root, new, &later.root, &path),
                    "size {old} is not proving a prefix of {new}"
                );
            }
        }
    }

    /// A log rewritten wholesale passes inclusion and fails consistency.
    #[test]
    fn a_rewritten_log_cannot_extend_the_head_it_replaced() {
        let honest = leaves(8);
        let published = Head::over(&honest);

        // The store drops what it published at index 2 and appends two more,
        // so the tree is the same width it would honestly have been.
        let mut rewritten: Vec<[u8; 32]> = honest.clone();
        rewritten.remove(2);
        rewritten.push(leaf_hash(b"deed-8"));
        rewritten.push(leaf_hash(b"deed-9"));
        let now = Head::over(&rewritten);

        // Inclusion against the new head is fine for everything in it, which
        // is exactly why inclusion alone does not answer this.
        let path = inclusion_proof(&rewritten, 0).expect("a path");
        assert!(verify_inclusion(
            &rewritten[0],
            0,
            now.size,
            &path,
            &now.root
        ));

        // Consistency is not: no proof the store can offer makes the head it
        // published an ancestor of the one it publishes now.
        let offered = consistency_proof(&rewritten, published.size).expect("a proof");
        assert!(
            !verify_consistency(
                published.size,
                &published.root,
                now.size,
                &now.root,
                &offered
            ),
            "a rewritten log passed as an extension"
        );
    }

    /// A proof for one pair of heads does not check another, and a shortened
    /// or padded one does not check at all.
    #[test]
    fn a_consistency_proof_does_not_travel() {
        let all = leaves(12);
        let later = Head::over(&all);
        let earlier = Head::over(&all[..5]);
        let path = consistency_proof(&all, 5).expect("a proof");
        assert!(verify_consistency(5, &earlier.root, 12, &later.root, &path));

        // A different old size.
        let other = Head::over(&all[..6]);
        assert!(!verify_consistency(6, &other.root, 12, &later.root, &path));
        // A root nobody published.
        assert!(!verify_consistency(
            5,
            &"0".repeat(64),
            12,
            &later.root,
            &path
        ));
        // Truncated, and padded.
        assert!(!verify_consistency(
            5,
            &earlier.root,
            12,
            &later.root,
            &path[..1]
        ));
        let mut padded = path.clone();
        padded.push([0u8; 32]);
        assert!(!verify_consistency(
            5,
            &earlier.root,
            12,
            &later.root,
            &padded
        ));
        // Backwards is not a prefix.
        assert!(!verify_consistency(
            12,
            &later.root,
            5,
            &earlier.root,
            &path
        ));
        // And an empty tree has no prefix to speak of.
        assert!(consistency_proof(&all, 0).is_none());
        assert!(consistency_proof(&all, 13).is_none());
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
