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

`trail` walks `--input` from the patch to the quote. `current`
follows a later take (`--supersedes`) to the tip; `get` still
returns the named frozen deed. `get` also accepts a unique
`sha256:` of the deed bytes or of one product path. `evidence`
checks the deed, its write-once bytes, and every deed in
`sources`. A `DEEDAR_HOST_KEY` (or `{store}/../host.key`) requires
`{id}.host`. `leave` copies the file bytes to `dest/{id}/` with a
hash sidecar (`manifest.json`). `timestamp` writes an RFC 3161
TimeStampReq over those evidence bytes (`.tsq` when `DEEDAR_TSA` is
unset; `.tsr` when the authority replies). `migrate` ensures
`{store}/layout`. `delete` writes a tombstone; `evidence` on that id
then fails.

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
        migrate -------+--> write {store}/layout when missing
```

## Build

```
just check
```

## License

MIT
