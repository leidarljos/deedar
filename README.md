# deedar

<p align="center">
  <img src="docs/logo/icon.svg" width="120" height="120" alt="deedar: a receipt pinned through a gold seal">
</p>

What did this unit of work produce? A deed is the frozen record of one
product: the bytes by hash, who made them, what they were made from. Every
deed goes into an append-only Merkle log, so a store can show a deed was
there before anyone asked and cannot drop one unnoticed. A tracker or a pack
cites a deed by its accession and never copies the bytes.

Docs: https://leidarljos.github.io/deedar/

| Page | What it answers |
|---|---|
| [Getting started](https://leidarljos.github.io/deedar/getting-started.html) | Freeze a file, prove it, supersede it |
| [How-to](https://leidarljos.github.io/deedar/howto.html) | Export, check, hand a receipt |
| [Reference](https://leidarljos.github.io/deedar/reference.html) | Kinds, accessions, the log |
| [Explanation](https://leidarljos.github.io/deedar/explanation.html) | Why a citation names a deed |

The seat that cites a deed is documented at https://leidarljos.github.io.

## Install

```console
$ cargo binstall deedar-cli
$ export DEEDAR_URL=file://$HOME/.local/share/deedar/store
```

`deedar-mcp` is the read-only MCP surface. There is no daemon: each command
opens the store.

## First minute

```console
$ echo 'fn main() {}' > note.rs
$ deedar create file --name "the parser patch" --path note.rs --agent you
id=deed-file-the-parser-patch kind=file name=the parser patch
$ deedar evidence deed-file-the-parser-patch
id=deed-file-the-parser-patch ok
$ deedar export --into bag/data/deeds deed-file-the-parser-patch
exported 1 deeds, 4 files
$ deedar check bag
1 deeds proven against a log of 1 entries, root f568...
```

## What holds

- A deed is never edited; a better take is a new deed that `--supersedes`
  the old one, and `current` says where a citation moved.
- `evidence`, `current` and `export` take one id, several, or `-` from a
  pipe, and exit non-zero on any failure.
- The log is RFC 9162 hashing (doi:10.17487/RFC9162): inclusion receipts
  travel with an export, `log bridge` proves a log grew from a head you
  kept, `log audit` finds a deed that was logged and is gone.
- A configured host key signs heads, sidecars and satchel manifests. With signing off, nothing is signed. A receiver lists accepted keys in its store's `layout`.
- Bytes and deeds are content addressed; log appends take a file lock, so
  two processes minting at once write whole lines.

## Kinds

`file`, `set`, `quote`, `patch`, `mailDraft`, `clip`, `page`, `form`,
`table`, `procedure`, `event`. The reference page lists each kind's flags.

## License

MIT.
