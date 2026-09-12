# Changelog

Versions follow semver at 0.x: a minor bump is a feature, a patch is a fix.

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
