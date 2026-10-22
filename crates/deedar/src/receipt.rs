//! Checking a deed somebody else handed over.
//!
//! Intact bytes and a good signature say nothing about when a deed came into
//! being. A deed travels with the path from its leaf to a log head (RFC 9162,
//! doi:10.17487/RFC9162), and the head is what a receiver writes down; a deed
//! minted after the fact needs a head the receiver does not hold.
//!
//! A [`Receipt`] is one deed against one head. A [`Bridge`] is one head
//! against an earlier one from the same log, and is the only thing that
//! catches a log rewritten between two handovers. A check passes or fails;
//! there is no note.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use deed::{Error, Result};

use crate::attest::ED25519;
use crate::log::{self, Head};

/// One deed's place in a log, as an export writes it beside the bytes.
///
/// Carries everything the leaf hashes over, so a receiver holding only the
/// deed can rebuild the leaf.
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
    /// Fails when a field is absent or malformed; no field has a default.
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

    /// Whether these bytes are the deed, and whether the named log holds it.
    /// The digest ties bytes to entry, the path ties entry to head; neither
    /// implies the other.
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

/// A head with a signature over it: RFC 9162's signed tree head. Unsigned, a
/// head is two numbers that arrived in the same bag as the deeds they vouch
/// for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedHead {
    /// What is being vouched for.
    pub head: Head,
    /// Hex SHA-256 of the bag's link sidecars, when this signature covers
    /// them. A log head has none.
    pub links: Option<String>,
    /// The verifying key, as 32 bytes.
    pub signer: [u8; 32],
    /// The signature over the covered bytes.
    pub signature: [u8; 64],
}

/// Domain separation, so a signature over a head cannot be replayed as a
/// signature over a manifest or a deed.
const HEAD_DOMAIN: &[u8] = b"deedar-head-v1";

/// The bytes a head signature actually covers.
///
/// `links` is the hex digest from [`link_commitment`]. A log head has none:
/// it is not a bag. A bag head has one, so the signature covers the
/// `sources`, `supersedes` and `successor` files and not only the log root.
fn head_message(head: &Head, links: Option<&str>) -> Vec<u8> {
    let mut message = Vec::from(HEAD_DOMAIN);
    message.push(0);
    message.extend_from_slice(format!("{} {}", head.size, head.root).as_bytes());
    if let Some(links) = links {
        message.push(0);
        message.extend_from_slice(links.as_bytes());
    }
    message
}

impl SignedHead {
    /// The file a handover carries.
    #[must_use]
    pub fn render(&self) -> String {
        let links = self
            .links
            .as_deref()
            .map(|links| format!(" links={links}"))
            .unwrap_or_default();
        format!(
            "size={} root={}{links} {ED25519} {} {}\n",
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
    /// and a 64 byte signature. `links=<hex>` may sit between the root and
    /// the scheme; that is the bag's link-sidecar digest.
    pub fn parse(text: &str) -> Result<Self> {
        let fields: Vec<&str> = text.split_whitespace().collect();
        let (size, root, links, scheme, key, sig) = match fields.as_slice() {
            [size, root, scheme, key, sig] => (*size, *root, None, *scheme, *key, *sig),
            [size, root, links, scheme, key, sig] => {
                (*size, *root, Some(*links), *scheme, *key, *sig)
            }
            _ => return Err(Error::Evidence("head: not a signed head line".into())),
        };
        if scheme != ED25519 {
            return Err(Error::Evidence(format!("head: scheme {scheme:?}")));
        }
        let links = match links {
            Some(raw) => {
                let hex = raw.strip_prefix("links=").ok_or_else(|| {
                    Error::Evidence("head: expected links= between the root and the scheme".into())
                })?;
                if !crate::fs::is_sha256_hex(hex) {
                    return Err(Error::Evidence("head: links is not a sha256 digest".into()));
                }
                Some(hex.to_string())
            }
            None => None,
        };
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
            links,
            signer,
            signature,
        })
    }

    /// Whether the signature covers this head, and whether the key is one the
    /// reader accepts. Returns `false` when no signer list was given: the head
    /// and signature go together, and that is all it says.
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
            &head_message(&self.head, self.links.as_deref()),
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
    sign_head_covering(head, None)
}

/// Sign a head, optionally covering a bag's [`link_commitment`].
///
/// # Errors
///
/// Fails when no signing key is configured.
pub(crate) fn sign_head_covering(head: &Head, links: Option<&str>) -> Result<SignedHead> {
    let key: ed25519_dalek::SigningKey = crate::host::load_signing_key()?.ok_or_else(|| {
        Error::Evidence(
            "no signing key: set DEEDAR_HOST_SIGNING_KEY to a 32 byte seed outside the store"
                .into(),
        )
    })?;
    let signature = ed25519_dalek::Signer::sign(&key, &head_message(head, links));
    Ok(SignedHead {
        head: head.clone(),
        links: links.map(str::to_string),
        signer: key.verifying_key().to_bytes(),
        signature: signature.to_bytes(),
    })
}

/// One head against an earlier one from the same log: the consistency proof
/// that catches a log rewritten between handovers.
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

/// Read the head a receiver kept from an earlier handover.
///
/// Any text that carries `size=N` and `root=HEX` will do: a bag's signed
/// `head` line, a `proof.txt`, or the file `deedar check --keep` wrote. A
/// bridge file is refused here, because a bridge comes from the sender and a
/// kept head has to come from the receiver.
///
/// # Errors
///
/// Fails when the text is a bridge, or names no size or no root.
pub fn kept_head(text: &str) -> Result<Head> {
    if text
        .lines()
        .any(|line| line.trim_start().starts_with("from="))
    {
        return Err(Error::Evidence(
            "this is a bridge, which the sender writes; --since takes the head you kept from the \
             last handover, and the bridge travels in the bag as bridge.txt"
                .into(),
        ));
    }
    let mut size = None;
    let mut root = None;
    for token in text.split_whitespace() {
        if let Some(value) = token.strip_prefix("size=") {
            size = Some(number(value, "size")? as usize);
        } else if let Some(value) = token.strip_prefix("root=") {
            root = Some(value.to_string());
        }
    }
    match (size, root) {
        (Some(size), Some(root)) => Ok(Head { size, root }),
        _ => Err(Error::Evidence(
            "kept head: needs size= and root=, as `deedar check --keep` writes them".into(),
        )),
    }
}

/// Whether the log behind `now` is the log behind `kept`, grown.
///
/// The bridge alone cannot answer this. It proves two heads are one log,
/// and the sender picks both of them; only the receiver's own record of the
/// earlier head says the sender started from the log the receiver saw. So
/// the bridge has to start at `kept` and end at `now`, the head every deed in
/// the bag is proven against.
///
/// # Errors
///
/// Fails when the log shrank, when no bridge came and the head moved, when
/// the bridge starts or ends somewhere else, or when its path does not join
/// the two heads.
pub fn check_since(kept: &Head, bridge: Option<&Bridge>, now: &Head) -> Result<()> {
    if now.size < kept.size {
        return Err(Error::Evidence(format!(
            "the log shrank from the {} entries you kept to {}",
            kept.size, now.size
        )));
    }
    let Some(bridge) = bridge else {
        if kept == now {
            return Ok(());
        }
        return Err(Error::Evidence(format!(
            "the bag carries no bridge from the head you kept ({} entries); the sender writes one \
             with `deedar log bridge {} > BAG/bridge.txt`",
            kept.size, kept.size
        )));
    };
    if bridge.from != *kept {
        return Err(Error::Evidence(format!(
            "the bridge starts from a log of {} entries, root {}, and the head you kept is {} \
             entries, root {}: this is not the log you saw before",
            bridge.from.size, bridge.from.root, kept.size, kept.root
        )));
    }
    if bridge.to != *now {
        return Err(Error::Evidence(format!(
            "the bridge ends at a log of {} entries, root {}, and the deeds in this bag are \
             proven against {} entries, root {}",
            bridge.to.size, bridge.to.root, now.size, now.root
        )));
    }
    bridge.check()
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
    /// Whether the reader listed the keys it accepts. An import that waives
    /// an unlisted signer checks again with no list, and keeps this set.
    pub signers_listed: bool,
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
            Some(signer) if self.signers_listed => out.push_str(&format!(
                "head signed by {}, which is not a key this reader accepts, and it was taken as \
                 unsigned, so every proof above is against a head this bag asserted about itself\n",
                log::hex(&signer)
            )),
            Some(signer) => out.push_str(&format!(
                "head signed by {}, and no signer list was given, so this says the head and the \
                 signature go together and nothing about who made them\n",
                log::hex(&signer)
            )),
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

/// Read a regular file, or `Ok(None)` when it is absent.
///
/// A symlink, fifo, or other non-regular file is refused. The open uses
/// `O_NOFOLLOW` and `O_NONBLOCK`, so a file replaced by a symlink or a fifo
/// between the type check and the read is not followed and does not block.
pub(crate) fn read_regular_opt(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Error::Io(format!("{}: {e}", path.display()))),
        Ok(meta) if !meta.file_type().is_file() => {
            return Err(Error::Evidence(format!(
                "{}: not a regular file",
                path.display()
            )));
        }
        Ok(_) => {}
    }
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => {
            return Err(Error::Evidence(format!(
                "{}: not a regular file",
                path.display()
            )));
        }
        Err(e) => return Err(Error::Io(format!("{}: {e}", path.display()))),
    };
    let meta = file
        .metadata()
        .map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
    if !meta.file_type().is_file() {
        return Err(Error::Evidence(format!(
            "{}: not a regular file",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut bytes)
        .map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
    Ok(Some(bytes))
}

/// Deed folders in `root`, in name order. A symlink is refused rather than
/// followed or skipped, and so is a fifo or anything else that is not a
/// directory or a regular file.
pub(crate) fn deed_folders(root: &Path) -> Result<Vec<PathBuf>> {
    if !real_dir(root)? {
        return Err(Error::Evidence(format!(
            "{}: not a directory",
            root.display()
        )));
    }
    let mut folders = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|e| Error::Io(e.to_string()))? {
        let path = entry.map_err(|e| Error::Io(e.to_string()))?.path();
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
        let kind = meta.file_type();
        if kind.is_symlink() {
            return Err(Error::Evidence(format!(
                "{}: symlink in the deeds directory",
                path.display()
            )));
        }
        if kind.is_dir() {
            folders.push(path);
        } else if !kind.is_file() {
            return Err(Error::Evidence(format!(
                "{}: not a regular file",
                path.display()
            )));
        }
    }
    folders.sort();
    Ok(folders)
}

/// The product files under a deed folder's `bytes/`, in name order, and none
/// when it has no `bytes/`. That has to be a real directory of regular
/// files: a symlink is refused, not followed.
pub(crate) fn product_files(folder: &Path) -> Result<Vec<PathBuf>> {
    let dir = folder.join("bytes");
    match std::fs::symlink_metadata(&dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::Io(format!("{}: {e}", dir.display()))),
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => {
            return Err(Error::Evidence(format!(
                "{}: not a directory",
                dir.display()
            )));
        }
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| Error::Io(e.to_string()))? {
        let path = entry.map_err(|e| Error::Io(e.to_string()))?.path();
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
        if !meta.file_type().is_file() {
            return Err(Error::Evidence(format!(
                "{}: not a regular file",
                path.display()
            )));
        }
        files.push(path);
    }
    files.sort();
    Ok(files)
}

/// Whether `path` is a real directory. Absent is `Ok(false)`. A symlink is
/// refused, including when the final component is `data/deeds`.
fn real_dir(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Error::Io(format!("{}: {e}", path.display()))),
        Ok(meta) if meta.file_type().is_symlink() => Err(Error::Evidence(format!(
            "{}: symlink in the deeds directory",
            path.display()
        ))),
        Ok(meta) if meta.file_type().is_dir() => Ok(true),
        Ok(_) => Err(Error::Evidence(format!(
            "{}: not a directory",
            path.display()
        ))),
    }
}

/// Domain separation for the links transcript. The transcript before it had
/// no tag, ended a folder name at a newline, and gave a missing sidecar the
/// length of an empty one, so two bags could share it. A head over that
/// transcript is refused.
const LINKS_DOMAIN: &[u8] = b"deedar-links-v2";

/// Hex SHA-256 of the link sidecars under a deeds directory.
///
/// The transcript is `deedar-links-v2` and a zero byte, then each deed folder
/// in byte order of the names: `<len>\n<name>`, then for `sources`,
/// `supersedes` and `successor` in that order `<kind>\n` followed by `-\n`
/// when the file is absent or `<len>\n<bytes>` when it is present. Every
/// field is framed, so other folders or other files make another transcript.
/// A symlink or other non-regular file is refused, not read.
///
/// This is what a bag's head signature covers beyond the log root. The
/// receipt does not: a file edited after export still sits beside a proof
/// that checks, until this digest is checked against the signature. An
/// unsigned head has no signature, so it does not authenticate these files.
pub(crate) fn link_commitment(root: &Path) -> Result<String> {
    let mut transcript = Vec::from(LINKS_DOMAIN);
    transcript.push(0);
    for folder in deed_folders(root)? {
        let name = folder
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                Error::Evidence(format!(
                    "{}: deed folder name is not utf-8",
                    folder.display()
                ))
            })?;
        transcript.extend_from_slice(format!("{}\n", name.len()).as_bytes());
        transcript.extend_from_slice(name.as_bytes());
        for kind in ["sources", "supersedes", "successor"] {
            transcript.extend_from_slice(kind.as_bytes());
            transcript.push(b'\n');
            match read_regular_opt(&folder.join(kind))? {
                None => transcript.extend_from_slice(b"-\n"),
                Some(bytes) => {
                    transcript.extend_from_slice(format!("{}\n", bytes.len()).as_bytes());
                    transcript.extend_from_slice(&bytes);
                }
            }
        }
    }
    Ok(log::hex(&Sha256::digest(&transcript)))
}

/// Whether `signed` covers the link sidecars actually in `root`.
///
/// A signature with no link digest covers a bag that carries none of them.
/// One that names a digest has to name this directory's.
pub(crate) fn links_bound(root: &Path, signed: &SignedHead) -> Result<()> {
    match &signed.links {
        Some(links) => {
            let got = link_commitment(root)?;
            if got != *links {
                return Err(Error::Evidence(
                    "the head signature does not cover the link sidecars in this bag as \
                     deedar-links-v2 frames them; a head signed over the earlier transcript is \
                     refused too, and exporting the bag again writes this one"
                        .into(),
                ));
            }
            Ok(())
        }
        None if link_sidecars_present(root)? => Err(Error::Evidence(
            "this bag carries source or supersede sidecars that the head signature does not cover"
                .into(),
        )),
        None => Ok(()),
    }
}

fn link_sidecars_present(root: &Path) -> Result<bool> {
    for folder in deed_folders(root)? {
        for kind in ["sources", "supersedes", "successor"] {
            match std::fs::symlink_metadata(folder.join(kind)) {
                Ok(_) => return Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(Error::Io(format!("{}: {e}", folder.join(kind).display())));
                }
            }
        }
    }
    Ok(false)
}

/// Check every deed a handover carried, given the bag or the deeds directory.
///
/// Every receipt has to name the same head, so the receiver has one thing to
/// record and nothing rides in under a head nobody kept.
///
/// # Errors
///
/// Fails when a deed carries no receipt, when bytes and digest disagree, when
/// a path does not reach the head, or when the receipts disagree about which
/// head they are against.
pub fn check_handover(dir: &Path, accept: &BTreeSet<[u8; 32]>) -> Result<Handover> {
    let root = deeds_in(dir)?.ok_or_else(|| {
        Error::Evidence(format!(
            "{}: no deeds here and none under data/deeds",
            dir.display()
        ))
    })?;

    let folders = deed_folders(&root)?;

    let mut proven = Vec::new();
    let mut head: Option<Head> = None;
    let mut wrong = Vec::new();
    for folder in folders {
        let name = folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = match read_regular_opt(&folder.join("proof.txt")) {
            Ok(Some(bytes)) => match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => {
                    wrong.push(format!("{name}: proof.txt is not text"));
                    continue;
                }
            },
            Ok(None) => {
                wrong.push(format!(
                    "{name}: no proof.txt, so nothing says this deed was logged before it was \
                     handed over"
                ));
                continue;
            }
            Err(e) => {
                wrong.push(said(&e));
                continue;
            }
        };
        let receipt = match Receipt::parse(&text) {
            Ok(receipt) => receipt,
            Err(e) => {
                wrong.push(format!("{name}: {}", said(&e)));
                continue;
            }
        };
        let bytes = match read_regular_opt(&folder.join("deed.bin")) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                wrong.push(format!("{name}: a receipt with no deed.bin beside it"));
                continue;
            }
            Err(e) => {
                wrong.push(said(&e));
                continue;
            }
        };
        if let Err(e) = receipt.check(&bytes) {
            wrong.push(said(&e));
            continue;
        }
        let named = crate::wire::deed_from_bytes(&bytes)
            .and_then(|deed| named_for(&folder, &receipt.id, deed.id.as_str()));
        if let Err(e) = named {
            wrong.push(format!("{name}: {}", said(&e)));
            continue;
        }
        if let Err(e) = product_files(&folder) {
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

    // The signed head, when one travelled, has to be the head the receipts are
    // against.
    let mut head_signer = None;
    let mut head_accepted = false;
    let head_text = match read_regular_opt(&head_beside(&root)) {
        Ok(Some(bytes)) => match String::from_utf8(bytes) {
            Ok(text) => Some(text),
            Err(_) => {
                wrong.push("head: not text".into());
                None
            }
        },
        Ok(None) => None,
        Err(e) => {
            wrong.push(said(&e));
            None
        }
    };
    if let Some(text) = head_text {
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
                if let Err(e) = links_bound(&root, &signed) {
                    wrong.push(said(&e));
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
            signers_listed: !accept.is_empty(),
        })
    } else {
        Err(Error::Evidence(format!(
            "this handover does not check out:\n{}",
            wrong.join("\n")
        )))
    }
}

/// Whether a deed folder is named for the deed it holds. The head signature
/// binds each folder's link sidecars to the folder's name, so another deed's
/// `deed.bin` and `proof.txt` moved into the folder would take those links.
///
/// # Errors
///
/// Fails when the name is not an accession, or `deed.bin` or `proof.txt`
/// names another one.
pub(crate) fn named_for(folder: &Path, proof_id: &str, deed_id: &str) -> Result<()> {
    let name = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if deed::DeedId::parse(&name).is_err() {
        return Err(Error::Evidence(
            "the folder name is not an accession, and a deed folder has to be named for the \
             deed it holds"
                .into(),
        ));
    }
    if name != proof_id || name != deed_id {
        return Err(Error::Evidence(format!(
            "deed.bin is {deed_id} and proof.txt is {proof_id}, and a deed folder has to be \
             named for the deed it holds"
        )));
    }
    Ok(())
}

/// One complaint's text, without the `evidence:` prefix the heading already
/// supplies.
pub(crate) fn said(e: &Error) -> String {
    match e {
        Error::Evidence(text) | Error::Io(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Where the deeds are, given either the bag or the directory inside it.
///
/// A symlink at `data/deeds`, or a symlink passed as the deeds directory, is
/// refused. Falling through to the parent would hide it.
pub(crate) fn deeds_in(dir: &Path) -> Result<Option<PathBuf>> {
    let nested = dir.join("data").join("deeds");
    if real_dir(&nested)? {
        return Ok(Some(nested));
    }
    if real_dir(dir)? {
        return Ok(Some(dir.to_path_buf()));
    }
    Ok(None)
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
                id: format!("deed-quote-thing{i}"),
                digest: format!("sha256:{}", log::hex(&Sha256::digest(body(i)))),
                unix_time: 1_700_000_000 + i as u64,
            })
            .collect()
    }

    /// The canonical bytes of deed `i`, as its `deed.bin` carries them.
    fn body(i: usize) -> Vec<u8> {
        let excerpt = format!("the bytes of deed {i}");
        let deed = deed::Deed::from_draft(deed::Draft {
            id: Some(deed::DeedId::parse(&format!("deed-quote-thing{i}")).expect("id")),
            name: format!("thing {i}"),
            sources: Vec::new(),
            produced_by: deed::ProducedBy {
                agent_id: "reader".into(),
                activity_id: None,
            },
            grants: Vec::new(),
            body: deed::Body::Quote {
                edition: "https://example.invalid/a".into(),
                start: 0,
                end: excerpt.len() as u64,
                excerpt,
                urls: vec!["https://example.invalid/a".into()],
            },
        })
        .expect("deed");
        crate::wire::deed_bytes(&deed)
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

    /// Bytes plus receipt rebuild the leaf and walk it to the head.
    #[test]
    fn a_receipt_carries_everything_the_leaf_needs() {
        let all = entries(9);
        for at in 0..all.len() {
            let receipt = receipt_for(&all, at);
            let read = Receipt::parse(&receipt.render()).expect("round trips");
            assert_eq!(read, receipt);
            read.check(&body(at)).expect("checks out");
        }
    }

    /// The digest catches swapped bytes; the tree catches a borrowed path.
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

    /// A receipt missing a field is refused, not defaulted.
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

    /// A grown log bridges to the kept head; a rewritten one does not.
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

    /// The bytes a producer hashes and signs for `links=`.
    #[test]
    fn the_link_commitment_is_the_transcript_a_producer_signs() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("deed-quote-a");
        std::fs::create_dir(&folder).unwrap();
        let supersedes = b"deed-quote-v1\nsha256:abababababababababababababababababababababababababababababababab\n";
        std::fs::write(folder.join("supersedes"), supersedes).unwrap();

        let mut transcript = b"deedar-links-v2\0".to_vec();
        transcript.extend_from_slice(b"12\ndeed-quote-a");
        transcript.extend_from_slice(b"sources\n-\n");
        transcript.extend_from_slice(b"supersedes\n");
        transcript.extend_from_slice(format!("{}\n", supersedes.len()).as_bytes());
        transcript.extend_from_slice(supersedes);
        transcript.extend_from_slice(b"successor\n-\n");
        let want = log::hex(&Sha256::digest(&transcript));
        assert_eq!(link_commitment(root.path()).unwrap(), want);

        let head = Head {
            size: 2,
            root: "cd".repeat(32),
        };
        let mut message = b"deedar-head-v1".to_vec();
        message.push(0);
        message.extend_from_slice(format!("2 {}", "cd".repeat(32)).as_bytes());
        message.push(0);
        message.extend_from_slice(want.as_bytes());
        assert_eq!(head_message(&head, Some(&want)), message);

        message.truncate(message.len() - 1 - want.len());
        assert_eq!(head_message(&head, None), message);
    }

    /// One folder name cannot stand for two folders, and a missing sidecar
    /// is not an empty one. A head over the transcript before
    /// `deedar-links-v2`, which allowed both, is refused.
    #[test]
    fn the_link_transcript_tells_bags_apart() {
        let links = |folders: &[&str], files: &[&str]| {
            let root = tempfile::tempdir().unwrap();
            for name in folders {
                std::fs::create_dir(root.path().join(name)).unwrap();
            }
            for file in files {
                std::fs::write(root.path().join(file), b"").unwrap();
            }
            (link_commitment(root.path()).unwrap(), root)
        };
        let (two, _) = links(&["deed-quote-a", "deed-quote-b"], &[]);
        let crafted = "deed-quote-a\nsources\n0\nsupersedes\n0\nsuccessor\n0\ndeed-quote-b";
        let (one, _) = links(&[crafted], &[]);
        assert_ne!(one, two, "one folder name stood for two folders");
        let (missing, bare) = links(&["deed-quote-a"], &[]);
        let (empty, _) = links(&["deed-quote-a"], &["deed-quote-a/supersedes"]);
        assert_ne!(missing, empty, "an empty sidecar passed for a missing one");

        let earlier = b"deed-quote-a\nsources\n0\nsupersedes\n0\nsuccessor\n0\n";
        let earlier = log::hex(&Sha256::digest(earlier));
        let key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
        let head = Head {
            size: 1,
            root: "ab".repeat(32),
        };
        let signature = ed25519_dalek::Signer::sign(&key, &head_message(&head, Some(&earlier)));
        let signed = SignedHead {
            head,
            links: Some(earlier),
            signer: key.verifying_key().to_bytes(),
            signature: signature.to_bytes(),
        };
        signed
            .check(&BTreeSet::new())
            .expect("the signature verifies");
        let err = links_bound(bare.path(), &signed).expect_err("the earlier transcript passed");
        assert!(format!("{err}").contains("deedar-links-v2"), "{err}");
    }

    /// The report distinguishes an unsigned head, a named signer, and an
    /// accepted one.
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
            signers_listed: false,
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

        let waived = Handover {
            signers_listed: true,
            ..named.clone()
        };
        let said = waived.render();
        assert!(!said.contains("no signer list"), "{said}");
        assert!(said.contains("not a key this reader accepts"), "{said}");

        let accepted = Handover {
            head_accepted: true,
            ..named
        };
        assert!(accepted.render().contains("(accepted)"));
    }

    /// A head signature covers that head, no other, and is not a manifest
    /// signature.
    #[test]
    fn a_head_signature_covers_that_head_only() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
        let head = Head {
            size: 9,
            root: "3c".repeat(32),
        };
        let signed = SignedHead {
            head: head.clone(),
            links: None,
            signer: key.verifying_key().to_bytes(),
            signature: ed25519_dalek::Signer::sign(&key, &head_message(&head, None)).to_bytes(),
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
        assert_ne!(
            head_message(&head, None),
            crate::vouch::covered_for_test(b"x")
        );
    }

    /// A handover fails whole: on a deed with no receipt, and on a receipt
    /// against a head the rest of the bag does not share.
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

        let bare = deeds.join("deed-thing-late");
        std::fs::create_dir_all(&bare).expect("dirs");
        std::fs::write(bare.join("deed.bin"), b"minted this morning").expect("bytes");
        let err = check_handover(dir.path(), &BTreeSet::new()).expect_err("a bare deed passed");
        assert!(format!("{err}").contains("no proof.txt"), "{err}");
        std::fs::remove_dir_all(&bare).expect("clean up");

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
