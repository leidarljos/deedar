use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::body::Body;
use crate::error::Result;
use crate::face::Face;
use crate::id::DeedId;
use crate::kind::Kind;
use crate::source::Source;

/// Who made the deed. `activity_id` is an optional assignment id
/// (a claimdag `WorkId` when that graph is the caller).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducedBy {
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_id: Option<String>,
}

/// Host or path the seat allowed this sitting.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Grant {
    Host(String),
    Path(PathBuf),
}

/// Writer-issued evidence about a deed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub deed_id: DeedId,
    pub produced_by: ProducedBy,
    pub grants: Vec<Grant>,
    pub unix_time: u64,
    pub signature: Vec<u8>,
}

/// The work plus its identity. The next unit opens it by `id`.
///
/// ```text
/// deeder.create --> Deed { id, kind, body, sources, producedBy }
/// next work --input id
/// deeder.trail(id) walks sources that name other deeds
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deed {
    pub id: DeedId,
    pub kind: Kind,
    pub name: String,
    pub paths: Vec<PathBuf>,
    pub sources: Vec<Source>,
    pub produced_by: ProducedBy,
    pub grants: Vec<Grant>,
    pub face: Face,
    pub body: Body,
}

/// Constructor input. `from_draft` is the shipped assemble.
#[derive(Clone, Debug)]
pub struct Draft {
    pub id: Option<DeedId>,
    pub name: String,
    pub sources: Vec<Source>,
    pub produced_by: ProducedBy,
    pub grants: Vec<Grant>,
    pub body: Body,
}

impl Deed {
    /// Assemble a deed from a draft. Kind, paths, and face come from the body.
    pub fn from_draft(draft: Draft) -> Result<Self> {
        draft.body.validate()?;
        if draft.produced_by.agent_id.trim().is_empty() {
            return Err(crate::Error::InvalidBody(
                "producedBy agent is empty".into(),
            ));
        }
        let kind = draft.body.kind();
        let id = match draft.id {
            Some(id) => {
                if !id.as_str().starts_with(&format!("deed-{}-", kind.token())) {
                    return Err(crate::Error::InvalidId(id.to_string()));
                }
                id
            }
            None => mint_id(&kind, &draft.name)?,
        };
        let mut sources = draft.sources;
        if let Body::Quote { urls, .. } = &draft.body {
            for url in urls {
                let src = Source::url(url)?;
                if !sources.contains(&src) {
                    sources.push(src);
                }
            }
        }
        let name = if draft.name.trim().is_empty() {
            id.to_string()
        } else {
            draft.name
        };
        let face = Face::from_kind_and_media(&kind, draft.body.media_type());
        Ok(Self {
            id,
            kind,
            name,
            paths: draft.body.paths(),
            sources,
            produced_by: draft.produced_by,
            grants: draft.grants,
            face,
            body: draft.body,
        })
    }

    pub fn label(&self) -> String {
        format!("{} · {}", self.kind.token(), self.name)
    }
}

fn mint_id(kind: &Kind, name: &str) -> Result<DeedId> {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if (c == ' ' || c == '-' || c == '_') && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "item" } else { slug };
    DeedId::mint(kind, slug)
}
