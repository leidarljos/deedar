# Changelog

Versions follow semver at 0.x: a minor bump is a feature, a patch is a fix.

## Unreleased

- `deedar create` warns on stderr when the host signs with a key the
  store's `layout` does not list, and names the `signer =` line to add.
  Every deed signed that way fails `evidence`, which used to say only
  `host signature`; it now names the layout file and how many signers it
  lists.
- `deedar host` says whether the store's `layout` lists the host signing
  key, and exits 1 with the line to add when it does not.

## 0.3.2 (2026-09-20)

- `deedar_trail` says what it walks: the deed and every input it was made
  from, back to the leaves. Whether a citation is still the tip is
  `deedar_current`'s question.
- Every MCP row carries `body`, the kind-specific product as the store
  holds it, so `deedar_get` answers with what `deedar get` prints.
- Every crate page carries the README.

## 0.3.1 (2026-09-15)

- `deedar --help` and `deedar create --help` print usage. `create file`
  without `--path` names `--path FILE`. `--help` does not need a store.

## 0.3.0 (2026-09-12)

- The host key is found at `~/.config/deedar/host.key` when
  `DEEDAR_HOST_SIGNING_KEY` is unset, so a seat signs with nothing set once
  it has a key. `DEEDAR_HOST_SIGNING_KEY=off` signs nothing.

## 0.2.0 (2026-09-12)

What a user gets:

- Handover receipts: `export --into DIR` writes each deed with its bytes,
  evidence and the inclusion path to the log head, plus the signed head when
  the store has a key; `check DIR [--since BRIDGE]` proves a bag with no
  store and no network, and the bridge shows the log grew from a head you
  kept.
- Signed manifests: `vouch sign FILE` and `vouch check FILE` with a detached
  Ed25519 signature; the receiver's accepted keys live in the store layout.
- `log bridge SIZE`, `log audit`, `log backfill`, signed `log head`.
- `evidence`, `current` and `export` take several ids or `-` from stdin, and
  exit non-zero on any failure, so a tracker or a pack can pipe its citations
  in.
- A retraction can name the deed that withdrew it.
- Log appends from two processes take the file lock; the export tree is
  cached on the log's size and mtime, so a hundred-deed handover reads the
  log once (397 ms to 49 ms).
- A read-only MCP surface with `deedar_check`, `deedar_log_bridge` and two
  prompts.
- A documentation site at https://leidarljos.github.io/deedar/.

## 0.1.0

The store: content-addressed deeds, an RFC 9162 log, host attestation.
