//! RFC 4648 base64 with padding, as notes and checkpoints carry hashes and
//! signatures. Decoding accepts the canonical encoding only: the unused bits
//! of the last symbol are zero and the padding is exactly what the length
//! calls for.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The padded base64 text of `bytes`.
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut block = [0u8; 3];
        block[..chunk.len()].copy_from_slice(chunk);
        let n = u32::from_be_bytes([0, block[0], block[1], block[2]]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn value(symbol: u8) -> Option<u32> {
    let v = match symbol {
        b'A'..=b'Z' => symbol - b'A',
        b'a'..=b'z' => symbol - b'a' + 26,
        b'0'..=b'9' => symbol - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    };
    Some(u32::from(v))
}

/// The bytes `text` encodes, or nothing when `text` is not a canonical padded
/// encoding.
#[must_use]
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (i, chunk) in bytes.chunks_exact(4).enumerate() {
        let last = i == bytes.len() / 4 - 1;
        let padding = chunk.iter().rev().take_while(|b| **b == b'=').count();
        if padding > 2 || (padding > 0 && !last) {
            return None;
        }
        let mut n = 0u32;
        for symbol in &chunk[..4 - padding] {
            n = (n << 6) | value(*symbol)?;
        }
        n <<= 6 * padding as u32;
        let block = n.to_be_bytes();
        let keep = 3 - padding;
        // Bits the kept bytes do not cover have to be zero, or two texts
        // would decode to the same bytes.
        if n & ((1u32 << (6 * padding as u32)) - 1) != 0 {
            return None;
        }
        if padding > 0 && block[1 + keep..].iter().any(|b| *b != 0) {
            return None;
        }
        out.extend_from_slice(&block[1..1 + keep]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rfc_4648_vectors() {
        let vectors = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (plain, encoded) in vectors {
            assert_eq!(encode(plain.as_bytes()), encoded);
            assert_eq!(decode(encoded).unwrap(), plain.as_bytes());
        }
    }

    #[test]
    fn only_the_canonical_text_decodes() {
        // "Zh==" has non-zero bits below the one byte it carries.
        assert_eq!(decode("Zh=="), None);
        assert_eq!(decode("Zm9="), None);
        assert_eq!(decode("Zg="), None);
        assert_eq!(decode("Zg"), None);
        assert_eq!(decode("Zg==Zg=="), None);
        assert_eq!(decode("Zm9v!A=="), None);
        assert_eq!(decode("===="), None);
        let root = "CsUYapGGPo4dkMgIAUqom/Xajj7h2fB2MPA3j2jxq2I=";
        let bytes = decode(root).unwrap();
        assert_eq!(bytes.len(), 32);
        assert_eq!(encode(&bytes), root);
    }
}
