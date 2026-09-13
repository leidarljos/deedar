.. raw:: html

   <div class="vi-hero">
     <div class="vi-hero-brand">
       <img class="vi-hero-mark" src="_static/mark.svg" width="64" height="64" alt="" />
       <div>
         <p class="vi-hero-name">deedar</p>
         <p class="vi-hero-tag">What did this unit of work produce?</p>
       </div>
     </div>
     <p class="vi-hero-tagline">Frozen products with an append-only log, so a citation still resolves next year.</p>
     <div class="vi-hero-pills">
       <span>Content addressed</span>
       <span>RFC 9162 log</span>
       <span>Ed25519 signed</span>
     </div>
     <div class="vi-hero-actions">
       <a class="vi-btn vi-btn-gold" href="getting-started.html">Get started</a>
       <a class="vi-btn vi-btn-ghost" href="reference.html">Reference</a>
     </div>
   </div>

A deed is a frozen record of one product: a file, a patch, a quote, a
page, a table. It names the bytes by hash, who produced them, and what they
were made from. Every deed goes into a Merkle log. The store can then show
a deed was there before anyone asked, and cannot drop one unnoticed by a
reader who kept a head. A tracker or a pack cites a deed by its
accession and never copies the bytes.

Install
=======

.. code:: console

   $ cargo binstall deedar-cli
   $ cargo binstall deedar-mcp   # optional
   $ export DEEDAR_URL=file://$HOME/.local/share/deedar/store

First minute
============

.. code:: console

   $ echo 'fn main() {}' > note.rs
   $ deedar create file --name "the parser patch" --path note.rs --agent you
   id=deed-file-the-parser-patch kind=file name=the parser patch
   paths=sha256:536e506b...
   $ deedar evidence deed-file-the-parser-patch
   id=deed-file-the-parser-patch ok
   $ deedar log head
   size=1 root=f56884f1...

The :doc:`tutorial <getting-started>` goes on to supersede the deed,
hand it to another store with its receipt, and check it there.

.. toctree::
   :maxdepth: 1
   :caption: Guides
   :hidden:

   getting-started
   howto
   reference
   explanation
   seat
