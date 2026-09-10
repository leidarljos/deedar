use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use deed::{walk_trail, Body, Deed, DeedId, Draft, Error, Evidence, ProducedBy, Result};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::timestamp::{imprint_hash, post_query, timestamp_req};
use crate::wire::{self, Request, Response};
use crate::CreateRequest;

type HmacSha256 = Hmac<Sha256>;

/// Directory of deeds, write-once bytes, and tombstones.
/// What an audit found, in both directions.
///
/// The two are different states and only one is tampering. A store written
/// before the log existed has deeds and no entries, and should be told to
/// backfill rather than accused; a store that logged a deed and no longer
/// serves it, or serves one it never logged, is what the log exists to catch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audit {
    /// Entries the log holds.
    pub logged: usize,
    /// Logged and not served as logged.
    pub missing: Vec<Missing>,
    /// Served and never logged.
    pub unlogged: Vec<String>,
}

impl Audit {
    /// Whether the store answers for everything, in both directions.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.unlogged.is_empty()
    }

    /// Whether backfilling is the whole answer.
    ///
    /// True when nothing was lost and something was never logged, which is a
    /// store that is behind rather than one that has been tampered with. The
    /// condition is not that the log is empty: a store with nine deeds from
    /// before the log and one from after has a log, is missing nothing, and
    /// still wants exactly this advice.
    #[must_use]
    pub fn backfill_settles_it(&self) -> bool {
        self.missing.is_empty() && !self.unlogged.is_empty()
    }

    /// Whether nothing here was ever logged, which is a store older than the
    /// log rather than one that fell behind it.
    #[must_use]
    pub fn predates_the_log(&self) -> bool {
        self.logged == 0 && self.backfill_settles_it()
    }
}

/// A deed the log names that the store no longer serves as logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// The accession the log recorded.
    pub id: String,
    /// What the store said when asked for it, or how it differs.
    pub why: String,
}

pub struct FsStore {
    dir: PathBuf,
    key: [u8; 32],
    /// The keyed-hash key, when the store has one beside it.
    host_key: Option<[u8; 32]>,
    /// The Ed25519 key this host signs with, when one is configured. The
    /// private half is deliberately not beside the store.
    signing_key: Option<ed25519_dalek::SigningKey>,
    /// What the layout says this store accepts.
    policy: crate::attest::Policy,
}

impl FsStore {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir.join("bytes")).map_err(|e| Error::Io(e.to_string()))?;
        fs::create_dir_all(dir.join("deeds")).map_err(|e| Error::Io(e.to_string()))?;
        crate::migrate::ensure_layout(dir)?;
        let key_path = dir.join("writer.key");
        let key = if key_path.exists() {
            let bytes = fs::read(&key_path).map_err(|e| Error::Io(e.to_string()))?;
            if bytes.len() != 32 {
                return Err(Error::Io("writer.key must be 32 bytes".into()));
            }
            let mut k = [0u8; 32];
            k.copy_from_slice(&bytes);
            k
        } else {
            let k = seed_key();
            fs::write(&key_path, k).map_err(|e| Error::Io(e.to_string()))?;
            k
        };
        Ok(Self {
            dir: dir.to_path_buf(),
            key,
            host_key: crate::host::load(dir)?,
            signing_key: crate::host::load_signing_key()?,
            policy: crate::attest::Policy::read(dir)?,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn handle(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        let req = wire::decode_request(bytes)?;
        let resp = match req {
            Request::Create(c) => match self.create(*c) {
                Ok((deed, evidence)) => Response::Created { deed, evidence },
                Err(e) => Response::Err(e.to_string()),
            },
            Request::Get(id) => match self.get(&id) {
                Ok(d) => Response::Deed(d),
                Err(e) => Response::Err(e.to_string()),
            },
            Request::List => match self.list() {
                Ok(ds) => Response::Deeds(ds),
                Err(e) => Response::Err(e.to_string()),
            },
            Request::Trail(id) => match self.trail(&id) {
                Ok(ds) => Response::Deeds(ds),
                Err(e) => Response::Err(e.to_string()),
            },
            Request::Delete(id) => match self.delete(&id) {
                Ok(()) => Response::Ok,
                Err(e) => Response::Err(e.to_string()),
            },
            Request::Evidence(id) => match self.evidence(&id) {
                Ok(ev) => Response::Evidence(ev),
                Err(e) => Response::Err(e.to_string()),
            },
            Request::Leave { id, dest } => match self.leave(&id, &dest) {
                Ok(_) => Response::Ok,
                Err(e) => Response::Err(e.to_string()),
            },
            Request::Timestamp(id) => match self.timestamp(&id) {
                Ok(_) => Response::Ok,
                Err(e) => Response::Err(e.to_string()),
            },
            Request::Current(id) => match self.current(&id) {
                Ok(d) => Response::Deed(d),
                Err(e) => Response::Err(e.to_string()),
            },
        };
        wire::encode_response(&resp)
    }

    pub fn create(&mut self, req: CreateRequest) -> Result<(Deed, Evidence)> {
        if let Some(id) = &req.id {
            if self.deed_path(id).exists() || self.tomb_path(id).exists() {
                return Err(Error::Frozen(id.to_string()));
            }
        }
        if let Some(prior) = &req.supersedes {
            if req.id.as_ref() == Some(prior) {
                return Err(Error::InvalidBody("supersedes self".into()));
            }
            if !self.deed_path(prior).exists() {
                return Err(Error::NotFound(prior.to_string()));
            }
            if self.successor_path(prior).exists() {
                return Err(Error::Frozen(prior.to_string()));
            }
        }
        let mut body = req.body;
        self.ingest_body(&mut body)?;
        let draft = Draft {
            id: req.id,
            name: req.name,
            sources: req.sources,
            produced_by: ProducedBy {
                agent_id: req.agent_id,
                activity_id: req.activity_id,
            },
            grants: req.grants,
            body,
        };
        let deed = Deed::from_draft(draft)?;
        if self.deed_path(&deed.id).exists() || self.tomb_path(&deed.id).exists() {
            return Err(Error::Frozen(deed.id.to_string()));
        }
        let evidence = self.issue_evidence(&deed)?;
        let frame = wire::encode_response(&Response::Deed(deed.clone()))?;
        atomic_write(&self.deed_path(&deed.id), &frame)?;
        let eframe = wire::encode_response(&Response::Created {
            deed: deed.clone(),
            evidence: evidence.clone(),
        })?;
        atomic_write(&self.evidence_path(&deed.id), &eframe)?;
        // A signature when the host has a signing key, and the keyed hash
        // otherwise, because a store that has always written one keeps working.
        // Both cover the same canonical bytes.
        if let Some(signing) = &self.signing_key {
            let signed =
                crate::host::sign_ed25519(signing, &wire::deed_bytes(&deed), evidence.unix_time);
            atomic_write(&crate::host::sidecar(&self.dir, &deed.id), &signed)?;
        } else if let Some(host_key) = &self.host_key {
            let mac = crate::host::sign(host_key, &wire::deed_bytes(&deed), evidence.unix_time)?;
            atomic_write(&crate::host::sidecar(&self.dir, &deed.id), &mac)?;
        }
        if let Some(prior) = req.supersedes {
            self.record_supersedes(&deed.id, &prior)?;
        }
        // The log goes last, so an entry never names a deed the store failed
        // to write. The other order would let a crash leave a head covering
        // something no reader can fetch.
        self.append_log(&deed, evidence.unix_time)?;
        Ok((deed, evidence))
    }

    /// Add one deed to the append-only log.
    fn append_log(&self, deed: &Deed, unix_time: u64) -> Result<()> {
        let entry = crate::log::Entry {
            id: deed.id.to_string(),
            digest: crate::digest::deed_digest(deed),
            unix_time,
        };
        let path = self.log_path();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| Error::Io(e.to_string()))?;
        std::io::Write::write_all(&mut file, entry.line().as_bytes())
            .map_err(|e| Error::Io(e.to_string()))?;
        Ok(())
    }

    /// Log every deed the store serves that the log does not name.
    ///
    /// A store written before the log existed is not a store that lost
    /// anything, and telling it so forever is not an answer. This appends what
    /// is already on the shelves, in accession order so two runs over the same
    /// store agree.
    ///
    /// What this cannot do is date them. The entry takes the evidence's time
    /// where there is one, and the moment of backfill where there is not, and
    /// either way the log says these were logged now rather than when they
    /// were made. A log that starts today is honest about starting today; one
    /// that claimed to have watched a deed it never saw would not be.
    ///
    /// # Errors
    ///
    /// Fails when the store or the log cannot be read, or the log cannot be
    /// appended to.
    pub fn log_backfill(&self) -> Result<Vec<String>> {
        let audit = self.log_audit()?;
        let mut added = Vec::new();
        for id in audit.unlogged {
            let Ok(parsed) = DeedId::parse(&id) else {
                continue;
            };
            let deed = self.get(&parsed)?;
            let when = self.evidence(&parsed).map(|e| e.unix_time).unwrap_or(0);
            let stamped = if when == 0 { Self::now_unix() } else { when };
            self.append_log(&deed, stamped)?;
            added.push(id);
        }
        Ok(added)
    }

    /// Seconds since the epoch, for a deed whose evidence carries no time.
    fn now_unix() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Every entry the log holds, in the order they were appended.
    ///
    /// # Errors
    ///
    /// Fails when a line is not an entry, rather than skipping it: a log that
    /// dropped what it could not read would report a tree the store never
    /// signed, which is the failure the log exists to make visible.
    pub fn log_entries(&self) -> Result<Vec<crate::log::Entry>> {
        let path = self.log_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(&path).map_err(|e| Error::Io(e.to_string()))?;
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(crate::log::Entry::parse)
            .collect()
    }

    /// The head over everything logged so far.
    ///
    /// # Errors
    ///
    /// Fails when the log cannot be read.
    pub fn log_head(&self) -> Result<crate::log::Head> {
        let leaves = self.log_leaves()?;
        Ok(crate::log::Head::over(&leaves))
    }

    /// The proof that `id` is in the tree the current head names, and where.
    ///
    /// The path on its own is for a reader who already holds the log and can
    /// rebuild the leaf themselves. Anything that travels wants [`receipt`],
    /// which carries the rest of what the leaf hashes over.
    ///
    /// [`receipt`]: Self::receipt
    ///
    /// # Errors
    ///
    /// Fails when the log cannot be read or holds no entry for `id`.
    pub fn log_proof(&self, id: &DeedId) -> Result<(usize, Vec<[u8; 32]>)> {
        let receipt = self.receipt(id)?;
        Ok((receipt.index, receipt.path))
    }

    /// The record a receiver checks a handed-over deed against.
    ///
    /// This is the proof plus the rest of what the leaf hashes over. The path
    /// alone is not enough for anybody who does not already hold the log:
    /// they have to rebuild the leaf first, and the leaf covers the entry's
    /// time as well as its accession and digest.
    ///
    /// # Errors
    ///
    /// Fails when the deed is absent or the log holds no entry for it.
    pub fn receipt(&self, id: &DeedId) -> Result<crate::receipt::Receipt> {
        let wanted = id.to_string();
        let entries = self.log_entries()?;
        let index = entries
            .iter()
            .position(|e| e.id == wanted)
            .ok_or(Error::NotFound(wanted))?;
        let leaves: Vec<[u8; 32]> = entries
            .iter()
            .map(|e| crate::log::leaf_hash(&e.material()))
            .collect();
        let path = crate::log::inclusion_proof(&leaves, index)
            .ok_or_else(|| Error::Io("log: no path for an entry that is in it".into()))?;
        let head = crate::log::Head::over(&leaves);
        let entry = &entries[index];
        Ok(crate::receipt::Receipt {
            id: entry.id.clone(),
            digest: entry.digest.clone(),
            unix_time: entry.unix_time,
            index,
            size: head.size,
            root: head.root,
            path,
        })
    }

    /// This log's head, with a signature over it when a key is configured.
    ///
    /// A head is the one thing a reader keeps between handovers, so an
    /// unsigned one is two numbers that arrived in the same bag as the deeds
    /// they vouch for. Nothing here can force a store to hold a key, so an
    /// absent key is reported as absent rather than made up: `Ok(None)`.
    ///
    /// # Errors
    ///
    /// Fails when the log cannot be read, or a configured key cannot be used.
    pub fn signed_head(&self) -> Result<Option<crate::receipt::SignedHead>> {
        let head = self.log_head()?;
        match crate::host::load_signing_key()? {
            Some(_) => crate::receipt::sign_head(&head).map(Some),
            None => Ok(None),
        }
    }

    /// The record joining a head somebody already holds to this log's own.
    ///
    /// A receiver who took a satchel last month wrote down a head. Handing
    /// them a second satchel proves each new deed against a new head and says
    /// nothing about whether that head is the old one grown. This is what says
    /// it, and it is the only thing that catches a log rewritten in between.
    ///
    /// # Errors
    ///
    /// Fails when the earlier size is zero, or larger than this log, or names
    /// a root this log never had at that size.
    pub fn bridge(&self, from: usize) -> Result<crate::receipt::Bridge> {
        let leaves = self.log_leaves()?;
        let path = crate::log::consistency_proof(&leaves, from).ok_or_else(|| {
            Error::Evidence(format!(
                "a log of {} entries cannot bridge from {from}",
                leaves.len()
            ))
        })?;
        Ok(crate::receipt::Bridge {
            from: crate::log::Head::over(&leaves[..from]),
            to: crate::log::Head::over(&leaves),
            path,
        })
    }

    /// What the log says the store holds, against what it will hand over.
    ///
    /// The log is append-only and the store is not: `delete` tombstones a deed
    /// and the leaf stays. That gap is the detection. A reader walks the log,
    /// asks for each deed, and gets back the ones that have gone missing or
    /// changed under their accession, which is a question no pile of
    /// signatures answers because each signature is still perfectly good.
    ///
    /// # Errors
    ///
    /// Fails when the log cannot be read.
    pub fn log_audit(&self) -> Result<Audit> {
        let entries = self.log_entries()?;
        let mut out = Vec::new();
        let mut logged: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for entry in &entries {
            logged.insert(entry.id.clone());
        }
        // The other direction, which is the one an empty log makes vacuous. A
        // store written before the log existed has deeds and no entries, and
        // walking only the log calls that clean because there was nothing to
        // walk. The emptier the log the better the verdict, which is backwards.
        let mut unlogged: Vec<String> = Vec::new();
        for deed in self.list()? {
            let id = deed.id.to_string();
            if !logged.contains(&id) {
                unlogged.push(id);
            }
        }
        unlogged.sort();
        for entry in entries {
            let Ok(id) = DeedId::parse(&entry.id) else {
                out.push(Missing {
                    id: entry.id.clone(),
                    why: "the log holds an accession the store cannot parse".into(),
                });
                continue;
            };
            match self.get(&id) {
                Err(e) => out.push(Missing {
                    id: entry.id.clone(),
                    why: e.to_string(),
                }),
                Ok(deed) => {
                    let now = crate::digest::deed_digest(&deed);
                    if now != entry.digest {
                        out.push(Missing {
                            id: entry.id.clone(),
                            why: format!("logged as {}, serves as {now}", entry.digest),
                        });
                    }
                }
            }
        }
        Ok(Audit {
            logged: logged.len(),
            missing: out,
            unlogged,
        })
    }

    /// Write one deed into a satchel: its canonical bytes, its evidence, and
    /// the proof it was in this store's log before the satchel was packed.
    ///
    /// The proof is the part that is not a copy. Bytes and a signature travel
    /// fine on their own and say a writer vouched for them; they do not say
    /// the deed existed before somebody wanted to hand it over. An inclusion
    /// path against a head the receiver can record is what separates a deed
    /// from a deed minted for the occasion.
    ///
    /// # Errors
    ///
    /// Fails when the deed is absent, the log has no entry for it, or `into`
    /// cannot be written.
    pub fn export_into(&self, id: &DeedId, into: &Path) -> Result<Vec<PathBuf>> {
        let deed = self.get(id)?;
        let out = into.join(id.as_str());
        fs::create_dir_all(&out).map_err(|e| Error::Io(e.to_string()))?;

        let mut written = Vec::new();
        let bytes = crate::wire::deed_bytes(&deed);
        let path = out.join("deed.bin");
        atomic_write(&path, &bytes)?;
        written.push(path);

        if let Ok(raw) = fs::read(self.evidence_path(id)) {
            let path = out.join("evidence.bin");
            atomic_write(&path, &raw)?;
            written.push(path);
        }
        // The host sidecar travels when there is one: it is the signature a
        // receiver checks the bytes against.
        if let Ok(raw) = fs::read(crate::host::sidecar(&self.dir, id)) {
            let path = out.join("host");
            atomic_write(&path, &raw)?;
            written.push(path);
        }

        let receipt = self.receipt(id)?;
        let path = out.join("proof.txt");
        atomic_write(&path, receipt.render().as_bytes())?;
        written.push(path);

        // The head every receipt in this bag is against, signed when this
        // store holds a key. Rewritten on each export rather than written
        // once, so a bag whose deeds were exported across a growing log ends
        // up naming the head they actually share, or failing the check.
        if let Some(signed) = self.signed_head()? {
            let path = crate::receipt::head_beside(into);
            atomic_write(&path, signed.render().as_bytes())?;
            written.push(path);
        }
        Ok(written)
    }

    fn log_leaves(&self) -> Result<Vec<[u8; 32]>> {
        Ok(self
            .log_entries()?
            .iter()
            .map(|e| crate::log::leaf_hash(&e.material()))
            .collect())
    }

    fn log_path(&self) -> PathBuf {
        self.dir.join("log")
    }

    pub fn get(&self, id: &DeedId) -> Result<Deed> {
        if self.tomb_path(id).exists() {
            return Err(Error::Tombstoned(id.to_string()));
        }
        let bytes = fs::read(self.deed_path(id)).map_err(|_| Error::NotFound(id.to_string()))?;
        match wire::decode_response(&bytes)? {
            Response::Deed(d) => {
                if d.id != *id {
                    return Err(Error::Decode(format!("accession {id} holds {}", d.id)));
                }
                Ok(d)
            }
            _ => Err(Error::Decode("expected deed frame".into())),
        }
    }

    pub fn list(&self) -> Result<Vec<Deed>> {
        let mut out = Vec::new();
        let entries = fs::read_dir(self.dir.join("deeds")).map_err(|e| Error::Io(e.to_string()))?;
        for entry in entries {
            let entry = entry.map_err(|e| Error::Io(e.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("cap") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if let Ok(id) = DeedId::parse(stem) {
                if self.tomb_path(&id).exists() {
                    continue;
                }
                out.push(self.get(&id)?);
            }
        }
        out.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Ok(out)
    }

    pub fn delete(&mut self, id: &DeedId) -> Result<()> {
        if self.tomb_path(id).exists() {
            return Err(Error::Tombstoned(id.to_string()));
        }
        if !self.deed_path(id).exists() {
            return Err(Error::NotFound(id.to_string()));
        }
        atomic_write(&self.tomb_path(id), b"tomb")?;
        Ok(())
    }

    pub fn trail(&self, id: &DeedId) -> Result<Vec<Deed>> {
        let start = self.get(id)?;
        walk_trail(&start, |next| self.get(next))
    }

    pub fn timestamp(&mut self, id: &DeedId) -> Result<PathBuf> {
        self.get(id)?;
        let bytes =
            fs::read(self.evidence_path(id)).map_err(|_| Error::Evidence("missing".into()))?;
        let req = timestamp_req(&imprint_hash(&bytes));
        match std::env::var("DEEDAR_TSA") {
            Ok(url) if !url.is_empty() => {
                let body = post_query(&url, &req)?;
                let dest = self.tsr_path(id);
                atomic_write(&dest, &body)?;
                Ok(dest)
            }
            _ => {
                let dest = self.tsq_path(id);
                atomic_write(&dest, &req)?;
                Ok(dest)
            }
        }
    }

    pub fn evidence(&self, id: &DeedId) -> Result<Evidence> {
        let trail = self.trail(id)?;
        let mut tip = None;
        for deed in &trail {
            let ev = self.evidence_local(deed)?;
            if deed.id == *id {
                tip = Some(ev);
            }
        }
        tip.ok_or_else(|| Error::Evidence("trail missing tip".into()))
    }

    fn evidence_local(&self, deed: &Deed) -> Result<Evidence> {
        let ev = self.load_evidence(&deed.id)?;
        self.check_paths(deed)?;
        self.check_signature(deed, &ev)?;
        self.check_host(deed, &ev)?;
        if ev.deed_id != deed.id || ev.produced_by != deed.produced_by || ev.grants != deed.grants {
            return Err(Error::Evidence("record does not match deed".into()));
        }
        Ok(ev)
    }

    pub fn leave(&mut self, id: &DeedId, dest: &Path) -> Result<PathBuf> {
        crate::leave::leave(self, id, dest)
    }

    fn load_evidence(&self, id: &DeedId) -> Result<Evidence> {
        let bytes =
            fs::read(self.evidence_path(id)).map_err(|_| Error::Evidence("missing".into()))?;
        match wire::decode_response(&bytes)? {
            Response::Created { evidence, .. } => Ok(evidence),
            _ => Err(Error::Decode("expected evidence frame".into())),
        }
    }

    fn check_paths(&self, deed: &Deed) -> Result<()> {
        for path in &deed.paths {
            let Some(hex) = path.to_str().and_then(|s| s.strip_prefix("sha256:")) else {
                return Err(Error::Evidence(format!(
                    "path {} is not write-once",
                    path.display()
                )));
            };
            let dest = self.dir.join("bytes").join(hex);
            let bytes =
                fs::read(&dest).map_err(|_| Error::Evidence(format!("path {hex} missing")))?;
            let digest = hex_encode(&Sha256::digest(&bytes));
            if digest != hex {
                return Err(Error::Evidence(format!("path sha256:{hex} does not match")));
            }
        }
        Ok(())
    }

    fn check_signature(&self, deed: &Deed, ev: &Evidence) -> Result<()> {
        let mut mac =
            HmacSha256::new_from_slice(&self.key).map_err(|e| Error::Io(e.to_string()))?;
        mac.update(&wire::deed_bytes(deed));
        mac.update(&ev.unix_time.to_le_bytes());
        mac.verify_slice(&ev.signature)
            .map_err(|_| Error::Evidence("keyed hash".into()))
    }

    /// Check whatever attestation the store demands, and whatever it has.
    ///
    /// A keyed hash never satisfies a demand for attestation. It says the bytes
    /// are the bytes; the demand is about who was entitled to make them, and a
    /// construction whose verifier can mint cannot answer that.
    fn check_host(&self, deed: &Deed, ev: &Evidence) -> Result<()> {
        let path = crate::host::sidecar(&self.dir, &deed.id);
        let present = path.exists();
        if !present {
            return if self.policy.permits_unattested() && self.host_key.is_none() {
                Ok(())
            } else {
                Err(Error::Evidence("host sidecar".into()))
            };
        }
        let bytes = fs::read(&path).map_err(|e| Error::Io(e.to_string()))?;
        let deed_bytes = wire::deed_bytes(deed);
        match crate::host::read_sidecar(&bytes) {
            Some(crate::host::Sidecar::Signature(signature)) => crate::host::verify_ed25519(
                &self.policy.signers,
                &deed_bytes,
                ev.unix_time,
                &signature,
            ),
            Some(crate::host::Sidecar::KeyedHash(mac)) => {
                if !self.policy.permits_unattested() {
                    return Err(Error::Evidence("host attestation required".into()));
                }
                match &self.host_key {
                    Some(key) => crate::host::verify(key, &deed_bytes, ev.unix_time, &mac),
                    None => Ok(()),
                }
            }
            None => Err(Error::Evidence("host sidecar unreadable".into())),
        }
    }

    fn ingest_body(&self, body: &mut Body) -> Result<()> {
        let originals = body.paths();
        for path in originals {
            if path.as_os_str().is_empty() {
                continue;
            }
            if path.to_str().is_some_and(|s| s.starts_with("sha256:")) {
                continue;
            }
            let addr = self.ingest_path(&path)?;
            body.rewrite_path(&path, addr);
        }
        Ok(())
    }

    fn ingest_path(&self, path: &Path) -> Result<PathBuf> {
        if !path.is_file() {
            return Err(Error::Io(format!("{} is not a file", path.display())));
        }
        let bytes = fs::read(path).map_err(|e| Error::Io(e.to_string()))?;
        let digest = Sha256::digest(&bytes);
        let hex = hex_encode(&digest);
        let dest = self.dir.join("bytes").join(&hex);
        if !dest.exists() {
            atomic_write(&dest, &bytes)?;
        }
        Ok(PathBuf::from(format!("sha256:{hex}")))
    }

    fn issue_evidence(&self, deed: &Deed) -> Result<Evidence> {
        let unix_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error::Io(e.to_string()))?
            .as_secs();
        let mut mac =
            HmacSha256::new_from_slice(&self.key).map_err(|e| Error::Io(e.to_string()))?;
        mac.update(&wire::deed_bytes(deed));
        mac.update(&unix_time.to_le_bytes());
        let signature = mac.finalize().into_bytes().to_vec();
        Ok(Evidence {
            deed_id: deed.id.clone(),
            produced_by: deed.produced_by.clone(),
            grants: deed.grants.clone(),
            unix_time,
            signature,
        })
    }

    pub(crate) fn deed_path(&self, id: &DeedId) -> PathBuf {
        self.dir.join("deeds").join(format!("{id}.cap"))
    }

    fn evidence_path(&self, id: &DeedId) -> PathBuf {
        self.dir.join("deeds").join(format!("{id}.evidence"))
    }

    fn tsq_path(&self, id: &DeedId) -> PathBuf {
        self.dir.join("deeds").join(format!("{id}.tsq"))
    }

    fn tsr_path(&self, id: &DeedId) -> PathBuf {
        self.dir.join("deeds").join(format!("{id}.tsr"))
    }

    pub(crate) fn tomb_path(&self, id: &DeedId) -> PathBuf {
        self.dir.join("deeds").join(format!("{id}.tomb"))
    }
}

pub(crate) fn atomic_write(dest: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = dest.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| Error::Io(e.to_string()))?;
    fs::rename(&tmp, dest).map_err(|e| Error::Io(e.to_string()))?;
    Ok(())
}

fn seed_key() -> [u8; 32] {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let digest = Sha256::digest(n.to_le_bytes());
    let mut k = [0u8; 32];
    k.copy_from_slice(&digest);
    k
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

pub fn is_write_once_addr(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .to_str()
        .is_some_and(|s| s.starts_with("sha256:"))
}
