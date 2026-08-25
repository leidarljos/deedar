//! Old store without layout still opens for get, trail, evidence, and list.

mod common;

use std::fs;
use std::path::Path;

use deed::DeedId;

use common::{file, open, tmp_url, write_blob};

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

#[test]
fn copy_without_layout_still_gets_trail_evidence_list() {
    let url = tmp_url();
    let id = DeedId::parse("deed-file-compat").unwrap();
    let root = deedar::store_dir(&url).unwrap();
    let sitting = write_blob(&root, "compat.txt", b"old store sitting\n");
    let mut client = open(&url);
    client.create(file("deed-file-compat", sitting)).unwrap();

    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dest = std::env::temp_dir().join(format!("deedar-compat-{n}"));
    copy_tree(&root, &dest);
    let layout = dest.join("layout");
    if layout.exists() {
        fs::remove_file(&layout).unwrap();
    }
    assert!(!layout.exists());

    let copy_url = format!("file://{}", dest.display());
    let mut opened = deedar::Client::open(&copy_url).expect("open copy");
    let got = opened.get(&id).expect("get");
    assert_eq!(got.id, id);
    let trail = opened.trail(&id).expect("trail");
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].id, id);
    opened.evidence(&id).expect("evidence");
    let listed = opened.list().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
}
