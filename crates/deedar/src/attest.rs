//! What the store accepts as a host attestation, and from whom: an Ed25519
//! signature, never a keyed hash, since an HMAC verifier can mint what it
//! checks. Which keys count, and whether one is required, is in `{store}/layout`.

use std::collections::BTreeSet;
use std::path::Path;

use deed::{Error, Result};

/// The scheme a sidecar names, so a reader never has to guess.
pub const ED25519: &str = "ed25519";

/// Whether a store demands an attestation before it will hand a deed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Demand {
    /// Verified when present. The default, because a store that never had
    /// attestation must keep opening.
    #[default]
    Optional,
    /// A deed without a valid attestation is refused.
    Required,
}

/// What `{store}/layout` says about host attestation.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    /// Whether an attestation is demanded.
    pub demand: Demand,
    /// Public keys this store accepts, as 32-byte Ed25519 verifying keys.
    pub signers: BTreeSet<[u8; 32]>,
}

impl Policy {
    /// Read the policy out of a store's layout file.
    ///
    /// A layout that says nothing about attestation means optional with no
    /// accepted signers.
    ///
    /// # Errors
    ///
    /// Fails when the layout cannot be read, names an unknown demand, or
    /// carries a signer that is not a 32-byte key.
    pub fn read(store_dir: &Path) -> Result<Self> {
        let path = store_dir.join("layout");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(Self::default());
        };
        let mut policy = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                // The first line is the layout version, which has no `=`.
                continue;
            };
            match key.trim() {
                "attestation" => {
                    policy.demand = match value.trim() {
                        "required" => Demand::Required,
                        "optional" => Demand::Optional,
                        other => {
                            return Err(Error::Io(format!(
                                "layout: attestation must be required or optional, got {other:?}"
                            )))
                        }
                    };
                }
                "signer" => {
                    policy.signers.insert(parse_signer(value.trim())?);
                }
                _ => {}
            }
        }
        Ok(policy)
    }

    /// Whether this store will hand over a deed with no valid attestation.
    #[must_use]
    pub fn permits_unattested(&self) -> bool {
        self.demand == Demand::Optional
    }
}

/// `ed25519:<64 hex>` to a verifying key.
fn parse_signer(raw: &str) -> Result<[u8; 32]> {
    let Some(hex) = raw.strip_prefix(&format!("{ED25519}:")) else {
        return Err(Error::Io(format!(
            "layout: a signer is {ED25519}:<64 hex>, got {raw:?}"
        )));
    };
    from_hex(hex).ok_or_else(|| Error::Io(format!("layout: signer is not 32 bytes: {hex:?}")))
}

/// Fixed-width hex to 32 bytes.
#[must_use]
pub fn from_hex(text: &str) -> Option<[u8; 32]> {
    let text = text.trim();
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (slot, pair) in out.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        let pair = std::str::from_utf8(pair).ok()?;
        *slot = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(out)
}

/// 32 bytes to fixed-width hex.
#[must_use]
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(layout: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("layout"), layout).unwrap();
        dir
    }

    #[test]
    fn a_layout_that_says_nothing_demands_nothing() {
        let dir = store_with("1\n");
        let policy = Policy::read(dir.path()).unwrap();
        assert_eq!(policy.demand, Demand::Optional);
        assert!(policy.signers.is_empty());
        assert!(policy.permits_unattested());
    }

    /// A store written before any of this existed has no layout at all.
    #[test]
    fn a_missing_layout_is_the_old_layout() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Policy::read(dir.path()).unwrap().permits_unattested());
    }

    #[test]
    fn a_layout_names_its_signers_and_its_demand() {
        let one = "a".repeat(64);
        let two = "b".repeat(64);
        let dir = store_with(&format!(
            "1\nattestation = required\nsigner = ed25519:{one}\nsigner = ed25519:{two}\n"
        ));
        let policy = Policy::read(dir.path()).unwrap();
        assert_eq!(policy.demand, Demand::Required);
        assert_eq!(policy.signers.len(), 2);
        assert!(!policy.permits_unattested());
    }

    /// A demand nobody can read is worse than none: the store would open
    /// wide while its layout says it should not.
    #[test]
    fn an_unreadable_demand_is_refused_rather_than_ignored() {
        let dir = store_with("1\nattestation = maybe\n");
        assert!(Policy::read(dir.path()).is_err());
    }

    #[test]
    fn a_signer_that_is_not_a_key_is_refused() {
        let dir = store_with("1\nsigner = ed25519:beef\n");
        assert!(Policy::read(dir.path()).is_err());
        let dir = store_with(&format!("1\nsigner = rsa:{}\n", "a".repeat(64)));
        assert!(Policy::read(dir.path()).is_err());
    }

    #[test]
    fn hex_round_trips() {
        let bytes = [0x00u8, 0x0f, 0xa5, 0xff]
            .iter()
            .cycle()
            .take(32)
            .copied()
            .collect::<Vec<u8>>();
        let text = to_hex(&bytes);
        assert_eq!(text.len(), 64);
        assert_eq!(from_hex(&text).unwrap().to_vec(), bytes);
        assert!(from_hex("short").is_none());
        assert!(from_hex(&"z".repeat(64)).is_none());
    }
}
