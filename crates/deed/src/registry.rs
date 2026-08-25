use crate::body::Body;
use crate::error::{Error, Result};
use crate::face::Face;
use crate::kind::Kind;

/// Compile-time kind handler. A new kind is a new handler.
pub trait KindHandler: Send + Sync {
    fn kind(&self) -> Kind;
    fn face(&self, body: &Body) -> Face;
    fn validate(&self, body: &Body) -> Result<()>;
}

macro_rules! handler {
    ($name:ident, $kind:expr, $pat:pat) => {
        struct $name;
        impl KindHandler for $name {
            fn kind(&self) -> Kind {
                $kind
            }
            fn face(&self, body: &Body) -> Face {
                Face::from_kind_and_media(&$kind, body.media_type())
            }
            fn validate(&self, body: &Body) -> Result<()> {
                match body {
                    $pat => body.validate(),
                    _ => Err(Error::InvalidBody("handler kind/body mismatch")),
                }
            }
        }
    };
}

handler!(FileH, Kind::File, Body::File { .. });
handler!(SetH, Kind::Set, Body::Set { .. });
handler!(QuoteH, Kind::Quote, Body::Quote { .. });
handler!(PatchH, Kind::Patch, Body::Patch { .. });
handler!(MailH, Kind::MailDraft, Body::MailDraft { .. });
handler!(ClipH, Kind::Clip, Body::Clip { .. });
handler!(PageH, Kind::Page, Body::Page { .. });
handler!(FormH, Kind::Form, Body::Form { .. });
handler!(TableH, Kind::Table, Body::Table { .. });
handler!(ProcH, Kind::Procedure, Body::Procedure { .. });
handler!(EventH, Kind::Event, Body::Event { .. });

pub struct Registry {
    handlers: Vec<Box<dyn KindHandler>>,
}

impl Registry {
    pub fn built_in() -> Self {
        Self {
            handlers: vec![
                Box::new(FileH),
                Box::new(SetH),
                Box::new(QuoteH),
                Box::new(PatchH),
                Box::new(MailH),
                Box::new(ClipH),
                Box::new(PageH),
                Box::new(FormH),
                Box::new(TableH),
                Box::new(ProcH),
                Box::new(EventH),
            ],
        }
    }

    pub fn require(&self, kind: &Kind) -> Result<()> {
        if self.handlers.iter().any(|h| &h.kind() == kind) {
            Ok(())
        } else {
            Err(Error::InvalidKind(kind.token().into()))
        }
    }

    pub fn face(&self, body: &Body) -> Face {
        let kind = body.kind();
        self.handlers
            .iter()
            .find(|h| h.kind() == kind)
            .map(|h| h.face(body))
            .unwrap_or_else(|| Face::from_kind_and_media(&kind, body.media_type()))
    }

    pub fn validate(&self, body: &Body) -> Result<()> {
        let kind = body.kind();
        let handler = self
            .handlers
            .iter()
            .find(|h| h.kind() == kind)
            .ok_or_else(|| Error::InvalidKind(kind.token().into()))?;
        handler.validate(body)
    }
}

pub fn registry() -> &'static Registry {
    static REG: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();
    REG.get_or_init(Registry::built_in)
}
