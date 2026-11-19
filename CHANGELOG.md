# Changelog

Versions follow semver at 0.x: a minor bump is a feature, a patch is a fix.

<!-- towncrier release notes start -->

## 0.7.1 (2026-11-18)

### Fixes

- A create that is missing a flag names that flag and `deedar create --help`.
  A path that is not a regular file says so, instead of a store io line.
- `DEEDAR_HOST_SIGNING_KEY` set to 32 bytes is the seed.
  A path that is not a file is refused, and the line names how to write one.
- `DEEDAR_HOST_SIGNING_KEY` takes a seed written `seed:` then 32 bytes, or `hex:` then 64 hex digits.
  A seed with `/` or `\` in it is now writable, where bare it was refused as a path.
  A bare 32-byte value with a separator is still a path, and its refusal names both forms.
  The reference lists the forms in the order they are read.
- `deedar VERB --help` and `deedar help VERB` print a page for that verb, with its arguments and exit status.
  It was the top-level usage for every verb except `create`.
- `deedar create --help` spells the kind `mailDraft` and lists the flags each kind reads.
  An unknown kind names the tokens and that page.
  An event `--title` is the deed name when `--name` is omitted.
- `deedar help` prints the same pages as `deedar --help`, and does not open a store.
  An unknown command, log verb, or vouch verb names the page that lists the real ones.
  An unknown command does not create a store.
- `deedar host accept` with no key names the command that writes a 32-byte seed.
  It no longer names `ljos onboard`.
- `deedar list` at a `--url` or `DEEDAR_URL` path with no store no longer makes the store.
  It prints one stderr line that there is no store yet and how to make one, and exits 0 with an empty list.
  Every other read exits 1 with that line, and `deedar url` prints the URL without opening it.
  `create`, `import` and `host accept` still make the store.
- `deedar log` with no command prints the log usage on stderr and exits 2.
  It printed the head and exited 0, which no page documents; `deedar log head` is the command.
- `deedar timestamp` with `DEEDAR_TSA` unset names the query file and the variable to set for a `.tsr`.
- `deedar-mcp --help` and `deedar-mcp --version` print and exit.
  An argument is refused with that page, instead of a protocol error.
- `deedar-mcp` started with no client on standard input prints one line that it expects an MCP client there, and exits 1.
  It ended on `connection closed: initialize request`.
- `deedar` with no verb prints usage and does not create a store.
  Usage names `url` and `leave ID DIR`.

### Documentation

- The getting-started page shows the sender key file, the `deedar host` line, and the `signer` line the receiver adds.
  `deedar host accept` adds the key on the machine that runs it.
- The install page names no release.
  Its download commands read the version and target from two variables, and `docs/scripts/check_org.py` refuses a typed version there.


## 0.7.0 (2026-11-18)

### Features

- A sha256 of a deed is read from the log instead of every frame.
- `deedar list` and `deedar_list` take optional kind, agent and text, and return the matching deeds in accession order.
- create, evidence and export stream a product file, so a large file no longer costs its size in memory.

### Fixes

- A deed file that does not decode no longer fails `list` or `log audit`; audit names it as missing or unreadable.
- A read from `deedar-mcp` of a missing seat store returns the command line's hint and creates nothing.
  An export creates that store and names its path, which is the write this server offers.
- A receipt or bridge with one byte changed in a path step now fails the check.
- A silent timestamp authority no longer hangs `deedar timestamp`: the command saves the `.tsq` and exits 1.
- An empty or relative `HOME` is ignored when choosing the seat store, as an empty or relative `XDG_DATA_HOME` already is.
- An excerpt, a name or a step spelled `--help`, `-h`, `--version`, `-V` or `--url` is stored as that text.
  `deedar --help`, `deedar create --help`, `deedar -V` and `deedar list --url FILE` are unchanged.
- An unknown accession over the Model Context Protocol (MCP) is invalid parameters.
  The error names `deedar_list`, or `deedar_get` with the one match.
  A prefix of more than one accession lists at most eight, in byte order.
- Four lookup errors now name the input and the next command to run.
  An ambiguous prefix also lists up to eight matches.
- Opening a store from two processes at once no longer reads an empty writer.key.
- `create` and `import` make the seat store on first use and name its path.
- `deedar check` and `deedar vouch check` exit `EXIT_UNTRUSTED_SIGNER` when the signer is not one the reader accepts.
  The report and the warning are still printed.
  A signature that does not cover the bytes still exits 1.
- `deedar check` and `deedar vouch check` report a key the layout does not list instead of failing.
  `deedar import --allow-unsigned-head` keeps those deeds too, and warns.
- `deedar create --supersedes P` names the current tip when P already has a successor.
- `deedar current` on a fork or a cycle prints every tip or member in accession order, with one warning.
- `deedar host accept` refuses to add a signer when the layout says `attestation = required`, and names the line to add by hand.
- `deedar-mcp` opens the same default store as the command line, under `$XDG_DATA_HOME` when it is set, and treats an empty `DEEDAR_URL` as unset.
  It used to read an empty store under `~/.local/share` that the command line never wrote.
- `deedar_current` on a fork or a cycle returns every current deed in accession order.
  A note says which, and the call succeeds.
- check and import name the field a receipt or signed head fails on.
  Each line gives the expected and found values and the file each came from.

### Documentation

- Added paragraphs are one sentence to a line.
- Each seat-store fact in the reference now has its own sentence.
- The first minute in the index shows the log root the binary prints.
  The reference no longer says that `check --keep` is missing from 0.4.0.
- The guide samples are the output of the binary itself.
  The docs show how to make a v2 revision of a frozen deed.
- The how-to lists all ten Model Context Protocol (MCP) tools.
  It says which read and which write.
  It shows the `deedar-mcp` command a client runs.
  It had named eight tools and called every one of them read-only.
- The reference lists deeds in accession order.
  Usage and the reference name `--kind`, `--agent` and `--text`.
- The status 2 row names the check, and the report detail sits under the table.

### Miscellaneous

- A round-trip test now covers export and import between stores.
- CI and `just check` run `cargo deny check`, so an advisory or a non-crates.io source fails the job.
  Four unused licence allowances are dropped.
- CI installs rustfmt and clippy with the toolchain.
  So `cog --check` and the macOS, licence and release jobs no longer fail when rustup adds them later.
- Changelog entries are fragments under `changelog.d`, and CI fails a draft whose fragment name is not a known type.
- Each test gets a scratch store of its own on macOS too, so the large-file tests no longer share one and fail at random.
- Every test scratch directory is named by pid and a counter, so no two tests share one on macOS.
- The handover bench measures receipts at the criterion default of 5 s, and protocol constants cite their source.
- The version is written by hand only in `Cargo.toml`.
  The Sphinx release and the internal crate pins are generated from it, and CI fails when a copy drifts.
- deedar_list now describes its order as accession order, not oldest first.


## 0.6.1 (2026-10-31)

- The 0.6.0 tag did not build: the workspace pinned its own crates at
  0.5.0 while the crates said 0.6.0. 0.6.1 is that release.
- `deedar check` refuses a `bytes/<sha256>` file that does not hash to
  its name, and a deed path whose digest has no file there. Check only
  required those files to be regular, so a tampered bag passed `check`
  and failed `import`. Both now share one function, and the refusal is
  the line import already printed.
- `deedar import BAG` checks a handover, then holds each deed in this
  store: its record, product bytes and supersede links, and a log entry
  with the digest and time from its receipt. Evidence is issued again
  under this store's `writer.key` at the receipt's time, a copied host
  signature is kept, and the receipt and its head stay beside the deed
  as `deeds/ID.proof` and `deeds/ID.head`. Every deed is checked before
  anything is written, so a frozen, tombstoned or differently logged
  accession, or a source or successor neither side has, writes nothing.
  The head has to be signed by a key the layout lists.
  `--allow-unsigned-head` takes a head whose signature is missing or
  from some other key. Deleting `head` is that
  same case. `attestation = required` refuses both, and `deedar check`
  still refuses a signer it was not given. Importing the same bag
  again writes nothing. A receipt already kept stays against the head
  it names: a later bag carrying a grown head is kept, and
  a missing head is filled only from that same head. A proof this
  store does not have yet is written with its head together. A link
  that names a deed the bag also carries has to name that deed's
  receipt digest. The imported supersedes file keeps the digest, so a
  later copy of that prior with other bytes is refused. A symlink,
  fifo, or other non-regular file in the bag is refused.
- Supersede links in a bag name the linked deed's digest as well as
  its accession. Import checks a local deed with that id before
  writing the sidecar. A tombstoned local deed does not gain a
  successor sidecar, and a source digest carried with a deed is
  checked before a local source is accepted. Exporting a revision
  names the stored digest when the prior deed is absent.
- `deedar export` writes each product path's bytes into the deed's
  folder as `bytes/<sha256>`, and its `sources`, `supersedes` and
  `successor` beside them. The reference listed bytes and no bag
  carried them, so a file deed arrived without its product, and a
  revision without the chain `current` walks. The signed head covers
  those three sidecars as well as the log root. An unsigned head does
  not. A successor `current` would not step to, such as a tombstoned
  one, is left out.
- A `sha256:` path is accepted only when the suffix is 64 lowercase
  hex digits. `create`, `export`, `leave` and `evidence` refuse any
  other suffix, so `sha256:../planted` is neither read nor written.
- `check` and `import` refuse a deed folder whose name differs from the
  accession in its `deed.bin` and `proof.txt`. The head signs each
  folder's links under the folder's name, so another deed's files moved
  into a revision's folder kept the signature, and `current` of the
  revision's prior stepped to that deed.
- The `links=` digest a bag's head signs is over the `deedar-links-v2`
  transcript, which gives each folder name its length and marks an
  absent sidecar apart from an empty one. One crafted folder name could
  stand for two folders, and `check` took an empty sidecar that import
  refused. A head over the earlier transcript is refused. No release
  wrote one.
- A second bag of the same log head imports beside a deed kept from the
  first. The two bags sign other links, so their head lines differ, and
  import refused the second with `this store kept another head`. A kept
  head that names the kept proof's size and root is left as it is.
- `check` refuses a symlink at or under a deed's `bytes/`, as import
  does. It never opened `bytes/`, so a bag that import refused passed
  `check`.
- A test imports over a supersedes file that holds only the accession
  and requires the digest to be added. That upgrade had no test.
- The `sha256:../planted` export test requires that no file the export
  left behind holds the planted bytes, as well as the refusal.
- When `--allow-unsigned-head` takes a head signed by a key the layout
  does not list, the import report says that key falls outside the keys
  this store accepts and that the head was taken as unsigned. It said no signer
  list was given. `Handover` gains `signers_listed`.

## 0.5.0 (2026-10-26)

- Each release archive gets a GitHub build attestation from the release workflow, so `gh attestation verify` and the ljos installer can check that this repository built it. Releases before this one have none, and the installer counts them as not published.
- `deedar create` run again with the same bytes and flags prints the deed
  it froze and exits 0, and writes nothing. It exited 1 with `deed
  frozen`, so a rerun of a script failed and did not say whether the
  bytes had changed. When they have, the refusal now says the store has
  that id with other content and prints the flags for a revision:
  `--name NAME-v2 --supersedes ID`.

- `deedar host accept` refuses to add a signer to a store whose layout
  says `attestation = required`, and prints the line to add by hand. Any
  process running as the store's owner could list a key of its own before.
- The README and the explanation page say what a host signature names: a
  user account on one machine. Seats under one account share the key, and
  `producedBy` is whatever `--agent` said. The explanation no longer calls
  the writer key's keyed hash a signature, or the host sidecar proof that
  an agent was entitled to the work.
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
