# Working on deedar

A command does the thing it was asked. What the layout says about who may
sign is a label on the result, printed as `deedar: warning:` on stderr, and
the report says whether a key was accepted.

- `deedar host accept` adds the host key, except when the layout says
  `attestation = required`. That store gains a signer by hand or not at
  all. The command refuses and names the line to add.
- `deedar check` and `deedar vouch check` report a signature by a key the
  layout does not list, warn, and exit `EXIT_UNTRUSTED_SIGNER`. The deeds,
  or the file, still check. A signature that does not cover the bytes
  still fails with status 1.
- `deedar import --allow-unsigned-head` keeps the deeds when the layout
  says `attestation = required`, and warns. Without the flag, an unsigned
  or unlisted head is not imported.

What still stops a command is the record. A frozen or tombstoned accession,
a hash that does not match, a bag that changed while it was read, a
malformed line, and a missing file are the deed not being the deed. The
command says so and writes nothing. `evidence` still fails a deed whose
host sidecar is not signed by a key the layout lists; `get` returns the deed.
