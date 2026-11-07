//! The C2SP checkpoint (<https://c2sp.org/tlog-checkpoint>) as a signed note
//! (<https://c2sp.org/signed-note>), and the witness cosignature
//! (<https://c2sp.org/tlog-cosignature>) over it.
//!
//! A checkpoint is three lines, the origin, the decimal size and the base64
//! root, plus optional extension lines. A note is that text, a blank line
//! and signature lines `— <name> base64(key id || signature)`. A log signs
//! the note text with Ed25519 under key id
//! `SHA-256(name || 0x0A || 0x01 || public key)[:4]`; a witness cosigns the
//! lines `cosignature/v1`, `time <posix seconds>` and the checkpoint lines,
//! under key id `SHA-256(name || 0x0A || 0x04 || public key)[:4]`, with the
//! eight-byte big-endian timestamp between key id and signature.

use std::path::Path;

use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::{
    Signature as EdSignature, Signer as _, SigningKey, Verifier as _, VerifyingKey,
};
use sha2::{Digest, Sha256};

use crate::base64;
use crate::error::{Error, Result};
use crate::tree::Hash;

/// The signature type byte of an Ed25519 log signature.
pub const ED25519_LOG: u8 = 0x01;
/// The signature type byte of an Ed25519 witness cosignature.
pub const ED25519_COSIGNATURE: u8 = 0x04;

/// The key id of a key under a name: the first four bytes of
/// `SHA-256(name || 0x0A || type || public key)`.
#[must_use]
pub fn key_id(name: &str, kind: u8, public_key: &[u8]) -> [u8; 4] {
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update([0x0A, kind]);
    hasher.update(public_key);
    let digest = hasher.finalize();
    [digest[0], digest[1], digest[2], digest[3]]
}

/// A key name: non-empty, no spaces, no `+`.
fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name.chars().any(|c| c.is_whitespace() || c == '+') {
        return Err(Error::Format(format!("key name {name:?}")));
    }
    Ok(())
}

/// What a log publishes: its origin, its size and the root at that size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// The log's origin, a schema-less URL such as `example.com/log`.
    pub origin: String,
    /// Entries the root covers.
    pub size: u64,
    /// The root at that size.
    pub root: Hash,
    /// Further non-empty lines, in order.
    pub extensions: Vec<String>,
}

impl Checkpoint {
    /// A checkpoint over `origin`, `size` and `root`, without extensions.
    #[must_use]
    pub fn new(origin: &str, size: u64, root: Hash) -> Self {
        Self {
            origin: origin.to_string(),
            size,
            root,
            extensions: Vec::new(),
        }
    }

    /// The note text: origin, size, base64 root and extension lines, each
    /// newline-terminated.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = format!(
            "{}\n{}\n{}\n",
            self.origin,
            self.size,
            base64::encode(&self.root)
        );
        for line in &self.extensions {
            text.push_str(line);
            text.push('\n');
        }
        text
    }

    /// The checkpoint a note text holds.
    pub fn parse(text: &str) -> Result<Self> {
        let body = text
            .strip_suffix('\n')
            .ok_or_else(|| Error::Format("checkpoint does not end in a newline".to_string()))?;
        let mut lines = body.split('\n');
        let origin = lines
            .next()
            .filter(|l| !l.is_empty())
            .ok_or_else(|| Error::Format("checkpoint has no origin".to_string()))?;
        let size = lines
            .next()
            .ok_or_else(|| Error::Format("checkpoint has no size".to_string()))?;
        if size != "0" && (size.starts_with('0') || size.is_empty()) {
            return Err(Error::Format(format!("size {size:?} is not canonical")));
        }
        let size: u64 = size
            .parse()
            .map_err(|_| Error::Format(format!("size {size:?} is not a decimal")))?;
        let root = lines
            .next()
            .and_then(base64::decode)
            .filter(|r| r.len() == 32)
            .ok_or_else(|| Error::Format("root is not a base64 32-byte hash".to_string()))?;
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&root);
        let extensions: Vec<String> = lines.map(str::to_string).collect();
        if extensions.iter().any(String::is_empty) {
            return Err(Error::Format("an extension line is empty".to_string()));
        }
        Ok(Self {
            origin: origin.to_string(),
            size,
            root: hash,
            extensions,
        })
    }
}

/// One signature line of a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSignature {
    /// The key name on the line.
    pub name: String,
    /// The first four bytes of the decoded payload.
    pub key_id: [u8; 4],
    /// The rest of the payload: the signature, or for a cosignature the
    /// timestamp and the signature.
    pub payload: Vec<u8>,
}

/// A note: text, a blank line, signature lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The signed text, newline-terminated.
    pub text: String,
    /// The signature lines, in order.
    pub signatures: Vec<NoteSignature>,
}

const DASH: &str = "\u{2014} ";

impl Note {
    /// The note as text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = self.text.clone();
        out.push('\n');
        for sig in &self.signatures {
            let mut payload = sig.key_id.to_vec();
            payload.extend_from_slice(&sig.payload);
            out.push_str(DASH);
            out.push_str(&sig.name);
            out.push(' ');
            out.push_str(&base64::encode(&payload));
            out.push('\n');
        }
        out
    }

    /// The note `text` spells: the text up to the last blank line, then one
    /// signature per line. Control characters other than newline, a text
    /// without a trailing newline, and a malformed signature line are refused.
    pub fn parse(text: &str) -> Result<Self> {
        if text.chars().any(|c| c.is_control() && c != '\n') {
            return Err(Error::Format("note holds a control character".to_string()));
        }
        let split = text
            .rfind("\n\n")
            .ok_or_else(|| Error::Format("note has no blank line".to_string()))?;
        let body = &text[..=split];
        let rest = &text[split + 2..];
        if body.len() < 2 {
            return Err(Error::Format("note has no text".to_string()));
        }
        let mut signatures = Vec::new();
        for line in rest.split_terminator('\n') {
            let line = line
                .strip_prefix(DASH)
                .ok_or_else(|| Error::Format(format!("signature line {line:?}")))?;
            let (name, encoded) = line
                .split_once(' ')
                .ok_or_else(|| Error::Format(format!("signature line {line:?}")))?;
            check_name(name)?;
            let payload = base64::decode(encoded)
                .filter(|p| p.len() > 4)
                .ok_or_else(|| Error::Format(format!("signature payload for {name}")))?;
            let mut key_id = [0u8; 4];
            key_id.copy_from_slice(&payload[..4]);
            signatures.push(NoteSignature {
                name: name.to_string(),
                key_id,
                payload: payload[4..].to_vec(),
            });
        }
        if signatures.is_empty() {
            return Err(Error::Format("note has no signature".to_string()));
        }
        if !rest.ends_with('\n') {
            return Err(Error::Format("note does not end in a newline".to_string()));
        }
        Ok(Self {
            text: body.to_string(),
            signatures,
        })
    }
}

/// A log's signing key under its name (the origin, by the checkpoint spec).
pub struct Signer {
    name: String,
    key: SigningKey,
    key_id: [u8; 4],
}

impl Signer {
    /// A signer from a key; `name` is checked as a key name.
    pub fn new(name: &str, key: SigningKey) -> Result<Self> {
        check_name(name)?;
        let key_id = key_id(name, ED25519_LOG, key.verifying_key().as_bytes());
        Ok(Self {
            name: name.to_string(),
            key,
            key_id,
        })
    }

    /// A signer from a PKCS#8 PEM file (`-----BEGIN PRIVATE KEY-----`).
    pub fn from_pem_file(name: &str, path: &Path) -> Result<Self> {
        let pem = std::fs::read_to_string(path)?;
        Self::from_pem(name, &pem)
    }

    /// A signer from PKCS#8 PEM text.
    pub fn from_pem(name: &str, pem: &str) -> Result<Self> {
        let key = SigningKey::from_pkcs8_pem(pem)
            .map_err(|e| Error::Format(format!("signing key: {e}")))?;
        Self::new(name, key)
    }

    /// The key name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The verifier for this key.
    #[must_use]
    pub fn verifier(&self) -> Verifier {
        Verifier {
            name: self.name.clone(),
            kind: ED25519_LOG,
            key: self.key.verifying_key(),
            key_id: self.key_id,
        }
    }

    /// The note over `checkpoint` with this key's signature line.
    #[must_use]
    pub fn sign(&self, checkpoint: &Checkpoint) -> Note {
        let text = checkpoint.to_text();
        let signature = self.key.sign(text.as_bytes());
        Note {
            text,
            signatures: vec![NoteSignature {
                name: self.name.clone(),
                key_id: self.key_id,
                payload: signature.to_bytes().to_vec(),
            }],
        }
    }
}

/// A public key under a name, with the id a verifier matches lines on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verifier {
    name: String,
    kind: u8,
    key: VerifyingKey,
    key_id: [u8; 4],
}

impl Verifier {
    /// A verifier for `kind` (`ED25519_LOG` or `ED25519_COSIGNATURE`) under
    /// `name`.
    pub fn new(name: &str, kind: u8, key: VerifyingKey) -> Result<Self> {
        check_name(name)?;
        if kind != ED25519_LOG && kind != ED25519_COSIGNATURE {
            return Err(Error::Format(format!("signature type {kind:#04x}")));
        }
        Ok(Self {
            name: name.to_string(),
            kind,
            key_id: key_id(name, kind, key.as_bytes()),
            key,
        })
    }

    /// A verifier from the signed-note key text
    /// `<name>+<hex key id>+base64(<type> || <public key>)`.
    pub fn parse(text: &str) -> Result<Self> {
        let mut parts = text.splitn(3, '+');
        let (Some(name), Some(id_hex), Some(encoded)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(Error::Format(format!("verifier key {text:?}")));
        };
        let bytes = base64::decode(encoded)
            .filter(|b| b.len() == 33)
            .ok_or_else(|| Error::Format(format!("verifier key {text:?}")))?;
        let key = VerifyingKey::from_bytes(bytes[1..].try_into().expect("32 bytes"))
            .map_err(|e| Error::Format(format!("verifier key: {e}")))?;
        let verifier = Self::new(name, bytes[0], key)?;
        if id_hex.len() != 8 || rfc9162::hex(&verifier.key_id) != id_hex.to_ascii_lowercase() {
            return Err(Error::Format(format!(
                "verifier key id {id_hex} is not the key's {}",
                rfc9162::hex(&verifier.key_id)
            )));
        }
        Ok(verifier)
    }

    /// The signed-note key text of this verifier.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut bytes = vec![self.kind];
        bytes.extend_from_slice(self.key.as_bytes());
        format!(
            "{}+{}+{}",
            self.name,
            rfc9162::hex(&self.key_id),
            base64::encode(&bytes)
        )
    }

    /// The key name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The key id.
    #[must_use]
    pub fn key_id(&self) -> [u8; 4] {
        self.key_id
    }

    /// The signature type byte.
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.kind
    }

    fn matches(&self, sig: &NoteSignature) -> bool {
        sig.name == self.name && sig.key_id == self.key_id
    }

    fn check(&self, message: &[u8], signature: &[u8]) -> Result<()> {
        let signature = EdSignature::from_slice(signature)
            .map_err(|_| Error::Signature(format!("{}: not an Ed25519 signature", self.name)))?;
        self.key
            .verify(message, &signature)
            .map_err(|_| Error::Signature(format!("{}: signature does not verify", self.name)))
    }
}

/// Checks a note's log signatures against `trusted` log keys: a line whose
/// name and key id match a trusted key has to verify, lines from unknown
/// keys are ignored, and at least one trusted key has to have signed.
/// Returns the trusted keys that verified, in note order.
pub fn verify<'a>(note: &Note, trusted: &'a [Verifier]) -> Result<Vec<&'a Verifier>> {
    let mut verified = Vec::new();
    for sig in &note.signatures {
        for key in trusted
            .iter()
            .filter(|k| k.kind == ED25519_LOG && k.matches(sig))
        {
            key.check(note.text.as_bytes(), &sig.payload)?;
            verified.push(key);
        }
    }
    if verified.is_empty() {
        return Err(Error::Signature(
            "no trusted key signed the note".to_string(),
        ));
    }
    Ok(verified)
}

/// A witness cosignature on a checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cosignature {
    /// The witness key name.
    pub name: String,
    /// Posix seconds the witness signed at.
    pub timestamp: u64,
}

/// The message a cosignature/v1 signs over the checkpoint text at `timestamp`.
#[must_use]
pub fn cosignature_message(checkpoint_text: &str, timestamp: u64) -> Vec<u8> {
    format!("cosignature/v1\ntime {timestamp}\n{checkpoint_text}").into_bytes()
}

/// A cosignature line's payload, after the key id: the eight-byte big-endian
/// timestamp and the Ed25519 signature.
#[must_use]
pub fn cosignature_payload(timestamp: u64, signature: &[u8]) -> Vec<u8> {
    let mut payload = timestamp.to_be_bytes().to_vec();
    payload.extend_from_slice(signature);
    payload
}

/// Checks the cosignature/v1 lines of `note` against `witnesses`: a line whose
/// name and key id match a witness has to verify over `cosignature/v1`,
/// `time <t>` and the note text. Returns each verified witness with its
/// timestamp, in note order; lines from unknown keys are ignored. A trusted
/// witness that signed nothing is not an error; a witness line that does not
/// verify is.
pub fn verify_cosignatures(note: &Note, witnesses: &[Verifier]) -> Result<Vec<Cosignature>> {
    let mut verified = Vec::new();
    for sig in &note.signatures {
        for witness in witnesses
            .iter()
            .filter(|w| w.kind == ED25519_COSIGNATURE && w.matches(sig))
        {
            let (stamp, signature) = sig.payload.split_at_checked(8).ok_or_else(|| {
                Error::Signature(format!("{}: cosignature payload is short", witness.name))
            })?;
            let timestamp = u64::from_be_bytes(stamp.try_into().expect("eight bytes"));
            witness.check(&cosignature_message(&note.text, timestamp), signature)?;
            verified.push(Cosignature {
                name: witness.name.clone(),
                timestamp,
            });
        }
    }
    Ok(verified)
}

/// A witness key that cosigns checkpoints; the counterpart of
/// [`verify_cosignatures`], for logs that also act as their own witness and
/// for tests.
pub struct Cosigner {
    name: String,
    key: SigningKey,
    key_id: [u8; 4],
}

impl Cosigner {
    /// A cosigner from a key under `name`.
    pub fn new(name: &str, key: SigningKey) -> Result<Self> {
        check_name(name)?;
        let key_id = key_id(name, ED25519_COSIGNATURE, key.verifying_key().as_bytes());
        Ok(Self {
            name: name.to_string(),
            key,
            key_id,
        })
    }

    /// The verifier for this witness key.
    #[must_use]
    pub fn verifier(&self) -> Verifier {
        Verifier {
            name: self.name.clone(),
            kind: ED25519_COSIGNATURE,
            key: self.key.verifying_key(),
            key_id: self.key_id,
        }
    }

    /// Adds this witness's cosignature line at `timestamp` to `note`.
    pub fn cosign(&self, note: &mut Note, timestamp: u64) {
        let signature = self.key.sign(&cosignature_message(&note.text, timestamp));
        note.signatures.push(NoteSignature {
            name: self.name.clone(),
            key_id: self.key_id,
            payload: cosignature_payload(timestamp, &signature.to_bytes()),
        });
    }
}
