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
pub struct FsStore {
    dir: PathBuf,
    key: [u8; 32],
    host_key: Option<[u8; 32]>,
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
        if let Some(host_key) = &self.host_key {
            let mac = crate::host::sign(host_key, &wire::deed_bytes(&deed), evidence.unix_time)?;
            atomic_write(&crate::host::sidecar(&self.dir, &deed.id), &mac)?;
        }
        if let Some(prior) = req.supersedes {
            self.record_supersedes(&deed.id, &prior)?;
        }
        Ok((deed, evidence))
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

    fn check_host(&self, deed: &Deed, ev: &Evidence) -> Result<()> {
        let Some(key) = &self.host_key else {
            return Ok(());
        };
        let path = crate::host::sidecar(&self.dir, &deed.id);
        if !path.exists() {
            return Err(Error::Evidence("host sidecar".into()));
        }
        let signature = fs::read(&path).map_err(|e| Error::Io(e.to_string()))?;
        crate::host::verify(key, &wire::deed_bytes(deed), ev.unix_time, &signature)
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
