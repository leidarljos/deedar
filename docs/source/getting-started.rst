Everything below runs in a scratch store and touches nothing else. By the
end you will have minted a deed, superseded it, packed it with its receipt
for another machine, and checked it there without the sender's store.

1. Point at a store
===================

A store is a directory. It is created on first write.

.. code:: console

   $ export DEEDAR_URL=file:///tmp/deeds
   $ deedar list

2. Mint a deed
==============

.. code:: console

   $ echo 'fn main() {}' > /tmp/note.rs
   $ deedar create file --name "the parser patch" --path /tmp/note.rs --agent you
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506b...
   producedBy=you -

The accession ``deed-file-the-parser-patch`` is what everything else cites.
The bytes are stored once under their hash; the deed names them.

3. Check it
===========

.. code:: console

   $ deedar evidence deed-file-the-parser-patch
   id=deed-file-the-parser-patch ok
   $ deedar current deed-file-the-parser-patch
   id=deed-file-the-parser-patch

``evidence`` says the bytes are intact. ``current`` says the deed is still the
tip: nothing has superseded it.

4. Supersede it
===============

A deed is never edited. A better take is a new deed that names the old one.

.. code:: console

   $ echo 'fn main() { println!("parsed"); }' > /tmp/note.rs
   $ deedar create file --name "the parser patch" --path /tmp/note.rs --agent you \
       --supersedes deed-file-the-parser-patch
   id=deed-file-the-parser-patch-2 ...
   $ deedar current deed-file-the-parser-patch
   id=deed-file-the-parser-patch -> deed-file-the-parser-patch-2

A citation of the first deed still resolves, and ``current`` tells the
citer it has moved on. That is the whole reason the tracker cites an
accession and not a path.

5. Hand it over with its receipt
================================

.. code:: console

   $ deedar export --into /tmp/bag/data/deeds deed-file-the-parser-patch-2
   exported 1 deeds, 4 files

The bag carries the deed, its bytes, its evidence, and a receipt: the
inclusion path from the deed to the log head at export time, plus the
signed head when the store has a key.

6. Check it on another machine
==============================

The receiver needs no store and no network.

.. code:: console

   $ deedar check /tmp/bag
   1 deeds proven against a log of 2 entries, root 3d8e0155...
   the head is unsigned, so every proof above is against a head this bag asserted about itself

Set ``DEEDAR_HOST_SIGNING_KEY`` on the sender to a 32-byte seed and the head
and the manifest come signed; add the verifying key to the receiver's
``layout`` and the last line reads ``(accepted)``. On the next handover from the
same sender, ``deedar check /tmp/bag2 --since head.txt`` also shows the log
grew from the head you kept, and was not rewritten.

Where next
==========

-  :doc:`How-to <howto>`: sign a satchel, audit a store, backfill a log, demand attestation.
-  :doc:`Reference <reference>`: kinds, verbs, the log, the store layout.
-  :doc:`Explanation <explanation>`: why signatures alone do not bind a set, and what the log adds.
