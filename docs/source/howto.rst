Each section is one task. ``DEEDAR_URL`` names the store; ``--url`` overrides
it for one command. Each transcript was taken in its own empty store with
the clock stopped, so a root or a signature here is the one those commands
produce. A run at another time prints a different one. The
:doc:`installation <install>` page builds the binary, and the
:doc:`tutorial <getting-started>` mints the first deed. The
:doc:`concepts <concepts>` page names the bag, the head, and the receipt
these tasks pass around.

Cite deeds from a list
======================

``evidence``, ``current`` and ``export`` take one accession, several, or ``-`` to
read them from standard input. A tracker or a pack hands its citations over
on a pipe, and the ``printf`` below stands in for that tracker. ``current``
exits non-zero when any citation is stale or missing, and ``evidence`` exits
non-zero when any deed does not check, so a shell hook can stop on them.

.. code:: console

   $ export DEEDAR_URL=file:///tmp/deeds
   $ export DEEDAR_HOST_SIGNING_KEY=off
   $ echo 'fn main() {}' > /tmp/note.rs
   $ deedar create file --name "the parser patch" --path /tmp/note.rs --agent you
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   accession=deed-file-the-parser-patch
   $ echo 'fn main() { println!("parsed"); }' > /tmp/note.rs
   $ deedar create file --name "the parser patch v2" --path /tmp/note.rs --agent you --supersedes deed-file-the-parser-patch
   id=deed-file-the-parser-patch-v2 kind=file name=the parser patch v2
   paths=sha256:49e7b69a7a6720e9928f98f9d095871453e1a3024276f713aeab2cfc0ef2ba07
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:49e7b69a7a6720e9928f98f9d095871453e1a3024276f713aeab2cfc0ef2ba07
   accession=deed-file-the-parser-patch-v2
   $ printf '%s\n' deed-file-the-parser-patch deed-file-the-parser-patch-v2 | deedar current -
   deed-file-the-parser-patch SUPERSEDED by deed-file-the-parser-patch-v2
   deed-file-the-parser-patch-v2 current
   1 of 2 current
   $ printf '%s\n' deed-file-the-parser-patch deed-file-the-parser-patch-v2 | deedar evidence -
   deed-file-the-parser-patch ok
   deed-file-the-parser-patch-v2 ok
   2 of 2 verified

Sign what you hand over
=======================

The seed below is 32 zero bytes, so the signature on this page is the one
those bytes produce. A key you will keep is 32 bytes from ``/dev/urandom``.

.. code:: console

   $ export DEEDAR_URL=file:///tmp/deeds
   $ export DEEDAR_HOST_SIGNING_KEY=/tmp/host.key
   $ head -c 32 /dev/zero > /tmp/host.key
   $ chmod 600 /tmp/host.key
   $ echo 'fn main() {}' > /tmp/note.rs
   $ deedar create file --name "the parser patch" --path /tmp/note.rs --agent you
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   accession=deed-file-the-parser-patch
   $ deedar host
   host key: ed25519:3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29, a signer /tmp/deeds/layout lists
   $ deedar log head
   size=1 root=b1799b9f22404b3626b3c43e80f26232922113b997a1e3377bc803ff799f4ee8 ed25519 3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29 ba6b3117579ce1f02ad154b4cd0d41b84360cb7abf81bb118450edc3f0fff39ad7e18c0504f3987a4478a51669d3628aba53bc7775d3e6016ab475ff3e12f90e
   $ deedar export --into /tmp/bag/data/deeds deed-file-the-parser-patch
   exported 1 deeds, 6 files
   $ printf '%s\n' 'sample manifest' > /tmp/bag/manifest-sha256.txt
   $ deedar vouch sign /tmp/bag/manifest-sha256.txt
   /tmp/bag/manifest-sha256.txt.sig
   $ deedar vouch check /tmp/bag/manifest-sha256.txt
   signed by 3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29 (accepted)

With ``DEEDAR_HOST_SIGNING_KEY`` unset, the key at
``~/.config/deedar/host.key`` signs when it exists. Set the variable to sign
with a key kept elsewhere, or to ``off`` to sign nothing.

The key lives outside the store. A receiver lists the keys they accept in
their own store's ``layout`` file, one ``signer = ed25519:<hex>`` line each,
and ``deedar vouch check bag/manifest-sha256.txt`` then says ``(accepted)``.

Keep a head between handovers
=============================

.. code:: console

   $ export DEEDAR_URL=file:///tmp/deeds
   $ export DEEDAR_HOST_SIGNING_KEY=off
   $ echo 'fn main() {}' > /tmp/note.rs
   $ deedar create file --name "the parser patch" --path /tmp/note.rs --agent you
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   accession=deed-file-the-parser-patch
   $ deedar export --into /tmp/bag/data/deeds deed-file-the-parser-patch
   exported 1 deeds, 4 files
   $ deedar check /tmp/bag --keep /tmp/sender.head
   1 deeds proven against a log of 1 entries, root b1799b9f22404b3626b3c43e80f26232922113b997a1e3377bc803ff799f4ee8
   the head is unsigned, so every proof above is against a head this bag asserted about itself
   kept this head in /tmp/sender.head; pass it to --since next time
   $ echo second > /tmp/note2.rs
   $ deedar create file --name "the notes" --path /tmp/note2.rs --agent you
   id=deed-file-the-notes kind=file name=the notes
   paths=sha256:480c2336b410f1ad5f8bf1b28944490255804b65350c527787e74ebdd511e3a4
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:480c2336b410f1ad5f8bf1b28944490255804b65350c527787e74ebdd511e3a4
   accession=deed-file-the-notes
   $ deedar export --into /tmp/bag2/data/deeds deed-file-the-notes
   exported 1 deeds, 4 files
   $ deedar log bridge 1 > /tmp/bag2/bridge.txt
   $ deedar check /tmp/bag2 --since /tmp/sender.head
   the log grew from the 1 entries you kept to 2 without dropping or rewriting one
   1 deeds proven against a log of 2 entries, root 794e67fe9ec9c80cfd3df9dd6bf5eb62e74c669c06a63693dde7a2b9bd97acc5
   the head is unsigned, so every proof above is against a head this bag asserted about itself

The bridge is the consistency path from the head you kept to the new one.
``check`` requires it to start at the kept head and end at the head the
bag's deeds are proven against, so a log rewritten in between fails. A
bridge on its own proves nothing here: the sender picks both of its heads.

Take a handover into a store
============================

.. code:: console

   $ export DEEDAR_URL=file:///tmp/deeds
   $ export DEEDAR_HOST_SIGNING_KEY=off
   $ echo 'fn main() {}' > /tmp/note.rs
   $ deedar create file --name "the parser patch" --path /tmp/note.rs --agent you
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   accession=deed-file-the-parser-patch
   $ deedar export --into /tmp/bag/data/deeds deed-file-the-parser-patch
   exported 1 deeds, 4 files
   $ export DEEDAR_URL=file:///tmp/recv
   $ deedar import /tmp/bag
   deedar: evidence: this handover's head is unsigned, so every receipt in it is against a head the bag asserted about itself; nothing was imported, and --allow-unsigned-head imports it anyway
   $ deedar import /tmp/bag --allow-unsigned-head
   1 deeds proven against a log of 1 entries, root b1799b9f22404b3626b3c43e80f26232922113b997a1e3377bc803ff799f4ee8
   the head is unsigned, so every proof above is against a head this bag asserted about itself
   imported 1 deeds, appended 1 log entries
   $ deedar import /tmp/bag --allow-unsigned-head
   1 deeds proven against a log of 1 entries, root b1799b9f22404b3626b3c43e80f26232922113b997a1e3377bc803ff799f4ee8
   the head is unsigned, so every proof above is against a head this bag asserted about itself
   imported 0 deeds, appended 0 log entries

``import`` runs the same check as ``check``, then holds every deed in the bag
here: the record, its product bytes, its ``supersedes`` and ``successor``
links, and a log entry with the digest and time from its receipt. The
evidence is issued again under this store's ``writer.key``, over the same
bytes and time, and the receipt and its head are kept as ``deeds/ID.proof``
and ``deeds/ID.head``. Once the bag is gone, ``evidence``, ``trail``, ``current``
and ``log audit`` still answer here.

Nothing is written unless every deed can be held. A deed frozen here with
other bytes, tombstoned, or logged with another digest or time refuses the
whole bag, and so does one made from a source or revised by a successor
that neither the bag nor the store has. Importing the same bag again
writes nothing.

The bag's head has to be signed by a key this store's ``layout`` lists.
``--allow-unsigned-head`` waives a missing signature and a signature by a
key the layout does not list: deleting ``head`` makes those the same case.
A store that says ``attestation = required`` still imports under that flag, and warns.
``check`` reports a signer the layout does not list, warns, and exits ``EXIT_UNTRUSTED_SIGNER``.
It does not fail the deeds.
A signature that does not cover the bytes is still status 1.

Audit a store
=============

.. code:: console

   $ export DEEDAR_URL=file:///tmp/deeds
   $ export DEEDAR_HOST_SIGNING_KEY=off
   $ echo 'fn main() {}' > /tmp/note.rs
   $ deedar create file --name "the parser patch" --path /tmp/note.rs --agent you
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   accession=deed-file-the-parser-patch
   $ deedar log audit
   ok 1 logged and served

Walks the log and asks the store for each deed. A deed that was logged and
is gone, or changed under its accession, is tampering. Deeds on the shelves
that were never logged mean the store predates the log and wants a backfill.

Backfill a log
==============

.. code:: console

   $ export DEEDAR_URL=file:///tmp/deeds
   $ export DEEDAR_HOST_SIGNING_KEY=off
   $ echo 'fn main() {}' > /tmp/note.rs
   $ deedar create file --name "the parser patch" --path /tmp/note.rs --agent you
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   accession=deed-file-the-parser-patch
   $ deedar log backfill
   logged 0 deeds already on the shelves

Appends every deed the store serves that the log does not name, in accession
order. When the log already names every deed, as here, it logs none. The
entries carry the evidence's time when there is one, else now; the log says
these were logged today.

Demand attestation
==================

``evidence`` fails a deed whose host sidecar lacks a signature from a key the layout accepts.
``get`` still returns the deed.
In the store's ``layout``, with the public key of the sample seed from the signing section:

.. code:: text

   1
   attestation = required
   signer = ed25519:3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29

Without the line, attestation is optional and a keyed-hash sidecar still
opens. A keyed hash proves the bytes are the bytes; it cannot prove who was
entitled to make them, because whoever can check it can mint it.

The demand is worth what guards ``layout`` and the key. An agent that can
edit the layout can list a key of its own, and an agent running as the
key's owner can read the key. ``deedar host accept`` refuses to add a signer
to a store that says required; add the line by hand.

Retract a deed
==============

.. code:: console

   $ export DEEDAR_URL=file:///tmp/deeds
   $ export DEEDAR_HOST_SIGNING_KEY=off
   $ echo 'fn main() {}' > /tmp/note.rs
   $ deedar create file --name "the parser patch" --path /tmp/note.rs --agent you
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   accession=deed-file-the-parser-patch
   $ deedar delete deed-file-the-parser-patch
   deleted deed-file-the-parser-patch
   $ deedar log audit
   deedar: missing deed-file-the-parser-patch  deed tombstoned: deed-file-the-parser-patch

The deed is tombstoned, the log entry stays, and ``log audit`` reports the
gap. Nothing is erased, so a citation of a deleted deed still reads as
deleted, and the log entry shows the deed was there.

Serve the store to an agent
===========================

.. code:: console

   $ deedar-mcp

The server is the ``deedar-mcp`` binary. It speaks on standard input and
output and takes no arguments. ``deedar mcp`` prints ``unknown command mcp``.

With ``DEEDAR_URL`` unset or empty, the server uses the seat store, the same
path the command line uses: ``$XDG_DATA_HOME/deedar/store``, or
``~/.local/share/deedar/store``. Starting the process does not create that
directory. A read of a missing seat store returns the same hint as
``deedar list`` and creates nothing. ``deedar_export`` creates that store and
prints ``deedar: created the store at PATH``, then writes the handover or
reports a missing accession. ``deedar_check`` does not open the store: it
checks the bag it was given.

A client runs ``deedar-mcp`` with no arguments:

.. code:: json

   {
     "mcpServers": {
       "deedar": {
         "command": "deedar-mcp"
       }
     }
   }

Set ``DEEDAR_URL`` in that client's environment for a store you name. The
seat store is the default while the variable is unset.
``DEEDAR_HOST_SIGNING_KEY`` is the same variable the command line reads.

Each line is one tool the server registers. ``reads`` leaves every deed as it
was. ``writes`` also writes a directory the caller names.

-  ``deedar_check`` reads. Check deeds somebody else handed over: that each one's bytes match the digest its log entry recorded, and that its path reaches the head the bag names.
-  ``deedar_current`` reads. The deed that supersedes this one, or this one when nothing does. A fork or a cycle is every current deed in accession order, with a note of which, and the call succeeds.
-  ``deedar_evidence`` reads. Whether the store still holds evidence for a deed, and what it says: who produced it and when.
-  ``deedar_export`` writes. Write deeds into a directory with the proof they were logged here first, which is what a handover carries.
-  ``deedar_get`` reads. One deed by accession, or by a ``sha256:`` of the deed or of one of its product paths.
-  ``deedar_list`` reads. Deeds the store serves, in accession order. kind and agent match exactly; text is a case-insensitive substring of the name or of a body's excerpt, title, subject, step text or url. Omit every argument for the whole shelf.
-  ``deedar_log_audit`` reads. Whether the store answers for what it logged, and logged what it serves.
-  ``deedar_log_bridge`` reads. The record joining a head a reader already holds to this log's own, so they can tell a log that grew from one that was rewritten.
-  ``deedar_log_head`` reads. The append-only log's head: how many deeds it covers and the Merkle root over them.
-  ``deedar_trail`` reads. The deed and every input it was made from: its sources that are deeds, and theirs, walked back to the leaves.

Two prompts, ``check_a_handover`` and ``stand_behind_a_deed``, sequence a
handover check and standing behind one deed.
