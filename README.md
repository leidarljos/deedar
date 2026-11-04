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

The block is a fresh store with signing off, recorded with the clock
stopped. Run it yourself and the time and the root will differ; the other
lines will not.

<!-- [[[cog
import sys
from pathlib import Path
start = Path(cog.inFile).resolve().parent
root = next(p for p in [start, *start.parents] if (p / "docs" / "scripts" / "samples.py").is_file())
sys.path[:0] = [str(root / "docs" / "scripts")]
import samples
cog.out(samples.transcript("readme"))
]]] -->
```console
$ export DEEDAR_URL=file:///tmp/deeds
$ export DEEDAR_HOST_SIGNING_KEY=off
$ echo 'fn main() {}' > note.rs
$ deedar create file --name "the parser patch" --path note.rs --agent you
id=deed-file-the-parser-patch kind=file name=the parser patch
paths=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
sources=
producedBy=you -
grants=
face=document
file=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
accession=deed-file-the-parser-patch
$ deedar evidence deed-file-the-parser-patch
id=deed-file-the-parser-patch ok
producedBy=you -
time=1700000000
$ deedar export --into bag/data/deeds deed-file-the-parser-patch
exported 1 deeds, 4 files
$ deedar check bag
1 deeds proven against a log of 1 entries, root b1799b9f22404b3626b3c43e80f26232922113b997a1e3377bc803ff799f4ee8
the head is unsigned, so every proof above is against a head this bag asserted about itself
```
<!-- [[[end]]] -->

## What holds

- A deed is never edited. A better take is a new deed that `--supersedes`
  the old one. `current` prints the tip; over several ids it says which
  citations moved.
- `evidence`, `current` and `export` take one id, several, or `-` from a
  pipe, and exit non-zero on any failure.
- The log is RFC 9162 hashing (doi:10.17487/RFC9162): inclusion receipts
  travel with an export, `log bridge` proves a log grew from a head you
  kept, `log audit` finds a deed that was logged and is gone.
- A configured host key signs heads, sidecars and satchel manifests. With signing off, nothing is signed. A receiver lists accepted keys in its store's `layout`.
  `deedar host accept` adds this host's own key there. A store whose layout
  says `attestation = required` still gains the key, and the command warns.
- The default host key is one file per user account, so a signature names an
  account on one machine. Seats and agents under that account share it, and
  `--agent` is what the caller says. See the explanation page for keeping
  the key out of an agent's reach.
- Bytes and deeds are content addressed; log appends take a file lock, so
  two processes minting at once write whole lines.

## Kinds

`file`, `set`, `quote`, `patch`, `mailDraft`, `clip`, `page`, `form`,
`table`, `procedure`, `event`. The reference page lists each kind's flags.

## License

MIT.
