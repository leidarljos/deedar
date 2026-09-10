//! The arguments each tool takes.
//!
//! One accession or a list of them, because that is what every verb here is
//! about: the identifier that crosses the three stores.

use schemars::JsonSchema;
use serde::Deserialize;

/// One deed, by accession or by a `sha256:` of it or of a product path.
#[derive(Deserialize, JsonSchema)]
pub struct IdArgs {
    /// `deed-<kind>-<slug>`, or `sha256:` of the deed or of one product path.
    pub id: String,
}

/// Where to write deeds, and which ones.
#[derive(Deserialize, JsonSchema)]
pub struct ExportArgs {
    /// Directory to write into. Created if absent.
    pub into: String,
    /// The accessions to export, as a satchel names them.
    pub accessions: Vec<String>,
}

/// A handover to check, and optionally the head the reader already holds.
#[derive(Deserialize, JsonSchema)]
pub struct CheckArgs {
    /// The satchel directory, or the directory the deeds were exported into.
    pub dir: String,
    /// A bridge file from `deedar log bridge`, joining a head this reader
    /// recorded from an earlier handover to the one this bag is against.
    pub since: Option<String>,
}

/// The size of a head somebody already holds.
#[derive(Deserialize, JsonSchema)]
pub struct BridgeArgs {
    /// Entries in the head the reader recorded last time.
    pub from: usize,
}
