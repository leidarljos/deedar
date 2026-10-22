//! Taking a checked handover into this store.
//!
//! The bag is checked as `deedar check` checks it, then read once, and every
//! record read is held to what the check saw. Each accession is then planned
//! against this store: its deed, tombstone, log line, product bytes, sidecars,
//! supersede links, and the deeds it was made from. Nothing is written until
//! all of them hold, so a refusal leaves the store as it was.
//!
//! Every store keys its evidence hash with its own `writer.key`, so the
//! sender's cannot be checked here. Evidence is issued again under this
//! store's key, over the same deed bytes and the time the receipt carries. A
//! host signature covers those same bytes and that time, and travels as it
//! was.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use deed::{walk_trail, Deed, DeedId, Error, Result};
use sha2::{Digest, Sha256};

use crate::fs::{atomic_write, FsStore};
use crate::log::Entry;
use crate::receipt::{said, Handover, Receipt, SignedHead};
use crate::wire::{self, Response};

/// Whether an import appends under an unsigned head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnsignedHead {
    /// Refuse the bag: an unsigned head is one the bag asserted about itself.
    #[default]
    Refuse,
    /// Import it anyway, unless the layout says `attestation = required`.
    /// A head signed by an unlisted key is still refused.
    Allow,
}

/// What an import checked, and what it added to this store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The handover, as `check` reports it.
    pub checked: Handover,
    /// Deeds this store did not hold before.
    pub deeds: usize,
    /// Entries appended to this store's log.
    pub logged: usize,
}

impl Imported {
    /// The check's report, then what was added.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = self.checked.render();
        out.push_str(&format!(
            "imported {} deeds, appended {} log entries\n",
            self.deeds, self.logged
        ));
        out
    }
}

/// One deed folder of a bag, read once.
#[derive(Clone)]
struct LinkedDeed {
    id: DeedId,
    digest: String,
}

/// One deed folder of a bag, read once.
struct Carried {
    deed: Deed,
    receipt: Receipt,
    /// `proof.txt` as it travelled.
    proof: String,
    host: Option<Vec<u8>>,
    supersedes: Option<LinkedDeed>,
    successor: Option<LinkedDeed>,
    sources: BTreeMap<String, String>,
}

/// A whole bag, read once.
struct Bag {
    /// In the order the sender logged them.
    carried: Vec<Carried>,
    /// Product bytes from every folder, by the sha256 each was checked against.
    bytes: BTreeMap<String, Vec<u8>>,
    /// The signed head line, as it travelled.
    head: Option<String>,
}

/// Every write an import will make, worked out before the first one.
#[derive(Default)]
struct Plan {
    /// In the order they are written.
    files: Vec<(PathBuf, Vec<u8>)>,
    planned: BTreeMap<PathBuf, usize>,
    /// In the sender's log order.
    log: Vec<(Deed, u64)>,
    deeds: usize,
}

impl Plan {
    /// Write `bytes` at `path` unless the file already holds them. Other bytes
    /// there, on disk or already planned, are refused with what `differs` says.
    fn put(
        &mut self,
        path: PathBuf,
        bytes: Vec<u8>,
        differs: impl FnOnce() -> String,
    ) -> Result<()> {
        if let Some(&at) = self.planned.get(&path) {
            return if self.files[at].1 == bytes {
                Ok(())
            } else {
                Err(Error::Evidence(differs()))
            };
        }
        match read_if_there(&path)? {
            Some(held) if held == bytes => Ok(()),
            Some(_) => Err(Error::Evidence(differs())),
            None => {
                self.planned.insert(path.clone(), self.files.len());
                self.files.push((path, bytes));
                Ok(())
            }
        }
    }

    /// Stage `bytes` at `path`, replacing bytes already on disk.
    fn replace(&mut self, path: PathBuf, bytes: Vec<u8>) {
        if let Some(&at) = self.planned.get(&path) {
            self.files[at].1 = bytes;
            return;
        }
        self.planned.insert(path.clone(), self.files.len());
        self.files.push((path, bytes));
    }
}

impl FsStore {
    /// Check a handover, then add every deed it carried to this store with
    /// evidence under this store's key, and append each one's log entry from
    /// the receipt that travelled with it. A deed already here and logged with
    /// the same fields is left as it is, so importing a bag twice adds nothing.
    ///
    /// # Errors
    ///
    /// Fails, having written nothing, when the bag does not check out or
    /// changed while it was read; when its head is not signed by a key this
    /// store lists and `unsigned` does not allow that, or the layout demands
    /// attestation; or when any deed in it cannot be held here as the sender
    /// held it. An I/O error part-way through the writes leaves files that
    /// importing the same bag again completes.
    pub fn import_handover(&mut self, dir: &Path, unsigned: UnsignedHead) -> Result<Imported> {
        let checked = crate::receipt::check_handover(dir, &self.policy().signers)?;
        if checked.proven.is_empty() {
            return Ok(Imported {
                checked,
                deeds: 0,
                logged: 0,
            });
        }
        let root = crate::receipt::deeds_in(dir)
            .ok_or_else(|| Error::Evidence(format!("{}: no deeds here", dir.display())))?;
        let bag = read_bag(&root)?;
        same_as_checked(&bag, &checked, &self.policy().signers, &root)?;
        self.admit_head(&checked, unsigned)?;

        let plan = self.plan_import(&bag)?;
        for (path, bytes) in &plan.files {
            atomic_write(path, bytes)?;
        }
        // The log goes last, as it does for a create: an entry never names a
        // deed this store failed to write.
        for (deed, unix_time) in &plan.log {
            self.append_log(deed, *unix_time)?;
        }
        Ok(Imported {
            checked,
            deeds: plan.deeds,
            logged: plan.log.len(),
        })
    }

    /// The bar for appending: a head signed by a key this store lists, or an
    /// explicit allowance for an unsigned head, which `attestation = required`
    /// never grants.
    fn admit_head(&self, checked: &Handover, unsigned: UnsignedHead) -> Result<()> {
        if checked.head_accepted {
            return Ok(());
        }
        let layout = self.dir().join("layout");
        if let Some(signer) = checked.head_signer {
            return Err(Error::Evidence(format!(
                "this handover's head is signed by {}, which {} does not list; nothing was \
                 imported",
                crate::log::hex(&signer),
                layout.display()
            )));
        }
        if self.policy().demand == crate::attest::Demand::Required {
            return Err(Error::Evidence(format!(
                "{} says attestation = required, and this handover's head is unsigned; nothing \
                 was imported",
                layout.display()
            )));
        }
        if unsigned == UnsignedHead::Refuse {
            return Err(Error::Evidence(
                "this handover's head is unsigned, so every receipt in it is against a head \
                 the bag asserted about \
                 itself; nothing was imported, and --allow-unsigned-head imports it anyway"
                    .into(),
            ));
        }
        Ok(())
    }

    fn plan_import(&self, bag: &Bag) -> Result<Plan> {
        let logged = self.logged_by_id()?;
        let carried: BTreeMap<&str, &Carried> = bag
            .carried
            .iter()
            .map(|c| (c.deed.id.as_str(), c))
            .collect();
        let mut plan = Plan::default();
        for c in &bag.carried {
            self.plan_deed(c, bag, &logged, &mut plan)?;
        }
        self.plan_links(bag, &carried, &mut plan)?;
        self.check_trails(bag, &carried)?;
        Ok(plan)
    }

    /// This store's log by accession. One accession logged twice with
    /// different fields has no single entry to hold a receipt to.
    fn logged_by_id(&self) -> Result<BTreeMap<String, Entry>> {
        let mut logged: BTreeMap<String, Entry> = BTreeMap::new();
        for entry in self.log_entries()? {
            match logged.get(&entry.id) {
                Some(held) if *held != entry => {
                    return Err(Error::Evidence(format!(
                        "{} is logged here more than once with different fields",
                        entry.id
                    )));
                }
                Some(_) => {}
                None => {
                    logged.insert(entry.id.clone(), entry);
                }
            }
        }
        Ok(logged)
    }

    /// One carried deed against this store: what it already holds has to be
    /// what the bag holds, and what it lacks goes into the plan.
    fn plan_deed(
        &self,
        c: &Carried,
        bag: &Bag,
        logged: &BTreeMap<String, Entry>,
        plan: &mut Plan,
    ) -> Result<()> {
        let deed = &c.deed;
        let id = &deed.id;
        if self.tomb_path(id).exists() {
            return Err(Error::Tombstoned(id.to_string()));
        }
        if self.deed_path(id).exists() {
            if self.get(id)? != *deed {
                return Err(Error::Frozen(id.to_string()));
            }
        } else {
            let frame = wire::encode_response(&Response::Deed(deed.clone()))?;
            plan.put(self.deed_path(id), frame, || {
                format!("{id}: frozen here to other bytes")
            })?;
            plan.deeds += 1;
        }

        if let Some(bound) = self.digest_kept_by_a_successor(id)? {
            if bound != c.receipt.digest {
                return Err(Error::Evidence(format!(
                    "{id} is {}, and this store kept it as {bound} on a deed that supersedes it",
                    c.receipt.digest
                )));
            }
        }

        match logged.get(id.as_str()) {
            Some(held)
                if held.digest == c.receipt.digest && held.unix_time == c.receipt.unix_time => {}
            Some(held) => {
                return Err(Error::Evidence(format!(
                    "{id} is already logged here as {} at time {}, and this handover says {} at \
                     time {}",
                    held.digest, held.unix_time, c.receipt.digest, c.receipt.unix_time
                )));
            }
            None => plan.log.push((deed.clone(), c.receipt.unix_time)),
        }

        let evidence = self.evidence_at(deed, c.receipt.unix_time)?;
        let frame = wire::encode_response(&Response::Created {
            deed: deed.clone(),
            evidence: evidence.clone(),
        })?;
        plan.put(self.evidence_path(id), frame, || {
            format!(
                "{id}: this store holds evidence for it at another time than the receipt's {}",
                c.receipt.unix_time
            )
        })?;

        let host_path = crate::host::sidecar(self.dir(), id);
        let host = match &c.host {
            Some(raw) => {
                plan.put(host_path, raw.clone(), || {
                    format!("{id}: this store holds another host sidecar for it")
                })?;
                Some(raw.clone())
            }
            None => read_if_there(&host_path)?,
        };
        self.check_host_sidecar(deed, &evidence, host.as_deref())
            .map_err(|e| Error::Evidence(format!("{id}: {}", said(&e))))?;

        for path in &deed.paths {
            let hex = path
                .to_str()
                .and_then(|text| text.strip_prefix("sha256:"))
                .filter(|hex| crate::fs::is_sha256_hex(hex))
                .ok_or_else(|| {
                    Error::Evidence(format!("{id}: path {} is not write-once", path.display()))
                })?;
            let dest = self.dir().join("bytes").join(hex);
            match read_if_there(&dest)? {
                Some(held) => {
                    if crate::log::hex(&Sha256::digest(&held)) != hex {
                        return Err(Error::Evidence(format!(
                            "{id}: bytes/{hex} in this store does not hash to its name"
                        )));
                    }
                }
                None => {
                    let raw = bag.bytes.get(hex).ok_or_else(|| {
                        Error::Evidence(format!(
                            "{id}: path sha256:{hex} has bytes in neither this handover nor \
                             this store"
                        ))
                    })?;
                    plan.put(dest, raw.clone(), || format!("{id}: bytes/{hex} differ"))?;
                }
            }
        }

        // A receipt already kept for the same entry stays, with the head it
        // was against: it still proves the entry, against an earlier head.
        // A later bag's head is that log grown. Writing it beside the kept
        // proof makes the two disagree, and refusing the bag blocks every
        // deed that travelled with it. The head is written only when it is
        // the head the kept proof names. A proof this store does not have
        // yet is written with its head in the same plan, so the pair lands
        // together or not at all.
        let proof_path = self.proof_path(id);
        match read_if_there(&proof_path)? {
            Some(held) => {
                let kept = String::from_utf8(held)
                    .ok()
                    .and_then(|text| Receipt::parse(&text).ok())
                    .filter(|kept| {
                        kept.id == c.receipt.id
                            && kept.digest == c.receipt.digest
                            && kept.unix_time == c.receipt.unix_time
                    });
                let Some(kept) = kept else {
                    return Err(Error::Evidence(format!(
                        "{id}: the receipt this store kept for it names another entry"
                    )));
                };
                if let Some(head) = bag.head.as_deref().filter(|head| proof_names(head, &kept)) {
                    plan.put(
                        self.kept_head_path(id),
                        head.to_string().into_bytes(),
                        || format!("{id}: this store kept another head for it"),
                    )?;
                }
            }
            None => {
                plan.put(proof_path, c.proof.clone().into_bytes(), || {
                    format!("{id}: proof differs")
                })?;
                if let Some(head) = &bag.head {
                    plan.put(self.kept_head_path(id), head.clone().into_bytes(), || {
                        format!("{id}: this store kept another head for it")
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Both sides of every supersede link the bag names, so `current` here
    /// steps along the chain the bag carried. `sources`, `supersedes` and
    /// `successor` are covered by the head signature when the bag has one.
    /// They are not what the receipt proved: an unsigned head does not
    /// authenticate them, and the flag that lets that head through is
    /// accepting the files as they stand.
    fn plan_links(
        &self,
        bag: &Bag,
        carried: &BTreeMap<&str, &Carried>,
        plan: &mut Plan,
    ) -> Result<()> {
        let mut by_old: BTreeMap<String, (LinkedDeed, LinkedDeed)> = BTreeMap::new();
        let mut by_new: BTreeMap<String, LinkedDeed> = BTreeMap::new();
        for c in &bag.carried {
            let id = &c.deed.id;
            if let Some(next) = &c.successor {
                require_carried_digest(next, carried)?;
                if !carried.contains_key(next.id.as_str()) {
                    if !self.serves(&next.id) {
                        return Err(Error::Evidence(format!(
                            "{id} is superseded by {} where it came from, and {} is in \
                             neither this handover nor this store, so current here would stop \
                             at {id}",
                            next.id, next.id
                        )));
                    }
                    let held = self.deed_digest_in_store(&next.id)?;
                    if held != next.digest {
                        return Err(Error::Evidence(format!(
                            "{id} is superseded by {} where it came from, and this store serves \
                             {} as {held}, not {}",
                            next.id, next.id, next.digest
                        )));
                    }
                }
            }
            if let Some(old) = &c.supersedes {
                require_carried_digest(old, carried)?;
            }
            let here = LinkedDeed {
                id: id.clone(),
                digest: c.receipt.digest.clone(),
            };
            let links = c
                .supersedes
                .iter()
                .cloned()
                .map(|old| (old, here.clone()))
                .chain(c.successor.iter().cloned().map(|new| (here.clone(), new)));
            for (old, new) in links {
                if old.id == new.id {
                    return Err(Error::Evidence(format!("{} supersedes itself", new.id)));
                }
                if let Some((held_old, held_new)) = by_old.get(old.id.as_str()) {
                    if held_new.id != new.id
                        || held_new.digest != new.digest
                        || held_old.digest != old.digest
                    {
                        return Err(Error::Evidence(format!(
                            "this handover has {} superseded by both {} and {}",
                            old.id, held_new.id, new.id
                        )));
                    }
                }
                if let Some(held) = by_new.get(new.id.as_str()) {
                    if held.id != old.id || held.digest != old.digest {
                        return Err(Error::Evidence(format!(
                            "this handover has {} superseding both {} and {}",
                            new.id, held.id, old.id
                        )));
                    }
                }
                by_old.insert(old.id.to_string(), (old.clone(), new.clone()));
                by_new.insert(new.id.to_string(), old.clone());
            }
        }
        for (old, new) in by_old.values() {
            if !carried.contains_key(new.id.as_str()) && self.deed_path(&new.id).exists() {
                let held = self.deed_digest_in_store(&new.id)?;
                if held != new.digest {
                    return Err(Error::Evidence(format!(
                        "{} supersedes {} where it came from, and this store holds {} as \
                         {held}, not {}",
                        new.id, old.id, new.id, new.digest
                    )));
                }
            }
            stage_supersedes(plan, self.supersedes_path(&new.id), old, &new.id)?;
            if !carried.contains_key(old.id.as_str()) && self.deed_path(&old.id).exists() {
                let held = self.deed_digest_in_store(&old.id)?;
                if held != old.digest {
                    return Err(Error::Evidence(format!(
                        "{} is superseded by {} where it came from, and this store holds {} as \
                         {held}, not {}",
                        old.id, new.id, old.id, old.digest
                    )));
                }
            }
            if carried.contains_key(old.id.as_str()) || self.serves(&old.id) {
                plan.put(
                    self.successor_path(&old.id),
                    format!("{}\n", new.id).into_bytes(),
                    || {
                        format!(
                            "{}: this store says another deed than {} supersedes it",
                            old.id, new.id
                        )
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Every deed a carried one was made from has to be carried or held here
    /// and pass evidence, or `evidence` on the carried deed fails once it
    /// lands. Carried deeds were checked as planned in [`Self::plan_deed`].
    fn check_trails(&self, bag: &Bag, carried: &BTreeMap<&str, &Carried>) -> Result<()> {
        let mut passed: BTreeSet<String> = BTreeSet::new();
        for c in &bag.carried {
            let id = &c.deed.id;
            let trail = walk_trail(&c.deed, |next| match carried.get(next.as_str()) {
                Some(other) => {
                    if let Some(want) = c.sources.get(next.as_str()) {
                        if want != &other.receipt.digest {
                            return Err(Error::Evidence(format!(
                                "{id}: made from {next} as {want}, and this handover carries \
                                 {next} as {}",
                                other.receipt.digest
                            )));
                        }
                    }
                    Ok(other.deed.clone())
                }
                None => {
                    let deed = self.get(next).map_err(|e| {
                        Error::Evidence(format!(
                            "{id}: made from {next}, which this handover does not carry and this \
                             store cannot serve: {}",
                            said(&e)
                        ))
                    })?;
                    let Some(want) = c.sources.get(next.as_str()) else {
                        return Err(Error::Evidence(format!(
                            "{id}: made from {next}, which this handover does not carry and does \
                             not name with a digest"
                        )));
                    };
                    let got = crate::digest::deed_digest(&deed);
                    if got != *want {
                        return Err(Error::Evidence(format!(
                            "{id}: made from {next} as {want}, and this store serves it as {got}"
                        )));
                    }
                    Ok(deed)
                }
            })?;
            for deed in trail {
                if carried.contains_key(deed.id.as_str()) || !passed.insert(deed.id.to_string()) {
                    continue;
                }
                self.evidence_local(&deed).map_err(|e| {
                    Error::Evidence(format!(
                        "{id}: made from {}, which fails evidence here: {}",
                        deed.id,
                        said(&e)
                    ))
                })?;
            }
        }
        Ok(())
    }

    /// Where an imported deed keeps the receipt it travelled with.
    fn proof_path(&self, id: &DeedId) -> PathBuf {
        self.dir().join("deeds").join(format!("{id}.proof"))
    }

    /// Where an imported deed keeps the signed head its receipt is against.
    fn kept_head_path(&self, id: &DeedId) -> PathBuf {
        self.dir().join("deeds").join(format!("{id}.head"))
    }

    fn serves(&self, id: &DeedId) -> bool {
        self.deed_path(id).exists() && !self.tomb_path(id).exists()
    }

    /// A digest already recorded on some `{other}.supersedes` for `id`.
    fn digest_kept_by_a_successor(&self, id: &DeedId) -> Result<Option<String>> {
        let deeds = self.dir().join("deeds");
        let entries = match fs::read_dir(&deeds) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::Io(e.to_string())),
        };
        let mut bound = None;
        for entry in entries {
            let path = entry.map_err(|e| Error::Io(e.to_string()))?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("supersedes") {
                continue;
            }
            let Some(link) = crate::fs::read_supersedes(&path)? else {
                continue;
            };
            if link.id != *id {
                continue;
            }
            let Some(digest) = link.digest else {
                continue;
            };
            if bound.as_ref().is_some_and(|held| held != &digest) {
                return Err(Error::Evidence(format!(
                    "{id} is named with two digests by supersedes sidecars in this store"
                )));
            }
            bound = Some(digest);
        }
        Ok(bound)
    }
}

/// Write `{new}.supersedes` as the id and the digest. An id-only file that
/// names the same accession is completed; a different id or digest is refused.
fn stage_supersedes(
    plan: &mut Plan,
    path: PathBuf,
    old: &LinkedDeed,
    new_id: &DeedId,
) -> Result<()> {
    let bytes = format!("{}\n{}\n", old.id, old.digest).into_bytes();
    let differs = || {
        format!(
            "{new_id}: this store says it supersedes another deed than {}",
            old.id
        )
    };
    let Some(held) = read_if_there(&path)? else {
        return plan.put(path, bytes, differs);
    };
    if held == bytes {
        return Ok(());
    }
    let text = String::from_utf8(held).map_err(|_| Error::Evidence(differs()))?;
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let held_id = lines.next().unwrap_or("");
    let held_digest = lines.next();
    let extra = lines.next().is_some();
    let upgrade = !extra && held_id == old.id.as_str() && held_digest.is_none();
    if !upgrade {
        return Err(Error::Evidence(differs()));
    }
    plan.replace(path, bytes);
    Ok(())
}

/// Read every deed folder once. Each receipt is checked again here, on the
/// bytes this import will write.
fn read_bag(root: &Path) -> Result<Bag> {
    let mut folders: Vec<PathBuf> = fs::read_dir(root)
        .map_err(|e| Error::Io(e.to_string()))?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();

    let mut carried = Vec::new();
    let mut bytes = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for folder in folders {
        let one = read_folder(&folder, &mut bytes)?;
        if !seen.insert(one.deed.id.to_string()) {
            return Err(Error::Evidence(format!(
                "{} appears more than once in this handover",
                one.deed.id
            )));
        }
        carried.push(one);
    }
    carried.sort_by_key(|c| c.receipt.index);

    let head = read_if_there(&crate::receipt::head_beside(root))?
        .map(String::from_utf8)
        .transpose()
        .map_err(|_| Error::Evidence("head: not text".into()))?;
    Ok(Bag {
        carried,
        bytes,
        head,
    })
}

fn read_folder(folder: &Path, bytes: &mut BTreeMap<String, Vec<u8>>) -> Result<Carried> {
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let proof = fs::read_to_string(folder.join("proof.txt"))
        .map_err(|_| Error::Evidence(format!("{name}: no proof.txt")))?;
    let receipt = Receipt::parse(&proof)?;
    let deed_bytes = fs::read(folder.join("deed.bin"))
        .map_err(|_| Error::Evidence(format!("{name}: a receipt with no deed.bin beside it")))?;
    receipt.check(&deed_bytes)?;
    let deed = wire::deed_from_bytes(&deed_bytes)?;
    if deed.id.as_str() != receipt.id {
        return Err(Error::Evidence(format!(
            "{name}: deed.bin says {}, and proof.txt says {}",
            deed.id, receipt.id
        )));
    }
    if crate::digest::deed_digest(&deed) != receipt.digest {
        return Err(Error::Evidence(format!(
            "{name}: deed.bin is not in canonical form, so this store would log another digest \
             than the receipt's"
        )));
    }
    let id = &deed.id;

    if let Some(raw) = read_if_there(&folder.join("evidence.bin"))? {
        match wire::decode_response(&raw)? {
            Response::Created {
                deed: framed,
                evidence,
            } if framed == deed && evidence.deed_id == deed.id => {
                if evidence.unix_time != receipt.unix_time {
                    return Err(Error::Evidence(format!(
                        "{id}: evidence.bin says time {}, and proof.txt says {}",
                        evidence.unix_time, receipt.unix_time
                    )));
                }
            }
            Response::Created { .. } => {
                return Err(Error::Evidence(format!(
                    "{id}: evidence.bin does not match deed.bin"
                )));
            }
            _ => {
                return Err(Error::Decode(format!(
                    "{id}: evidence.bin is not a created frame"
                )));
            }
        }
    }

    let host = read_if_there(&folder.join("host"))?;
    if host
        .as_deref()
        .is_some_and(|raw| crate::host::read_sidecar(raw).is_none())
    {
        return Err(Error::Evidence(format!("{id}: host sidecar unreadable")));
    }

    let dir = folder.join("bytes");
    if dir.is_dir() {
        for entry in fs::read_dir(&dir).map_err(|e| Error::Io(e.to_string()))? {
            let path = entry.map_err(|e| Error::Io(e.to_string()))?.path();
            let file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let raw = fs::read(&path).map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
            let got = crate::log::hex(&Sha256::digest(&raw));
            if got != file {
                return Err(Error::Evidence(format!(
                    "{id}: bytes/{file} hashes to sha256:{got}"
                )));
            }
            bytes.insert(got, raw);
        }
    }

    Ok(Carried {
        supersedes: read_link(&folder.join("supersedes"))?,
        successor: read_link(&folder.join("successor"))?,
        sources: read_sources(&folder.join("sources"))?,
        deed,
        receipt,
        proof,
        host,
    })
}

/// The bag as read here against what `check_handover` read a moment before.
/// Both readings have to be one bag, record for record, or what lands here is
/// not what was checked.
fn same_as_checked(
    bag: &Bag,
    checked: &Handover,
    accept: &BTreeSet<[u8; 32]>,
    root: &Path,
) -> Result<()> {
    let changed = |what: String| {
        Error::Evidence(format!(
            "this handover changed while it was being imported: {what}"
        ))
    };
    let mut read: Vec<String> = bag.carried.iter().map(|c| c.receipt.id.clone()).collect();
    read.sort();
    if read != checked.proven {
        return Err(changed(
            "it carries other deeds than the ones checked".into(),
        ));
    }
    for c in &bag.carried {
        if checked.head.as_ref() != Some(&c.receipt.head()) {
            return Err(changed(format!(
                "{}: proof.txt is against another head than the one checked",
                c.receipt.id
            )));
        }
    }
    let signer = match bag.head.as_deref().map(SignedHead::parse).transpose()? {
        Some(signed) => {
            let accepted = signed.check(accept)?;
            if checked.head.as_ref() != Some(&signed.head) || accepted != checked.head_accepted {
                return Err(changed("the signed head is not the one checked".into()));
            }
            crate::receipt::links_bound(root, &signed).map_err(|e| changed(said(&e)))?;
            Some(signed.signer)
        }
        None => None,
    };
    if signer != checked.head_signer {
        return Err(changed("the signed head is not the one checked".into()));
    }
    Ok(())
}

fn read_link(path: &Path) -> Result<Option<LinkedDeed>> {
    let Some(raw) = read_if_there(path)? else {
        return Ok(None);
    };
    let text = String::from_utf8(raw)
        .map_err(|_| Error::Evidence(format!("{}: not text", path.display())))?;
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let id = lines
        .next()
        .ok_or_else(|| Error::Evidence(format!("{}: empty", path.display())))
        .and_then(DeedId::parse)?;
    let digest = parse_digest(lines.next(), path)?;
    if lines.next().is_some() {
        return Err(Error::Evidence(format!(
            "{}: expected id and digest only",
            path.display()
        )));
    }
    Ok(Some(LinkedDeed { id, digest }))
}

fn read_sources(path: &Path) -> Result<BTreeMap<String, String>> {
    let Some(raw) = read_if_there(path)? else {
        return Ok(BTreeMap::new());
    };
    let text = String::from_utf8(raw)
        .map_err(|_| Error::Evidence(format!("{}: not text", path.display())))?;
    let mut out = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((raw_id, raw_digest)) = line.split_once('\t') else {
            return Err(Error::Evidence(format!(
                "{}: expected `<id><tab><digest>` per line",
                path.display()
            )));
        };
        let id = DeedId::parse(raw_id)?;
        let digest = parse_digest(Some(raw_digest), path)?;
        match out.insert(id.to_string(), digest.clone()) {
            Some(held) if held != digest => {
                return Err(Error::Evidence(format!(
                    "{}: {} named with two digests",
                    path.display(),
                    id
                )));
            }
            _ => {}
        }
    }
    Ok(out)
}

fn parse_digest(raw: Option<&str>, path: &Path) -> Result<String> {
    let digest = raw
        .map(str::trim)
        .filter(|text| text.starts_with("sha256:"))
        .ok_or_else(|| {
            Error::Evidence(format!(
                "{}: expected a sha256 digest beside the id",
                path.display()
            ))
        })?;
    let hex = digest
        .strip_prefix("sha256:")
        .filter(|hex| crate::fs::is_sha256_hex(hex))
        .ok_or_else(|| Error::Evidence(format!("{}: invalid sha256 digest", path.display())))?;
    Ok(format!("sha256:{hex}"))
}

/// A link to a deed this bag carries has to name that deed's receipt digest.
/// The two sidecars are only compared with each other when both are present,
/// so a one-sided link would otherwise bind a carried deed to other bytes.
fn require_carried_digest(link: &LinkedDeed, carried: &BTreeMap<&str, &Carried>) -> Result<()> {
    let Some(other) = carried.get(link.id.as_str()) else {
        return Ok(());
    };
    if other.receipt.digest != link.digest {
        return Err(Error::Evidence(format!(
            "this handover names {} as {}, and carries it as {}",
            link.id, link.digest, other.receipt.digest
        )));
    }
    Ok(())
}

/// Whether `head_line` is the head `proof` was built against. A signed line
/// and a bare `size`/`root` both carry those two fields.
fn proof_names(head_line: &str, proof: &Receipt) -> bool {
    crate::receipt::kept_head(head_line).is_ok_and(|head| head == proof.head())
}

fn read_if_there(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io(format!("{}: {e}", path.display()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deed::{Body, Draft, ProducedBy};

    fn quote(id: &str) -> Deed {
        Deed::from_draft(Draft {
            id: Some(DeedId::parse(id).expect("id")),
            name: id.into(),
            sources: Vec::new(),
            produced_by: ProducedBy {
                agent_id: "reader".into(),
                activity_id: None,
            },
            grants: Vec::new(),
            body: Body::Quote {
                edition: "https://example.invalid/a".into(),
                start: 0,
                end: 4,
                excerpt: id.into(),
                urls: vec!["https://example.invalid/a".into()],
            },
        })
        .expect("deed")
    }

    /// Write `deeds` into a bag with receipts against the log of `logged`.
    fn bag_against(root: &Path, deeds: &[Deed], logged: &[Deed]) {
        let entries: Vec<Entry> = logged
            .iter()
            .enumerate()
            .map(|(at, deed)| Entry {
                id: deed.id.to_string(),
                digest: crate::digest::deed_digest(deed),
                unix_time: 1_800_000_000 + at as u64,
            })
            .collect();
        let leaves: Vec<[u8; 32]> = entries
            .iter()
            .map(|e| crate::log::leaf_hash(&e.material()))
            .collect();
        let head = crate::log::Head::over(&leaves);
        for deed in deeds {
            let at = entries
                .iter()
                .position(|e| e.id == deed.id.as_str())
                .expect("logged");
            let receipt = Receipt {
                id: entries[at].id.clone(),
                digest: entries[at].digest.clone(),
                unix_time: entries[at].unix_time,
                index: at,
                size: head.size,
                root: head.root.clone(),
                path: crate::log::inclusion_proof(&leaves, at).expect("path"),
            };
            let out = root.join(deed.id.as_str());
            fs::create_dir_all(&out).expect("dirs");
            fs::write(out.join("deed.bin"), wire::deed_bytes(deed)).expect("deed");
            fs::write(out.join("proof.txt"), receipt.render()).expect("proof");
        }
    }

    /// A bag checked and then rewritten before the import read it, into one
    /// that checks out on its own, is refused: the import holds what it read
    /// to what was checked, receipt for receipt, not only the accessions.
    #[test]
    fn a_bag_rewritten_after_its_check_is_refused() {
        let all = [quote("deed-quote-one"), quote("deed-quote-two")];
        let dir = tempfile::tempdir().expect("tempdir");
        bag_against(dir.path(), &all[..1], &all[..1]);
        let checked =
            crate::receipt::check_handover(dir.path(), &BTreeSet::new()).expect("checks out");

        bag_against(dir.path(), &all[..1], &all);
        crate::receipt::check_handover(dir.path(), &BTreeSet::new())
            .expect("the rewritten bag checks out on its own");
        let read = read_bag(dir.path()).expect("reads");
        assert_eq!(read.carried[0].receipt.id, checked.proven[0]);
        let err =
            same_as_checked(&read, &checked, &BTreeSet::new(), dir.path()).expect_err("passed");
        assert!(format!("{err}").contains("changed while"), "{err}");
        assert!(format!("{err}").contains("another head"), "{err}");

        bag_against(dir.path(), &all[..1], &all[..1]);
        let read = read_bag(dir.path()).expect("reads");
        same_as_checked(&read, &checked, &BTreeSet::new(), dir.path()).expect("the bag as checked");
    }
}
