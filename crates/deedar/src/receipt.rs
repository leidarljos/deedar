//! Checking a deed somebody else handed over.
//!
//! A satchel arrives with deed bytes, a manifest and a signature. Between
//! them those say the payload is intact and that a key the receiver accepts
//! stood behind the manifest. Neither says when the deed came into being. A
//! sender who mints a deed the morning they are asked for it produces bytes
//! that are just as intact and a signature that is just as good.
//!
//! The answer the transparency logs settled on (RFC 9162,
//! doi:10.17487/RFC9162, which obsoletes RFC 6962) is that a deed travels with
//! the path from its leaf to
//! a tree head, and the head is a short thing a receiver can write down. A
//! sender who wants to slip in a deed after the fact has to produce a head
//! that covers it, and that head will not be the one the receiver already
//! holds.
//!
//! So this module is the receiving half: it reads what an export wrote, and
//! it either checks out or it fails. There is no middle report, because a
//! provenance claim that is reported as a note is one nobody acts on.
//!
//! Two records travel. A [`Receipt`] is one deed against one head, which is
//! the RFC's audit path. A [`Bridge`] is one head against an earlier one from
//! the same log, which is the RFC's consistency proof and the only thing that
//! catches a sender who rewrote history between two handovers: the receipt of
//! the second satchel is perfectly good against its own head, and only the
//! bridge to the head the receiver kept from the first says whether that head
//! is the same log grown or a different log entirely.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use deed::{Error, Result};

use crate::attest::ED25519;
use crate::log::{self, Head};

/// One deed's place in a log, as an export writes it beside the bytes.
///
/// The digest and the time are here because the leaf hashes over the whole
/// log entry, and a receiver holding only the deed cannot rebuild a leaf from
/// a field the record left out. A path that cannot be walked back to a leaf is
/// not a weaker proof, it is not a proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// The deed's accession.
    pub id: String,
    /// `sha256:` of the canonical deed bytes, as the log entry recorded it.
    pub digest: String,
    /// The entry's time, which is the third field the leaf hashes over.
    pub unix_time: u64,
    /// Where in the log the entry sits.
    pub index: usize,
    /// How many entries the head covers.
    pub size: usize,
    /// The head's root, hex.
    pub root: String,
    /// Leaf to root, one sibling a level.
    pub path: Vec<[u8; 32]>,
}

impl Receipt {
    /// The file an export writes and a check reads.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!(
            "id={}\ndigest={}\ntime={}\nindex={}\nsize={}\nroot={}\n",
            self.id, self.digest, self.unix_time, self.index, self.size, self.root
        );
        for step in &self.path {
            out.push_str(&format!("path={}\n", log::hex(step)));
        }
        out
    }

    /// Read one back.
    ///
    /// # Errors
    ///
    /// Fails when a field is absent or is not what it has to be. A receipt
    /// missing a field is refused rather than checked against a default,
    /// because every default here is a value that makes some other proof
    /// verify.
    pub fn parse(text: &str) -> Result<Self> {
        let mut id = None;
        let mut digest = None;
        let mut unix_time = None;
        let mut index = None;
        let mut size = None;
        let mut root = None;
        let mut path = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let Some((key, value)) = line.split_once('=') else {
                return Err(Error::Evidence(format!("receipt: not a field: {line:?}")));
            };
            match key {
                "id" => id = Some(value.to_string()),
                "digest" => digest = Some(value.to_string()),
                "time" => unix_time = Some(number(value, "time")?),
                "index" => index = Some(number(value, "index")? as usize),
                "size" => size = Some(number(value, "size")? as usize),
                "root" => root = Some(value.to_string()),
                "path" => path.push(
                    log::from_hex(value)
                        .ok_or_else(|| Error::Evidence(format!("receipt: path {value:?}")))?,
                ),
                other => return Err(Error::Evidence(format!("receipt: field {other:?}"))),
            }
        }
        let (Some(id), Some(digest), Some(unix_time), Some(index), Some(size), Some(root)) =
            (id, digest, unix_time, index, size, root)
        else {
            return Err(Error::Evidence(
                "receipt: a field is missing, and every one of them is needed to rebuild the leaf"
                    .into(),
            ));
        };
        Ok(Self {
            id,
            digest,
            unix_time,
            index,
            size,
            root,
            path,
        })
    }

    /// The head this receipt is against, which is what a receiver records.
    #[must_use]
    pub fn head(&self) -> Head {
        Head {
            size: self.size,
            root: self.root.clone(),
        }
    }

    /// Whether these bytes are the deed this receipt is about, and whether the
    /// log this receipt names holds it.
    ///
    /// Both halves are needed and neither implies the other. The digest ties
    /// the bytes to the entry; the path ties the entry to the head. Checking
    /// only the first accepts a deed the log never held, and checking only the
    /// second accepts any bytes at all under a logged accession.
    ///
    /// # Errors
    ///
    /// Fails when the bytes do not hash to the digest, or the path does not
    /// carry the leaf to the root.
    pub fn check(&self, deed_bytes: &[u8]) -> Result<()> {
        let got = format!("sha256:{}", log::hex(&Sha256::digest(deed_bytes)));
        if got != self.digest {
            return Err(Error::Evidence(format!(
                "{}: the bytes handed over hash to {got} and the log entry says {}",
                self.id, self.digest
            )));
        }
        let entry = log::Entry {
            id: self.id.clone(),
            digest: self.digest.clone(),
            unix_time: self.unix_time,
        };
        let leaf = log::leaf_hash(&entry.material());
        if !log::verify_inclusion(&leaf, self.index, self.size, &self.path, &self.root) {
            return Err(Error::Evidence(format!(
                "{}: the path does not carry this entry to the head it names ({} entries, {})",
                self.id, self.size, self.root
            )));
        }
        Ok(())
    }
}

/// A head with a signature over it.
///
/// The head is the one thing a receiver keeps between handovers, and until it
/// is signed it is two numbers that arrived in the same bag as the deeds they
/// are supposed to vouch for. A sender who wants to show two readers different
/// histories has nothing to forge: they write the size and root they like.
///
/// RFC 9162 makes the signed tree head the object rather than the pair, and
/// this is that: the same size and root, over a domain-separated message, by a
/// key a reader either accepts or does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedHead {
    /// What is being vouched for.
    pub head: Head,
    /// The verifying key, as 32 bytes.
    pub signer: [u8; 32],
    /// The signature over the covered bytes.
    pub signature: [u8; 64],
}

/// Domain separation, so a signature over a head cannot be replayed as a
/// signature over a manifest or a deed.
const HEAD_DOMAIN: &[u8] = b"deedar-head-v1";

/// The bytes a head signature actually covers.
fn head_message(head: &Head) -> Vec<u8> {
    let mut message = Vec::from(HEAD_DOMAIN);
    message.push(0);
    message.extend_from_slice(format!("{} {}", head.size, head.root).as_bytes());
    message
}

impl SignedHead {
    /// The file a handover carries.
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "size={} root={} {ED25519} {} {}
",
            self.head.size,
            self.head.root,
            log::hex(&self.signer),
            log::hex(&self.signature)
        )
    }

    /// Read one back.
    ///
    /// # Errors
    ///
    /// Fails when the line is not a size, a root, this scheme, a 32 byte key
    /// and a 64 byte signature.
    pub fn parse(text: &str) -> Result<Self> {
        let mut fields = text.split_whitespace();
        let (Some(size), Some(root), Some(scheme), Some(key), Some(sig), None) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            return Err(Error::Evidence("head: not a signed head line".into()));
        };
        if scheme != ED25519 {
            return Err(Error::Evidence(format!("head: scheme {scheme:?}")));
        }
        let size = size
            .strip_prefix("size=")
            .ok_or_else(|| Error::Evidence("head: no size".into()))?;
        let root = root
            .strip_prefix("root=")
            .ok_or_else(|| Error::Evidence("head: no root".into()))?;
        let signer =
            crate::attest::from_hex(key).ok_or_else(|| Error::Evidence("head: key".into()))?;
        let mut signature = [0u8; 64];
        if sig.len() != 128 {
            return Err(Error::Evidence("head: signature length".into()));
        }
        for (slot, pair) in signature.iter_mut().zip(sig.as_bytes().chunks_exact(2)) {
            let pair =
                std::str::from_utf8(pair).map_err(|_| Error::Evidence("head: hex".into()))?;
            *slot =
                u8::from_str_radix(pair, 16).map_err(|_| Error::Evidence("head: hex".into()))?;
        }
        Ok(Self {
            head: Head {
                size: number(size, "size")? as usize,
                root: root.to_string(),
            },
            signer,
            signature,
        })
    }

    /// Whether the signature covers this head, and whether the key is one the
    /// reader accepts.
    ///
    /// The two are reported apart, the same way a manifest signature is. A
    /// signature that verifies against the key it names says the head and the
    /// signature go together and nothing about who made them, and a reader who
    /// gave no signer list has to be told that is what they got.
    ///
    /// # Errors
    ///
    /// Fails when the signature does not cover the head, or names a key the
    /// reader does not accept.
    pub fn check(&self, accept: &BTreeSet<[u8; 32]>) -> Result<bool> {
        let key = ed25519_dalek::VerifyingKey::from_bytes(&self.signer)
            .map_err(|_| Error::Evidence("head: not a verifying key".into()))?;
        ed25519_dalek::Verifier::verify(
            &key,
            &head_message(&self.head),
            &ed25519_dalek::Signature::from_bytes(&self.signature),
        )
        .map_err(|_| Error::Evidence("head: the signature does not cover this head".into()))?;
        if accept.is_empty() {
            return Ok(false);
        }
        if !accept.contains(&self.signer) {
            return Err(Error::Evidence(format!(
                "head: signed by {}, which is not a key this reader accepts",
                log::hex(&self.signer)
            )));
        }
        Ok(true)
    }
}

/// Sign a head with this host's key.
///
/// # Errors
///
/// Fails when no signing key is configured.
pub fn sign_head(head: &Head) -> Result<SignedHead> {
    let key: ed25519_dalek::SigningKey = crate::host::load_signing_key()?.ok_or_else(|| {
        Error::Evidence(
            "no signing key: set DEEDAR_HOST_SIGNING_KEY to a 32 byte seed outside the store"
                .into(),
        )
    })?;
    let signature = ed25519_dalek::Signer::sign(&key, &head_message(head));
    Ok(SignedHead {
        head: head.clone(),
        signer: key.verifying_key().to_bytes(),
        signature: signature.to_bytes(),
    })
}

/// One head against an earlier one from the same log.
///
/// The receipt says a deed is in the tree the sender is showing. It says
/// nothing about whether that tree is the one the sender showed last time,
/// which is the question that catches a log rewritten between handovers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bridge {
    /// The head the receiver already holds.
    pub from: Head,
    /// The head the sender is showing now.
    pub to: Head,
    /// The RFC's consistency path between them.
    pub path: Vec<[u8; 32]>,
}

impl Bridge {
    /// The file a sender writes for a receiver at a known size.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!(
            "from={} {}\nto={} {}\n",
            self.from.size, self.from.root, self.to.size, self.to.root
        );
        for step in &self.path {
            out.push_str(&format!("path={}\n", log::hex(step)));
        }
        out
    }

    /// Read one back.
    ///
    /// # Errors
    ///
    /// Fails when either head is not a size and a root, or a step is not hex.
    pub fn parse(text: &str) -> Result<Self> {
        let mut from = None;
        let mut to = None;
        let mut path = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let Some((key, value)) = line.split_once('=') else {
                return Err(Error::Evidence(format!("bridge: not a field: {line:?}")));
            };
            match key {
                "from" => from = Some(head(value)?),
                "to" => to = Some(head(value)?),
                "path" => path.push(
                    log::from_hex(value)
                        .ok_or_else(|| Error::Evidence(format!("bridge: path {value:?}")))?,
                ),
                other => return Err(Error::Evidence(format!("bridge: field {other:?}"))),
            }
        }
        let (Some(from), Some(to)) = (from, to) else {
            return Err(Error::Evidence("bridge: needs both heads".into()));
        };
        Ok(Self { from, to, path })
    }

    /// Whether the newer head is the older one grown rather than replaced.
    ///
    /// # Errors
    ///
    /// Fails when the path does not join the two heads, which is what a log
    /// that dropped or rewrote an entry looks like from here.
    pub fn check(&self) -> Result<()> {
        if log::verify_consistency(
            self.from.size,
            &self.from.root,
            self.to.size,
            &self.to.root,
            &self.path,
        ) {
            return Ok(());
        }
        Err(Error::Evidence(format!(
            "the log at {} entries is not the log at {} entries grown: something was dropped or \
             rewritten between the two handovers",
            self.to.size, self.from.size
        )))
    }
}

/// What checking a whole handover established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handover {
    /// The accessions whose bytes and paths both check out, in id order.
    pub proven: Vec<String>,
    /// The head every one of them was against, which is what to record for
    /// next time. Absent only when the handover carried no deeds.
    pub head: Option<Head>,
    /// Who signed that head, when the bag carried a signature over it.
    pub head_signer: Option<[u8; 32]>,
    /// Whether that signer was on the reader's list, as opposed to merely
    /// being the one the file named.
    pub head_accepted: bool,
}

impl Handover {
    /// One line a person reads, then what it does not say.
    #[must_use]
    pub fn render(&self) -> String {
        let Some(head) = &self.head else {
            return "no deeds travelled with this satchel\n".to_string();
        };
        let mut out = format!(
            "{} deeds proven against a log of {} entries, root {}\n",
            self.proven.len(),
            head.size,
            head.root
        );
        match self.head_signer {
            Some(signer) if self.head_accepted => {
                out.push_str(&format!(
                    "head signed by {} (accepted)\n",
                    log::hex(&signer)
                ));
            }
            Some(signer) => out.push_str(&format!(
                "head signed by {}, and no signer list was given, so this says the head and the \
                 signature go together and nothing about who made them\n",
                log::hex(&signer)
            )),
            // Worth saying rather than leaving as an absence. Every proof here
            // is against this head, so a head nobody vouched for is a bag that
            // is internally consistent and unattributed.
            None => out.push_str(
                "the head is unsigned, so every proof above is against a head this bag asserted \
                 about itself\n",
            ),
        }
        out
    }
}

/// The file a handover carries its signed head in.
#[must_use]
pub fn head_beside(deeds: &Path) -> PathBuf {
    deeds.join("head")
}

/// Check every deed a handover carried.
///
/// Takes either the directory the deeds were exported into or the satchel
/// around it, because a receiver is handed a bag and should not have to know
/// which directory inside it holds the proofs before they can check them.
///
/// Every receipt has to name the same head. One export walks one log, so two
/// heads in one bag means the deeds came from two moments and the receiver has
/// no single thing to write down; taking the largest would let a sender
/// smuggle a deed under a head nobody recorded.
///
/// # Errors
///
/// Fails when a deed carries no receipt, when bytes and digest disagree, when
/// a path does not reach the head, or when the receipts disagree about which
/// head they are against.
pub fn check_handover(dir: &Path, accept: &BTreeSet<[u8; 32]>) -> Result<Handover> {
    let root = deeds_in(dir).ok_or_else(|| {
        Error::Evidence(format!(
            "{}: no deeds here and none under data/deeds",
            dir.display()
        ))
    })?;

    let mut folders: Vec<PathBuf> = std::fs::read_dir(&root)
        .map_err(|e| Error::Io(e.to_string()))?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();

    let mut proven = Vec::new();
    let mut head: Option<Head> = None;
    let mut wrong = Vec::new();
    for folder in folders {
        let name = folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(text) = std::fs::read_to_string(folder.join("proof.txt")) else {
            // A deed with no receipt is the case this whole module exists for:
            // bytes that arrived with nothing saying they predate the asking.
            wrong.push(format!(
                "{name}: no proof.txt, so nothing says this deed was logged before it was handed \
                 over"
            ));
            continue;
        };
        let receipt = match Receipt::parse(&text) {
            Ok(receipt) => receipt,
            Err(e) => {
                wrong.push(format!("{name}: {}", said(&e)));
                continue;
            }
        };
        let Ok(bytes) = std::fs::read(folder.join("deed.bin")) else {
            wrong.push(format!("{name}: a receipt with no deed.bin beside it"));
            continue;
        };
        if let Err(e) = receipt.check(&bytes) {
            wrong.push(said(&e));
            continue;
        }
        match &head {
            None => head = Some(receipt.head()),
            Some(held) if *held != receipt.head() => {
                wrong.push(format!(
                    "{name}: against a log of {} entries where the rest of this satchel is against \
                     {}",
                    receipt.size, held.size
                ));
                continue;
            }
            Some(_) => {}
        }
        proven.push(receipt.id);
    }

    // The signed head, when one travelled. It has to be the head the receipts
    // are against: a bag whose deeds prove membership in one tree and whose
    // signature covers another is a bag where the signature vouches for
    // nothing that arrived.
    let mut head_signer = None;
    let mut head_accepted = false;
    if let Ok(text) = std::fs::read_to_string(head_beside(&root)) {
        match SignedHead::parse(&text).and_then(|signed| {
            let accepted = signed.check(accept)?;
            Ok((signed, accepted))
        }) {
            Ok((signed, accepted)) => {
                if head.as_ref().is_some_and(|held| *held != signed.head) {
                    wrong.push(format!(
                        "the signed head covers a log of {} entries and the deeds prove membership \
                         in one of {}",
                        signed.head.size,
                        head.as_ref().map_or(0, |held| held.size)
                    ));
                }
                head_signer = Some(signed.signer);
                head_accepted = accepted;
            }
            Err(e) => wrong.push(said(&e)),
        }
    }

    if wrong.is_empty() {
        proven.sort();
        Ok(Handover {
            proven,
            head,
            head_signer,
            head_accepted,
        })
    } else {
        Err(Error::Evidence(format!(
            "this handover does not check out:\n{}",
            wrong.join("\n")
        )))
    }
}

/// One complaint's text, without the prefix Display would add again.
///
/// Every entry in the wrong-list is joined under one "does not check out"
/// heading that already says what kind of failure this is, so letting each
/// inner error render its own `evidence:` produces `evidence: evidence:` and
/// makes the line harder to read than the fault it is reporting.
fn said(e: &Error) -> String {
    match e {
        Error::Evidence(text) | Error::Io(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Where the deeds are, given either the bag or the directory inside it.
fn deeds_in(dir: &Path) -> Option<PathBuf> {
    let nested = dir.join("data").join("deeds");
    if nested.is_dir() {
        return Some(nested);
    }
    dir.is_dir().then(|| dir.to_path_buf())
}

fn head(value: &str) -> Result<Head> {
    let Some((size, root)) = value.split_once(' ') else {
        return Err(Error::Evidence(format!("not a head: {value:?}")));
    };
    Ok(Head {
        size: number(size, "size")? as usize,
        root: root.to_string(),
    })
}

fn number(value: &str, what: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| Error::Evidence(format!("{what}: not a number: {value:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(n: usize) -> Vec<log::Entry> {
        (0..n)
            .map(|i| log::Entry {
                id: format!("deed-thing-{i}"),
                digest: format!("sha256:{}", log::hex(&Sha256::digest(body(i)))),
                unix_time: 1_700_000_000 + i as u64,
            })
            .collect()
    }

    fn body(i: usize) -> Vec<u8> {
        format!("the bytes of deed {i}").into_bytes()
    }

    fn receipt_for(all: &[log::Entry], at: usize) -> Receipt {
        let leaves: Vec<[u8; 32]> = all.iter().map(|e| log::leaf_hash(&e.material())).collect();
        let head = Head::over(&leaves);
        Receipt {
            id: all[at].id.clone(),
            digest: all[at].digest.clone(),
            unix_time: all[at].unix_time,
            index: at,
            size: head.size,
            root: head.root,
            path: log::inclusion_proof(&leaves, at).expect("a path for an entry in the tree"),
        }
    }

    /// The point of the record: a receiver holding the bytes and the receipt
    /// can rebuild the leaf and walk it to the head, with nothing else.
    #[test]
    fn a_receipt_carries_everything_the_leaf_needs() {
        let all = entries(9);
        for at in 0..all.len() {
            let receipt = receipt_for(&all, at);
            // Through the file, because the file is the only thing that
            // travels and a field that renders but does not parse is a field
            // the receiver does not have.
            let read = Receipt::parse(&receipt.render()).expect("round trips");
            assert_eq!(read, receipt);
            read.check(&body(at)).expect("checks out");
        }
    }

    /// Bytes swapped under a logged accession are caught by the digest, and a
    /// path taken from a different entry is caught by the tree.
    #[test]
    fn neither_half_of_the_check_is_redundant() {
        let all = entries(7);
        let receipt = receipt_for(&all, 3);

        let err = receipt.check(&body(4)).expect_err("other bytes passed");
        assert!(format!("{err}").contains("hash to"), "{err}");

        let borrowed = Receipt {
            path: receipt_for(&all, 5).path,
            ..receipt.clone()
        };
        let err = borrowed
            .check(&body(3))
            .expect_err("somebody else's path passed");
        assert!(format!("{err}").contains("does not carry"), "{err}");
    }

    /// A time nobody recorded is a leaf nobody can rebuild, which is why the
    /// record refuses rather than filling one in.
    #[test]
    fn a_receipt_missing_a_field_is_refused() {
        let all = entries(4);
        let full = receipt_for(&all, 1).render();
        let thinned: String = full
            .lines()
            .filter(|line| !line.starts_with("time="))
            .map(|line| format!("{line}\n"))
            .collect();
        let err = Receipt::parse(&thinned).expect_err("a receipt with no time parsed");
        assert!(format!("{err}").contains("rebuild the leaf"), "{err}");
        assert!(Receipt::parse("id").is_err());
        assert!(Receipt::parse("colour=blue\n").is_err());
    }

    /// A grown log bridges to the head a receiver kept; a rewritten one does
    /// not, and that is the only thing that catches it.
    #[test]
    fn a_rewritten_log_does_not_bridge() {
        let all = entries(9);
        let leaves: Vec<[u8; 32]> = all.iter().map(|e| log::leaf_hash(&e.material())).collect();
        let earlier = Head::over(&leaves[..4]);
        let now = Head::over(&leaves);
        let bridge = Bridge {
            from: earlier.clone(),
            to: now.clone(),
            path: log::consistency_proof(&leaves, 4).expect("a path between the two"),
        };
        Bridge::parse(&bridge.render())
            .expect("round trips")
            .check()
            .expect("a grown log bridges");

        // The same sender, having quietly replaced an entry the receiver
        // already saw. Every receipt against the new head is perfectly good.
        let mut rewritten = leaves.clone();
        rewritten[2] = log::leaf_hash(b"a deed nobody was shown");
        let swapped = Bridge {
            from: earlier,
            to: Head::over(&rewritten),
            path: log::consistency_proof(&rewritten, 4).expect("a path"),
        };
        let err = swapped.check().expect_err("a rewrite bridged");
        assert!(format!("{err}").contains("dropped or rewritten"), "{err}");
    }

    /// A head that nobody signed is reported as one nobody signed.
    ///
    /// Every proof in a bag is against its head, so an unsigned head makes the
    /// whole bag internally consistent and unattributed: a sender showing two
    /// readers different histories has nothing to forge. That is worth a line
    /// rather than an absence.
    #[test]
    fn an_unsigned_head_says_so() {
        let done = Handover {
            proven: vec!["deed-file-one".into()],
            head: Some(Head {
                size: 4,
                root: "ab".repeat(32),
            }),
            head_signer: None,
            head_accepted: false,
        };
        let said = done.render();
        assert!(said.contains("the head is unsigned"), "{said}");
        assert!(said.contains("asserted about itself"), "{said}");

        let named = Handover {
            head_signer: Some([7u8; 32]),
            ..done.clone()
        };
        let said = named.render();
        assert!(said.contains("nothing about who made them"), "{said}");

        let accepted = Handover {
            head_accepted: true,
            ..named
        };
        assert!(accepted.render().contains("(accepted)"));
    }

    /// A head signature covers the head and stops covering a different one,
    /// and it is not replayable as a signature over anything else.
    #[test]
    fn a_head_signature_covers_that_head_only() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
        let head = Head {
            size: 9,
            root: "3c".repeat(32),
        };
        let signed = SignedHead {
            head: head.clone(),
            signer: key.verifying_key().to_bytes(),
            signature: ed25519_dalek::Signer::sign(&key, &head_message(&head)).to_bytes(),
        };
        // Through the file, since the file is what travels.
        let read = SignedHead::parse(&signed.render()).expect("round trips");
        assert_eq!(read, signed);
        assert!(!read.check(&BTreeSet::new()).expect("verifies"));
        assert!(read
            .check(&BTreeSet::from([signed.signer]))
            .expect("verifies"));

        // A stranger's list refuses it rather than reporting it.
        let stranger = ed25519_dalek::SigningKey::from_bytes(&[6u8; 32])
            .verifying_key()
            .to_bytes();
        let err = read
            .check(&BTreeSet::from([stranger]))
            .expect_err("a key nobody accepts passed");
        assert!(
            format!("{err}").contains("not a key this reader accepts"),
            "{err}"
        );

        // The same signature over a head one entry longer does not verify.
        let moved = SignedHead {
            head: Head {
                size: 10,
                ..signed.head.clone()
            },
            ..signed.clone()
        };
        let err = moved
            .check(&BTreeSet::new())
            .expect_err("a moved head passed");
        assert!(format!("{err}").contains("does not cover"), "{err}");

        // And it is domain separated from the manifest signature, so one
        // cannot be presented as the other.
        assert_ne!(head_message(&head), crate::vouch::covered_for_test(b"x"));
    }

    /// A handover is checked whole: a deed with no receipt fails it, and so
    /// does one against a head the rest of the bag does not share.
    #[test]
    fn a_handover_is_checked_whole() {
        let all = entries(6);
        let dir = tempfile::tempdir().expect("tempdir");
        let deeds = dir.path().join("data").join("deeds");
        for at in 0..3 {
            let out = deeds.join(&all[at].id);
            std::fs::create_dir_all(&out).expect("dirs");
            std::fs::write(out.join("deed.bin"), body(at)).expect("bytes");
            std::fs::write(out.join("proof.txt"), receipt_for(&all, at).render()).expect("receipt");
        }
        let done = check_handover(dir.path(), &BTreeSet::new()).expect("checks out");
        assert_eq!(done.proven.len(), 3);
        assert_eq!(done.head.as_ref().expect("a head").size, 6);
        // Handed the bag or handed the deeds, the same answer.
        assert_eq!(
            check_handover(&deeds, &BTreeSet::new()).expect("checks out"),
            done
        );

        // A deed that arrived with nothing saying it predates the asking.
        let bare = deeds.join("deed-thing-late");
        std::fs::create_dir_all(&bare).expect("dirs");
        std::fs::write(bare.join("deed.bin"), b"minted this morning").expect("bytes");
        let err = check_handover(dir.path(), &BTreeSet::new()).expect_err("a bare deed passed");
        assert!(format!("{err}").contains("no proof.txt"), "{err}");
        std::fs::remove_dir_all(&bare).expect("clean up");

        // And one whose receipt is against a different moment of the log.
        let odd = &all[4];
        let out = deeds.join(&odd.id);
        std::fs::create_dir_all(&out).expect("dirs");
        std::fs::write(out.join("deed.bin"), body(4)).expect("bytes");
        let leaves: Vec<[u8; 32]> = all[..5]
            .iter()
            .map(|e| log::leaf_hash(&e.material()))
            .collect();
        let short = Head::over(&leaves);
        std::fs::write(
            out.join("proof.txt"),
            Receipt {
                id: odd.id.clone(),
                digest: odd.digest.clone(),
                unix_time: odd.unix_time,
                index: 4,
                size: short.size,
                root: short.root,
                path: log::inclusion_proof(&leaves, 4).expect("a path"),
            }
            .render(),
        )
        .expect("receipt");
        let err = check_handover(dir.path(), &BTreeSet::new()).expect_err("two heads passed");
        assert!(
            format!("{err}").contains("where the rest of this satchel"),
            "{err}"
        );
    }
}
