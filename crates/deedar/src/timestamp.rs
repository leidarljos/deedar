//! RFC 3161 TimeStampReq over the on-disk evidence file.

use std::io::{self, Read};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// How long a seat waits on a hook before a quiet `timestamp` counts as stuck.
///
/// RFC 3161 sets no deadline. The command runs in the foreground, and about
/// ten seconds of silence is when a person stops waiting and assumes it has
/// hung. A live authority finishes a handshake and one short DER reply well
/// inside that, so both the connect and the read use this bound.
const FOREGROUND_DEADLINE: Duration = Duration::from_secs(10);

/// Connect deadline for `DEEDAR_TSA`.
///
/// A handshake that has not finished within [`FOREGROUND_DEADLINE`] is not
/// going to: the peer is not answering. The seat still gets the `.tsq`.
pub(crate) const CONNECT_TIMEOUT: Duration = FOREGROUND_DEADLINE;

/// Per-read deadline for the timestamp reply.
///
/// ureq's default agent never times out a read, so an authority that accepts
/// and then sends nothing blocks the command for good. The reply is one short
/// DER body, so the same [`FOREGROUND_DEADLINE`] covers a slow signer and still
/// returns while the seat is there.
pub(crate) const READ_TIMEOUT: Duration = FOREGROUND_DEADLINE;

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
    // X.690: a length under 128 is one short-form byte. A longer length sets
    // the high bit and then counts the length octets that follow.
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

pub(crate) enum QueryError {
    /// The authority did not answer within `timeout`.
    TimedOut {
        timeout: Duration,
    },
    Other(String),
}

pub(crate) fn post_query(url: &str, req: &[u8]) -> Result<Vec<u8>, QueryError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .build();
    let resp = agent
        .post(url)
        .set("Content-Type", "application/timestamp-query")
        .set("Accept", "application/timestamp-reply")
        .send_bytes(req)
        .map_err(query_error)?;
    let mut body = Vec::new();
    resp.into_reader()
        .read_to_end(&mut body)
        .map_err(read_error)?;
    Ok(body)
}

fn query_error(err: ureq::Error) -> QueryError {
    let timeout = match err.kind() {
        // Connect gives up before any reply byte. A later stall is a read.
        ureq::ErrorKind::ConnectionFailed if io_timed_out(&err) => Some(CONNECT_TIMEOUT),
        ureq::ErrorKind::Io if io_timed_out(&err) => Some(READ_TIMEOUT),
        _ => None,
    };
    match timeout {
        Some(timeout) => QueryError::TimedOut { timeout },
        None => QueryError::Other(err.to_string()),
    }
}

fn read_error(err: io::Error) -> QueryError {
    if err.kind() == io::ErrorKind::TimedOut {
        QueryError::TimedOut {
            timeout: READ_TIMEOUT,
        }
    } else {
        QueryError::Other(err.to_string())
    }
}

fn io_timed_out(err: &ureq::Error) -> bool {
    let mut source = std::error::Error::source(err);
    while let Some(cur) = source {
        if let Some(io_err) = cur.downcast_ref::<io::Error>() {
            return io_err.kind() == io::ErrorKind::TimedOut;
        }
        source = std::error::Error::source(cur);
    }
    false
}
