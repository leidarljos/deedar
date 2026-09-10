//! The writer's output against the reader, on a real store.
//!
//! The proof format has unit tests over hand-built trees, and those say the
//! checker is right about trees. They do not say the exporter writes what the
//! checker reads, and that is the join that had never been walked: a producer
//! whose consumer was never written is a producer nobody has checked.

mod common;

use std::collections::BTreeSet;

use deed::DeedId;
use deedar::{check_handover, Bridge, Client, FsStore, Receipt};

use common::{quote, tmp_url};

fn store(url: &str) -> FsStore {
    FsStore::open(deedar::store_dir(url).unwrap()).unwrap()
}

fn mint(url: &str, n: usize) -> Vec<DeedId> {
    let mut client = Client::open(url).unwrap();
    (0..n)
        .map(|at| {
            let id = format!("deed-quote-handed{at}");
            client
                .create(quote(
                    &id,
                    &format!("what the work said, take {at}"),
                    "https://example.invalid/a",
                ))
                .unwrap();
            DeedId::parse(&id).unwrap()
        })
        .collect()
}

/// A receiver holding the bag and nothing else can check every deed in it.
#[test]
fn an_exported_bag_checks_out_with_no_store() {
    let url = tmp_url();
    let ids = mint(&url, 5);
    let store = store(&url);

    let bag = tempfile::tempdir().unwrap();
    let deeds = bag.path().join("data").join("deeds");
    for id in &ids[..3] {
        store.export_into(id, &deeds).unwrap();
    }

    let done = check_handover(bag.path()).unwrap();
    assert_eq!(done.proven.len(), 3);
    let head = done.head.clone().expect("a head to record");
    assert_eq!(head, store.log_head().unwrap());
    // Handed the bag or the directory inside it, the same answer, because a
    // receiver is given a bag and should not have to learn the layout to
    // check it.
    assert_eq!(check_handover(&deeds).unwrap(), done);
}

/// Bytes swapped after the export are caught, which is the property the
/// manifest of a bag cannot give: a manifest recomputed from the bag is the
/// bag checked against itself.
#[test]
fn bytes_that_moved_after_the_export_are_caught() {
    let url = tmp_url();
    let ids = mint(&url, 3);
    let store = store(&url);

    let bag = tempfile::tempdir().unwrap();
    let deeds = bag.path().join("data").join("deeds");
    for id in &ids {
        store.export_into(id, &deeds).unwrap();
    }
    check_handover(bag.path()).unwrap();

    let swapped = deeds.join(ids[1].as_str()).join("deed.bin");
    let mut bytes = std::fs::read(&swapped).unwrap();
    bytes.extend_from_slice(b"and one more thing");
    std::fs::write(&swapped, &bytes).unwrap();

    let err = check_handover(bag.path()).expect_err("edited bytes passed");
    assert!(format!("{err}").contains("hash to"), "{err}");
}

/// A deed minted after the receiver's last visit cannot be slipped in under
/// the head they already hold, which is the whole point of writing one down.
#[test]
fn a_later_deed_is_not_under_an_earlier_head() {
    let url = tmp_url();
    let ids = mint(&url, 2);
    let store = store(&url);
    let kept = store.log_head().unwrap();

    let later = mint_more(&url, "deed-quote-afterwards");
    let bag = tempfile::tempdir().unwrap();
    let deeds = bag.path().join("data").join("deeds");
    store.export_into(&later, &deeds).unwrap();

    let done = check_handover(bag.path()).unwrap();
    let now = done.head.expect("a head");
    assert_ne!(now, kept, "a new deed left the head where it was");

    // The bridge is what says the new head is the old one grown. It is the
    // only thing that does: the receipt for the later deed is perfectly good
    // against the head the sender is showing.
    let bridge = store.bridge(kept.size).unwrap();
    assert_eq!(bridge.from, kept);
    assert_eq!(bridge.to, now);
    Bridge::parse(&bridge.render()).unwrap().check().unwrap();

    // And the sender cannot bridge from a head this log never had.
    let counterfeit = Bridge {
        from: deedar::LogHead {
            size: kept.size,
            root: "0".repeat(64),
        },
        ..bridge
    };
    let err = counterfeit.check().expect_err("a made-up head bridged");
    assert!(format!("{err}").contains("dropped or rewritten"), "{err}");

    // A deed the sender never logged is not something they can even export.
    assert!(ids.iter().all(|id| *id != later));
}

/// Every deed in one bag is against one head, so there is one thing to record,
/// and a receipt from another moment is refused rather than averaged in.
#[test]
fn one_bag_is_one_head() {
    let url = tmp_url();
    let ids = mint(&url, 2);
    let store = store(&url);

    let bag = tempfile::tempdir().unwrap();
    let deeds = bag.path().join("data").join("deeds");
    store.export_into(&ids[0], &deeds).unwrap();
    let early = std::fs::read_to_string(deeds.join(ids[0].as_str()).join("proof.txt")).unwrap();

    // The log grows, and the second deed is exported against the head that
    // covers both.
    let later = mint_more(&url, "deed-quote-secondwave");
    store.export_into(&later, &deeds).unwrap();

    let err = check_handover(bag.path()).expect_err("two heads in one bag passed");
    assert!(
        format!("{err}").contains("where the rest of this satchel"),
        "{err}"
    );

    // Re-exported against the head the rest of the bag is against, it checks.
    store.export_into(&ids[0], &deeds).unwrap();
    let done = check_handover(bag.path()).unwrap();
    assert_eq!(done.proven.len(), 2);
    assert_ne!(
        early,
        std::fs::read_to_string(deeds.join(ids[0].as_str()).join("proof.txt")).unwrap(),
        "the receipt did not move with the head"
    );
}

/// The record carries what the leaf hashes over, and the log agrees.
#[test]
fn a_receipt_is_the_entry_the_log_holds() {
    let url = tmp_url();
    let ids = mint(&url, 4);
    let store = store(&url);
    let entries = store.log_entries().unwrap();

    let loose = tempfile::tempdir().unwrap();
    for (at, id) in ids.iter().enumerate() {
        let receipt = store.receipt(id).unwrap();
        assert_eq!(receipt.index, at);
        assert_eq!(receipt.id, entries[at].id);
        assert_eq!(receipt.digest, entries[at].digest);
        assert_eq!(receipt.unix_time, entries[at].unix_time);
        // Through the file, since the file is what travels.
        store.export_into(id, loose.path()).unwrap();
        let bytes = std::fs::read(loose.path().join(id.as_str()).join("deed.bin")).unwrap();
        Receipt::parse(&receipt.render())
            .unwrap()
            .check(&bytes)
            .unwrap();
    }

    // Every accession appears once, which is what makes the bag's proven list
    // a set a caller can compare against what they asked for.
    let asked: BTreeSet<String> = ids.iter().map(DeedId::to_string).collect();
    let bag = tempfile::tempdir().unwrap();
    let deeds = bag.path().join("data").join("deeds");
    for id in &ids {
        store.export_into(id, &deeds).unwrap();
    }
    let done = check_handover(bag.path()).unwrap();
    assert_eq!(done.proven.into_iter().collect::<BTreeSet<_>>(), asked);
}

fn mint_more(url: &str, id: &str) -> DeedId {
    Client::open(url)
        .unwrap()
        .create(quote(id, "a later take", "https://example.invalid/b"))
        .unwrap();
    DeedId::parse(id).unwrap()
}
