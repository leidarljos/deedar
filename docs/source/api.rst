The library crates are ``deed`` (package ``deedar-deed``) and ``deedar``.
``deedar-cli`` and ``deedar-mcp`` are binaries. Their commands are on the
:doc:`reference <reference>` page, and the server's tools are on
the :doc:`how-to <howto>`. The data those types store is drawn on
the :doc:`concepts <concepts>` page.

The tables below are generated from the rustdoc HTML of:

.. code:: console

   $ cargo doc --no-deps --locked -p deedar-deed -p deedar

Documenting the whole workspace would let the ``deedar`` binary replace
the ``deedar`` library pages, because both use that output directory.
``./docs/build.sh`` runs the command above, exports these pages, builds
the HTML, and copies ``target/doc`` to ``rustdoc/`` beside them. Each item
links there. The Sphinx search covers the tables. The copied rustdoc
has its own search box.

Crates
======

.. include:: _api_index.rst
