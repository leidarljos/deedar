``deedar`` is the command line. ``deedar-mcp`` is the read-only Model Context
Protocol server. Each command opens the store named by ``--url``, else
``DEEDAR_URL``, else the seat store at ``$XDG_DATA_HOME/deedar/store`` or
``~/.local/share/deedar/store``. There is no daemon.

The checkout pins Rust 1.97.1 in ``rust-toolchain.toml``. A release archive
already contains the binaries. Building from source uses that channel.
The documentation samples also compile a small C library with ``cc`` or
``gcc``, used only to pin the clock while the pages are generated.

Set ``DEEDAR_URL`` to ``file://`` plus an absolute path before the first
command that reads a store. ``DEEDAR_HOST_SIGNING_KEY`` names a 32-byte
Ed25519 seed, or ``off`` to sign nothing. The
:doc:`reference <reference>` lists ``DEEDAR_HOST_KEY``, ``DEEDAR_TSA``,
and the ``XDG`` paths. The :doc:`tutorial <getting-started>` is the
first session.

Linux
=====

The v0.6.1 release publishes
``deedar-v0.6.1-x86_64-unknown-linux-gnu.tar.gz`` and
``deedar-v0.6.1-aarch64-unknown-linux-gnu.tar.gz``, each with a
``.sha256`` sidecar. The transcript below is the x86\ :sub:`64` archive, checked
and listed, then ``deedar --version``. The aarch64 archive has the same
layout with its own target triple in the name.

.. code:: console

   $ curl -fsSL -o deedar-v0.6.1-x86_64-unknown-linux-gnu.tar.gz https://github.com/leidarljos/deedar/releases/download/v0.6.1/deedar-v0.6.1-x86_64-unknown-linux-gnu.tar.gz
   $ curl -fsSL -o deedar-v0.6.1-x86_64-unknown-linux-gnu.tar.gz.sha256 https://github.com/leidarljos/deedar/releases/download/v0.6.1/deedar-v0.6.1-x86_64-unknown-linux-gnu.tar.gz.sha256
   $ sha256sum -c deedar-v0.6.1-x86_64-unknown-linux-gnu.tar.gz.sha256
   deedar-v0.6.1-x86_64-unknown-linux-gnu.tar.gz: OK
   $ tar tzf deedar-v0.6.1-x86_64-unknown-linux-gnu.tar.gz
   deedar-v0.6.1-x86_64-unknown-linux-gnu/
   deedar-v0.6.1-x86_64-unknown-linux-gnu/LICENSE
   deedar-v0.6.1-x86_64-unknown-linux-gnu/README.md
   deedar-v0.6.1-x86_64-unknown-linux-gnu/deedar-mcp
   deedar-v0.6.1-x86_64-unknown-linux-gnu/deedar
   deedar-v0.6.1-x86_64-unknown-linux-gnu/CHANGELOG.md
   $ ./deedar-v0.6.1-x86_64-unknown-linux-gnu/deedar --version
   deedar 0.6.1

macOS
=====

The same release publishes
``deedar-v0.6.1-aarch64-apple-darwin.tar.gz`` and
``deedar-v0.6.1-x86_64-apple-darwin.tar.gz``, again with ``.sha256``
sidecars. The directory inside uses that target triple and holds
``deedar``, ``deedar-mcp``, the README, the changelog, and the license.
``sha256sum -c`` on macOS is ``shasum -a 256 -c``. The crates.io and
checkout commands below are the ones to run when an archive is the
wrong target.

Windows
=======

The release workflow uploads Linux and macOS archives. A Windows build
uses the crates.io command or a checkout, both below.

cargo binstall
==============

``cargo binstall`` reads ``[package.metadata.binstall]`` and downloads the
release archive for the running target. This run is cargo-binstall
1.26.2 on x86\ :sub:`64` Linux, with the prefix ``/tmp/deedar-binstall2``. The
``Done in`` lines are that run.

.. code:: console

   $ cargo binstall -V
   1.26.2
   $ cargo binstall --no-confirm --root /tmp/deedar-binstall2 deedar-cli
   cargo-binstall:  INFO resolve: Resolving package: 'deedar-cli'
   cargo-binstall:  WARN The package deedar-cli v0.6.1 (x86_64-unknown-linux-gnu) has been downloaded from github.com
   cargo-binstall:  INFO This will install the following binaries:
   cargo-binstall:  INFO   - deedar => /tmp/deedar-binstall2/bin/deedar
   cargo-binstall:  INFO Installing binaries...
   cargo-binstall:  INFO Done in 2.128478814s
   $ cargo binstall --no-confirm --root /tmp/deedar-binstall2 deedar-mcp
   cargo-binstall:  INFO resolve: Resolving package: 'deedar-mcp'
   cargo-binstall:  WARN The package deedar-mcp v0.6.1 (x86_64-unknown-linux-gnu) has been downloaded from github.com
   cargo-binstall:  INFO This will install the following binaries:
   cargo-binstall:  INFO   - deedar-mcp => /tmp/deedar-binstall2/bin/deedar-mcp
   cargo-binstall:  INFO Installing binaries...
   cargo-binstall:  INFO Done in 1.900891635s
   $ /tmp/deedar-binstall2/bin/deedar --version
   deedar 0.6.1

``--root`` chooses the prefix. Omit it and binstall uses its default.
``deedar-mcp`` speaks the protocol on standard input, so running it with
no client closes the connection. The :doc:`how-to <howto>` shows
the server line and the client entry.

crates.io
=========

Both packages are published. This run used a prefix so the install
stayed off the default Cargo bin directory. Omit ``--root`` to use that
default. Compiling printed one line per dependency, then:

::

   Installing /tmp/deedar-from-crates/bin/deedar
    Installed package `deedar-cli v0.6.1` (executable `deedar`)
   Installing deedar-mcp v0.6.1
   Installing /tmp/deedar-from-crates/bin/deedar-mcp
    Installed package `deedar-mcp v0.6.1` (executable `deedar-mcp`)
      Summary Successfully installed deedar-cli, deedar-mcp!

Run it as:

.. code:: bash

   cargo install --locked --root /tmp/deedar-from-crates deedar-cli deedar-mcp

``/tmp/deedar-from-crates/bin/deedar --version`` printed ``deedar 0.6.1``.

From a checkout
===============

From this tree the same two packages install with ``--path``. The run
used ``--root /tmp/deedar-from-path`` and finished:

::

   Installed package `deedar-cli v0.6.1 (/agent/repos/deedar/crates/deedar-cli)` (executable `deedar`)
   Installed package `deedar-mcp v0.6.1 (/agent/repos/deedar/crates/deedar-mcp)` (executable `deedar-mcp`)

.. code:: bash

   cargo install --locked --path crates/deedar-cli --root /tmp/deedar-from-path
   cargo install --locked --path crates/deedar-mcp --root /tmp/deedar-from-path

The path inside the package line is the checkout Cargo compiled.
``/tmp/deedar-from-path/bin/deedar --version`` printed ``deedar 0.6.1``.
Cargo also warns that the prefix's ``bin`` directory has to be on ``PATH``.

What the binary prints
======================

The block is ``deedar --version`` and ``deedar --help`` from a build of
this tree. The published v0.6.1 archive reports the same version. Its
help text is the release's; this help text is the tree's, including
the ``list`` flags.

.. code:: console

   $ deedar --version
   deedar 0.6.1
   $ deedar --help
   usage: deedar [--url FILE] create|get|list|trail|evidence|delete|leave|timestamp|current|log|export|import|check|vouch|migrate|host
   deedar --help            this page
   deedar create --help     kinds and flags, including --path
   evidence, current and export take one id, several ids, or - to read them from stdin
   list takes --kind KIND, --agent ID and --text WORDS
   log takes head, list, audit, backfill, prove ID, or bridge SIZE
   import takes a satchel directory, and --allow-unsigned-head for a missing or unlisted signature
   check takes a satchel directory, --since KEPT_HEAD, --bridge FILE and --keep FILE; vouch takes sign FILE or check FILE
   host says whether the store's layout lists the host signing key, and exits 1 when it does not
   host accept adds the host signing key to the layout as a signer, and leaves every other line

Where next
==========

The :doc:`tutorial <getting-started>` mints a deed in a scratch
store. The :doc:`reference <reference>` is the verb list. The
:doc:`API reference <api>` is the library crates.
