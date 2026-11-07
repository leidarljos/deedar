//! A split-block Bloom filter: 256-bit blocks, one block per key, one bit in
//! each of the block's eight 32-bit words. A probe touches one cache line.
//! At 10 bits per key the false-positive rate is about 1 percent.
//!
//! Keys are hashed with SHA-256; the first eight bytes, little-endian, pick
//! the block (high half) and the bits (low half). The bit of word `i` is
//! `(low * SALT[i]) >> 27`, the Parquet and Impala construction.

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// Bits per key the writer builds filters with.
pub const BITS_PER_KEY: usize = 10;

const SALT: [u32; 8] = [
    0x47b6_137b,
    0x4497_4d91,
    0x8824_ad5b,
    0xa2b7_289d,
    0x7054_95c7,
    0x2df1_424b,
    0x9efc_4947,
    0x5c6b_fb31,
];

/// The filter over a set of keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bloom {
    blocks: Vec<[u32; 8]>,
}

impl Bloom {
    /// A filter over `keys` at `bits_per_key`; at least one block.
    #[must_use]
    pub fn build<K: AsRef<[u8]>>(keys: &[K], bits_per_key: usize) -> Self {
        let bits = keys.len().saturating_mul(bits_per_key);
        let blocks = bits.div_ceil(256).max(1);
        let mut filter = Self {
            blocks: vec![[0u32; 8]; blocks],
        };
        for key in keys {
            filter.insert(key.as_ref());
        }
        filter
    }

    fn insert(&mut self, key: &[u8]) {
        let (block, mask) = self.locate(key);
        for (word, bit) in self.blocks[block].iter_mut().zip(mask) {
            *word |= bit;
        }
    }

    /// Whether `key` may be in the set: never false for a key that is, true
    /// for about 1 percent of keys that are not.
    #[must_use]
    pub fn contains(&self, key: &[u8]) -> bool {
        let (block, mask) = self.locate(key);
        self.blocks[block]
            .iter()
            .zip(mask)
            .all(|(word, bit)| word & bit == bit)
    }

    fn locate(&self, key: &[u8]) -> (usize, [u32; 8]) {
        let digest = Sha256::digest(key);
        let mut first = [0u8; 8];
        first.copy_from_slice(&digest[..8]);
        let h = u64::from_le_bytes(first);
        let block = (((h >> 32) * self.blocks.len() as u64) >> 32) as usize;
        let low = h as u32;
        let mut mask = [0u32; 8];
        for (bit, salt) in mask.iter_mut().zip(SALT) {
            *bit = 1u32 << (low.wrapping_mul(salt) >> 27);
        }
        (block, mask)
    }

    /// Blocks in the filter.
    #[must_use]
    pub fn blocks(&self) -> usize {
        self.blocks.len()
    }

    /// The filter as bytes: each word little-endian, blocks in order.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.blocks.len() * 32);
        for block in &self.blocks {
            for word in block {
                out.extend_from_slice(&word.to_le_bytes());
            }
        }
        out
    }

    /// The filter `to_bytes` wrote; refused unless whole blocks, at least one.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() % 32 != 0 {
            return Err(Error::Format(format!(
                "a filter of {} bytes is not whole 32-byte blocks",
                bytes.len()
            )));
        }
        let blocks = bytes
            .chunks_exact(32)
            .map(|chunk| {
                let mut block = [0u32; 8];
                for (word, raw) in block.iter_mut().zip(chunk.chunks_exact(4)) {
                    *word = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                }
                block
            })
            .collect();
        Ok(Self { blocks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_member_is_found_and_the_bytes_round_trip() {
        let keys: Vec<Vec<u8>> = (0..500u32).map(|i| i.to_be_bytes().to_vec()).collect();
        let filter = Bloom::build(&keys, BITS_PER_KEY);
        assert_eq!(filter.blocks(), (500 * 10usize).div_ceil(256));
        assert!(keys.iter().all(|k| filter.contains(k)));
        assert_eq!(Bloom::from_bytes(&filter.to_bytes()).unwrap(), filter);
        assert!(Bloom::from_bytes(&[0u8; 31]).is_err());
        let empty = Bloom::build::<&[u8]>(&[], BITS_PER_KEY);
        assert_eq!(empty.blocks(), 1);
        assert!(!empty.contains(b"anything"));
    }

    #[test]
    fn false_positives_stay_near_one_percent() {
        let keys: Vec<Vec<u8>> = (0..512u32)
            .map(|i| format!("member-{i}").into_bytes())
            .collect();
        let filter = Bloom::build(&keys, BITS_PER_KEY);
        let probes = 200_000u32;
        let hits = (0..probes)
            .filter(|i| filter.contains(format!("stranger-{i}").as_bytes()))
            .count();
        let rate = hits as f64 / f64::from(probes);
        assert!(rate < 0.02, "false-positive rate {rate}");
        assert!(
            rate > 0.002,
            "false-positive rate {rate} is too good to be a filter"
        );
    }
}
