//! A whole store exported and imported keeps every deed, byte and log entry.
//!
//! Each seed builds a sender with deeds of every kind, supersede chains, deeds
//! made from other deeds, product bytes shared between deeds, and leave
//! records. A receiver mints deeds of its own and takes part of the sender in
//! an earlier handover. Then the sender revises deeds the receiver already
//! holds, and the receiver revises and cites deeds it took. The whole sender
//! goes to an empty store and to that receiver, and the receiver's union goes
//! on to a third store. Each store that took a bag is asked what a seat asks:
//! get, evidence, trail, current, leave and the log.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use deed::{Body, Deed, DeedId, FormField, Grant, Kind, MailMessageId, Measure, Source, Step};
use deedar::{Client, CreateRequest, LogEntry, LogHead, Receipt, UnsignedHead};

/// Seeds every `cargo test` runs: eight signed and eight not, about 1.8 s in
/// the dev profile on an 8-core Intel Xeon at 0bca3d7. Seeds 0..1000 pass as
/// well, at about 0.1 s a seed; sixteen keeps the suite quick.
const SEEDS: std::ops::Range<u64> = 0..16;

/// Deeds a seat mints in one sitting, at most, on top of the sender's first
/// one of every kind. As many as there are kinds, so over its two sittings a
/// sender holds up to three deeds a kind: room for one kind to be an
/// original, a revision of it, and the source of another deed.
const MORE: usize = 11;

/// Entries in a list field, at most: set members, urls, diffs, steps, and the
/// deeds one is made from. One is the least `validate` takes, and three
/// leaves room for an order and a repeat.
const ITEMS: usize = 3;

/// Bytes in a product file, at most: two SHA-256 blocks of 64 bytes, so the
/// empty file, a part block and a file over a block boundary all travel.
const BYTES: usize = 128;

/// What text is drawn from: ASCII, a space, a tab, a newline, and letters of
/// two, three and four bytes in UTF-8. Text travels length-prefixed, and a
/// byte count mistaken for a char count would cut one of these.
const LETTERS: &[char] = &['a', 'z', '0', ' ', '\t', '\n', 'þ', '日', '𝄞'];

/// Letters in a text field past the leading one, at most: room for every one
/// of [`LETTERS`] in one field.
const TEXT: usize = LETTERS.len();

static ENV: Mutex<()> = Mutex::new(());

/// SplitMix64 (Steele, Lea and Flood, OOPSLA 2014), with its published
/// constants. A seed is a whole sitting, so a failure names the seed that
/// builds the stores again.
struct Rng(u64);

impl Rng {
    fn draw(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Below `n`. The modulo bias is `n / 2^64`, which no store here shows.
    fn below(&mut self, n: usize) -> usize {
        (self.draw() % n as u64) as usize
    }

    fn coin(&mut self) -> bool {
        self.draw() & 1 == 1
    }

    fn pick<T: Clone>(&mut self, from: &[T]) -> Option<T> {
        (!from.is_empty()).then(|| from[self.below(from.len())].clone())
    }

    fn items(&mut self) -> usize {
        1 + self.below(ITEMS)
    }

    /// Never blank, since `validate` refuses a blank title or excerpt.
    fn text(&mut self) -> String {
        let mut out = String::from("t");
        for _ in 0..self.below(TEXT + 1) {
            out.push(LETTERS[self.below(LETTERS.len())]);
        }
        out
    }

    fn url(&mut self) -> String {
        format!("https://example.invalid/{}", self.text())
    }

    /// Positive and finite, so `validate` takes it, with whatever mantissa
    /// the generator says: the two top bits clear keep the sign positive and
    /// the exponent below the one for infinity.
    fn point(&mut self) -> f64 {
        f64::from_bits(self.draw() >> 2)
    }
}

/// The scratch directory every store of one seed lives under, with the
/// environment those stores read held for the whole seed.
struct Seats {
    root: tempfile::TempDir,
    /// The layout every store of a signed seed starts from.
    layout: Option<String>,
    signed: bool,
    _guard: MutexGuard<'static, ()>,
}

impl Seats {
    /// Even seeds sign every deed and head with one host key, and every store
    /// lists it and demands attestation, so a deed that lost its host
    /// signature on the way fails `evidence`. Odd seeds sign nothing and
    /// demand nothing. Both kinds of bag go through the one property.
    fn new(seed: u64) -> Self {
        let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let root = tempfile::tempdir().unwrap();
        let signed = seed % 2 == 0;
        let seed_bytes: [u8; 32] = seed.to_le_bytes().repeat(4).try_into().unwrap();
        let key = root.path().join("signing.key");
        fs::write(&key, seed_bytes).unwrap();
        let public = ed25519_dalek::SigningKey::from_bytes(&seed_bytes)
            .verifying_key()
            .to_bytes();
        let hex: String = public.iter().map(|b| format!("{b:02x}")).collect();
        let layout = signed.then(|| format!("1\nattestation = required\nsigner = ed25519:{hex}\n"));
        // SAFETY: ENV is held for the whole seed that reads these.
        unsafe {
            std::env::remove_var("DEEDAR_HOST_KEY");
            std::env::remove_var("DEEDAR_TSA");
            if signed {
                std::env::set_var("DEEDAR_HOST_SIGNING_KEY", &key);
            } else {
                std::env::set_var("DEEDAR_HOST_SIGNING_KEY", "off");
            }
        }
        Self {
            root,
            layout,
            signed,
            _guard: guard,
        }
    }

    /// A signed head is one every store here lists, so only an unsigned bag
    /// needs the waiver.
    fn import(&self, url: &str, bag: &Path) -> deedar::Imported {
        let unsigned = if self.signed {
            UnsignedHead::Refuse
        } else {
            UnsignedHead::Allow
        };
        Client::open(url)
            .unwrap()
            .import_handover(bag, unsigned)
            .unwrap_or_else(|e| panic!("import into {url}: {e}"))
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    /// A store here, laid out for this seed before anything opens it.
    fn url(&self, name: &str) -> String {
        let dir = self.dir(name);
        if let Some(layout) = self.layout.as_ref().filter(|_| !dir.exists()) {
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("layout"), layout).unwrap();
        }
        format!("file://{}", dir.display())
    }
}

/// One store being built, and what the builder knows of it: the deeds it
/// holds, what each was made from, and which one superseded which.
struct Seat {
    url: String,
    client: Client,
    /// In every accession this seat mints, so two seats never mint the same.
    tag: &'static str,
    work: PathBuf,
    minted: usize,
    products: usize,
    held: Vec<DeedId>,
    /// Deeds this seat may revise without forking a chain another seat owns.
    tips: Vec<DeedId>,
    made_from: BTreeMap<String, Vec<DeedId>>,
    successor: BTreeMap<String, DeedId>,
    addrs: Vec<PathBuf>,
    mail: Vec<MailMessageId>,
}

impl Seat {
    fn open(seats: &Seats, name: &str, tag: &'static str) -> Self {
        let url = seats.url(name);
        let work = seats.dir(&format!("{name}-work"));
        fs::create_dir_all(&work).unwrap();
        Self {
            client: Client::open(&url).unwrap(),
            url,
            tag,
            work,
            minted: 0,
            products: 0,
            held: Vec::new(),
            tips: Vec::new(),
            made_from: BTreeMap::new(),
            successor: BTreeMap::new(),
            addrs: Vec::new(),
            mail: Vec::new(),
        }
    }

    fn mint_some(&mut self, rng: &mut Rng) {
        for _ in 0..rng.below(MORE + 1) {
            let kind = rng.pick(Kind::all()).unwrap();
            self.mint(&kind, rng);
        }
    }

    /// Each choice is a fair coin, so plain deeds, deeds made from others,
    /// revisions, and revisions made from others come up alike.
    fn mint(&mut self, kind: &Kind, rng: &mut Rng) {
        let id = DeedId::parse(&format!(
            "deed-{}-{}-{}",
            kind.token(),
            self.tag,
            self.minted
        ))
        .unwrap();
        self.minted += 1;
        let mut made_from = Vec::new();
        if rng.coin() {
            for _ in 0..rng.items() {
                made_from.extend(rng.pick(&self.held));
            }
        }
        let mut sources: Vec<Source> = made_from.iter().cloned().map(Source::deed).collect();
        if rng.coin() {
            sources.push(Source::url(&rng.url()).unwrap());
        }
        if rng.coin() {
            sources.push(Source::path(rng.text()));
        }
        let supersedes = if rng.coin() && !self.tips.is_empty() {
            let at = rng.below(self.tips.len());
            Some(self.tips.swap_remove(at))
        } else {
            None
        };
        let mut grants = Vec::new();
        for _ in 0..rng.below(ITEMS + 1) {
            grants.push(if rng.coin() {
                Grant::Host(rng.text())
            } else {
                Grant::Path(rng.text().into())
            });
        }
        let body = self.body(kind, rng);
        let (deed, _) = self
            .client
            .create(CreateRequest {
                id: Some(id.clone()),
                name: rng.text(),
                sources,
                agent_id: rng.text(),
                activity_id: rng.coin().then(|| rng.text()),
                grants,
                body,
                supersedes: supersedes.clone(),
            })
            .unwrap_or_else(|e| panic!("create {id}: {e}"));
        if let Some(prior) = supersedes {
            self.successor.insert(prior.to_string(), id.clone());
        }
        self.made_from.insert(id.to_string(), made_from);
        self.addrs.extend(deed.paths.iter().cloned());
        if let Body::MailDraft { message_id, .. } = &deed.body {
            self.mail.push(message_id.clone());
        }
        if matches!(kind, Kind::File | Kind::Set | Kind::Clip) && rng.coin() {
            self.client
                .leave(&id, &self.work.join("left"))
                .unwrap_or_else(|e| panic!("leave {id}: {e}"));
        }
        self.held.push(id.clone());
        self.tips.push(id);
    }

    fn body(&mut self, kind: &Kind, rng: &mut Rng) -> Body {
        match kind {
            Kind::File => Body::File {
                path: self.product(rng),
                media_type: rng.coin().then(|| "text/plain".to_string()),
            },
            Kind::Set => Body::Set {
                title: rng.text(),
                members: self.products(rng),
            },
            Kind::Quote => {
                let excerpt = rng.text();
                // The top bit clear leaves room to add the excerpt's length.
                let start = rng.draw() >> 1;
                Body::Quote {
                    edition: rng.url(),
                    start,
                    end: start + excerpt.len() as u64,
                    excerpt,
                    urls: (0..rng.items()).map(|_| rng.url()).collect(),
                }
            }
            Kind::Patch => Body::Patch {
                tree: rng.text(),
                diffs: self.products(rng),
                functionaries: (0..rng.items()).map(|_| rng.text()).collect(),
            },
            Kind::MailDraft => Body::MailDraft {
                message_id: MailMessageId::parse(&format!(
                    "<{}.{}@example.invalid>",
                    self.tag, self.minted
                ))
                .unwrap(),
                in_reply_to: if rng.coin() {
                    rng.pick(&self.mail)
                } else {
                    None
                },
                subject: rng.text(),
                path: self.product(rng),
            },
            Kind::Clip => {
                let in_point = rng.point();
                Body::Clip {
                    sources: self.products(rng),
                    in_point,
                    out_point: in_point + rng.point(),
                    duration: rng.point(),
                    path: self.product(rng),
                }
            }
            Kind::Page => Body::Page {
                url: rng.url(),
                snapshot: self.product(rng),
            },
            Kind::Form => Body::Form {
                blank: rng.text(),
                fields: (0..rng.items())
                    .map(|_| FormField {
                        name: rng.text(),
                        value: rng.text(),
                    })
                    .collect(),
                path: self.product(rng),
            },
            Kind::Table => Body::Table {
                measures: (0..rng.items())
                    .map(|_| Measure {
                        name: rng.text(),
                        value: rng.text(),
                    })
                    .collect(),
            },
            Kind::Procedure => Body::Procedure {
                steps: (0..rng.items())
                    .map(|_| Step {
                        text: rng.text(),
                        done: rng.coin(),
                    })
                    .collect(),
            },
            Kind::Event => Body::Event {
                when: rng.text(),
                where_: rng.text(),
                who: rng.text(),
            },
        }
    }

    fn products(&mut self, rng: &mut Rng) -> Vec<PathBuf> {
        (0..rng.items()).map(|_| self.product(rng)).collect()
    }

    /// An address this store already holds, or a new file for `create` to
    /// take in, on a fair coin.
    fn product(&mut self, rng: &mut Rng) -> PathBuf {
        if rng.coin() {
            if let Some(addr) = rng.pick(&self.addrs) {
                return addr;
            }
        }
        let bytes: Vec<u8> = (0..rng.below(BYTES + 1))
            .map(|_| rng.draw() as u8)
            .collect();
        let path = self.work.join(format!("product-{}", self.products));
        self.products += 1;
        fs::write(&path, bytes).unwrap();
        path
    }

    /// `picked` and what a store needs beside them to take them: the deeds
    /// each was made from, and every later take of each.
    fn closed(&self, picked: Vec<DeedId>) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut todo = picked;
        while let Some(id) = todo.pop() {
            if !out.insert(id.to_string()) {
                continue;
            }
            todo.extend(self.made_from[id.as_str()].iter().cloned());
            todo.extend(self.successor.get(id.as_str()).cloned());
        }
        out
    }

    /// Hold what an import brought from `from`: those deeds can be sources
    /// here, their bytes reused, and the ones `from` never revised revised.
    fn took(&mut self, from: &Seat, ids: &BTreeSet<String>) {
        for raw in ids {
            let id = DeedId::parse(raw).unwrap();
            self.addrs
                .extend(self.client.get(&id).unwrap().paths.iter().cloned());
            if !from.successor.contains_key(raw) {
                self.tips.push(id.clone());
            }
            self.held.push(id);
        }
    }
}

/// What a seat sees of one store: every deed it serves, where `current` and
/// `trail` lead from each, and the log.
struct Shelf {
    deeds: BTreeMap<String, Deed>,
    current: BTreeMap<String, String>,
    trail: BTreeMap<String, Vec<String>>,
    log: Vec<LogEntry>,
    head: LogHead,
}

impl Shelf {
    fn read(url: &str) -> Self {
        let mut client = Client::open(url).unwrap();
        let mut shelf = Self {
            deeds: BTreeMap::new(),
            current: BTreeMap::new(),
            trail: BTreeMap::new(),
            log: client.log_entries().unwrap(),
            head: client.log_head().unwrap(),
        };
        for deed in client.list().unwrap() {
            let id = deed.id.to_string();
            let tip = client.current(&deed.id).unwrap().id.to_string();
            let trail = client.trail(&deed.id).unwrap();
            shelf.current.insert(id.clone(), tip);
            shelf
                .trail
                .insert(id.clone(), trail.iter().map(|d| d.id.to_string()).collect());
            shelf.deeds.insert(id, deed);
        }
        shelf
    }
}

fn export<'a>(url: &str, ids: impl IntoIterator<Item = &'a String>, bag: &Path) {
    let client = Client::open(url).unwrap();
    let exporter = client.exporter().unwrap();
    let deeds = bag.join("data").join("deeds");
    for raw in ids {
        exporter
            .export(&DeedId::parse(raw).unwrap(), &deeds)
            .unwrap_or_else(|e| panic!("export {raw}: {e}"));
    }
}

/// Every file under `dir`, with its bytes.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(
                    path.strip_prefix(dir).unwrap().to_path_buf(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    out
}

fn hex(addr: &Path) -> &str {
    addr.to_str().unwrap().strip_prefix("sha256:").unwrap()
}

/// Where `current` leads from `id` once one store holds both chains: each
/// deed has one successor in the union, held by whichever side recorded it.
fn tip(id: &str, from: &Shelf, before: &Shelf) -> String {
    let mut at = id.to_string();
    while let Some(next) = [from.current.get(&at), before.current.get(&at)]
        .into_iter()
        .flatten()
        .find(|next| **next != at)
    {
        at = next.clone();
    }
    at
}

/// `got` held `before` and then took a bag of every deed `from` serves. It
/// serves the union; every deed passes evidence, leads where it led on its
/// side, and leaves the same bytes; each one it took keeps the receipt it came
/// with, and the head when the bag was signed; and its log is `before`'s with
/// `from`'s missing entries appended in `from`'s order.
fn assert_took(
    got_url: &str,
    from_url: &str,
    from: &Shelf,
    before: &Shelf,
    signed: bool,
    scratch: &Path,
) {
    let got = Shelf::read(got_url);
    let mut want = before.deeds.clone();
    for (id, deed) in &from.deeds {
        if let Some(held) = want.insert(id.clone(), deed.clone()) {
            assert_eq!(&held, deed, "{id} was held here as other bytes");
        }
    }
    assert_eq!(
        got.deeds.keys().collect::<Vec<_>>(),
        want.keys().collect::<Vec<_>>(),
        "the deeds served are not the union"
    );
    assert_eq!(got.deeds, want);

    let mut client = Client::open(got_url).unwrap();
    let got_dir = deedar::store_dir(got_url).unwrap();
    let from_dir = deedar::store_dir(from_url).unwrap();
    for (id, deed) in &want {
        let ev = client
            .evidence(&deed.id)
            .unwrap_or_else(|e| panic!("evidence {id}: {e}"));
        let trail = from.trail.get(id).or_else(|| before.trail.get(id));
        assert_eq!(Some(&got.trail[id]), trail, "trail {id}");
        assert_eq!(got.current[id], tip(id, from, before), "current {id}");
        let Some(entry) = from.log.iter().find(|e| e.id == *id) else {
            continue;
        };
        assert_eq!(ev.unix_time, entry.unix_time, "evidence time {id}");
        for addr in &deed.paths {
            assert_eq!(
                fs::read(got_dir.join("bytes").join(hex(addr))).unwrap(),
                fs::read(from_dir.join("bytes").join(hex(addr))).unwrap(),
                "{id}: {}",
                addr.display()
            );
        }
        let proof = fs::read_to_string(got_dir.join("deeds").join(format!("{id}.proof")))
            .unwrap_or_else(|e| panic!("{id}.proof: {e}"));
        let receipt = Receipt::parse(&proof).unwrap();
        assert_eq!(
            (&receipt.id, &receipt.digest, receipt.unix_time),
            (&entry.id, &entry.digest, entry.unix_time),
            "{id}: the kept receipt is not the entry it came with"
        );
        assert_eq!(receipt.digest, deedar::deed_digest(deed), "{id}");
        match fs::read_to_string(got_dir.join("deeds").join(format!("{id}.head"))) {
            Ok(head) => assert_eq!(deedar::kept_head(&head).unwrap(), receipt.head(), "{id}"),
            Err(_) => assert!(!signed, "{id}: a signed head was not kept"),
        }
        if matches!(deed.kind, Kind::File | Kind::Set | Kind::Clip) {
            let here = client.leave(&deed.id, &scratch.join("got")).unwrap();
            let there = Client::open(from_url)
                .unwrap()
                .leave(&deed.id, &scratch.join("from"))
                .unwrap();
            assert_eq!(snapshot(&here), snapshot(&there), "leave {id}");
        }
    }

    let kept = before.log.len();
    assert_eq!(
        got.log[..kept],
        before.log[..],
        "the log this store held before was rewritten"
    );
    let had: BTreeSet<&str> = before.log.iter().map(|e| e.id.as_str()).collect();
    let appended: Vec<&LogEntry> = from
        .log
        .iter()
        .filter(|e| !had.contains(e.id.as_str()))
        .collect();
    assert_eq!(got.log[kept..].iter().collect::<Vec<_>>(), appended);
    for entry in &from.log {
        let named: Vec<&LogEntry> = got.log.iter().filter(|e| e.id == entry.id).collect();
        assert_eq!(named, [entry], "{} in the log", entry.id);
    }
    let audit = client.log_audit().unwrap();
    assert!(audit.is_clean(), "{audit:?}");
    if kept > 0 {
        let bridge = client.bridge(kept).unwrap();
        assert_eq!(bridge.from, before.head);
        deedar::check_since(&before.head, Some(&bridge), &got.head).unwrap();
    }
}

fn round_trip(seed: u64) {
    let seats = Seats::new(seed);
    let mut rng = Rng(seed);
    let mut sender = Seat::open(&seats, "sender", "sent");
    let mut receiver = Seat::open(&seats, "receiver", "kept");

    let mut kinds = Kind::all().to_vec();
    for at in (1..kinds.len()).rev() {
        kinds.swap(at, rng.below(at + 1));
    }
    for kind in &kinds {
        sender.mint(kind, &mut rng);
    }
    sender.mint_some(&mut rng);
    receiver.mint_some(&mut rng);

    let mut picked: Vec<DeedId> = sender.held.iter().filter(|_| rng.coin()).cloned().collect();
    picked.extend(rng.pick(&sender.held));
    let earlier = sender.closed(picked);
    let bag = seats.dir("earlier");
    export(&sender.url, &earlier, &bag);
    let done = seats.import(&receiver.url, &bag);
    assert_eq!((done.deeds, done.logged), (earlier.len(), earlier.len()));

    sender.mint_some(&mut rng);
    receiver.took(&sender, &earlier);
    receiver.mint_some(&mut rng);

    let sent = Shelf::read(&sender.url);
    let everything = seats.dir("everything");
    export(&sender.url, sent.deeds.keys(), &everything);
    let addrs: BTreeSet<&Path> = sent
        .deeds
        .values()
        .flat_map(|d| d.paths.iter().map(PathBuf::as_path))
        .collect();
    println!(
        "seed {seed}: {}, sender {} deeds over {} product files, earlier handover {}, \
         receiver minted {}",
        if seats.signed { "signed" } else { "unsigned" },
        sent.deeds.len(),
        addrs.len(),
        earlier.len(),
        receiver.minted,
    );

    for name in ["empty", "receiver"] {
        let url = seats.url(name);
        let before = Shelf::read(&url);
        let new = sent
            .deeds
            .keys()
            .filter(|id| !before.deeds.contains_key(*id))
            .count();
        let done = seats.import(&url, &everything);
        assert_eq!((done.deeds, done.logged), (new, new), "{name}");
        let dir = deedar::store_dir(&url).unwrap();
        let written = snapshot(&dir);
        let again = seats.import(&url, &everything);
        assert_eq!((again.deeds, again.logged), (0, 0), "{name} again");
        assert_eq!(snapshot(&dir), written, "{name}: importing again wrote");
        assert_took(
            &url,
            &sender.url,
            &sent,
            &before,
            seats.signed,
            &seats.dir(&format!("{name}-left")),
        );
    }

    let union = Shelf::read(&receiver.url);
    let bag = seats.dir("union");
    export(&receiver.url, union.deeds.keys(), &bag);
    let third = seats.url("third");
    let before = Shelf::read(&third);
    let done = seats.import(&third, &bag);
    assert_eq!(
        (done.deeds, done.logged),
        (union.deeds.len(), union.log.len())
    );
    assert_took(
        &third,
        &receiver.url,
        &union,
        &before,
        seats.signed,
        &seats.dir("third-left"),
    );
    // The sender's deeds came here through the receiver; the sender's own bag
    // of them adds nothing.
    let direct = seats.import(&third, &everything);
    assert_eq!((direct.deeds, direct.logged), (0, 0));
}

#[test]
fn export_then_import_keeps_every_deed_byte_and_log_entry() {
    for seed in SEEDS {
        round_trip(seed);
    }
}
