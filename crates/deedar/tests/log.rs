//! What the append-only log catches that a pile of signatures does not.
//!
//! Every deed here is signed and every signature stays good after a deletion,
//! because a signature is a statement about one deed and says nothing about
//! which deeds exist. The log is the statement about the set.

mod common;

use deed::DeedId;
use deedar::{log, Client, FsStore};

use common::{quote, tmp_url};

fn store(url: &str) -> FsStore {
    FsStore::open(deedar::store_dir(url).unwrap()).unwrap()
}

fn three(url: &str) -> Vec<DeedId> {
    let mut client = Client::open(url).unwrap();
    let mut ids = Vec::new();
    for n in 0..3 {
        let id = format!("deed-quote-entry{n}");
        client
            .create(quote(
                &id,
                &format!("excerpt {n}"),
                "https://example.invalid/a",
            ))
            .unwrap();
        ids.push(DeedId::parse(&id).unwrap());
    }
    ids
}

/// The head moves once per deed and covers exactly what was written.
#[test]
fn every_created_deed_lands_in_the_log() {
    let url = tmp_url();
    let ids = three(&url);
    let store = store(&url);

    let entries = store.log_entries().unwrap();
    assert_eq!(entries.len(), 3, "{entries:?}");
    for (entry, id) in entries.iter().zip(&ids) {
        assert_eq!(entry.id, id.to_string());
        // The entry names the deed by content, so a store that swapped the
        // bytes under an accession would not match its own log.
        assert_eq!(entry.digest, deedar::deed_digest(&store.get(id).unwrap()));
    }
    let head = store.log_head().unwrap();
    assert_eq!(head.size, 3);
    assert_eq!(head.root.len(), 64);
}

/// Each deed proves it is in the tree the store publishes.
#[test]
fn a_deed_proves_it_is_in_the_published_tree() {
    let url = tmp_url();
    let ids = three(&url);
    let store = store(&url);
    let head = store.log_head().unwrap();
    let entries = store.log_entries().unwrap();

    for (at, id) in ids.iter().enumerate() {
        let (index, path) = store.log_proof(id).unwrap();
        assert_eq!(index, at);
        let leaf = log::leaf_hash(&entries[index].material());
        assert!(
            log::verify_inclusion(&leaf, index, head.size, &path, &head.root),
            "{id} did not prove in"
        );
    }
}

/// The point of the whole thing: a deletion leaves every remaining signature
/// good, and the log still says the deed was there.
#[test]
fn a_deletion_is_visible_in_the_log_and_nowhere_else() {
    let url = tmp_url();
    let ids = three(&url);
    let mut client = Client::open(&url).unwrap();

    // Before: the store answers for everything, in both directions.
    assert!(store(&url).log_audit().unwrap().is_clean());

    client.delete(&ids[1]).unwrap();
    let store = store(&url);

    // The two survivors still verify. Nothing about them changed, which is
    // exactly why a signature cannot answer this question.
    for id in [&ids[0], &ids[2]] {
        assert!(store.get(id).is_ok(), "{id} should have survived");
        assert!(store.evidence(id).is_ok(), "{id} lost its evidence");
    }

    // The log is append-only, so it still holds three entries and its head is
    // unchanged. The store now serves two.
    let entries = store.log_entries().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(store.log_head().unwrap().size, 3);

    // And the audit names the one that went missing, rather than the reader
    // having to know what to look for.
    let audit = store.log_audit().unwrap();
    assert!(!audit.is_clean(), "{audit:?}");
    assert_eq!(audit.missing.len(), 1, "{audit:?}");
    assert_eq!(audit.missing[0].id, ids[1].to_string());
    // And this is a deletion, not a store that never logged: telling the two
    // apart is what stops a pre-log store being called tampered with.
    assert!(audit.unlogged.is_empty(), "{audit:?}");
    assert!(!audit.predates_the_log(), "{audit:?}");
}

/// A store that never wrote a log answers for an empty one rather than
/// failing, because stores written before it exists keep opening.
#[test]
fn a_store_with_no_log_reads_as_empty() {
    let url = tmp_url();
    let store = store(&url);
    assert!(store.log_entries().unwrap().is_empty());
    assert_eq!(store.log_head().unwrap().size, 0);
    assert!(store.log_audit().unwrap().is_clean());
    assert!(store
        .log_proof(&DeedId::parse("deed-quote-absent").unwrap())
        .is_err());
}

/// A head is over a tree, not over a count: two stores with the same number of
/// deeds and different deeds publish different roots.
#[test]
fn the_head_is_about_which_deeds_not_how_many() {
    let first = tmp_url();
    let second = tmp_url();
    three(&first);
    let mut client = Client::open(&second).unwrap();
    for n in 0..3 {
        client
            .create(quote(
                &format!("deed-quote-other{n}"),
                &format!("excerpt {n}"),
                "https://example.invalid/a",
            ))
            .unwrap();
    }
    let a = store(&first).log_head().unwrap();
    let b = store(&second).log_head().unwrap();
    assert_eq!(a.size, b.size);
    assert_ne!(a.root, b.root);
}

/// A store from before the log has deeds and no entries, and walking only the
/// log calls that clean because there is nothing to walk.
///
/// Found on a real store: nine deeds, an empty log, and `ok 0 logged, 0
/// served` with a zero exit. The emptier the log the better the verdict, which
/// is the wrong way round.
#[test]
fn a_store_that_predates_the_log_is_not_clean_and_not_tampered() {
    let url = tmp_url();
    let ids = three(&url);

    // Drop the log the way a store written before it would never have had one.
    let dir = deedar::store_dir(&url).unwrap();
    std::fs::remove_file(dir.join("log")).expect("remove the log");
    let store = store(&url);

    let audit = store.log_audit().unwrap();
    assert!(!audit.is_clean(), "an unlogged store passed as clean");
    assert_eq!(audit.logged, 0);
    assert_eq!(audit.unlogged.len(), 3, "{audit:?}");
    assert!(audit.missing.is_empty(), "nothing was lost, only unlogged");
    // The state has its own name, because the advice differs.
    assert!(audit.predates_the_log(), "{audit:?}");
    assert!(audit.backfill_settles_it(), "{audit:?}");

    // Backfilling is the way forward, and afterwards the store answers for
    // itself in both directions.
    let added = store.log_backfill().unwrap();
    assert_eq!(added.len(), 3, "{added:?}");
    let after = store.log_audit().unwrap();
    assert!(after.is_clean(), "{after:?}");
    assert_eq!(after.logged, 3);
    assert!(!after.predates_the_log());

    // And every backfilled deed proves into the head that now covers it.
    let head = store.log_head().unwrap();
    let entries = store.log_entries().unwrap();
    for id in &ids {
        let (index, path) = store.log_proof(id).unwrap();
        let leaf = log::leaf_hash(&entries[index].material());
        assert!(
            log::verify_inclusion(&leaf, index, head.size, &path, &head.root),
            "{id} did not prove in after backfill"
        );
    }

    // Backfilling twice adds nothing: the second run has nothing unlogged.
    assert!(store.log_backfill().unwrap().is_empty());
}

/// A store holding deeds from both sides of the log is behind, not tampered
/// with: the question is whether anything was lost, not whether the log is
/// empty.
#[test]
fn a_store_that_fell_behind_still_gets_told_to_backfill() {
    let url = tmp_url();
    three(&url);

    // Drop the log, then log one more deed the way a rebuilt binary would.
    let dir = deedar::store_dir(&url).unwrap();
    std::fs::remove_file(dir.join("log")).expect("remove the log");
    let mut client = Client::open(&url).unwrap();
    client
        .create(quote(
            "deed-quote-after",
            "written once the log existed",
            "https://example.invalid/a",
        ))
        .unwrap();

    let store = store(&url);
    let audit = store.log_audit().unwrap();
    assert_eq!(audit.logged, 1, "{audit:?}");
    assert_eq!(audit.unlogged.len(), 3, "{audit:?}");
    assert!(audit.missing.is_empty(), "nothing was lost");
    // Not a store older than the log, because the log holds something.
    assert!(!audit.predates_the_log(), "{audit:?}");
    // But backfilling is still the whole answer, which is what the operator
    // needs to be told.
    assert!(audit.backfill_settles_it(), "{audit:?}");

    store.log_backfill().unwrap();
    assert!(store.log_audit().unwrap().is_clean());
}
