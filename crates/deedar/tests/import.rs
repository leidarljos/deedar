//! Importing a checked handover into a receiving store.

mod common;

use deed::DeedId;
use deedar::{FsStore, Receipt};

use common::{open, quote, tmp_url};

fn store(url: &str) -> FsStore {
    FsStore::open(deedar::store_dir(url).unwrap()).unwrap()
}

fn mint(url: &str, id: &str, excerpt: &str) {
    open(url)
        .create(quote(id, excerpt, "https://example.invalid/handover"))
        .unwrap();
}

#[test]
fn import_checks_a_bag_and_replays_receipts_into_the_log() {
    let sender = tmp_url();
    mint(&sender, "deed-quote-first", "first export");
    mint(&sender, "deed-quote-second", "second export");
    mint(&sender, "deed-quote-third", "third export");
    let sent = store(&sender);

    let bag = tempfile::tempdir().unwrap();
    let deeds = bag.path().join("data").join("deeds");
    sent.export_into(&DeedId::parse("deed-quote-third").unwrap(), &deeds)
        .unwrap();
    sent.export_into(&DeedId::parse("deed-quote-first").unwrap(), &deeds)
        .unwrap();

    let receiver_url = tmp_url();
    let mut receiver = store(&receiver_url);
    let checked = receiver.import_handover(bag.path()).unwrap();
    assert_eq!(
        checked.proven,
        vec![
            "deed-quote-first".to_string(),
            "deed-quote-third".to_string()
        ]
    );

    let mut sender_view = open(&sender);
    let entries = receiver.log_entries().unwrap();
    assert_eq!(entries.len(), checked.proven.len());
    for id in &checked.proven {
        let proof = std::fs::read_to_string(deeds.join(id).join("proof.txt")).unwrap();
        let receipt = Receipt::parse(&proof).unwrap();
        let logged = entries.iter().find(|entry| entry.id == *id).unwrap();
        assert_eq!(logged.digest, receipt.digest);
        assert_eq!(logged.unix_time, receipt.unix_time);

        let id = DeedId::parse(id).unwrap();
        assert_eq!(
            receiver.get(&id).unwrap(),
            sender_view.get(&id).unwrap(),
            "{id} changed across import"
        );
    }
}

#[test]
fn importing_the_same_bag_twice_is_a_no_op_the_second_time() {
    let sender = tmp_url();
    mint(&sender, "deed-quote-once", "one export");
    let sent = store(&sender);

    let bag = tempfile::tempdir().unwrap();
    sent.export_into(
        &DeedId::parse("deed-quote-once").unwrap(),
        &bag.path().join("data").join("deeds"),
    )
    .unwrap();

    let receiver_url = tmp_url();
    let mut receiver = store(&receiver_url);
    receiver.import_handover(bag.path()).unwrap();
    let once = receiver.log_entries().unwrap();

    receiver.import_handover(bag.path()).unwrap();
    let twice = receiver.log_entries().unwrap();
    assert_eq!(twice, once, "the second import appended to the log");
}
