//! Open a pre-layout store copy; get still works and layout is written.

mod common;

use std::fs;
use std::path::Path;

use deed::DeedId;

use common::{open, quote, tmp_url};

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
fn open_writes_layout_and_gets_a_pre_layout_deed() {
    let url = tmp_url();
    let id = DeedId::parse("deed-quote-heatpump").unwrap();
    let mut client = open(&url);
    client
        .create(quote(
            "deed-quote-heatpump",
            "heat",
            "https://example.com/heat",
        ))
        .unwrap();

    let src = deeder::store_dir(&url).unwrap();
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dest = std::env::temp_dir().join(format!("deeder-pre-layout-{n}"));
    copy_tree(&src, &dest);
    let layout = dest.join("layout");
    if layout.exists() {
        fs::remove_file(&layout).unwrap();
    }
    assert!(!layout.exists());

    let copy_url = format!("file://{}", dest.display());
    let mut opened = deeder::Client::open(&copy_url).expect("open copy");
    let got = opened.get(&id).expect("get pre-layout id");
    assert_eq!(got.id.as_str(), "deed-quote-heatpump");
    assert_eq!(fs::read_to_string(&layout).expect("layout"), "1\n");
}
