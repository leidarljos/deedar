//! An append-only Merkle log over the deeds a store has issued, hashed as
//! RFC 9162 (doi:10.17487/RFC9162): leaf `SHA-256(0x00 || entry)`, node
//! `SHA-256(0x01 || left || right)`. Inclusion proofs catch a dropped deed;
//! consistency proofs catch a rewritten log. Signatures alone bind neither.
//! No gossip: a holder showing two consistent logs to two readers is not caught.

use deed::{Error, Result};

/// One entry: the deed's content address and the instant it was logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The deed's accession.
    pub id: String,
    /// `sha256:` of the canonical deed bytes.
    pub digest: String,
    /// Seconds since the epoch, as the evidence records them.
    pub unix_time: u64,
}

impl Entry {
    /// The bytes a leaf hashes over: tab-separated, fixed order.
    #[must_use]
    pub fn material(&self) -> Vec<u8> {
        format!("{}\t{}\t{}", self.id, self.digest, self.unix_time).into_bytes()
    }

    /// The line this entry is stored as.
    #[must_use]
    pub fn line(&self) -> String {
        format!("{}\t{}\t{}\n", self.id, self.digest, self.unix_time)
    }

    /// Parse one stored line.
    ///
    /// # Errors
    ///
    /// Fails when the line is not three tab-separated fields with a numeric time.
    pub fn parse(line: &str) -> Result<Self> {
        let mut parts = line.trim_end_matches('\n').split('\t');
        let (Some(id), Some(digest), Some(time), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(Error::Io(format!("log: not an entry: {line:?}")));
        };
        let unix_time = time
            .parse()
            .map_err(|_| Error::Io(format!("log: not a time: {time:?}")))?;
        Ok(Self {
            id: id.to_string(),
            digest: digest.to_string(),
            unix_time,
        })
    }
}

// The tree, the proofs and the head live in the `rfc9162` crate, which other
// logs share; deedar keeps its entry format here.
pub use rfc9162::{
    consistency_proof, from_hex, hex, inclusion_proof, leaf_hash, node_hash, root,
    verify_consistency, verify_inclusion, Head,
};

/// The name of deedar's log as a `Head::material` domain. A signed head does
/// not use it: `receipt` signs `deedar-head-v1`, a NUL byte, then the size and
/// root.
pub const DOMAIN: &str = "deedar-log-v1";

#[cfg(test)]
mod tests {
    use super::*;

    /// An entry round-trips through the line the log stores it as, and a line
    /// that is not an entry is refused rather than skipped.
    #[test]
    fn an_entry_round_trips_and_a_bad_line_is_refused() {
        let entry = Entry {
            id: "deed-file-note".into(),
            digest: "sha256:abc".into(),
            unix_time: 1_757_000_000,
        };
        let back = Entry::parse(&entry.line()).expect("parses");
        assert_eq!(back, entry);
        assert!(Entry::parse("two\tfields\n").is_err());
        assert!(Entry::parse("a\tb\tnot-a-time\n").is_err());
        assert!(Entry::parse("a\tb\t1\tc\n").is_err());
    }

    /// `Head::material(DOMAIN)` is `deedar-log-v1`, the size and the root,
    /// tab-separated.
    #[test]
    fn the_head_material_is_unchanged() {
        let head = Head {
            size: 3,
            root: "ab".repeat(32),
        };
        assert_eq!(
            head.material(DOMAIN),
            format!("deedar-log-v1\t3\t{}", "ab".repeat(32)).into_bytes()
        );
    }
}
