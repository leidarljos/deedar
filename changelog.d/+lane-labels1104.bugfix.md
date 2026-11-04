`deedar host accept` adds the host key when the layout says `attestation = required`, and warns.
`deedar check` and `deedar vouch check` report a key the layout does not list instead of failing.
`deedar import --allow-unsigned-head` keeps those deeds too, and warns.
