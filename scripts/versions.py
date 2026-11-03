#!/usr/bin/env python3
"""The one hand-written version, and the copies cog writes from it.

The version is written by hand in ``[workspace.package]`` in the workspace
Cargo.toml. Every other copy is a cogapp block listed in cogfiles.txt.

    cog -r @cogfiles.txt        rewrites those copies from this module
    cog --check @cogfiles.txt   fails when a copy has drifted
"""

from __future__ import annotations

import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "Cargo.toml"


def version() -> str:
    """The workspace package version, the only hand-written copy."""
    with MANIFEST.open("rb") as handle:
        return tomllib.load(handle)["workspace"]["package"]["version"]


def minor() -> str:
    """Sphinx ``|version|``: everything before the last dot.

    Sphinx uses ``version`` as major.minor and ``release`` as the full
    crate version. The last dot separates them, so ``1.2.3`` yields ``1.2``.
    """
    return version().rpartition(".")[0]


def pins() -> str:
    """``[workspace.dependencies]`` lines for this workspace's own crates.

    A published crate that depends on another by path also has to require
    the version that crate publishes, or the published package does not
    resolve. ``deed`` is the library package ``deedar-deed`` in
    ``crates/deed``. ``deedar`` is the store crate. The other members
    depend on these two and are not themselves required by path.
    """
    current = version()
    lines = [
        f'deed = {{ package = "deedar-deed", path = "crates/deed", version = "{current}" }}',
        f'deedar = {{ path = "crates/deedar", version = "{current}" }}',
    ]
    return "\n".join(lines) + "\n"


def main() -> None:
    print(version())


if __name__ == "__main__":
    main()
