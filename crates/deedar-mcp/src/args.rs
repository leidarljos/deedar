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
