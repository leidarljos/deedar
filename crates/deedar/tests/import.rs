//! Importing a handover into another store, then asking that store about what
//! it took: evidence, trail, current and the log audit. Every refusal is held
//! to having written nothing, file for file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use deed::{Body, DeedId, Source};
use deedar::{Client, CreateRequest, Receipt, Response, UnsignedHead};

static ENV: Mutex<()> = Mutex::new(());

/// A sender, a receiver, and a bag between them, under one scratch directory,
/// with the environment every store here reads held for the whole sitting.
struct Seats {
    root: tempfile::TempDir,
    _guard: MutexGuard<'static, ()>,
}

impl Seats {
    fn new() -> Self {
        let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().unwrap();
        // SAFETY: ENV is held for the whole sitting that reads these.
        unsafe {
            std::env::remove_var("DEEDAR_HOST_KEY");
            std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
            std::env::set_var("DEEDAR_HOST_SIGNING_KEY", "off");
        }
        Self {
            root,
            _guard: guard,
        }
    }

    fn url(&self, name: &str) -> String {
        format!("file://{}", self.root.path().join(name).display())
    }

    fn bag(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    /// Sign with the key seeded by `fill` from here on; the public half as a
    /// layout names it.
    fn sign_as(&self, fill: u8) -> String {
        let path = self.root.path().join(format!("signing-{fill}"));
        fs::write(&path, [fill; 32]).unwrap();
        // SAFETY: ENV is held for the whole sitting.
        unsafe { std::env::set_var("DEEDAR_HOST_SIGNING_KEY", &path) };
        let public = ed25519_dalek::SigningKey::from_bytes(&[fill; 32])
            .verifying_key()
            .to_bytes();
        format!(
            "ed25519:{}",
            public
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    }

    fn sign_nothing(&self) {
        // SAFETY: ENV is held for the whole sitting.
        unsafe { std::env::set_var("DEEDAR_HOST_SIGNING_KEY", "off") };
    }
}

fn id(raw: &str) -> DeedId {
    DeedId::parse(raw).unwrap()
}

fn quote(raw: &str, excerpt: &str) -> CreateRequest {
    CreateRequest {
        id: Some(id(raw)),
        name: raw.into(),
        sources: Vec::new(),
        agent_id: "reader".into(),
        activity_id: None,
        grants: Vec::new(),
        body: Body::Quote {
            edition: "https://example.invalid/edition".into(),
            start: 0,
            end: excerpt.len() as u64,
            excerpt: excerpt.into(),
            urls: vec!["https://example.invalid/edition".into()],
        },
        supersedes: None,
    }
}

fn mint(url: &str, req: CreateRequest) {
    Client::open(url).unwrap().create(req).unwrap();
}

fn export(url: &str, bag: &Path, ids: &[&str]) {
    let client = Client::open(url).unwrap();
    let deeds = bag.join("data").join("deeds");
    for raw in ids {
        client.export_into(&id(raw), &deeds).unwrap();
    }
}

fn import(url: &str, bag: &Path, unsigned: UnsignedHead) -> deedar::Imported {
    Client::open(url)
        .unwrap()
        .import_handover(bag, unsigned)
        .unwrap_or_else(|e| panic!("import: {e}"))
}

fn layout(url: &str, text: &str) {
    drop(Client::open(url).unwrap());
    fs::write(deedar::store_dir(url).unwrap().join("layout"), text).unwrap();
}

/// Every file under a store, with its bytes.
fn snapshot(url: &str) -> BTreeMap<PathBuf, Vec<u8>> {
    let dir = deedar::store_dir(url).unwrap();
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.clone()];
    while let Some(at) = stack.pop() {
        for entry in fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(
                    path.strip_prefix(&dir).unwrap().to_path_buf(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    out
}

/// The import is refused, and the receiver is file for file what it was.
fn refused(url: &str, bag: &Path, unsigned: UnsignedHead) -> String {
    let mut client = Client::open(url).unwrap();
    let before = snapshot(url);
    let err = client
        .import_handover(bag, unsigned)
        .expect_err("the import passed");
    assert_eq!(snapshot(url), before, "a refused import wrote: {err}");
    err.to_string()
}

/// What a receiver asks of a deed it took: evidence, trail and current, and
/// a log that answers for the shelves.
fn holds(url: &str, raw: &str) -> deedar::Evidence {
    let mut client = Client::open(url).unwrap();
    let ev = client
        .evidence(&id(raw))
        .unwrap_or_else(|e| panic!("evidence {raw}: {e}"));
    client
        .trail(&id(raw))
        .unwrap_or_else(|e| panic!("trail {raw}: {e}"));
    client
        .current(&id(raw))
        .unwrap_or_else(|e| panic!("current {raw}: {e}"));
    let audit = client.log_audit().unwrap();
    assert!(audit.is_clean(), "{audit:?}");
    ev
}

/// The handover the command is for: another store, its own writer key, and a
/// head signed by a key the receiver lists. Evidence there is issued under
/// the receiver's key at the receipt's time, the host signature still covers
/// it, and the receipt and signed head stay beside the deed.
#[test]
fn a_signed_bag_is_evidenced_by_the_store_that_took_it() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    let public = seats.sign_as(41);
    mint(&sender, quote("deed-quote-shared", "shared excerpt"));
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-shared"]);
    let carried = bag.join("data/deeds/deed-quote-shared");
    let receipt = Receipt::parse(&fs::read_to_string(carried.join("proof.txt")).unwrap()).unwrap();

    seats.sign_nothing();
    let receiver = seats.url("receiver");
    layout(&receiver, &format!("1\nsigner = {public}\n"));
    let done = import(&receiver, &bag, UnsignedHead::Refuse);
    assert_eq!((done.deeds, done.logged), (1, 1));
    assert!(done.render().contains("(accepted)"), "{}", done.render());
    assert!(
        done.render()
            .contains("imported 1 deeds, appended 1 log entries"),
        "{}",
        done.render()
    );

    let ev = holds(&receiver, "deed-quote-shared");
    assert_eq!(ev.unix_time, receipt.unix_time);
    let sent = Client::open(&sender)
        .unwrap()
        .evidence(&id("deed-quote-shared"))
        .unwrap();
    assert_ne!(ev.signature, sent.signature, "the sender's keyed hash");
    let mut client = Client::open(&receiver).unwrap();
    assert_eq!(
        client.get(&id("deed-quote-shared")).unwrap(),
        Client::open(&sender)
            .unwrap()
            .get(&id("deed-quote-shared"))
            .unwrap()
    );
    let logged = client.log_entries().unwrap();
    assert_eq!(logged.len(), 1);
    assert_eq!(
        (logged[0].digest.as_str(), logged[0].unix_time),
        (receipt.digest.as_str(), receipt.unix_time)
    );
    let shelf = deedar::store_dir(&receiver).unwrap().join("deeds");
    for (kept, carried) in [
        ("deed-quote-shared.proof", carried.join("proof.txt")),
        ("deed-quote-shared.head", bag.join("data/deeds/head")),
        ("deed-quote-shared.host", carried.join("host")),
    ] {
        assert_eq!(
            fs::read(shelf.join(kept)).unwrap(),
            fs::read(&carried).unwrap(),
            "{kept}"
        );
    }

    let before = snapshot(&receiver);
    let again = import(&receiver, &bag, UnsignedHead::Refuse);
    assert_eq!((again.deeds, again.logged), (0, 0));
    assert_eq!(snapshot(&receiver), before, "the second import wrote");

    // Back into the store it came from: nothing is appended, and its own
    // evidence still holds.
    let back = import(&sender, &bag, UnsignedHead::Refuse);
    assert_eq!((back.deeds, back.logged), (0, 0));
    holds(&sender, "deed-quote-shared");
}

/// A deed already kept under an earlier head must not block a later bag, and
/// a missing head is not filled from a head the kept proof is not against.
#[test]
fn a_later_bag_imports_beside_a_deed_kept_under_an_earlier_head() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    let public = seats.sign_as(41);
    mint(&sender, quote("deed-quote-a", "first"));
    let first = seats.bag("first");
    export(&sender, &first, &["deed-quote-a"]);

    seats.sign_nothing();
    let receiver = seats.url("receiver");
    layout(&receiver, &format!("1\nsigner = {public}\n"));
    import(&receiver, &first, UnsignedHead::Refuse);
    let shelf = deedar::store_dir(&receiver).unwrap().join("deeds");
    let kept_head = fs::read(shelf.join("deed-quote-a.head")).unwrap();
    let kept_proof = fs::read(shelf.join("deed-quote-a.proof")).unwrap();

    seats.sign_as(41);
    mint(&sender, quote("deed-quote-b", "second"));
    let both = seats.bag("both");
    export(&sender, &both, &["deed-quote-a", "deed-quote-b"]);

    seats.sign_nothing();
    let done = import(&receiver, &both, UnsignedHead::Refuse);
    assert_eq!((done.deeds, done.logged), (1, 1));
    holds(&receiver, "deed-quote-a");
    holds(&receiver, "deed-quote-b");
    assert_eq!(
        fs::read(shelf.join("deed-quote-a.proof")).unwrap(),
        kept_proof,
        "a grown log replaced the proof the store kept"
    );
    assert_eq!(
        fs::read(shelf.join("deed-quote-a.head")).unwrap(),
        kept_head,
        "a grown log replaced the head the kept proof is against"
    );

    fs::remove_file(shelf.join("deed-quote-a.head")).unwrap();
    import(&receiver, &both, UnsignedHead::Refuse);
    assert!(
        !shelf.join("deed-quote-a.head").exists(),
        "a later bag repaired a head the kept proof is not against"
    );
}

fn flip_last_byte(path: &Path) {
    let mut raw = fs::read(path).unwrap();
    let at = raw
        .iter()
        .rposition(|byte| *byte != b'\n')
        .expect("file has a byte");
    raw[at] = if raw[at] == b'a' { b'b' } else { b'a' };
    fs::write(path, raw).unwrap();
}

/// `sources`, `supersedes` and `successor` sit outside the receipt. A signed
/// bag that changes one of them must fail `check`: the head signature is
/// what covers them, and the receipt does not.
#[test]
fn a_forged_link_sidecar_fails_the_head_signature() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    seats.sign_as(9);
    mint(&sender, quote("deed-quote-v1", "first take"));
    let mut v2 = quote("deed-quote-v2", "second take");
    v2.supersedes = Some(id("deed-quote-v1"));
    mint(&sender, v2);
    let mut made = quote("deed-quote-made", "made from the first");
    made.sources = vec![Source::deed(id("deed-quote-v1"))];
    mint(&sender, made);
    let accept =
        std::collections::BTreeSet::from([ed25519_dalek::SigningKey::from_bytes(&[9u8; 32])
            .verifying_key()
            .to_bytes()]);

    let cases = [
        (
            "supersedes",
            &["deed-quote-v2"][..],
            "deed-quote-v2/supersedes",
        ),
        (
            "successor",
            &["deed-quote-v1"][..],
            "deed-quote-v1/successor",
        ),
        (
            "sources",
            &["deed-quote-made"][..],
            "deed-quote-made/sources",
        ),
    ];
    for (name, ids, file) in cases {
        let bag = seats.bag(name);
        export(&sender, &bag, ids);
        deedar::check_handover(&bag, &accept).unwrap_or_else(|e| panic!("{name} before: {e}"));
        flip_last_byte(&bag.join("data/deeds").join(file));
        let err = deedar::check_handover(&bag, &accept).expect_err(name);
        assert!(err.to_string().contains("does not cover"), "{name}: {err}");
        let err = refused(
            &seats.url(&format!("recv-{name}")),
            &bag,
            UnsignedHead::Allow,
        );
        assert!(err.contains("does not cover"), "{name}: {err}");
    }
}

/// A kept receipt with no kept head is a partial write. Re-importing the same
/// bag must put the head back.
#[test]
fn a_kept_proof_with_no_head_is_repaired_on_reimport() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    let public = seats.sign_as(41);
    mint(&sender, quote("deed-quote-shared", "shared excerpt"));
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-shared"]);

    seats.sign_nothing();
    let receiver = seats.url("receiver");
    layout(&receiver, &format!("1\nsigner = {public}\n"));
    let shelf = deedar::store_dir(&receiver).unwrap().join("deeds");
    fs::copy(
        bag.join("data/deeds/deed-quote-shared/proof.txt"),
        shelf.join("deed-quote-shared.proof"),
    )
    .unwrap();

    import(&receiver, &bag, UnsignedHead::Refuse);
    let kept = shelf.join("deed-quote-shared.head");
    assert!(kept.exists(), "first import did not restore the kept head");
    fs::remove_file(&kept).unwrap();
    import(&receiver, &bag, UnsignedHead::Refuse);
    assert!(kept.exists(), "second import did not restore the kept head");
}

/// A file deed lands with its product bytes; a bag without them is refused.
#[test]
fn a_file_deed_lands_with_its_bytes() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    let letter = seats.root.path().join("letter.txt");
    fs::write(&letter, b"the letter itself").unwrap();
    let (deed, _) = Client::open(&sender)
        .unwrap()
        .create(CreateRequest {
            id: Some(id("deed-file-letter")),
            name: "letter".into(),
            sources: Vec::new(),
            agent_id: "reader".into(),
            activity_id: None,
            grants: Vec::new(),
            body: Body::File {
                path: letter,
                media_type: Some("text/plain".into()),
            },
            supersedes: None,
        })
        .unwrap();
    let hex = deed.paths[0]
        .to_str()
        .unwrap()
        .strip_prefix("sha256:")
        .unwrap()
        .to_string();

    let bare = seats.bag("bare");
    export(&sender, &bare, &["deed-file-letter"]);
    fs::remove_dir_all(bare.join("data/deeds/deed-file-letter/bytes")).unwrap();
    let receiver = seats.url("receiver");
    let err = refused(&receiver, &bare, UnsignedHead::Allow);
    assert!(err.contains("has bytes in neither"), "{err}");

    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-file-letter"]);
    import(&receiver, &bag, UnsignedHead::Allow);
    holds(&receiver, "deed-file-letter");
    assert_eq!(
        fs::read(
            deedar::store_dir(&receiver)
                .unwrap()
                .join("bytes")
                .join(hex)
        )
        .unwrap(),
        b"the letter itself"
    );
}

/// The deeds a quote was made from and the revision that replaced it land
/// with it: trail walks to the source, and current steps to the revision.
#[test]
fn sources_and_revisions_land_with_the_deed() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-src", "the source"));
    let mut made = quote("deed-quote-made", "made from the source");
    made.sources = vec![Source::deed(id("deed-quote-src"))];
    mint(&sender, made);
    mint(&sender, quote("deed-quote-v1", "first take"));
    let mut v2 = quote("deed-quote-v2", "second take");
    v2.supersedes = Some(id("deed-quote-v1"));
    mint(&sender, v2);

    let bag = seats.bag("bag");
    export(
        &sender,
        &bag,
        &[
            "deed-quote-src",
            "deed-quote-made",
            "deed-quote-v1",
            "deed-quote-v2",
        ],
    );
    let receiver = seats.url("receiver");
    let done = import(&receiver, &bag, UnsignedHead::Allow);
    assert_eq!((done.deeds, done.logged), (4, 4));
    for raw in [
        "deed-quote-src",
        "deed-quote-made",
        "deed-quote-v1",
        "deed-quote-v2",
    ] {
        holds(&receiver, raw);
    }
    let mut client = Client::open(&receiver).unwrap();
    let trail: Vec<String> = client
        .trail(&id("deed-quote-made"))
        .unwrap()
        .iter()
        .map(|d| d.id.to_string())
        .collect();
    assert_eq!(trail, ["deed-quote-made", "deed-quote-src"]);
    assert_eq!(
        client.current(&id("deed-quote-v1")).unwrap().id,
        id("deed-quote-v2")
    );
}

/// A source the bag leaves out, which this store does not hold either, is
/// refused; once the store holds it, the same bag imports.
#[test]
fn a_source_left_out_of_the_bag_is_refused() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-src", "the source"));
    let mut made = quote("deed-quote-made", "made from the source");
    made.sources = vec![Source::deed(id("deed-quote-src"))];
    mint(&sender, made);
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-made"]);

    let receiver = seats.url("receiver");
    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("made from deed-quote-src"), "{err}");

    let first = seats.bag("first");
    export(&sender, &first, &["deed-quote-src"]);
    import(&receiver, &first, UnsignedHead::Allow);
    import(&receiver, &bag, UnsignedHead::Allow);
    holds(&receiver, "deed-quote-made");
}

/// A source omitted from the bag still has to match the sender's bytes. The
/// source digest sidecar names those bytes.
#[test]
fn a_local_source_with_other_bytes_is_refused() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-src", "source from sender"));
    let mut made = quote("deed-quote-made", "made from the source");
    made.sources = vec![Source::deed(id("deed-quote-src"))];
    mint(&sender, made);
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-made"]);

    let receiver = seats.url("receiver");
    mint(&receiver, quote("deed-quote-src", "other local bytes"));
    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("made from deed-quote-src"), "{err}");
    assert!(err.contains("serves it as"), "{err}");
}

/// A deed superseded where it came from, handed over without its successor,
/// would stop `current` short here, so it is refused. The successor alone
/// imports, and then the deed it supersedes does, and current steps on.
#[test]
fn a_successor_left_out_of_the_bag_is_refused() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-v1", "first take"));
    let mut v2 = quote("deed-quote-v2", "second take");
    v2.supersedes = Some(id("deed-quote-v1"));
    mint(&sender, v2);
    let receiver = seats.url("receiver");

    let alone = seats.bag("alone");
    export(&sender, &alone, &["deed-quote-v1"]);
    let err = refused(&receiver, &alone, UnsignedHead::Allow);
    assert!(
        err.contains("current here would stop at deed-quote-v1"),
        "{err}"
    );

    let later = seats.bag("later");
    export(&sender, &later, &["deed-quote-v2"]);
    import(&receiver, &later, UnsignedHead::Allow);
    import(&receiver, &alone, UnsignedHead::Allow);
    holds(&receiver, "deed-quote-v1");
    assert_eq!(
        Client::open(&receiver)
            .unwrap()
            .current(&id("deed-quote-v1"))
            .unwrap()
            .id,
        id("deed-quote-v2")
    );
}

/// One sidecar, naming a deed the bag also carries, has to name that deed's
/// receipt digest. The two sidecars are compared only when both are present.
#[test]
fn a_one_sided_link_to_a_carried_deed_must_name_its_digest() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-v1", "first take"));
    let mut v2 = quote("deed-quote-v2", "second take");
    v2.supersedes = Some(id("deed-quote-v1"));
    mint(&sender, v2);

    let successor_only = seats.bag("successor-only");
    export(
        &sender,
        &successor_only,
        &["deed-quote-v1", "deed-quote-v2"],
    );
    fs::remove_file(successor_only.join("data/deeds/deed-quote-v2/supersedes")).unwrap();
    flip_last_byte(&successor_only.join("data/deeds/deed-quote-v1/successor"));
    let err = refused(
        &seats.url("recv-successor"),
        &successor_only,
        UnsignedHead::Allow,
    );
    assert!(err.contains("carries it as"), "{err}");

    let supersedes_only = seats.bag("supersedes-only");
    export(
        &sender,
        &supersedes_only,
        &["deed-quote-v1", "deed-quote-v2"],
    );
    fs::remove_file(supersedes_only.join("data/deeds/deed-quote-v1/successor")).unwrap();
    flip_last_byte(&supersedes_only.join("data/deeds/deed-quote-v2/supersedes"));
    let err = refused(
        &seats.url("recv-supersedes"),
        &supersedes_only,
        UnsignedHead::Allow,
    );
    assert!(err.contains("carries it as"), "{err}");
}

/// A sidecar link to a local successor holds only when that successor's bytes
/// match what the sender named.
#[test]
fn a_local_successor_with_other_bytes_is_refused() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-v1", "first take"));
    let mut v2 = quote("deed-quote-v2", "second take");
    v2.supersedes = Some(id("deed-quote-v1"));
    mint(&sender, v2);
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-v1"]);

    let receiver = seats.url("receiver");
    mint(&receiver, quote("deed-quote-v2", "receiver bytes"));
    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("deed-quote-v2"), "{err}");
    assert!(err.contains("serves deed-quote-v2 as"), "{err}");
}

/// A sidecar link from a local prior deed holds only when that prior deed's
/// bytes match what the sender named.
#[test]
fn a_local_superseded_deed_with_other_bytes_is_refused() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-v1", "first take"));
    let mut v2 = quote("deed-quote-v2", "second take");
    v2.supersedes = Some(id("deed-quote-v1"));
    mint(&sender, v2);
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-v2"]);

    let receiver = seats.url("receiver");
    mint(&receiver, quote("deed-quote-v1", "receiver bytes"));
    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("deed-quote-v1"), "{err}");
    assert!(err.contains("holds deed-quote-v1 as"), "{err}");
}

/// A tombstoned local deed still cannot gain a successor sidecar.
#[test]
fn a_tombstoned_deed_does_not_gain_a_successor_on_import() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-v1", "first take"));
    let mut v2 = quote("deed-quote-v2", "second take");
    v2.supersedes = Some(id("deed-quote-v1"));
    mint(&sender, v2);
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-v2"]);

    let receiver = seats.url("receiver");
    mint(&receiver, quote("deed-quote-v1", "first take"));
    Client::open(&receiver)
        .unwrap()
        .delete(&id("deed-quote-v1"))
        .unwrap();

    import(&receiver, &bag, UnsignedHead::Allow);
    let mut client = Client::open(&receiver).unwrap();
    client.evidence(&id("deed-quote-v2")).unwrap();
    assert_eq!(
        client.current(&id("deed-quote-v2")).unwrap().id,
        id("deed-quote-v2")
    );
    assert!(!deedar::store_dir(&receiver)
        .unwrap()
        .join("deeds")
        .join("deed-quote-v1.successor")
        .exists());
}

/// One deed the receiver cannot take stops the whole bag before anything is
/// written, including the deed ahead of it in the sender's log.
#[test]
fn a_frozen_deed_leaves_the_rest_of_the_bag_unwritten() {
    let seats = Seats::new();
    let receiver = seats.url("receiver");
    mint(&receiver, quote("deed-quote-keep", "from receiver"));
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-extra", "ahead in the log"));
    mint(&sender, quote("deed-quote-keep", "from sender"));
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-extra", "deed-quote-keep"]);

    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("deed frozen: deed-quote-keep"), "{err}");
    let mut client = Client::open(&receiver).unwrap();
    assert!(client.get(&id("deed-quote-extra")).is_err());
    assert_eq!(client.log_entries().unwrap().len(), 1);
}

/// A log line naming the accession with other bytes is not overwritten by
/// the bag's deed, even with the deed itself gone from the shelf.
#[test]
fn a_logged_accession_with_other_bytes_is_refused() {
    let seats = Seats::new();
    let receiver = seats.url("receiver");
    mint(
        &receiver,
        quote("deed-quote-shared", "the receiver's excerpt"),
    );
    fs::remove_file(
        deedar::store_dir(&receiver)
            .unwrap()
            .join("deeds/deed-quote-shared.cap"),
    )
    .unwrap();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-shared", "different excerpt"));
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-shared"]);

    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("already logged here as"), "{err}");
    assert!(Client::open(&receiver)
        .unwrap()
        .get(&id("deed-quote-shared"))
        .is_err());
}

/// A tombstone here is a refusal, not a deed to put back.
#[test]
fn a_tombstoned_accession_is_refused() {
    let seats = Seats::new();
    let receiver = seats.url("receiver");
    mint(&receiver, quote("deed-quote-gone", "deleted here"));
    Client::open(&receiver)
        .unwrap()
        .delete(&id("deed-quote-gone"))
        .unwrap();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-gone", "deleted here"));
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-gone"]);

    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("deed tombstoned: deed-quote-gone"), "{err}");
}

/// An unsigned head is refused unless allowed, and a layout that demands
/// attestation refuses it even then.
#[test]
fn an_unsigned_head_is_refused_unless_allowed_and_always_under_required() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    mint(
        &sender,
        quote("deed-quote-plain", "nobody signed this head"),
    );
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-plain"]);
    assert!(!bag.join("data/deeds/head").exists());

    let receiver = seats.url("receiver");
    let err = refused(&receiver, &bag, UnsignedHead::Refuse);
    assert!(err.contains("head is unsigned"), "{err}");
    assert!(err.contains("--allow-unsigned-head"), "{err}");

    layout(&receiver, "1\nattestation = required\n");
    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("attestation = required"), "{err}");

    layout(&receiver, "1\n");
    let done = import(&receiver, &bag, UnsignedHead::Allow);
    assert_eq!(done.logged, 1);
    holds(&receiver, "deed-quote-plain");
}

/// A head signed by a key the receiver's layout leaves out is refused: by
/// the check when the layout lists others, and as unsigned when it lists
/// none. Allowed through, the deed's own host signature still has to verify
/// here, or evidence would fail after the import.
#[test]
fn a_head_signed_by_an_unlisted_key_is_refused() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    seats.sign_as(42);
    mint(&sender, quote("deed-quote-signed", "signed by a stranger"));
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-signed"]);

    seats.sign_nothing();
    let receiver = seats.url("receiver");
    let stranger = seats.sign_as(43);
    seats.sign_nothing();
    layout(&receiver, &format!("1\nsigner = {stranger}\n"));
    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("not a key this reader accepts"), "{err}");

    layout(&receiver, "1\n");
    let err = refused(&receiver, &bag, UnsignedHead::Refuse);
    assert!(err.contains("does not list"), "{err}");
    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("does not list"), "{err}");
}

/// The time an import dates evidence from is the receipt's, and an
/// `evidence.bin` that says otherwise is refused, though `check` never reads
/// that file.
#[test]
fn evidence_dated_other_than_the_receipt_is_refused() {
    let seats = Seats::new();
    let sender = seats.url("sender");
    mint(&sender, quote("deed-quote-dated", "when was this"));
    let bag = seats.bag("bag");
    export(&sender, &bag, &["deed-quote-dated"]);
    let path = bag.join("data/deeds/deed-quote-dated/evidence.bin");
    let Response::Created { deed, mut evidence } =
        deedar::decode_response(&fs::read(&path).unwrap()).unwrap()
    else {
        panic!("evidence.bin is a created frame");
    };
    evidence.unix_time += 1;
    fs::write(
        &path,
        deedar::encode_response(&Response::Created { deed, evidence }).unwrap(),
    )
    .unwrap();
    deedar::check_handover(&bag, &std::collections::BTreeSet::new()).unwrap();

    let receiver = seats.url("receiver");
    let err = refused(&receiver, &bag, UnsignedHead::Allow);
    assert!(err.contains("evidence.bin says time"), "{err}");
}
