use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Closed catalog. Each kind is a handler.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    File,
    Set,
    Quote,
    Patch,
    MailDraft,
    Clip,
    Page,
    Form,
    Table,
    Procedure,
    Event,
}

impl Kind {
    pub fn parse(raw: &str) -> Result<Self> {
        Self::known(raw).ok_or_else(|| Error::InvalidKind(raw.into()))
    }

    pub fn known(raw: &str) -> Option<Self> {
        match raw {
            "file" => Some(Self::File),
            "set" => Some(Self::Set),
            "quote" => Some(Self::Quote),
            "patch" => Some(Self::Patch),
            "mailDraft" => Some(Self::MailDraft),
            "clip" => Some(Self::Clip),
            "page" => Some(Self::Page),
            "form" => Some(Self::Form),
            "table" => Some(Self::Table),
            "procedure" => Some(Self::Procedure),
            "event" => Some(Self::Event),
            _ => None,
        }
    }

    pub fn token(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Set => "set",
            Self::Quote => "quote",
            Self::Patch => "patch",
            Self::MailDraft => "mailDraft",
            Self::Clip => "clip",
            Self::Page => "page",
            Self::Form => "form",
            Self::Table => "table",
            Self::Procedure => "procedure",
            Self::Event => "event",
        }
    }

    pub fn all() -> &'static [Kind] {
        &[
            Self::File,
            Self::Set,
            Self::Quote,
            Self::Patch,
            Self::MailDraft,
            Self::Clip,
            Self::Page,
            Self::Form,
            Self::Table,
            Self::Procedure,
            Self::Event,
        ]
    }
}
