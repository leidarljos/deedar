# rfc9162

The Merkle tree of RFC 9162 (Certificate Transparency 2.0, doi:10.17487/RFC9162) as pure
functions over 32-byte leaf hashes: the root, inclusion proofs and consistency proofs, and the
checks for both. Leaf `SHA-256(0x00 || entry)`, node `SHA-256(0x01 || left || right)`.

It stores nothing and serialises nothing. A user computes each leaf from the canonical bytes of
its own record, keeps its own file format, and signs `Head::material(domain)` with a domain that
names its log. There is no gossip: two readers shown two different consistent logs are not told.

```rust
use rfc9162::{inclusion_proof, leaf_hash, verify_inclusion, Head};

let leaves: Vec<[u8; 32]> = ["a", "b", "c"].iter().map(|r| leaf_hash(r.as_bytes())).collect();
let head = Head::over(&leaves);
let path = inclusion_proof(&leaves, 1).unwrap();
assert!(verify_inclusion(&leaves[1], 1, head.size, &path, &head.root));
```

Split out of [deedar](https://github.com/leidarljos/deedar), which uses it for its deed log.
MIT.
