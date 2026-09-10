//! One-writer deed store. Clients speak the Cap'n schema via [`Client`].
//!
//! ```text
//! just deedar / any client
//!           |
//!      Client::create|get|list|trail|evidence|delete|leave|timestamp|current|migrate
//!           |  Cap'n  (schema/deedar.capnp)
//!           v
//!        FsStore  -- write-once bytes + frozen deed + evidence
//! ```

use std::path::{Path, PathBuf};

mod attest;
mod current;
mod digest;
mod fs;
mod host;
mod leave;
pub mod log;
mod migrate;
mod timestamp;
mod url;
mod wire;

pub use attest::{Demand, Policy};
pub use deed::{
    walk_trail, Body, Deed, DeedId, Draft, Error, Evidence, Face, FormField, Grant, Kind,
    MailMessageId, Measure, ProducedBy, Result, Source, Step,
};
pub use digest::{deed_digest, resolve};
pub use fs::Missing;
pub use fs::{is_write_once_addr, FsStore};
pub use log::{Entry as LogEntry, Head as LogHead};
pub use timestamp::{imprint_hash, timestamp_req};
pub use url::{open, StoreUrl};
pub use wire::{
    decode_request, decode_response, encode_request, encode_response, Request, Response,
};

/// Agent and grants the writer stamps onto the deed.
pub struct CreateRequest {
    pub id: Option<DeedId>,
    pub name: String,
    pub sources: Vec<Source>,
    pub agent_id: String,
    pub activity_id: Option<String>,
    pub grants: Vec<Grant>,
    pub body: Body,
    /// Prior accession this take replaces. Sidecar only; not a Deed field.
    pub supersedes: Option<DeedId>,
}

/// Cap'n client. Same path the `deedar` command uses.
pub struct Client {
    store: FsStore,
}

impl Client {
    pub fn open(url: &str) -> Result<Self> {
        Ok(Self {
            store: crate::url::open(url)?,
        })
    }

    pub fn create(&mut self, req: CreateRequest) -> Result<(Deed, deed::Evidence)> {
        let bytes = wire::encode_request(&Request::Create(Box::new(req)))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Created { deed, evidence } => Ok((deed, evidence)),
            Response::Err(e) => Err(map_err(&e)),
            _ => Err(deed::Error::Decode("create response".into())),
        }
    }

    pub fn resolve(&mut self, raw: &str) -> Result<DeedId> {
        crate::digest::resolve(&self.store, raw)
    }

    pub fn get(&mut self, id: &DeedId) -> Result<Deed> {
        let bytes = wire::encode_request(&Request::Get(id.clone()))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Deed(d) => Ok(d),
            Response::Err(e) => Err(map_err(&e)),
            _ => Err(deed::Error::Decode("get response".into())),
        }
    }

    pub fn list(&mut self) -> Result<Vec<Deed>> {
        let bytes = wire::encode_request(&Request::List)?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Deeds(ds) => Ok(ds),
            Response::Err(e) => Err(map_err(&e)),
            _ => Err(deed::Error::Decode("list response".into())),
        }
    }

    pub fn trail(&mut self, id: &DeedId) -> Result<Vec<Deed>> {
        let bytes = wire::encode_request(&Request::Trail(id.clone()))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Deeds(ds) => Ok(ds),
            Response::Err(e) => Err(map_err(&e)),
            _ => Err(deed::Error::Decode("trail response".into())),
        }
    }

    pub fn delete(&mut self, id: &DeedId) -> Result<()> {
        let bytes = wire::encode_request(&Request::Delete(id.clone()))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Ok => Ok(()),
            Response::Err(e) => Err(map_err(&e)),
            _ => Err(deed::Error::Decode("delete response".into())),
        }
    }

    pub fn evidence(&mut self, id: &DeedId) -> Result<deed::Evidence> {
        let bytes = wire::encode_request(&Request::Evidence(id.clone()))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Evidence(ev) => Ok(ev),
            Response::Err(e) => Err(map_err(&e)),
            _ => Err(deed::Error::Decode("evidence response".into())),
        }
    }

    pub fn leave(&mut self, id: &DeedId, dest: &Path) -> Result<PathBuf> {
        let bytes = wire::encode_request(&Request::Leave {
            id: id.clone(),
            dest: dest.to_path_buf(),
        })?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Ok => Ok(dest.join(id.as_str())),
            Response::Err(e) => Err(map_err(&e)),
            _ => Err(deed::Error::Decode("leave response".into())),
        }
    }

    pub fn timestamp(&mut self, id: &DeedId) -> Result<PathBuf> {
        let bytes = wire::encode_request(&Request::Timestamp(id.clone()))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Ok => {
                let deeds = self.store.dir().join("deeds");
                let tsr = deeds.join(format!("{id}.tsr"));
                if tsr.exists() {
                    Ok(tsr)
                } else {
                    Ok(deeds.join(format!("{id}.tsq")))
                }
            }
            Response::Err(e) => Err(map_err(&e)),
            _ => Err(deed::Error::Decode("timestamp response".into())),
        }
    }

    pub fn current(&mut self, id: &DeedId) -> Result<Deed> {
        let bytes = wire::encode_request(&Request::Current(id.clone()))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Deed(d) => Ok(d),
            Response::Err(e) => Err(map_err(&e)),
            _ => Err(deed::Error::Decode("current response".into())),
        }
    }

    pub fn migrate(&mut self) -> Result<()> {
        self.store.migrate()
    }
}

fn map_err(msg: &str) -> deed::Error {
    if let Some(s) = msg.strip_prefix("deed frozen: ") {
        deed::Error::Frozen(s.into())
    } else if let Some(s) = msg.strip_prefix("deed tombstoned: ") {
        deed::Error::Tombstoned(s.into())
    } else if let Some(s) = msg.strip_prefix("deed not found: ") {
        deed::Error::NotFound(s.into())
    } else if let Some(s) = msg.strip_prefix("evidence: ") {
        deed::Error::Evidence(s.into())
    } else if let Some(s) = msg.strip_prefix("invalid deed id: ") {
        deed::Error::InvalidId(s.into())
    } else if let Some(s) = msg.strip_prefix("invalid kind: ") {
        deed::Error::InvalidKind(s.into())
    } else if let Some(s) = msg.strip_prefix("invalid url: ") {
        deed::Error::InvalidUrl(s.into())
    } else if let Some(s) = msg.strip_prefix("invalid message-id: ") {
        deed::Error::InvalidMessageId(s.into())
    } else if let Some(s) = msg.strip_prefix("invalid body: ") {
        deed::Error::InvalidBody(s.into())
    } else if let Some(s) = msg.strip_prefix("store decode: ") {
        deed::Error::Decode(s.into())
    } else if let Some(s) = msg.strip_prefix("store io: ") {
        deed::Error::Io(s.into())
    } else {
        deed::Error::Io(msg.into())
    }
}

pub fn store_dir(url: &str) -> Result<PathBuf> {
    StoreUrl::parse(url).map(|u| u.dir().to_path_buf())
}

pub fn ensure_dir(dir: impl AsRef<Path>) -> Result<PathBuf> {
    let dir = dir.as_ref();
    std::fs::create_dir_all(dir).map_err(|e| deed::Error::Io(e.to_string()))?;
    Ok(dir.to_path_buf())
}
