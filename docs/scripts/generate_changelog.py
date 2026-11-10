#!/usr/bin/env python3
"""Write docs/source/_changelog_body.rst from CHANGELOG.md.

The changelog page supplies its own title and the towncrier note. This
keeps the version sections and drops the markdown title, the one-line
semver preface, and the towncrier HTML comment.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs" / "source" / "_changelog_body.rst"
_RAW = re.compile(r"\n*\.\. raw:: html\n(?:   .*\n)+", re.MULTILINE)
_VERSION = re.compile(r"(?m)^(\d+\.\d+\.\d+ \(\d{4}-\d{2}-\d{2}\))\n-+$")


def main() -> int:
    raw = subprocess.check_output(
        [
            "pandoc",
            "-f",
            "markdown-smart",
            "-t",
            "rst",
            "--wrap=preserve",
            "--columns=400",
            str(ROOT / "CHANGELOG.md"),
        ],
        cwd=ROOT,
        text=True,
    )
    raw = _RAW.sub("\n\n", raw)
    # Pandoc emits ``.. _section-N:`` anchors. Sphinx also numbers sections,
    # and the two schemes collide inside one changelog page.
    raw = re.sub(r"(?m)^[.][.] _section-\d+:\n+", "", raw)
    match = _VERSION.search(raw)
    if not match:
        print("generate_changelog: no version heading in CHANGELOG.md", file=sys.stderr)
        return 1
    body = raw[match.start() :].strip() + "\n"
    OUT.write_text(body, encoding="utf-8")
    print(f"generate_changelog: wrote {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
