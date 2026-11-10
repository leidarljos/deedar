.. raw:: html

   <div class="vi-hero">
     <p class="vi-hero-name">deedar</p>
     <p class="vi-hero-tagline">deedar stores the product of one unit of work as a deed: the bytes by hash, who produced them, and the deeds they were made from. Each deed is appended to a Merkle log. A reader who kept a head can show the deed was in that log, and a citation names the deed by its accession.</p>
     <div class="vi-hero-actions">
       <a class="vi-btn vi-btn-gold" href="getting-started.html">Get started</a>
       <a class="vi-btn vi-btn-ghost" href="install.html">Installation</a>
     </div>
   </div>

This site documents deedar 0.6.1.

Why
===

The next unit of work opens that product by accession. A better take is a
new deed with ``--supersedes``, so the earlier accession still resolves and
``current`` prints the tip. The log binds the deeds a store has published:
an inclusion proof places one deed under a head, and a consistency proof
shows a later head extends an earlier one. The
:doc:`concepts <concepts>` page draws the parts. The
:doc:`explanation <explanation>` says what a signature covers and what
the log adds. The :doc:`changelog <changelog>` records each version.

Guides
======

.. raw:: html

   <div class="vi-grid">
     <a class="vi-card" href="install.html"><strong>Installation</strong><span>Linux, macOS, and Windows, from a release archive, cargo binstall, crates.io, or a checkout.</span></a>
     <a class="vi-card" href="getting-started.html"><strong>Tutorial</strong><span>Mint a deed, supersede it, and check the bag on a second store.</span></a>
     <a class="vi-card" href="howto.html"><strong>How-to guides</strong><span>Sign a handover, import it, audit a log, backfill, and serve the store.</span></a>
     <a class="vi-card" href="concepts.html"><strong>Topics</strong><span>The concepts diagram, what the log adds, and the seat this crate sits in.</span></a>
     <a class="vi-card" href="reference.html"><strong>Reference</strong><span>Verbs, kinds, accessions, the store layout, the log, and the environment.</span></a>
     <a class="vi-card" href="api.html"><strong>API reference</strong><span>Public items of the deed and deedar crates, generated from rustdoc.</span></a>
   </div>

First minute
============

The block is a fresh store with signing off, recorded with the clock
stopped. Run it yourself and the time and the root will differ; the other
lines will not. The :doc:`tutorial <getting-started>` continues from
here: supersede the deed, hand it over, and check it.

.. code:: console

   $ export DEEDAR_URL=file:///tmp/deeds
   $ export DEEDAR_HOST_SIGNING_KEY=off
   $ echo 'fn main() {}' > note.rs
   $ deedar create file --name "the parser patch" --path note.rs --agent you
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   sources=
   producedBy=you -
   grants=
   face=document
   file=sha256:536e506bb90914c243a12b397b9a998f85ae2cbd9ba02dfd03a9e155ca5ca0f4
   accession=deed-file-the-parser-patch
   $ deedar evidence deed-file-the-parser-patch
   id=deed-file-the-parser-patch ok
   producedBy=you -
   time=1700000000
   $ deedar log head
   size=1 root=b1799b9f22404b3626b3c43e80f26232922113b997a1e3377bc803ff799f4ee8

.. toctree::
   :maxdepth: 1
   :caption: Install
   :hidden:

   install

.. toctree::
   :maxdepth: 1
   :caption: Tutorials
   :hidden:

   getting-started

.. toctree::
   :maxdepth: 1
   :caption: How-to guides
   :hidden:

   howto

.. toctree::
   :maxdepth: 1
   :caption: Topics
   :hidden:

   concepts
   explanation
   seat

.. toctree::
   :maxdepth: 1
   :caption: Reference
   :hidden:

   reference
   api
   changelog
