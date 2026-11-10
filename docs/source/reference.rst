Install paths are on the :doc:`installation <install>` page. The
:doc:`API reference <api>` is generated from the library crates. The
:doc:`changelog <changelog>` lists what each version changed. The
:doc:`concepts <concepts>` page is the picture of the files this page names.

Command line
============

``deedar [--url FILE] VERB ...``.
The store is ``--url``, else ``DEEDAR_URL``, else the seat's own store under the user's data directory (``$XDG_DATA_HOME/deedar/store``).
``create`` and ``import`` make that store on the first write, and print ``deedar: created the store at PATH`` on stderr.
A read names the missing path and stops.

=================================================================================== ===============================================================================================================================================================================================================================================================
Verb                                                                                Does
=================================================================================== ===============================================================================================================================================================================================================================================================
``create KIND --name N --agent A [--path P] [--input DEED]... [--supersedes DEED]`` mint one deed; kind-specific flags below
``get ID``                                                                          one deed, flattened
``list``                                                                            every deed in accession order; ``--kind``, ``--agent`` and ``--text`` narrow it
``trail ID``                                                                        the deed and every input it was made from
``evidence ID... | -``                                                              the bytes are intact; one line each; non-zero on any failure
``current ID... | -``                                                               the tip of each supersede chain; non-zero when any citation is stale or missing
``delete ID``                                                                       tombstone
``leave ID``                                                                        a left-behind marker for a deed that moved store
``timestamp ID``                                                                    a timestamp token over the deed
``export --into DIR ID... | -``                                                     deed, bytes, evidence, receipt (and signed head) into a satchel
``import DIR [--allow-unsigned-head]``                                              check a satchel and hold its deeds here with the receipt fields they travelled with; nothing is written unless every deed can be held, ``--allow-unsigned-head`` waives a missing or unlisted signature, and ``attestation = required`` warns and still imports
``check DIR [--since KEPT] [--bridge FILE] [--keep FILE]``                          every receipt against the head in the bag; with ``--since``, that the bag's bridge (``bridge.txt``) grows the head you kept into this one; ``--keep`` records this head for next time
``log head | list | audit | backfill | prove ID | bridge SIZE``                     the append-only log
``vouch sign FILE | check FILE``                                                    a detached Ed25519 signature beside a file
``migrate``                                                                         move a store to the current layout
``host``                                                                            whether the store's layout lists the host signing key; exits 1 when it does not
``host accept``                                                                     add the host signing key to the layout as a signer, and leave every other line. Refuses when the layout says ``attestation = required``
``url``                                                                             print the store URL this command resolved: ``--url``, else ``DEEDAR_URL``, else the seat store when that directory is already there
=================================================================================== ===============================================================================================================================================================================================================================================================

Exit codes
==========

+------+------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------+
| Code | When                                                                                                                                                                                                                                                                                                                                                                                                 |
+======+======================================================================================================================================================================================================================================================================================================================================================================================================+
| 0    | the command did what it was asked. ``evidence`` and ``current`` over several accessions also exit 0 when every one is ok                                                                                                                                                                                                                                                                             |
+------+------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------+
| 1    | the command stopped short of what it was asked. An unknown accession, an ambiguous prefix, an unset ``DEEDAR_URL``, and a path outside a store are this code, and so is every other refusal, including ``host`` when the layout does not list the signing key. ``evidence`` and ``current`` over several accessions exit 1 when any line failed. A refusal prints one stderr line, ``deedar:`` and   |
|      | the reason, naming the input and the next command. An ambiguous prefix also lists the matching accessions, in byte order, up to the bound that line states                                                                                                                                                                                                                                           |
+------+------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------+
| 2    | ``check`` and ``vouch check`` finished, and the signer falls outside the accepted set                                                                                                                                                                                                                                                                                                                |
+------+------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------+

The constant is ``EXIT_UNTRUSTED_SIGNER``.
Stdout is the report and stderr is ``deedar: warning:``.
A signature that does not cover the bytes is still 1.

Kinds
=====

``file``, ``set``, ``quote``, ``patch``, ``mailDraft``, ``clip``, ``page``, ``form``,
``table``, ``procedure``, ``event``. The flags a kind reads:

============= ===========================================================================
Kind          Flags
============= ===========================================================================
``file``      ``--path``
``set``       ``--member PATH``, repeatable
``quote``     ``--excerpt``, ``--edition``, ``--start``, ``--end``, ``--src-url``
``patch``     ``--tree``, ``--diff PATH`` repeatable, ``--functionary`` repeatable
``mailDraft`` ``--message-id``, ``--in-reply-to``, ``--subject``
``clip``      ``--media``, ``--clip-src`` repeatable, ``--in``, ``--out``, ``--duration``
``page``      ``--page-url``, ``--snapshot``
``form``      ``--blank``, ``--field name=value`` repeatable
``table``     ``--measure name=value`` repeatable
``procedure`` ``--step TEXT`` repeatable
``event``     ``--title``, ``--when``, ``--where``, ``--who``
============= ===========================================================================

Every kind takes ``--name``, ``--agent``, ``--activity``, ``--input DEED``
(repeatable), ``--grant-host``, ``--grant-path``, ``--supersedes DEED``, and
``--id`` to keep an accession instead of minting one.

Accessions
==========

``deed-<kind>-<slug>`` names a deed. ``sha256:<hex>`` names the canonical deed
or one of its product paths. Both resolve with ``get``. A tracker or a pack
cites either and nothing else.

The store
=========

A directory with ``bytes/`` (content-addressed), ``deeds/`` (canonical
records), ``log`` (one entry a line: content address, time), ``layout``
(version, attestation policy, accepted signers), and ``writer.key``. A deed
taken in with ``import`` keeps its receipt and the head it is against beside
it, as ``deeds/ID.proof`` and ``deeds/ID.head``, so its inclusion proof
outlives the bag.

A handover
==========

``export --into DIR`` writes ``DIR/<id>/`` for each deed and one ``head``
beside those folders. Wrapped as a satchel, that directory is
``data/deeds``. A producer writing the bag by hand uses the same files.
Each folder is named for the accession in its ``deed.bin`` and
``proof.txt``, and a folder with any other name is refused.
Every path is a regular file or a real directory. A symlink or a fifo is
refused. A symlinked deed folder stays unopened.

``deed.bin`` is the canonical deed bytes. ``proof.txt`` names that digest
and the inclusion path:

::

   id=<accession>
   digest=sha256:<64 lowercase hex>
   time=<unix seconds>
   index=<leaf index>
   size=<log length>
   root=<64 lowercase hex>
   path=<64 lowercase hex>

one ``path`` line per step from the leaf to the root. The digest is
SHA-256 of ``deed.bin``. The log entry is ``<id><tab><digest><tab><time>``,
and the leaf is SHA-256 of a ``0x00`` byte followed by that entry.

``bytes/<hex>`` is the product file for a deed path ``sha256:<hex>``.
``<hex>`` is 64 lowercase hex digits and is the SHA-256 of that file.

``supersedes`` and ``successor`` are two lines, the accession and then the
digest:

::

   <accession>
   sha256:<64 lowercase hex>

``sources`` is one tab-separated line per deed the bag names:

::

   <accession><tab>sha256:<64 lowercase hex>

``evidence.bin`` and ``host`` are copied when the store has them. ``check``
does not read ``evidence.bin``.

``head``, when the bag is signed, is one line:

::

   size=<n> root=<64 hex> links=<64 hex> ed25519 <key hex> <signature hex>

``links`` is lowercase hex with no ``sha256:`` prefix. It is the SHA-256 of
this transcript. Write ``deedar-links-v2`` and a ``0x00`` byte, then take
the deed directories in byte order of their names. For each, write the
decimal byte length of the name, a newline, and the name. Then for
``sources``, ``supersedes`` and ``successor`` in that order write the kind
and a newline, then ``-`` and a newline when the file is absent, or the
decimal byte length, a newline, and the raw file bytes when it is present.
An empty file has length 0. An absent file is the dash line, a separate case.
The Ed25519 signature covers, with ``||`` concatenation:

``deedar-head-v1 || 0x00 || "<size> <root>" || 0x00 || <links hex>``

``<size>`` and the root are the decimal length and the hex root, separated
by one space. A head line with no ``links`` field still verifies, and only
when none of those three files is present. An export always writes
``links``, including when no folder has a sidecar, so adding one later
breaks the signature. An unsigned bag omits ``head``. The sidecars are
then not authenticated. ``deedar log head`` prints the size and the root
only. The signed head line above is a different record.

The transcript before ``deedar-links-v2`` had no tag, ended a name at a
newline, and gave an absent file length 0, so one directory name could
stand for two folders and an empty file for a missing one. A head over
that transcript is refused. Verification applies to the current transcript.
No release wrote the old one, and
exporting the bag again writes the current one.

Import stores ``<id>.supersedes`` as the same two lines. A file that
already holds just the accession is completed with the digest.

The log
=======

Certificate Transparency hashing, RFC 9162 (doi:10.17487/RFC9162): a leaf is ``SHA-256(0x00 || entry)``,
a node ``SHA-256(0x01 || left || right)``, the tree splits at the largest power
of two below the width. ``head`` is ``size`` and ``root``, signed when a key is
set. ``prove ID`` is the inclusion path; ``bridge SIZE`` the consistency path
from an earlier head; ``audit`` the log against the shelves.

Environment
===========

=========================== ============================================================================================================================================================================================================================================
Variable                    Meaning
=========================== ============================================================================================================================================================================================================================================
``DEEDAR_URL``              the store, as ``file://PATH``. ``--url`` overrides it for one command
``DEEDAR_HOST_SIGNING_KEY`` a 32-byte Ed25519 seed, or a path to one; signs heads, sidecars and manifests. Unset, ``$XDG_CONFIG_HOME/deedar/host.key`` is used when it exists, or ``~/.config/deedar/host.key`` when ``XDG_CONFIG_HOME`` is unset. ``off`` signs nothing
``DEEDAR_HOST_KEY``         a 32-byte keyed-hash key for the host sidecar. Unset, the file is ``{store}/../host.key``
``DEEDAR_TSA``              URL of an RFC 3161 timestamp authority. ``timestamp`` writes a ``.tsr`` when the authority answers, and a ``.tsq`` when the variable is unset or the authority times out
=========================== ============================================================================================================================================================================================================================================

Crates
======

============== ==================================================
Crate          Carries
============== ==================================================
``deed``       the deed types and canonical encoding
``deedar``     store, log, receipts, signatures, attestation
``deedar-cli`` ``deedar``
``deedar-mcp`` the read-only Model Context Protocol (MCP) surface
============== ==================================================
