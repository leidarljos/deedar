# deedar

A unit of work produces something. deedar records it as a **deed**:
here is the work, this is its identity, take it. The next unit
opens that deed by `id`.

deedar is the writer. A deed is the noun. `quote`, `patch`, and
`file` are kinds. `DEEDAR_URL` is the contract. Clients speak
`schema/deedar.capnp`: `create`, `get`, `list`, `trail`, `evidence`,
`delete`, `leave`, `timestamp`, `current`. `get` accepts a slug or a
unique `sha256:` of the deed or of one product path. Next work
takes deed ids as `--input`; they become `sources`. `deedar migrate`
writes `{store}/layout` when that file is missing; `open` does the
same.

There is no daemon. Each command opens the store at `DEEDAR_URL`.

## First path

Every verb. A quote, a file, a patch that cites the quote. The next
sitting opens only the patch id.

```sh
rm -rf /tmp/deedar-demo
mkdir -p /tmp/deedar-demo
printf 'a short note\n' > /tmp/deedar-demo/note.md
printf '%s\n' \
  '--- a/note.md' '+++ b/note.md' \
  '@@ -1 +1 @@' '-a short note' '+a named note' \
  > /tmp/deedar-demo/note.patch
export DEEDAR_URL=file:///tmp/deedar-demo/store

just deedar create quote \
  --id deed-quote-rfc2094-nll \
  --name "NLL: lifetimes from the CFG" \
  --edition https://rust-lang.github.io/rfcs/2094-nll.html \
  --excerpt "lifetimes that are based on the control-flow graph" \
  --src-url https://rust-lang.github.io/rfcs/2094-nll.html \
  --agent reader

just deedar create file \
  --id deed-file-note \
  --name "the note" \
  --path /tmp/deedar-demo/note.md \
  --media text/plain \
  --agent reader

just deedar create patch \
  --id deed-patch-note \
  --name "name the note" \
  --tree /tmp/deedar-demo \
  --diff /tmp/deedar-demo/note.patch \
  --functionary reader \
  --agent reader \
  --input deed-quote-rfc2094-nll

just deedar get deed-patch-note
just deedar trail deed-patch-note
just deedar current deed-patch-note
just deedar evidence deed-file-note
just deedar evidence deed-patch-note
just deedar leave deed-file-note /tmp/deedar-demo/left
just deedar timestamp deed-file-note
just deedar list
just deedar migrate
just deedar delete deed-file-note
```

`evidence` also takes several ids, or `-` to read them from standard
input. That is the form a working set arrives in, and the check fails
closed on the whole list: one deed that cannot be evidenced is enough to
make the set untrustworthy, and the report says which one rather than
stopping at it.

```sh
just deedar evidence deed-file-note deed-patch-note
# deed-file-note ok
# deed-patch-note ok
# 2 of 2 verified

# or from whatever holds the citations, one id per line
vissue recall <id> --deeds-only | deedar evidence -
```

`current` takes a set the same way, which is the other half of checking
a citation: `evidence` says the bytes are intact, and `current` says
whether the thing cited is still the tip. It exits non-zero when any of
them moved, so a hook can gate on a stale citation.

```sh
vissue recall <id> --deeds-only | deedar current -
# deed-file-note current
# deed-patch-note SUPERSEDED by deed-patch-note-v2
# 1 of 2 current
```

`trail` walks `--input` from the patch to the quote. `current`
follows a later take (`--supersedes`) to the tip; `get` still
returns the named frozen deed. `get` also accepts a unique
`sha256:` of the deed bytes or of one product path. `evidence`
checks the deed, its write-once bytes, and every deed in
`sources`. A `DEEDAR_HOST_KEY` (or `{store}/../host.key`) requires
`{id}.host`; `DEEDAR_HOST_SIGNING_KEY` writes that sidecar as an
Ed25519 signature instead, which is the only form a store can demand.
`{store}/layout` says which public keys it accepts and whether an
attestation is `required`:

```
1
attestation = required
signer = ed25519:<64 hex>
```

`leave` copies the file bytes to `dest/{id}/` with a
hash sidecar (`manifest.json`). `timestamp` writes an RFC 3161
TimeStampReq over those evidence bytes (`.tsq` when `DEEDAR_TSA` is
unset; `.tsr` when the authority replies). `migrate` ensures
`{store}/layout`. `delete` writes a tombstone; `evidence` on that id
then fails.

## Handing work over

`export` writes deeds into a directory somebody else will open: the
canonical bytes, the evidence, the host signature when there is one,
and a `proof.txt`.

```sh
just deedar export --into /tmp/bag/data/deeds deed-patch-note
# or the list a tracker names
vissue recall <id> --deeds-only | deedar export --into /tmp/bag/data/deeds -
```

The proof is the part that is not a copy. Bytes and a signature travel
fine on their own and say a writer vouched for them; they do not say the
deed existed before somebody asked for it. `proof.txt` carries the
deed's place in this store's append-only log, the path from its leaf to
the log head, and the rest of what that leaf hashes over. A receiver
holding nothing but the bag can rebuild the leaf and walk it.

`check` is the other end, and it is the one verb that needs no store:

```sh
just deedar check /tmp/bag
# 3 deeds proven against a log of 41 entries, root 9f2c...
```

Write that head down. Every deed in a bag has to be against one head, so
there is one thing to keep, and the next bag from the same sender is
checked against it:

```sh
# the sender, told which head the receiver holds
just deedar log bridge 41 > /tmp/bag2/bridge.txt
# the receiver
just deedar check /tmp/bag2 --since /tmp/bag2/bridge.txt
# the log grew from 41 entries to 58 without dropping or rewriting one
# 2 deeds proven against a log of 58 entries, root 4a71...
```

Without the bridge a clean answer means every deed is in the tree the
sender is showing. With it, that tree is also the one the receiver
already saw, grown rather than replaced. Both proof kinds are RFC 9162's
(`log prove`, `log bridge`); `log head` is what a reader keeps between
visits and `log audit` walks the log against the shelves, which is how a
deletion becomes visible when every signature that is left is still
good.

The head itself is signed when the store holds a signing key, and the bag
carries it. `check` says who signed it, and whether that key is one this
reader accepts, because a head nobody vouched for makes a bag internally
consistent and unattributed: every proof in it is against a head the bag
asserted about itself.

`vouch sign` and `vouch check` cover the manifest of a bag rather than
the deeds inside it. They answer who packed it; the proofs answer
whether what is in it predates the packing. A receiver wants both.

Other kinds and store law: [DESIGN.md](DESIGN.md).

```
work happens
     |
deedar.create --> deed + evidence
                       |
        get / list ----+--> a viewer paints face
        next work -----+--> --input ids --> trail
        evidence ------+--> check the deed, its bytes, and sources
                       |      (several ids, or - for a list on stdin)
        leave ---------+--> dest/{id} + hash sidecar
        timestamp -----+--> RFC 3161 TimeStampReq over evidence
        current -------+--> follow --supersedes to the tip
                       |      (several ids, or - for a list on stdin)
        migrate -------+--> write {store}/layout when missing
        export --------+--> dest/{id} + proof.txt against the log head
        check ---------+--> the receiving end: bytes, leaf, path, head
                       |      (--since a bridge, for a head kept earlier)
```

## Build

```
just check
```

## License

MIT
