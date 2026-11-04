# Working on deedar

A command does the thing it was asked.
What the layout says about who may sign is a label on the result.
The label is printed as `deedar: warning:` on stderr.
The report says whether a key was accepted.

- `deedar host accept` adds the host key, except when the layout says `attestation = required`.
That store gains a signer by hand or not at all.
The command refuses and names the line to add.
- `deedar check` and `deedar vouch check` report a signature by a key the layout does not list.
They warn, and they exit `EXIT_UNTRUSTED_SIGNER`.
The deeds, or the file, still check.
A signature that does not cover the bytes still fails with status 1.
- `deedar import --allow-unsigned-head` keeps the deeds when the layout says `attestation = required`, and warns.
Without the flag, an unsigned or unlisted head is not imported.

What still stops a command is the record.
A frozen or tombstoned accession is not edited in place.
A hash that does not match is not the record.
A bag that changed while it was read is not written.
A malformed line and a missing file are not a deed.
The command says so and writes nothing.
`evidence` still fails a deed whose host sidecar is not signed by a listed key.
`get` returns the deed.
