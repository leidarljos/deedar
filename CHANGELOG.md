# Changelog

Versions follow semver at 0.x: a minor bump is a feature, a patch is a fix.

## Unreleased

- A new store's `writer.key` comes from `/dev/urandom` and is written with
  mode 0600. It was the SHA-256 of the clock in nanoseconds, written with
  the default mode (0644 under a usual umask), and the file's mtime put the
  key within about two million guesses. An existing key is read as before;
  run `chmod 600` on it by hand.
- `deedar check BAG --since KEPT` now compares the bag's bridge with the
  head the receiver kept. Before, `--since` took the sender's bridge and
  checked only that its two heads joined, and the sender picks both, so a
  log rewritten between handovers passed. The bridge now has to start at
  the kept head and end at the head the bag's deeds are proven against.
  The bridge travels in the bag as `bridge.txt` (or `--bridge FILE`), and
  `check --keep FILE` records a head for next time. Passing a bridge to
  `--since` is refused with a message that says why.
- `cargo binstall deedar-cli` and `cargo binstall deedar-mcp` build from
  source on a target with no release tarball, such as Windows or musl
  Linux. The `compile` strategy was off, so binstall failed there.
  cargo-quickinstall stays off.

## 0.4.0 (2026-10-10)

- `deedar host accept` lists the host signing key as a signer in an
  existing store's layout and keeps every other line. A store made
  before `ljos onboard` wrote a host key needed a hand edit before. The
  rewritten layout keeps its file mode, and `deedar host` refuses a verb
  it does not know.

## 0.3.4 (2026-10-10)

- A store this host creates lists the host's signing key in its layout.
  The first deed a fresh seat signs then passes `evidence`. A store that
  already has a layout or deeds keeps the signers it lists.
- The explanation places the deed log beside Rekor, the Update
  Framework, and Supply-chain Levels for Software Artifacts. The hash
  stays Certificate Transparency's. The history tree is Crosby and
  Wallach, USENIX Security 2009.

## 0.3.3 (2026-09-29)

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
