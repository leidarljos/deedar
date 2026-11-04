Each section is one task. ``DEEDAR_URL`` names the store; ``--url`` overrides
it for one command.

Cite deeds from a list
======================

``evidence``, ``current`` and ``export`` take one accession, several, or ``-`` to
read them from standard input. A tracker or a pack hands its citations over
on a pipe:

.. code:: console

   $ packset accessions seat | deedar current -
   $ vissue recall proj-1a2b --deeds-only | deedar evidence -

Both exit non-zero when any accession fails, so a shell hook can stop on them.

Sign what you hand over
=======================

.. code:: console

   $ mkdir -p ~/.config/deedar
   $ head -c 32 /dev/urandom > ~/.config/deedar/host.key && chmod 600 ~/.config/deedar/host.key
   $ deedar log head
   size=34 root=... ed25519 b2942e7e... <signature>
   $ deedar vouch sign bag/manifest-sha256.txt

That path is the default: with ``DEEDAR_HOST_SIGNING_KEY`` unset the key at
``~/.config/deedar/host.key`` signs when it exists. Set the variable to sign
with a key kept elsewhere, or to ``off`` to sign nothing.

The key lives outside the store. A receiver lists the keys they accept in
their own store's ``layout`` file, one ``signer = ed25519:<hex>`` line each,
and ``deedar vouch check bag/manifest-sha256.txt`` then says ``(accepted)``.

Keep a head between handovers
=============================

.. code:: console

   $ deedar check bag --keep sender.head          # on the receiver
   $ deedar log bridge 34 > bag2/bridge.txt       # on the sender, at the next export
   $ deedar check bag2 --since sender.head        # on the receiver

``--keep`` is available from the next release, not in 0.4.0.

The bridge is the consistency path from the head you kept to the new one.
``check`` requires it to start at the kept head and end at the head the
bag's deeds are proven against, so a log rewritten in between fails. A
bridge on its own proves nothing here: the sender picks both of its heads.

Take a handover into a store
============================

.. code:: console

   $ deedar import bag
   $ deedar import bag --allow-unsigned-head      # an unsigned head

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
A store that says ``attestation = required`` still imports under that flag,
and warns. ``check`` reports a signer the layout does not list, warns, and
exits ``EXIT_UNTRUSTED_SIGNER``. It does not fail the deeds. A signature
that does not cover the bytes is still status 1.

Audit a store
=============

.. code:: console

   $ deedar log audit

Walks the log and asks the store for each deed. A deed that was logged and
is gone, or changed under its accession, is tampering. Deeds on the shelves
that were never logged mean the store predates the log and wants a backfill.

Backfill a log
==============

.. code:: console

   $ deedar log backfill

Appends every deed the store serves that the log does not name, in accession
order. The entries carry the evidence's time when there is one, else now;
the log says these were logged today.

Demand attestation
==================

``evidence`` fails a deed whose host sidecar is not signed by a key the
layout accepts. ``get`` still returns the deed. In the store's ``layout``:

.. code:: text

   1
   attestation = required
   signer = ed25519:b2942e7e...

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

   $ deedar delete deed-file-the-parser-patch

The deed is tombstoned, the log entry stays, and ``log audit`` reports the
gap. Nothing is erased, so a citation of a deleted deed still reads as
deleted rather than as never having existed.

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

Set ``DEEDAR_URL`` in that client's environment when the store is not the
seat store. ``DEEDAR_HOST_SIGNING_KEY`` is the same variable the command line
reads.

Each line is one tool the server registers. ``reads`` leaves every deed as it
was. ``writes`` also writes a directory the caller names.

- ``deedar_check`` reads. Check deeds somebody else handed over: that each one's bytes match the digest its log entry recorded, and that its path reaches the head the bag names.
- ``deedar_current`` reads. The deed that supersedes this one, or this one when nothing does. A fork or a cycle is every current deed in accession order, with a note of which, and the call succeeds.
- ``deedar_evidence`` reads. Whether the store still holds evidence for a deed, and what it says: who produced it and when.
- ``deedar_export`` writes. Write deeds into a directory with the proof they were logged here first, which is what a handover carries.
- ``deedar_get`` reads. One deed by accession, or by a ``sha256:`` of the deed or of one of its product paths.
- ``deedar_list`` reads. Deeds the store serves, in accession order. kind and agent match exactly; text is a case-insensitive substring of the name or of a body's excerpt, title, subject, step text or url. Omit every argument for the whole shelf.
- ``deedar_log_audit`` reads. Whether the store answers for what it logged, and logged what it serves.
- ``deedar_log_bridge`` reads. The record joining a head a reader already holds to this log's own, so they can tell a log that grew from one that was rewritten.
- ``deedar_log_head`` reads. The append-only log's head: how many deeds it covers and the Merkle root over them.
- ``deedar_trail`` reads. The deed and every input it was made from: its sources that are deeds, and theirs, walked back to the leaves.

Two prompts, ``check_a_handover`` and ``stand_behind_a_deed``, sequence a
handover check and standing behind one deed.
