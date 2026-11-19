``deedar`` is the command line. ``deedar-mcp`` is the read-only Model Context
Protocol server. Each command opens the store named by ``--url``, else
``DEEDAR_URL``, else the seat store at ``$XDG_DATA_HOME/deedar/store`` or
``~/.local/share/deedar/store``. There is no daemon.

The checkout pins its Rust channel in ``rust-toolchain.toml``. A release archive
already contains the binaries. Building from source uses that channel.
The documentation samples also compile a small C library with ``cc`` or
``gcc``, used only to pin the clock while the pages are generated.

Set ``DEEDAR_URL`` to ``file://`` plus an absolute path before the first
command that reads a store. ``DEEDAR_HOST_SIGNING_KEY`` names a 32-byte
Ed25519 seed, or a path to a file of those bytes, or ``off`` to sign nothing.
A path that is not a file is refused, and the message names the command that writes one.
The
:doc:`reference <reference>` lists ``DEEDAR_HOST_KEY``, ``DEEDAR_TSA``,
and the ``XDG`` paths. The :doc:`tutorial <getting-started>` is the
first session.

Linux
=====

Each release publishes one archive per target, named
``deedar-vVERSION-TARGET.tar.gz``, with a ``.sha256`` sidecar beside it. On
Linux the targets are ``x86_64-unknown-linux-gnu`` and
``aarch64-unknown-linux-gnu``. VERSION is a tag on the
`releases page <https://github.com/leidarljos/deedar/releases>`__, without
the leading ``v``. This page names no release, so it cannot fall behind one:
replace ``X.Y.Z`` below with a version that page lists.

.. code:: console

   $ VERSION=X.Y.Z
   $ TARGET=x86_64-unknown-linux-gnu
   $ NAME=deedar-v$VERSION-$TARGET
   $ BASE=https://github.com/leidarljos/deedar/releases/download/v$VERSION
   $ curl -fsSLO $BASE/$NAME.tar.gz
   $ curl -fsSLO $BASE/$NAME.tar.gz.sha256
   $ sha256sum -c $NAME.tar.gz.sha256
   $ tar xzf $NAME.tar.gz
   $ ./$NAME/deedar --version

``sha256sum -c`` prints the archive name and ``OK``. The archive unpacks to a
directory of the same name as the archive, holding ``deedar``,
``deedar-mcp``, the README, the changelog, and the license. ``deedar
--version`` prints ``deedar`` and the version you chose.

macOS
=====

The same release publishes ``aarch64-apple-darwin`` and
``x86_64-apple-darwin`` archives, again with ``.sha256`` sidecars. Set
``TARGET`` to one of them and run the commands above, with ``shasum -a 256
-c`` in place of ``sha256sum -c``. The crates.io and checkout commands below
are the ones to run when an archive is the wrong target.

Windows
=======

The release workflow uploads Linux and macOS archives. A Windows build
uses the crates.io command or a checkout, both below.

cargo binstall
==============

``cargo binstall`` reads ``[package.metadata.binstall]`` and downloads the
release archive for the running target. It prints the package and
version it fetched, and where it put each binary.

.. code:: bash

   cargo binstall --no-confirm deedar-cli
   cargo binstall --no-confirm deedar-mcp

``--root DIR`` chooses the prefix. Omit it and binstall uses its default.
``deedar-mcp`` speaks the protocol on standard input, so running it with
no client closes the connection.
``deedar-mcp --help`` and ``deedar-mcp --version`` print and exit.
The :doc:`how-to <howto>` shows
the server line and the client entry.

crates.io
=========

Both packages are published. Cargo compiles each dependency, then prints
the package and version it installed. Omit ``--root`` to use the default
Cargo bin directory.

.. code:: bash

   cargo install --locked deedar-cli deedar-mcp

From a checkout
===============

From this tree the same two packages install with ``--path``. Cargo warns
when the prefix's ``bin`` directory is not on ``PATH``.

.. code:: bash

   cargo install --locked --path crates/deedar-cli
   cargo install --locked --path crates/deedar-mcp

What the binary prints
======================

The block is ``deedar --version`` and ``deedar --help`` from a build of
this tree. A release archive prints its own version and its own help
text, which can differ from this tree's, including the ``list`` flags.

.. code:: console

   $ deedar --version
   deedar 0.7.0
   $ deedar --help
   usage: deedar [--url FILE] create|get|list|trail|evidence|delete|leave|timestamp|current|log|export|import|check|vouch|migrate|host|url
   deedar --help            this page
   deedar help              this page
   deedar create --help     kinds and flags, including --path
   deedar help create       kinds and flags, including --path
   evidence, current and export take one id, several ids, or - to read them from stdin
   list takes --kind KIND, --agent ID and --text WORDS
   log takes head, list, audit, backfill, prove ID, or bridge SIZE
   import takes a satchel directory, and --allow-unsigned-head for a missing or unlisted signature
   check takes a satchel directory, --since KEPT_HEAD, --bridge FILE and --keep FILE; vouch takes sign FILE or check FILE
   leave takes an id and a destination directory
   timestamp writes a .tsr when DEEDAR_TSA answers, and names the .tsq when that variable is unset
   host says whether the store's layout lists the host signing key, and exits 1 when it does not
   host accept adds the host signing key to the layout as a signer, and leaves every other line
   url prints the store this command resolved

Where next
==========

The :doc:`tutorial <getting-started>` mints a deed in a scratch
store. The :doc:`reference <reference>` is the verb list. The
:doc:`API reference <api>` is the library crates.
