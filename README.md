# deeder

A unit of work produces something. deeder records it as a **deed**:
here is the work, this is its identity, take it. The next unit
opens that deed by `id`.

deeder is the writer. A deed is the noun. `quote`, `patch`, and
`file` are kinds. `DEEDER_URL` is the contract. Clients speak
`schema/deeder.capnp`: `create`, `get`, `list`, `trail`, `evidence`,
`delete`, `leave`, `timestamp`, `current`. `get` accepts a slug or a
unique `sha256:` of the deed or of one product path. Next work
takes deed ids as `--input`; they become `sources`. `deeder migrate`
writes `{store}/layout` when that file is missing; `open` does the
same.

There is no daemon. Each command opens the store at `DEEDER_URL`.

## First path

Every verb. A quote, a file, a patch that cites the quote. The next
sitting opens only the patch id.

```sh
rm -rf /tmp/deeder-demo
mkdir -p /tmp/deeder-demo
printf 'a short note\n' > /tmp/deeder-demo/note.md
printf '%s\n' \
  '--- a/note.md' '+++ b/note.md' \
  '@@ -1 +1 @@' '-a short note' '+a named note' \
  > /tmp/deeder-demo/note.patch
export DEEDER_URL=file:///tmp/deeder-demo/store

just deeder create quote \
  --id deed-quote-rfc2094-nll \
  --name "NLL: lifetimes from the CFG" \
  --edition https://rust-lang.github.io/rfcs/2094-nll.html \
  --excerpt "lifetimes that are based on the control-flow graph" \
  --src-url https://rust-lang.github.io/rfcs/2094-nll.html \
  --agent reader

just deeder create file \
  --id deed-file-note \
  --name "the note" \
  --path /tmp/deeder-demo/note.md \
  --media text/plain \
  --agent reader

just deeder create patch \
  --id deed-patch-note \
  --name "name the note" \
  --tree /tmp/deeder-demo \
  --diff /tmp/deeder-demo/note.patch \
  --functionary reader \
  --agent reader \
  --input deed-quote-rfc2094-nll

just deeder get deed-patch-note
just deeder trail deed-patch-note
just deeder current deed-patch-note
just deeder evidence deed-file-note
just deeder evidence deed-patch-note
just deeder leave deed-file-note /tmp/deeder-demo/left
just deeder timestamp deed-file-note
just deeder list
just deeder migrate
just deeder delete deed-file-note
```

`trail` walks `--input` from the patch to the quote. `current`
follows a later take (`--supersedes`) to the tip; `get` still
returns the named frozen deed. `get` also accepts a unique
`sha256:` of the deed bytes or of one product path. `evidence`
checks the deed, its write-once bytes, and every deed in
`sources`. A `DEEDER_HOST_KEY` (or `{store}/../host.key`) requires
`{id}.host`. `leave` copies the file bytes to `dest/{id}/` with a
hash sidecar (`manifest.json`). `timestamp` writes an RFC 3161
TimeStampReq over those evidence bytes (`.tsq` when `DEEDER_TSA` is
unset; `.tsr` when the authority replies). `migrate` ensures
`{store}/layout`. `delete` writes a tombstone; `evidence` on that id
then fails.

Other kinds and store law: [DESIGN.md](DESIGN.md).

```
work happens
     |
deeder.create --> deed + evidence
                       |
        get / list ----+--> a viewer paints face
        next work -----+--> --input ids --> trail
        evidence ------+--> check the deed, its bytes, and sources
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
