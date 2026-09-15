use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::kind::Kind;

/// RFC 5322 msg-id, stored as `<local@domain>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MailMessageId(String);

impl MailMessageId {
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        let inner = trimmed
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .unwrap_or(trimmed);
        let (local, domain) = inner
            .split_once('@')
            .ok_or_else(|| Error::InvalidMessageId(raw.into()))?;
        if local.is_empty()
            || domain.is_empty()
            || local.contains(char::is_whitespace)
            || domain.contains(char::is_whitespace)
        {
            return Err(Error::InvalidMessageId(raw.into()));
        }
        if !domain.contains('.') && domain != "localhost" {
            return Err(Error::InvalidMessageId(raw.into()));
        }
        Ok(Self(format!("<{local}@{domain}>")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measure {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub text: String,
    pub done: bool,
}

/// Kind-specific product. Adding a variant does not rewrite the envelope.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Body {
    File {
        path: PathBuf,
        media_type: Option<String>,
    },
    Set {
        title: String,
        members: Vec<PathBuf>,
    },
    Quote {
        edition: String,
        start: u64,
        end: u64,
        excerpt: String,
        urls: Vec<String>,
    },
    Patch {
        tree: String,
        diffs: Vec<PathBuf>,
        functionaries: Vec<String>,
    },
    MailDraft {
        message_id: MailMessageId,
        in_reply_to: Option<MailMessageId>,
        subject: String,
        path: PathBuf,
    },
    Clip {
        sources: Vec<PathBuf>,
        in_point: f64,
        out_point: f64,
        duration: f64,
        path: PathBuf,
    },
    Page {
        url: String,
        snapshot: PathBuf,
    },
    Form {
        blank: String,
        fields: Vec<FormField>,
        path: PathBuf,
    },
    Table {
        measures: Vec<Measure>,
    },
    Procedure {
        steps: Vec<Step>,
    },
    Event {
        when: String,
        where_: String,
        who: String,
    },
}

impl PartialEq for Body {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Clip {
                    sources: a_s,
                    in_point: a_i,
                    out_point: a_o,
                    duration: a_d,
                    path: a_p,
                },
                Self::Clip {
                    sources: b_s,
                    in_point: b_i,
                    out_point: b_o,
                    duration: b_d,
                    path: b_p,
                },
            ) => {
                a_s == b_s
                    && a_p == b_p
                    && a_i.to_bits() == b_i.to_bits()
                    && a_o.to_bits() == b_o.to_bits()
                    && a_d.to_bits() == b_d.to_bits()
            }
            _ => format!("{self:?}") == format!("{other:?}"),
        }
    }
}

impl Eq for Body {}

impl Body {
    pub fn kind(&self) -> Kind {
        match self {
            Self::File { .. } => Kind::File,
            Self::Set { .. } => Kind::Set,
            Self::Quote { .. } => Kind::Quote,
            Self::Patch { .. } => Kind::Patch,
            Self::MailDraft { .. } => Kind::MailDraft,
            Self::Clip { .. } => Kind::Clip,
            Self::Page { .. } => Kind::Page,
            Self::Form { .. } => Kind::Form,
            Self::Table { .. } => Kind::Table,
            Self::Procedure { .. } => Kind::Procedure,
            Self::Event { .. } => Kind::Event,
        }
    }

    pub fn media_type(&self) -> Option<&str> {
        match self {
            Self::File { media_type, .. } => media_type.as_deref(),
            _ => None,
        }
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        match self {
            Self::File { path, .. } => vec![path.clone()],
            Self::Set { members, .. } => members.clone(),
            Self::Quote { .. } => Vec::new(),
            Self::Patch { diffs, .. } => diffs.clone(),
            Self::MailDraft { path, .. } => vec![path.clone()],
            Self::Clip { path, sources, .. } => {
                let mut p = sources.clone();
                p.push(path.clone());
                p
            }
            Self::Page { snapshot, .. } => vec![snapshot.clone()],
            Self::Form { path, .. } => vec![path.clone()],
            Self::Table { .. } | Self::Procedure { .. } | Self::Event { .. } => Vec::new(),
        }
    }

    pub fn rewrite_path(&mut self, from: &std::path::Path, to: PathBuf) {
        match self {
            Self::File { path, .. } | Self::MailDraft { path, .. } | Self::Form { path, .. } => {
                if path == from {
                    *path = to;
                }
            }
            Self::Set { members, .. } => {
                for p in members.iter_mut() {
                    if p == from {
                        *p = to.clone();
                    }
                }
            }
            Self::Patch { diffs, .. } => {
                for p in diffs.iter_mut() {
                    if p == from {
                        *p = to.clone();
                    }
                }
            }
            Self::Clip { path, sources, .. } => {
                if path == from {
                    *path = to.clone();
                }
                for p in sources.iter_mut() {
                    if p == from {
                        *p = to.clone();
                    }
                }
            }
            Self::Page { snapshot, .. } => {
                if snapshot == from {
                    *snapshot = to;
                }
            }
            Self::Quote { .. }
            | Self::Table { .. }
            | Self::Procedure { .. }
            | Self::Event { .. } => {}
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::File { path, .. } => {
                if path.as_os_str().is_empty() {
                    return Err(Error::InvalidBody(
                        "create file wants --path FILE".into(),
                    ));
                }
            }
            Self::Set { title, members } => {
                if title.trim().is_empty() {
                    return Err(Error::InvalidBody("set title is empty".into()));
                }
                if members.is_empty() {
                    return Err(Error::InvalidBody("set has no members".into()));
                }
                if members.iter().any(|p| p.as_os_str().is_empty()) {
                    return Err(Error::InvalidBody("set member path is empty".into()));
                }
            }
            Self::Quote {
                edition,
                excerpt,
                urls,
                start,
                end,
            } => {
                if edition.trim().is_empty() {
                    return Err(Error::InvalidBody("quote edition is empty".into()));
                }
                if excerpt.trim().is_empty() {
                    return Err(Error::InvalidBody("quote excerpt is empty".into()));
                }
                if urls.is_empty() {
                    return Err(Error::InvalidBody("quote has no source urls".into()));
                }
                if start > end {
                    return Err(Error::InvalidBody("quote range start after end".into()));
                }
                for url in urls {
                    crate::source::Source::url(url)?;
                }
            }
            Self::Patch {
                tree,
                diffs,
                functionaries,
            } => {
                if tree.trim().is_empty() {
                    return Err(Error::InvalidBody("patch tree is empty".into()));
                }
                if diffs.is_empty() {
                    return Err(Error::InvalidBody("patch has no diffs".into()));
                }
                if functionaries.is_empty() {
                    return Err(Error::InvalidBody("patch has no functionaries".into()));
                }
            }
            Self::MailDraft { subject, path, .. } => {
                if subject.trim().is_empty() {
                    return Err(Error::InvalidBody("mail draft subject is empty".into()));
                }
                if path.as_os_str().is_empty() {
                    return Err(Error::InvalidBody("mail draft path is empty".into()));
                }
            }
            Self::Clip {
                sources,
                path,
                in_point,
                out_point,
                duration,
            } => {
                if sources.is_empty() {
                    return Err(Error::InvalidBody("clip has no sources".into()));
                }
                if path.as_os_str().is_empty() {
                    return Err(Error::InvalidBody("clip path is empty".into()));
                }
                if *out_point < *in_point {
                    return Err(Error::InvalidBody("clip out before in".into()));
                }
                if *duration < 0.0 {
                    return Err(Error::InvalidBody("clip duration is negative".into()));
                }
            }
            Self::Page { url, snapshot } => {
                crate::source::Source::url(url)?;
                if snapshot.as_os_str().is_empty() {
                    return Err(Error::InvalidBody("page snapshot is empty".into()));
                }
            }
            Self::Form {
                blank,
                fields,
                path,
            } => {
                if blank.trim().is_empty() {
                    return Err(Error::InvalidBody("form blank is empty".into()));
                }
                if fields.is_empty() {
                    return Err(Error::InvalidBody("form has no fields".into()));
                }
                if path.as_os_str().is_empty() {
                    return Err(Error::InvalidBody("form path is empty".into()));
                }
            }
            Self::Table { measures } => {
                if measures.is_empty() {
                    return Err(Error::InvalidBody("table has no measures".into()));
                }
            }
            Self::Procedure { steps } => {
                if steps.is_empty() {
                    return Err(Error::InvalidBody("procedure has no steps".into()));
                }
            }
            Self::Event { when, where_, who } => {
                if when.trim().is_empty() {
                    return Err(Error::InvalidBody("event when is empty".into()));
                }
                if where_.trim().is_empty() {
                    return Err(Error::InvalidBody("event where is empty".into()));
                }
                if who.trim().is_empty() {
                    return Err(Error::InvalidBody("event who is empty".into()));
                }
            }
        }
        Ok(())
    }
}
