#!/usr/bin/env python3
"""Finish the org to RST export: restore the toctree, refuse a corrupting shape.

Two failures here are quiet rather than loud, which is why they are checked
instead of trusted:

- Pandoc drops a trailing ``rst`` export block often enough that a build comes
  out with every guide orphaned and no way to reach it from the index.
- A line that begins with ``[12]`` is a footnote *definition* in RST. When the
  citation markers in this project's prose land at column zero, docutils eats
  the surrounding sentence into a footnote nobody references. The build still
  succeeds; the page loses text.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# The pages a site has are the org files it has, index aside, in Diataxis order.
# A file that is not in a section is still listed, so a new org page is reachable.
_ORG = Path(__file__).resolve().parents[1] / "orgmode"
_SECTIONS = (
    ("Install", ["install"]),
    ("Tutorials", ["getting-started"]),
    ("How-to guides", ["howto"]),
    ("Topics", ["concepts", "explanation", "seat"]),
    ("Reference", ["reference", "api", "changelog"]),
)


def site_sections() -> list[tuple[str, list[str]]]:
    known = {name for _title, names in _SECTIONS for name in names}
    sections = []
    for title, names in _SECTIONS:
        present = [name for name in names if (_ORG / f"{name}.org").is_file()]
        if present:
            sections.append((title, present))
    extras = sorted(
        path.stem
        for path in _ORG.glob("*.org")
        if path.stem not in known and path.stem != "index"
    )
    if extras:
        sections.append(("More", extras))
    return sections


PAGES = [page for _title, pages in site_sections() for page in pages]


def _toctree() -> str:
    blocks = []
    for title, pages in site_sections():
        body = "".join(f"   {page}\n" for page in pages)
        blocks.append(
            f"\n.. toctree::\n   :maxdepth: 1\n   :caption: {title}\n   :hidden:\n\n{body}"
        )
    return "".join(blocks)


TOCTREE = _toctree()

_FOOTNOTE_DEF = re.compile(r"^\[\d+\]")


def restore_toctree(index: Path) -> None:
    """Write the section toctrees, replacing any pandoc kept from the org file."""
    text = index.read_text(encoding="utf-8")
    marker = "\n.. toctree::"
    if marker in text:
        text = text[: text.index(marker)]
    index.write_text(text.rstrip() + "\n" + TOCTREE, encoding="utf-8")
    print("index.rst: toctree written")


def bracket_leading_lines(path: Path) -> list[tuple[int, str]]:
    return [
        (number, line)
        for number, line in enumerate(path.read_text(encoding="utf-8").split("\n"), 1)
        if _FOOTNOTE_DEF.match(line)
    ]


def main() -> int:
    source = Path("docs/source")
    index = source / "index.rst"
    if not index.is_file():
        print("missing docs/source/index.rst after export", file=sys.stderr)
        return 1
    restore_toctree(index)

    missing = [page for page in PAGES if not (source / f"{page}.rst").is_file()]
    if missing:
        print(f"missing exported pages: {', '.join(missing)}", file=sys.stderr)
        return 1

    failed = False
    for org in sorted(Path("docs/orgmode").glob("*.org")):
        for number, line in bracket_leading_lines(org):
            print(
                f"{org}:{number}: a line starting with a citation marker becomes an "
                f"RST footnote and swallows the sentence; rewrap it\n    {line}",
                file=sys.stderr,
            )
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
