//! One-writer deed store. Clients speak the Cap'n schema via [`Client`].
//!
//! ```text
//! just deeder / any client
//!           |
//!      Client::create|get|list|trail|evidence|delete
//!           |  Cap'n  (schema/deeder.capnp)
//!           v
//!        FsStore  -- write-once bytes + frozen deed + evidence
//! ```

use std::path::{Path, PathBuf};

mod fs;
mod url;
mod wire;

pub use deed::{
    walk_trail, Body, Deed, DeedId, Draft, Error, Evidence, Face, FormField, Grant, Kind,
    MailMessageId, Measure, ProducedBy, Result, Source, Step,
};
pub use fs::{is_write_once_addr, FsStore};
pub use url::{open, StoreUrl};
pub use wire::{
    decode_request, decode_response, encode_request, encode_response, Request, Response,
};

/// Seat + policy values the writer stamps onto the deed.
pub struct CreateRequest {
    pub id: Option<DeedId>,
    pub name: String,
    pub sources: Vec<Source>,
    pub seat_agent_id: String,
    pub seat_activity_id: Option<String>,
    pub policy_grants: Vec<Grant>,
    pub body: Body,
}

/// Cap'n client. Same path the `deeder` command uses.
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
            Response::Err(e) => Err(deed::Error::Io(e)),
            _ => Err(deed::Error::Decode("create response".into())),
        }
    }

    pub fn get(&mut self, id: &DeedId) -> Result<Deed> {
        let bytes = wire::encode_request(&Request::Get(id.clone()))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Deed(d) => Ok(d),
            Response::Err(e) => Err(map_err(&e, id)),
            _ => Err(deed::Error::Decode("get response".into())),
        }
    }

    pub fn list(&mut self) -> Result<Vec<Deed>> {
        let bytes = wire::encode_request(&Request::List)?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Deeds(ds) => Ok(ds),
            Response::Err(e) => Err(deed::Error::Io(e)),
            _ => Err(deed::Error::Decode("list response".into())),
        }
    }

    pub fn trail(&mut self, id: &DeedId) -> Result<Vec<Deed>> {
        let bytes = wire::encode_request(&Request::Trail(id.clone()))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Deeds(ds) => Ok(ds),
            Response::Err(e) => Err(map_err(&e, id)),
            _ => Err(deed::Error::Decode("trail response".into())),
        }
    }

    pub fn delete(&mut self, id: &DeedId) -> Result<()> {
        let bytes = wire::encode_request(&Request::Delete(id.clone()))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Ok => Ok(()),
            Response::Err(e) => Err(map_err(&e, id)),
            _ => Err(deed::Error::Decode("delete response".into())),
        }
    }

    pub fn evidence(&mut self, id: &DeedId) -> Result<deed::Evidence> {
        let bytes = wire::encode_request(&Request::Evidence(id.clone()))?;
        match wire::decode_response(&self.store.handle(&bytes)?)? {
            Response::Evidence(ev) => Ok(ev),
            Response::Err(e) => Err(map_err(&e, id)),
            _ => Err(deed::Error::Decode("evidence response".into())),
        }
    }
}

fn map_err(msg: &str, id: &DeedId) -> deed::Error {
    if msg.contains("tombstoned") {
        deed::Error::Tombstoned(id.to_string())
    } else if msg.contains("frozen") {
        deed::Error::Frozen(id.to_string())
    } else if msg.contains("not found") {
        deed::Error::NotFound(id.to_string())
    } else if let Some(rest) = msg.strip_prefix("evidence: ") {
        deed::Error::Evidence(rest.into())
    } else if msg.starts_with("evidence") {
        deed::Error::Evidence(msg.into())
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
