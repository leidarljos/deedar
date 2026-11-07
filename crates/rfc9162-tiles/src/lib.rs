//! Tiled, indexed storage for an RFC 9162 log, laid out as C2SP tlog-tiles
//! (<https://c2sp.org/tlog-tiles>): 256-hash tiles, 256-entry bundles, and the
//! tree [`rfc9162`] hashes. Any tlog-tiles client reads the tree. The index
//! sidecars beside it are this crate's addition and tlog clients ignore them.
//!
//! The root over the tiles is `rfc9162::root` over the same leaves, and the
//! proofs a reader assembles from tiles are the bytes `rfc9162::inclusion_proof`
//! and `rfc9162::consistency_proof` give over the flat leaf slice. The head
//! is a C2SP checkpoint, an Ed25519 signed note, with witness cosignatures
//! checked as cosignature/v1.

pub mod base64;
pub mod bloom;
pub mod bundle;
pub mod checkpoint;
pub mod error;
pub mod index;
pub mod layout;
pub mod log;
pub mod store;
pub mod tree;

pub use bloom::Bloom;
pub use checkpoint::{Checkpoint, Cosigner, Note, Signer, Verifier};
pub use error::{Error, Result};
pub use index::{KeyFn, Manifest, Sidecar};
pub use layout::{Layout, Tile};
pub use log::{Reader, TreeState, Writer};
pub use store::{MemoryStore, PosixStore, TileStore};
pub use tree::{Frontier, Hash, TileHashReader};
