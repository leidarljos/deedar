//! Entry bundles, `tile/entries/<N>[.p/<W>]`: a sequence of big-endian uint16
//! length-prefixed entries, 256 to a full bundle. Entry `i` of bundle `N`
//! hashes to hash `i` of level-0 tile `N`.

use crate::error::{Error, Result};
use crate::layout::MAX_ENTRY_LEN;

/// The bytes of a bundle over `entries`; an entry over 65535 bytes is refused.
pub fn encode_bundle<E: AsRef<[u8]>>(entries: &[E]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.as_ref();
        if entry.len() > MAX_ENTRY_LEN {
            return Err(Error::EntryTooLong(entry.len()));
        }
        out.extend_from_slice(&(entry.len() as u16).to_be_bytes());
        out.extend_from_slice(entry);
    }
    Ok(out)
}

/// The entries of a bundle's bytes; refused when a length prefix runs past
/// the end or bytes trail the last entry.
pub fn decode_bundle(path: &str, bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut entries = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        let Some(prefix) = bytes.get(at..at + 2) else {
            return Err(Error::Corrupt {
                path: path.to_string(),
                reason: format!("a length prefix is cut at byte {at}"),
            });
        };
        let len = usize::from(u16::from_be_bytes([prefix[0], prefix[1]]));
        at += 2;
        let Some(entry) = bytes.get(at..at + len) else {
            return Err(Error::Corrupt {
                path: path.to_string(),
                reason: format!("an entry of {len} bytes is cut at byte {at}"),
            });
        };
        entries.push(entry.to_vec());
        at += len;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_round_trips_including_an_empty_entry() {
        let entries: Vec<Vec<u8>> = vec![b"one".to_vec(), Vec::new(), vec![7u8; 65535]];
        let bytes = encode_bundle(&entries).unwrap();
        assert_eq!(bytes.len(), 2 + 3 + 2 + 2 + 65535);
        assert_eq!(decode_bundle("b", &bytes).unwrap(), entries);
        assert!(decode_bundle("b", &bytes[..bytes.len() - 1]).is_err());
        assert!(decode_bundle("b", &[0]).is_err());
        assert!(decode_bundle("b", &[]).unwrap().is_empty());
    }

    #[test]
    fn an_entry_past_the_prefix_is_refused() {
        let long = vec![0u8; 65536];
        assert!(matches!(
            encode_bundle(&[long]),
            Err(Error::EntryTooLong(65536))
        ));
    }
}
