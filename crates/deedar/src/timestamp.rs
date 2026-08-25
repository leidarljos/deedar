//! RFC 3161 TimeStampReq over the on-disk evidence file.

use std::io::Read;

use sha2::{Digest, Sha256};

/// SHA-256 of the on-disk `{id}.evidence` file bytes.
pub fn imprint_hash(evidence_bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(evidence_bytes).into()
}

/// RFC 3161 TimeStampReq version 1. MessageImprint is SHA-256 of `hashed_message`.
pub fn timestamp_req(hashed_message: &[u8; 32]) -> Vec<u8> {
    // 2.16.840.1.101.3.4.2.1
    const SHA256_OID: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];
    let oid = der_tlv(0x06, SHA256_OID);
    let null = der_tlv(0x05, &[]);
    let alg = der_seq(&[&oid, &null]);
    let hash = der_tlv(0x04, hashed_message);
    let imprint = der_seq(&[&alg, &hash]);
    let version = der_tlv(0x02, &[1]);
    der_seq(&[&version, &imprint])
}

fn der_seq(parts: &[&[u8]]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(part);
    }
    der_tlv(0x30, &body)
}

fn der_tlv(tag: u8, contents: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + contents.len());
    out.push(tag);
    put_len(&mut out, contents.len());
    out.extend_from_slice(contents);
    out
}

fn put_len(out: &mut Vec<u8>, n: usize) {
    if n < 128 {
        out.push(n as u8);
        return;
    }
    let mut bytes = Vec::new();
    let mut rest = n;
    while rest > 0 {
        bytes.push((rest & 0xff) as u8);
        rest >>= 8;
    }
    bytes.reverse();
    out.push(0x80 | bytes.len() as u8);
    out.extend_from_slice(&bytes);
}

pub(crate) fn post_query(url: &str, req: &[u8]) -> deed::Result<Vec<u8>> {
    let resp = ureq::post(url)
        .set("Content-Type", "application/timestamp-query")
        .set("Accept", "application/timestamp-reply")
        .send_bytes(req)
        .map_err(|e| deed::Error::Io(e.to_string()))?;
    let mut body = Vec::new();
    resp.into_reader()
        .read_to_end(&mut body)
        .map_err(|e| deed::Error::Io(e.to_string()))?;
    Ok(body)
}
