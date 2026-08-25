#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use deed::{Body, DeedId};
use deeder::{Client, CreateRequest};

pub fn tmp_url() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("deeder-suite-{n}"));
    std::fs::create_dir_all(&dir).unwrap();
    format!("file://{}", dir.display())
}

pub fn write_blob(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

pub fn quote(id: &str, excerpt: &str, url: &str) -> CreateRequest {
    CreateRequest {
        id: Some(DeedId::parse(id).unwrap()),
        name: id.into(),
        sources: Vec::new(),
        agent_id: "reader".into(),
        activity_id: None,
        grants: Vec::new(),
        body: Body::Quote {
            edition: url.into(),
            start: 0,
            end: excerpt.len() as u64,
            excerpt: excerpt.into(),
            urls: vec![url.into()],
        },
        supersedes: None,
    }
}

pub fn file(id: &str, path: PathBuf) -> CreateRequest {
    CreateRequest {
        id: Some(DeedId::parse(id).unwrap()),
        name: id.into(),
        sources: Vec::new(),
        agent_id: "reader".into(),
        activity_id: None,
        grants: Vec::new(),
        body: Body::File {
            path,
            media_type: Some("text/plain".into()),
        },
        supersedes: None,
    }
}

pub fn open(url: &str) -> Client {
    Client::open(url).unwrap()
}
