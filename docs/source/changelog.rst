Versions follow semver at 0.x. A minor bump is a feature. A patch is a
fix. Fragments in ``changelog.d`` use the towncrier types ``feature``,
``bugfix``, ``doc``, ``removal`` and ``misc``. ``towncrier build`` collects them
into ``CHANGELOG.md``, which is the text below. The command line those
notes describe is the :doc:`reference <reference>`.

Releases
========

.. include:: _changelog_body.rst
