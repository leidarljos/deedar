use serde::{Deserialize, Serialize};

use crate::kind::Kind;

/// How a viewer looks at a deed. Face follows media type and body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Face {
    Syntax,
    Sheet,
    Letter,
    Waveform,
    Document,
}

impl Face {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Sheet => "sheet",
            Self::Letter => "letter",
            Self::Waveform => "waveform",
            Self::Document => "document",
        }
    }

    /// Face from kind plus optional file media type.
    pub fn from_kind_and_media(kind: &Kind, media_type: Option<&str>) -> Self {
        match kind {
            Kind::Set => Self::Sheet,
            Kind::Clip => Self::Waveform,
            Kind::MailDraft => Self::Letter,
            Kind::Patch => Self::Syntax,
            Kind::Quote | Kind::Page | Kind::Form | Kind::Table | Kind::Procedure | Kind::Event => {
                Self::Document
            }
            Kind::File => from_media(media_type),
        }
    }
}

fn from_media(media_type: Option<&str>) -> Face {
    let Some(mt) = media_type.map(str::trim).filter(|s| !s.is_empty()) else {
        return Face::Document;
    };
    let lower = mt.to_ascii_lowercase();
    if lower.starts_with("audio/") || lower.starts_with("video/") {
        Face::Waveform
    } else if lower == "message/rfc822" || lower == "message/rfc2822" {
        Face::Letter
    } else if lower.starts_with("image/") || lower == "application/pdf" {
        Face::Document
    } else if lower.starts_with("text/")
        || lower.contains("source")
        || lower == "application/javascript"
        || lower == "application/json"
        || lower == "application/x-rust"
        || lower.ends_with("+xml")
    {
        Face::Syntax
    } else {
        Face::Document
    }
}
