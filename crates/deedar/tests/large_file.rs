//! An artefact larger than one read: what is stored, checked and handed over
//! is the whole file, under the sha256 of the whole file.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use deed::DeedId;
use sha2::{Digest, Sha256};

use common::{file, open, tmp_url, write_blob};

/// Larger than the store's read at any size up to 1 MiB and not a multiple
/// of it, so the bytes take several full reads and a short last one.
const LARGE: usize = 3 * 1024 * 1024 + 7;

/// Counting words, so a read dropped, repeated or out of order changes the
/// bytes and their hash.
fn artefact() -> Vec<u8> {
    (0u32..).flat_map(u32::to_le_bytes).take(LARGE).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The names in `dir`, sorted; none when it is not there.
fn names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn a_file_larger_than_one_read_is_stored_checked_and_handed_over_whole() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let bytes = artefact();
    let want = sha256_hex(&bytes);
    let src = write_blob(&root, "survey.tar", &bytes);
    let id = DeedId::parse("deed-file-survey").unwrap();
    let mut client = open(&url);

    let (deed, _) = client.create(file(id.as_str(), src.clone())).unwrap();
    assert_eq!(deed.paths, vec![PathBuf::from(format!("sha256:{want}"))]);
    assert_eq!(fs::read(root.join("bytes").join(&want)).unwrap(), bytes);
    client.evidence(&id).expect("evidence over the whole file");

    // A rerun answers with the same deed and leaves nothing else on the shelf.
    let (again, _) = client.create(file(id.as_str(), src)).unwrap();
    assert_eq!(again, deed);
    assert_eq!(names(&root.join("bytes")), vec![want.clone()]);

    let bag = tempfile::tempdir().unwrap();
    client.export_into(&id, bag.path()).unwrap();
    let carried = bag.path().join(id.as_str()).join("bytes");
    assert_eq!(names(&carried), vec![want.clone()]);
    assert_eq!(fs::read(carried.join(&want)).unwrap(), bytes);
}

#[test]
fn a_rerun_into_a_shelf_this_process_cannot_write_still_answers() {
    use std::os::unix::fs::PermissionsExt;
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let src = write_blob(&root, "survey.tar", &artefact());
    let id = DeedId::parse("deed-file-survey").unwrap();
    let mut client = open(&url);
    let (deed, _) = client.create(file(id.as_str(), src.clone())).unwrap();

    let shelf = root.join("bytes");
    fs::set_permissions(&shelf, fs::Permissions::from_mode(0o555)).unwrap();
    let again = client.create(file(id.as_str(), src));
    fs::set_permissions(&shelf, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(again.unwrap().0, deed);
}

#[test]
fn a_change_past_the_first_read_fails_evidence_and_export_and_leaves_no_copy() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let bytes = artefact();
    let want = sha256_hex(&bytes);
    let src = write_blob(&root, "survey.tar", &bytes);
    let id = DeedId::parse("deed-file-survey").unwrap();
    let mut client = open(&url);
    client.create(file(id.as_str(), src)).unwrap();

    let shelved = root.join("bytes").join(&want);
    let mut changed = bytes;
    *changed.last_mut().unwrap() ^= 1;
    fs::write(&shelved, &changed).unwrap();
    let err = client.evidence(&id).expect_err("last byte changed");
    assert!(err.to_string().contains("does not match"), "{err}");
    let bag = tempfile::tempdir().unwrap();
    let err = client
        .export_into(&id, bag.path())
        .expect_err("last byte changed");
    assert!(err.to_string().contains("does not match"), "{err}");
    let carried = bag.path().join(id.as_str()).join("bytes");
    assert_eq!(names(&carried), Vec::<String>::new());

    fs::remove_file(&shelved).unwrap();
    let err = client.evidence(&id).expect_err("bytes gone");
    assert!(err.to_string().contains("missing"), "{err}");
    let err = client.export_into(&id, bag.path()).expect_err("bytes gone");
    assert!(err.to_string().contains("no bytes"), "{err}");
    assert_eq!(names(&carried), Vec::<String>::new());
}
