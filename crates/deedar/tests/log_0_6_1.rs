//! A store, a satchel and the log records the deedar 0.6.1 binary wrote,
//! read by this build. The log, the receipts, the bridge and the signed heads
//! are files other people keep, so this build has to give the same bytes for
//! the same store and accept every proof 0.6.1 handed out.
//! `fixtures/deedar-0.6.1/make.sh` says how the fixture was written.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use deed::DeedId;
use deedar::{check_handover, check_since, kept_head, log, Bridge, Client, Receipt, SignedHead};

use common::{open, scratch};

const IDS: [&str; 5] = [
    "deed-quote-compat0",
    "deed-quote-compat1",
    "deed-quote-compat2",
    "deed-quote-compat3",
    "deed-quote-compat4",
];

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/deedar-0.6.1")
}

fn read(name: &str) -> String {
    fs::read_to_string(fixture().join(name)).unwrap()
}

fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to);
        } else {
            fs::copy(&from, &to).unwrap();
        }
    }
}

/// The store on a scratch copy, so a read that writes leaves the fixture alone.
fn store() -> Client {
    let dest = scratch("deedar-0-6-1");
    copy_tree(&fixture().join("store"), &dest);
    open(&format!("file://{}", dest.display()))
}

/// The one signer the fixture's layout lists.
fn signer() -> BTreeSet<[u8; 32]> {
    let layout = read("store/layout");
    let hex = layout
        .lines()
        .find_map(|line| line.strip_prefix("signer = ed25519:"))
        .expect("the layout lists a signer");
    BTreeSet::from([log::from_hex(hex).expect("a 32 byte key")])
}

#[test]
fn the_log_lists_the_entries_0_6_1_wrote() {
    let client = store();
    let mut listed = String::new();
    for (at, entry) in client.log_entries().unwrap().iter().enumerate() {
        listed.push_str(&format!(
            "{at}\t{}\t{}\t{}\n",
            entry.id, entry.digest, entry.unix_time
        ));
    }
    assert_eq!(listed, read("list.txt"));
    assert!(client.log_audit().unwrap().is_clean());
}

#[test]
fn the_head_is_the_one_0_6_1_signed() {
    let client = store();
    let signed = SignedHead::parse(&read("head.txt")).unwrap();
    assert_eq!(client.log_head().unwrap(), signed.head);
    assert!(signed.links.is_none());
    assert!(signed.check(&signer()).unwrap());
}

#[test]
fn a_receipt_is_the_one_0_6_1_proved() {
    let client = store();
    let id = DeedId::parse("deed-quote-compat1").unwrap();
    let ours = client.receipt(&id).unwrap();
    assert_eq!(ours.render(), read("receipt-compat1.txt"));
    assert_eq!(Receipt::parse(&read("receipt-compat1.txt")).unwrap(), ours);
}

#[test]
fn the_bridge_is_the_one_0_6_1_built() {
    let client = store();
    let ours = client.bridge(3).unwrap();
    assert_eq!(ours.render(), read("bridge.txt"));
    let theirs = Bridge::parse(&read("bridge.txt")).unwrap();
    theirs.check().unwrap();
    assert_eq!(theirs, ours);
}

#[test]
fn the_0_6_1_satchel_checks_since_the_kept_head() {
    let done = check_handover(&fixture().join("bag"), &signer()).unwrap();
    assert_eq!(done.proven, IDS);
    assert!(done.head_accepted);
    let now = done.head.expect("a head");
    assert_eq!(now, store().log_head().unwrap());

    let kept = kept_head(&read("kept.txt")).unwrap();
    assert_eq!(kept.size, 3);
    let bridge = Bridge::parse(&read("bridge.txt")).unwrap();
    check_since(&kept, Some(&bridge), &now).unwrap();
}

#[test]
fn every_0_6_1_deed_still_has_evidence() {
    let mut client = store();
    for raw in IDS {
        client.evidence(&DeedId::parse(raw).unwrap()).unwrap();
    }
}
