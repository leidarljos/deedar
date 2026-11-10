Install the binary first, on the :doc:`installation <install>` page.
Everything below runs in a scratch store and touches nothing else. By the
end you will have minted a deed, superseded it, packed it with its receipt
for another machine, and checked it there. The receiver's command reads
the bag. The sender's store stays on the sender. The transcripts were taken
with the clock stopped. Run them yourself and the time and the root will
differ; the other lines will not. The parts this session builds are drawn
on the :doc:`concepts <concepts>` page.

1. Point at a store
===================

A store is a directory. It is created on first write. With no deeds,
``list`` prints nothing.

.. code:: console

   $ export DEEDAR_URL=file:///tmp/deeds
   $ export DEEDAR_HOST_SIGNING_KEY=off
   $ deedar list

2. Mint a deed
==============

.. code:: console

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

The accession ``deed-file-the-parser-patch`` is what everything else cites.
The bytes are stored once under their hash; the deed names them.

3. Check it
===========

.. code:: console

   $ deedar evidence deed-file-the-parser-patch
   id=deed-file-the-parser-patch ok
   producedBy=you -
   time=1700000000
   $ deedar current deed-file-the-parser-patch
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4

``evidence`` says the bytes are intact. ``current`` prints the tip, which is
this deed until something supersedes it.

4. Supersede it
===============

A deed is never edited. A better take is a new deed that names the old one.

.. code:: console

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
   $ deedar current deed-file-the-parser-patch
   id=deed-file-the-parser-patch-v2 kind=file name=the parser patch v2
   paths=sha256:49e7b69a7a6720e9928f98f9d095871453e1a3024276f713aeab2cfc0ef2ba07
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:49e7b69a7a6720e9928f98f9d095871453e1a3024276f713aeab2cfc0ef2ba07

A citation of the first deed still resolves. ``current`` prints the tip, so
the accession in that answer is the new deed. The tracker cites that
accession. A path names a place on one machine.

5. Hand it over with its receipt
================================

.. code:: console

   $ deedar export --into /tmp/bag/data/deeds deed-file-the-parser-patch-v2
   exported 1 deeds, 5 files

The bag carries the deed, its bytes, its evidence, and a receipt: the
inclusion path from the deed to the log head at export time, plus the
signed head when the store has a key.

6. Check it on another machine
==============================

The receiver needs no store and no network.

.. code:: console

   $ deedar check /tmp/bag
   1 deeds proven against a log of 2 entries, root d4d103452fb3ae6627162c8bc78f4169a47a31e825866693b548874ff5e59eca
   the head is unsigned, so every proof above is against a head this bag asserted about itself

Set ``DEEDAR_HOST_SIGNING_KEY`` on the sender to a 32-byte seed and the head
and the manifest come signed; add the verifying key to the receiver's
``layout`` and the last line reads ``(accepted)``. On the next handover from the
same sender, record the head with ``deedar check /tmp/bag --keep head.txt``;
on the next handover ``deedar check /tmp/bag2 --since head.txt`` also shows the
log grew from that head, and was not rewritten.

Where next
==========

-  :doc:`How-to <howto>`: sign a satchel, take a handover in, audit a store, backfill a log, demand attestation.
-  :doc:`Concepts <concepts>`: the bytes, the deed, the log, the bag, and the citation.
-  :doc:`Explanation <explanation>`: what a signature covers, and what the log adds.
-  :doc:`Reference <reference>`: kinds, verbs, the log, the store layout.
-  :doc:`API reference <api>`: the ``deed`` and ``deedar`` crates, generated from rustdoc.
