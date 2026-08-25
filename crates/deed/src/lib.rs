//! A named product of a unit of work: a deed the next unit opens by id.
//!
//! ```text
//! work happens --> deeder.create --> deed id
//!                                       |
//!                    get / list / face -+
//!                    evidence ----------+
//!                    next work --input -+
//! ```

mod body;
mod error;
mod face;
mod id;
mod kind;
mod record;
mod source;
mod trail;

pub use body::{Body, FormField, MailMessageId, Measure, Step};
pub use error::{Error, Result};
pub use face::Face;
pub use id::DeedId;
pub use kind::Kind;
pub use record::{Deed, Draft, Evidence, Grant, ProducedBy};
pub use source::Source;
pub use trail::walk_trail;
