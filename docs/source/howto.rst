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

   $ deedar check bag > /dev/null && deedar log head > sender.head
   $ deedar log bridge 34 > bag2/bridge.txt   # on the sender, at the next export
   $ deedar check bag2 --since bag2/bridge.txt

The bridge is the consistency path from the head you kept to the new one.
A log rewritten in between cannot produce it.

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

A store can refuse to hand over a deed whose host sidecar is not signed by a
key it accepts. In the store's ``layout``:

.. code:: text

   1
   attestation = required
   signer = ed25519:b2942e7e...

Without the line, attestation is optional and a keyed-hash sidecar still
opens. A keyed hash proves the bytes are the bytes; it cannot prove who was
entitled to make them, because whoever can check it can mint it.

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

Every tool reads: ``deedar_get``, ``deedar_trail``, ``deedar_evidence``,
``deedar_current``, ``deedar_check``, ``deedar_log_head``, ``deedar_log_bridge``,
``deedar_log_audit``. Two prompts sequence a handover check and standing
behind one deed.
