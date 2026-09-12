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

/// One reading of the log, held for a handover of many deeds; see
/// [`FsStore::exporter`]. Every receipt is a lookup in a tree built once.
pub struct Exporter<'a> {
    store: &'a FsStore,
    tree: std::sync::Arc<LogTree>,
}

/// The log as a tree: entries, leaf hashes, the head, its signature when there
/// is one, and each accession's index.
pub struct LogTree {
    entries: Vec<crate::log::Entry>,
    leaves: Vec<[u8; 32]>,
    head: crate::log::Head,
    signed: Option<crate::receipt::SignedHead>,
    position: std::collections::HashMap<String, usize>,
}

impl Exporter<'_> {
    /// The head every receipt from this exporter is against.
    #[must_use]
    pub fn head(&self) -> &crate::log::Head {
        &self.tree.head
    }

    /// The receipt for one deed, from the tree already built.
    ///
    /// # Errors
    ///
    /// Fails when the log holds no entry for `id`.
    pub fn receipt(&self, id: &DeedId) -> Result<crate::receipt::Receipt> {
        let tree = &self.tree;
        let index = *tree
            .position
            .get(id.as_str())
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        let path = crate::log::inclusion_proof(&tree.leaves, index)
            .ok_or_else(|| Error::Io("log: no path for an entry that is in it".into()))?;
        let entry = &tree.entries[index];
        Ok(crate::receipt::Receipt {
            id: entry.id.clone(),
            digest: entry.digest.clone(),
            unix_time: entry.unix_time,
            index,
            size: tree.head.size,
            root: tree.head.root.clone(),
            path,
        })
    }

    /// Write one deed into a satchel: bytes, evidence, sidecar, receipt, and
    /// the signed head when there is one.
    ///
    /// # Errors
    ///
    /// Fails when the deed is absent, the log has no entry for it, or `into`
    /// cannot be written.
    pub fn export(&self, id: &DeedId, into: &Path) -> Result<Vec<PathBuf>> {
        let deed = self.store.get(id)?;
        let (out, mut written) = self.store.export_files(id, into, &deed)?;

        let receipt = self.receipt(id)?;
        let path = out.join("proof.txt");
        atomic_write(&path, receipt.render().as_bytes())?;
        written.push(path);

        if let Some(signed) = &self.tree.signed {
            let path = crate::receipt::head_beside(into);
            atomic_write(&path, signed.render().as_bytes())?;
            written.push(path);
        }
        Ok(written)
    }
}

/// What an audit found in both directions: logged and gone is tampering;
/// served and never logged is a store that wants backfilling.
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

    /// Nothing lost and something never logged: backfilling is the answer.
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

/// The log file's size and modification time: what says whether a tree built
/// over it is still the tree.
type LogStamp = (u64, Option<std::time::SystemTime>);

/// One reading of the log, and the stamp of the file it was read from.
type TreeCache = std::sync::Mutex<Option<(LogStamp, std::sync::Arc<LogTree>)>>;

/// Directory of deeds, write-once bytes, and tombstones.
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
    /// The log as a tree, kept between exports while the log file stands
    /// still. See [`FsStore::export_into`].
    tree: TreeCache,
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
            tree: std::sync::Mutex::new(None),
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

    /// Log every deed the store serves that the log does not name, in
    /// accession order. The entry time is the evidence's when there is one,
    /// else now.
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
    /// Fails when a line is not an entry; nothing is skipped.
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
    /// For a reader who holds the log; what travels is [`Self::receipt`].
    ///
    /// # Errors
    ///
    /// Fails when the log cannot be read or holds no entry for `id`.
    pub fn log_proof(&self, id: &DeedId) -> Result<(usize, Vec<[u8; 32]>)> {
        let receipt = self.receipt(id)?;
        Ok((receipt.index, receipt.path))
    }

    /// The record a receiver checks a handed-over deed against: the path plus
    /// everything the leaf hashes over.
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

    /// This log's head, signed when a key is configured; `Ok(None)` when not.
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

    /// The consistency proof from a head somebody holds to this log's own.
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

    /// The log against the shelves: deeds logged and gone or changed, and
    /// deeds served and never logged.
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
        // The other direction; walking the log alone calls an empty log clean.
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

    /// Write one deed into a satchel: canonical bytes, evidence, and the
    /// inclusion proof against a head the receiver can record.
    ///
    /// # Errors
    ///
    /// Fails when the deed is absent, the log has no entry for it, or `into`
    /// cannot be written.
    pub fn export_into(&self, id: &DeedId, into: &Path) -> Result<Vec<PathBuf>> {
        // The tree is kept while the log file's size and mtime stand still, so
        // a loop through this costs one reading too.
        let stamp = self.log_stamp();
        {
            let held = self.tree.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((seen, tree)) = held.as_ref() {
                if *seen == stamp {
                    return Exporter {
                        store: self,
                        tree: std::sync::Arc::clone(tree),
                    }
                    .export(id, into);
                }
            }
        }
        let exporter = self.exporter()?;
        let mut held = self.tree.lock().unwrap_or_else(|e| e.into_inner());
        *held = Some((stamp, std::sync::Arc::clone(&exporter.tree)));
        drop(held);
        exporter.export(id, into)
    }

    /// The log file's size and modification time, which is what says whether a
    /// tree built over it is still the tree.
    fn log_stamp(&self) -> LogStamp {
        fs::metadata(self.log_path())
            .map(|m| (m.len(), m.modified().ok()))
            .unwrap_or((0, None))
    }

    /// The log read and hashed once, for a handover of many deeds; every
    /// receipt is then a lookup.
    ///
    /// # Errors
    ///
    /// Fails when the log cannot be read.
    pub fn exporter(&self) -> Result<Exporter<'_>> {
        let entries = self.log_entries()?;
        let leaves: Vec<[u8; 32]> = entries
            .iter()
            .map(|e| crate::log::leaf_hash(&e.material()))
            .collect();
        let head = crate::log::Head::over(&leaves);
        let signed = match crate::host::load_signing_key()? {
            Some(_) => Some(crate::receipt::sign_head(&head)?),
            None => None,
        };
        let position: std::collections::HashMap<String, usize> = entries
            .iter()
            .enumerate()
            .map(|(at, e)| (e.id.clone(), at))
            .collect();
        Ok(Exporter {
            store: self,
            tree: std::sync::Arc::new(LogTree {
                entries,
                leaves,
                head,
                signed,
                position,
            }),
        })
    }

    fn export_files(
        &self,
        id: &DeedId,
        into: &Path,
        deed: &Deed,
    ) -> Result<(PathBuf, Vec<PathBuf>)> {
        let out = into.join(id.as_str());
        fs::create_dir_all(&out).map_err(|e| Error::Io(e.to_string()))?;

        let mut written = Vec::new();
        let bytes = crate::wire::deed_bytes(deed);
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
        Ok((out, written))
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

    /// Check the attestation the store demands. A keyed hash never satisfies
    /// it: a verifier that can mint cannot attest.
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
