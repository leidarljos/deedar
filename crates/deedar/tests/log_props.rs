//! The log's promises over random append sequences rather than examples:
//! every leaf proves in, every earlier size proves a prefix of every later
//! one, one changed byte in a proof fails it, and a store closed and opened
//! again proves what it proved before.
//!
//! Cases come from a seeded generator, so a failure reruns exactly as it
//! failed; every assertion names the seed and the sizes. `LOG_PROPS_SEED`
//! runs another seed.

mod common;

use deed::DeedId;
use deedar::log::{self, Head};
use deedar::{Bridge, Receipt};
use sha2::{Digest, Sha256};

/// SplitMix64 (Steele, Lea and Flood, doi:10.1145/2714064.2660195), with the
/// paper's constants: enough to pick sizes, indices and bytes without a
/// dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform enough in `low..=high` for picking cases.
    fn within(&mut self, low: usize, high: usize) -> usize {
        low + (self.next() % (high - low + 1) as u64) as usize
    }
}

/// Any value will do; a fixed one makes a failure rerun as it failed.
const SEED: u64 = 0x6d30_7365_6564;

fn seed() -> u64 {
    std::env::var("LOG_PROPS_SEED")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(SEED)
}

/// Logs grow to one past `2^TOP` = 4097 entries, crossing every power of two
/// to 4096: a few thousand deeds, which the store test writes in 0.3 s (test
/// profile, 8-core Intel Xeon, 0bca3d7).
const TOP: u32 = 12;

/// Every index is proven while a tree is at most `2^EVERY_INDEX_TOP + 1`
/// wide. A proof costs the tree's width in hashes, so every index at every
/// size to 4097 took 13.6 s, and every index to 1025 with the chosen ones past
/// it takes 1.1 s (test profile, 8-core Intel Xeon, 0bca3d7).
const EVERY_INDEX_TOP: u32 = 10;

/// Each size either side of every power of two to `2^top`, where the tree
/// gains a level and its split point moves, and one drawn from inside each
/// level, so a level is also tried at a width beside no boundary.
fn sizes(rng: &mut Rng, top: u32) -> Vec<usize> {
    let mut sizes = Vec::new();
    for level in 0..=top {
        let power = 1usize << level;
        sizes.extend([power - 1, power, power + 1]);
        if level < top && power + 2 <= 2 * power - 2 {
            sizes.push(rng.within(power + 2, 2 * power - 2));
        }
    }
    sizes.retain(|&size| size > 0);
    sizes.sort_unstable();
    sizes.dedup();
    sizes
}

/// Every index of a narrow tree. In a wide one: the first and the last,
/// either side of each power of two inside it, where a leaf crosses from a
/// left subtree to a right one, and one drawn from inside each level.
fn indices(size: usize, rng: &mut Rng) -> Vec<usize> {
    if size <= (1 << EVERY_INDEX_TOP) + 1 {
        return (0..size).collect();
    }
    let mut out = vec![0, size - 1];
    let mut power = 1;
    while power < size {
        out.extend([
            power - 1,
            power,
            rng.within(power, (2 * power).min(size) - 1),
        ]);
        power *= 2;
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn leaves(rng: &mut Rng, count: usize) -> Vec<[u8; 32]> {
    (0..count)
        .map(|_| log::leaf_hash(&rng.next().to_le_bytes()))
        .collect()
}

/// `path` with one byte of step `at` changed; the xor is not zero, so it is a
/// change.
fn flipped(path: &[[u8; 32]], at: usize, rng: &mut Rng) -> Vec<[u8; 32]> {
    let mut out = path.to_vec();
    out[at][rng.within(0, 31)] ^= rng.within(1, 255) as u8;
    out
}

#[test]
fn every_leaf_proves_in_and_one_changed_byte_fails_it() {
    let seed = seed();
    let mut rng = Rng(seed);
    let sizes = sizes(&mut rng, TOP);
    let all = leaves(&mut rng, *sizes.last().expect("a size"));
    let mut proofs = 0usize;
    for &size in &sizes {
        let tree = &all[..size];
        let head = Head::over(tree);
        for index in indices(size, &mut rng) {
            let path = log::inclusion_proof(tree, index).expect("a path");
            assert!(
                log::verify_inclusion(&tree[index], index, size, &path, &head.root),
                "seed {seed}: leaf {index} of {size} did not prove in"
            );
            for at in 0..path.len() {
                let bad = flipped(&path, at, &mut rng);
                assert!(
                    !log::verify_inclusion(&tree[index], index, size, &bad, &head.root),
                    "seed {seed}: leaf {index} of {size} proved in with step {at} changed"
                );
            }
            proofs += 1;
        }
    }
    eprintln!(
        "seed {seed}: {proofs} inclusion proofs over {} sizes",
        sizes.len()
    );
}

#[test]
fn every_earlier_size_proves_a_prefix_and_one_changed_byte_fails_it() {
    let seed = seed();
    let mut rng = Rng(seed);
    let sizes = sizes(&mut rng, TOP);
    let all = leaves(&mut rng, *sizes.last().expect("a size"));
    let heads: Vec<Head> = sizes.iter().map(|&size| Head::over(&all[..size])).collect();
    let mut proofs = 0usize;
    for (&new, new_head) in sizes.iter().zip(&heads) {
        for (&old, old_head) in sizes.iter().zip(&heads).take_while(|(&old, _)| old <= new) {
            let path = log::consistency_proof(&all[..new], old).expect("a proof");
            assert!(
                log::verify_consistency(old, &old_head.root, new, &new_head.root, &path),
                "seed {seed}: {old} did not prove a prefix of {new}"
            );
            for at in 0..path.len() {
                let bad = flipped(&path, at, &mut rng);
                assert!(
                    !log::verify_consistency(old, &old_head.root, new, &new_head.root, &bad),
                    "seed {seed}: {old} proved a prefix of {new} with step {at} changed"
                );
            }
            proofs += 1;
        }
    }
    eprintln!(
        "seed {seed}: {proofs} consistency proofs over {} sizes",
        sizes.len()
    );
}

/// Written proofs come from logs to one past `2^TEXT_TOP` = 17 entries,
/// whose paths run to five steps and whose sizes to two digits. Reading a line
/// does not depend on the tree's size, and every byte at every other value is
/// already 1.1 million changes and 2.4 s (test profile, 8-core Intel Xeon,
/// 0bca3d7), so a wider tree adds time and no new kind of line.
const TEXT_TOP: u32 = 4;

/// `text` with byte `at` set to each other value that leaves it UTF-8, as a
/// reader would get it from disk; the rest never reach a parser.
fn one_byte_changes(text: &str, at: usize) -> impl Iterator<Item = (u8, String)> + '_ {
    let original = text.as_bytes()[at];
    (0..=u8::MAX)
        .filter(move |&b| b != original)
        .filter_map(move |b| {
            let mut bytes = text.as_bytes().to_vec();
            bytes[at] = b;
            String::from_utf8(bytes).ok().map(|changed| (b, changed))
        })
}

/// Every one-byte change to a written receipt or bridge fails to check, or
/// checks against a head other than the one the reader holds. A path cannot
/// see its head's size (leaf 0 joins the same two siblings in trees of 3 and
/// 4), which is why `check_handover` holds receipts to the signed head and
/// `check_since` holds the bridge to the kept one.
#[test]
fn one_changed_byte_in_a_written_proof_fails_against_the_head_held() {
    let seed = seed();
    let mut rng = Rng(seed);
    let sizes = sizes(&mut rng, TEXT_TOP);
    let count = *sizes.last().expect("a size");
    let bodies: Vec<[u8; 8]> = (0..count).map(|_| rng.next().to_le_bytes()).collect();
    let entries: Vec<log::Entry> = bodies
        .iter()
        .enumerate()
        .map(|(n, body)| log::Entry {
            id: format!("deed-quote-prop{n}"),
            digest: format!("sha256:{}", log::hex(&Sha256::digest(body))),
            unix_time: rng.next() >> 32,
        })
        .collect();
    let all: Vec<[u8; 32]> = entries
        .iter()
        .map(|e| log::leaf_hash(&e.material()))
        .collect();
    let (mut tried, mut elsewhere) = (0usize, 0usize);
    for &size in &sizes {
        let head = Head::over(&all[..size]);
        let index = rng.within(0, size - 1);
        let receipt = Receipt {
            id: entries[index].id.clone(),
            digest: entries[index].digest.clone(),
            unix_time: entries[index].unix_time,
            index,
            size,
            root: head.root.clone(),
            path: log::inclusion_proof(&all[..size], index).expect("a path"),
        };
        let text = receipt.render();
        Receipt::parse(&text)
            .and_then(|r| r.check(&bodies[index]))
            .expect("the written receipt checks");
        let held = receipt.head();
        for at in 0..text.len() {
            for (value, changed) in one_byte_changes(&text, at) {
                let checked = Receipt::parse(&changed)
                    .and_then(|r| r.check(&bodies[index]).map(|()| r.head()));
                assert!(
                    !checked.as_ref().is_ok_and(|named| *named == held),
                    "seed {seed}: receipt {index} of {size} checked with byte {at} set to \
                     {value:#04x}:\n{changed}"
                );
                elsewhere += usize::from(checked.is_ok());
                tried += 1;
            }
        }

        let old = rng.within(1, size);
        let bridge = Bridge {
            from: Head::over(&all[..old]),
            to: head,
            path: log::consistency_proof(&all[..size], old).expect("a proof"),
        };
        let text = bridge.render();
        Bridge::parse(&text)
            .and_then(|b| b.check())
            .expect("the written bridge checks");
        let held = (bridge.from.clone(), bridge.to.clone());
        for at in 0..text.len() {
            for (value, changed) in one_byte_changes(&text, at) {
                let checked =
                    Bridge::parse(&changed).and_then(|b| b.check().map(|()| (b.from, b.to)));
                assert!(
                    !checked.as_ref().is_ok_and(|named| *named == held),
                    "seed {seed}: bridge {old} to {size} checked with byte {at} set to \
                     {value:#04x}:\n{changed}"
                );
                elsewhere += usize::from(checked.is_ok());
                tried += 1;
            }
        }
    }
    eprintln!(
        "seed {seed}: {tried} one-byte changes over {} sizes, {elsewhere} checked against \
         another head",
        sizes.len()
    );
}

/// Deeds go in through the client and the store is closed and opened again at
/// every size. The reader keeps each head it saw, so a bridge has to start at
/// exactly that head, not at one the reopened store works out.
#[test]
fn a_store_closed_and_opened_again_proves_what_it_proved_before() {
    let seed = seed();
    let mut rng = Rng(seed);
    let url = common::tmp_url();
    let sizes = sizes(&mut rng, TOP);
    let mut written: Vec<log::Entry> = Vec::new();
    let mut kept: Vec<Head> = Vec::new();
    let mut receipts = 0usize;
    for &size in &sizes {
        let mut client = common::open(&url);
        while written.len() < size {
            let n = written.len();
            let id = format!("deed-quote-prop{n}");
            let (deed, evidence) = client
                .create(common::quote(
                    &id,
                    &format!("excerpt {n}"),
                    "https://example.invalid/a",
                ))
                .expect("create");
            written.push(log::Entry {
                id,
                digest: deedar::deed_digest(&deed),
                unix_time: evidence.unix_time,
            });
        }
        let before = client.log_head().expect("head");
        drop(client);

        let client = common::open(&url);
        let head = client.log_head().expect("head");
        assert_eq!(head, before, "seed {seed}: {size} reopened as another head");
        let leaves: Vec<[u8; 32]> = written
            .iter()
            .map(|e| log::leaf_hash(&e.material()))
            .collect();
        assert_eq!(
            head,
            Head::over(&leaves),
            "seed {seed}: the head at {size} is not over what was written"
        );
        kept.push(head.clone());

        let exporter = client.exporter().expect("exporter");
        for index in indices(size, &mut rng) {
            let entry = &written[index];
            let receipt = exporter
                .receipt(&DeedId::parse(&entry.id).expect("an id"))
                .expect("a receipt");
            assert_eq!(
                (receipt.index, receipt.size, &receipt.root),
                (index, size, &head.root),
                "seed {seed}: receipt {index} of {size}"
            );
            assert!(
                log::verify_inclusion(&leaves[index], index, size, &receipt.path, &head.root),
                "seed {seed}: {} did not prove in at {size} after reopening",
                entry.id
            );
            for at in 0..receipt.path.len() {
                let bad = flipped(&receipt.path, at, &mut rng);
                assert!(
                    !log::verify_inclusion(&leaves[index], index, size, &bad, &head.root),
                    "seed {seed}: {} proved in at {size} with step {at} changed",
                    entry.id
                );
            }
            receipts += 1;
        }

        for earlier in &kept {
            let bridge = client.bridge(earlier.size).expect("a bridge");
            assert_eq!(
                (&bridge.from, &bridge.to),
                (earlier, &head),
                "seed {seed}: the bridge from {} to {size} is not between the heads kept",
                earlier.size
            );
            bridge.check().unwrap_or_else(|e| {
                panic!(
                    "seed {seed}: {} to {size} after reopening: {e}",
                    earlier.size
                )
            });
            for at in 0..bridge.path.len() {
                let bad = Bridge {
                    path: flipped(&bridge.path, at, &mut rng),
                    ..bridge.clone()
                };
                assert!(
                    bad.check().is_err(),
                    "seed {seed}: {} to {size} checked with step {at} changed",
                    earlier.size
                );
            }
        }
    }
    eprintln!(
        "seed {seed}: {} deeds, reopened at {} sizes, {receipts} receipts, {} bridges",
        written.len(),
        sizes.len(),
        kept.len() * (kept.len() + 1) / 2
    );
}
