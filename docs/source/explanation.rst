===========
Explanation
===========



A product, frozen
-----------------

Work produces things: a patch, a table, a quote from a source, a page as it
looked. The next unit of work needs to open the thing, not reread the
conversation that made it. A deed is that thing, frozen: the bytes by
hash, the producer, the inputs it was made from, and a kind that says what
shape it is. Because the bytes are content addressed, a deed cannot drift.
Because a better take is a new deed with ``--supersedes``, a citation of the
old one still resolves and ``current`` says where it went.

Why a log
---------

Every deed is signed by the store's writer key, and a signature says who
wrote a deed. It does not say what else the store contains. A holder can
hand one reader the store and another the same store with a deed removed,
and both sets verify. ``delete`` is a supported verb, so this is not a
hypothetical.

The log binds the set. Each deed appends a leaf, the leaves hash into a
Merkle tree, and the store signs the root and the size as a head. Two
proofs follow that a pile of signatures cannot give. An inclusion proof
shows a deed is in the tree a head names, so a store that dropped it cannot
produce a head that still covers it. A consistency proof shows a later head
extends an earlier one rather than replacing it, so a store cannot rewrite
what it published without every reader who kept a head noticing. The
construction is Certificate Transparency's (https://doi.org/10.17487/RFC9162), and it
fits here because the party being audited is the party serving the data.

What the log does not do is gossip. One store signing its own heads catches
a holder who rewrites history between two readings by the same reader, and
catches deletion. It does not catch a holder who keeps two consistent logs
and shows one to each reader. That needs heads compared somewhere neither
controls, which is a network protocol rather than a file format.

Integrity, authenticity, authorisation
--------------------------------------

Three questions, three constructions. A hash answers integrity: these are
the bytes. A signature over the manifest answers authenticity: this key
made the bag, and a receiver checks it against the keys they accept. A
signed host sidecar answers authorisation: this agent, under this grant,
was entitled to produce these bytes. The sidecar is a signature and never a
keyed hash: a keyed-hash (HMAC) verifier has the key and can mint what it
checks, and with the key beside the store that includes the agent being
vouched for. A public key cannot mint.

One identifier
--------------

The accession is the only thing that crosses the stack. A tracker cites it
on a node; a pack cites it in a claim; this store answers ``get``, ``trail``,
``evidence`` and ``current`` for it. None of the three opens another's format.
The seat composes them by passing accessions on pipes, and a handover
carries the deeds beside the tracker slice and the claims that cite them.
