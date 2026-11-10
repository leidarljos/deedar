#!/usr/bin/env python3
"""Write docs/source/_api_index.rst from the rustdoc HTML of the library crates.

``cargo doc`` for the whole workspace lets the ``deedar`` binary overwrite
the ``deedar`` library pages, because both use that directory name. This
script documents ``deedar-deed`` and ``deedar`` only, then lists every
public item those pages publish.
"""

from __future__ import annotations

import html
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOC = ROOT / "target" / "doc"
OUT = ROOT / "docs" / "source" / "_api_index.rst"

# Package to build, then the rustdoc directory name.
CRATES = (
    ("deedar-deed", "deed"),
    ("deedar", "deedar"),
)

_TAG = re.compile(r"<[^>]+>")
_WBR = re.compile(r"<wbr\s*/?>")
_CODE = re.compile(r"<code>(.*?)</code>", re.DOTALL)
_H2 = re.compile(r'<h2 id="([^"]+)"[^>]*>')
_TABLE = re.compile(r'<dl class="item-table">(.*?)</dl>', re.DOTALL)
_DTDD = re.compile(r"<dt>(.*?)</dt>|<dd>(.*?)</dd>", re.DOTALL)
_CLASS = re.compile(r'class="([^"]+)"')
_HREF = re.compile(r'href="([^"]+)"')
_PARA = re.compile(r'<div class="docblock">\s*<p>(.*?)</p>', re.DOTALL)
_VERSION = re.compile(r'<span class="version">([^<]+)</span>')


def clean_summary(fragment: str) -> str:
    """One line of visible text, with code spans kept as RST literals.

    Angle brackets inside a code span are real characters (``deed-<kind>``).
    They are copied out before the remaining tags are stripped, so a later
    pass does not read ``<kind>`` as markup.
    """
    pieces: list[str] = []
    pos = 0
    for match in _CODE.finditer(fragment):
        pieces.append(_TAG.sub("", fragment[pos : match.start()]))
        inner = html.unescape(_TAG.sub("", match.group(1))).replace("`", "")
        pieces.append(f"``{inner}``")
        pos = match.end()
    pieces.append(_TAG.sub("", fragment[pos:]))
    text = html.unescape("".join(pieces))
    text = text.replace("\\", "\\\\").replace("*", "\\*")
    return re.sub(r"\s+", " ", text).strip()


def parse_dt(fragment: str) -> tuple[str, str, str] | None:
    if "Restricted Visibility" in fragment:
        return None
    kind_m = _CLASS.search(fragment)
    href_m = _HREF.search(fragment)
    if not kind_m or not href_m:
        return None
    name = _WBR.sub("", fragment)
    name = _TAG.sub("", name)
    name = re.sub(r"\s+", "", html.unescape(name))
    return kind_m.group(1), name, href_m.group(1)


def items(table: str) -> list[tuple[str, str, str, str]]:
    """Items in one rustdoc table. A run of names can share one summary."""
    pending: list[tuple[str, str, str]] = []
    found: list[tuple[str, str, str, str]] = []

    def flush(summary: str) -> None:
        for kind, name, href in pending:
            found.append((kind, name, href, summary))
        pending.clear()

    for dt, dd in _DTDD.findall(table):
        if dt:
            parsed = parse_dt(dt)
            if parsed:
                pending.append(parsed)
        else:
            flush(clean_summary(dd))
    flush("")
    return found


def sections(page: str) -> list[tuple[str, list[tuple[str, str, str, str]]]]:
    out = []
    parts = _H2.split(page)
    for index in range(1, len(parts), 2):
        body = parts[index + 1]
        table = _TABLE.search(body)
        if not table:
            continue
        rows = items(table.group(1))
        if rows:
            out.append((parts[index], rows))
    return out


def blurb(page: str) -> str:
    """The item's own opening paragraph, ignoring docs on methods below it."""
    top = re.search(r'<details class="toggle top-doc".*?</details>', page, re.DOTALL)
    if not top:
        return ""
    match = _PARA.search(top.group(0))
    if not match:
        return ""
    return clean_summary(match.group(1))


def version(page: str) -> str:
    match = _VERSION.search(page)
    return match.group(1).strip() if match else ""


def link(href: str, label: str) -> str:
    return f"`{label} <{href}>`__"


def item_summary(page_dir: Path, href: str) -> str:
    """First sentence on the item's own rustdoc page.

    The crate index repeats one ``<dd>`` after several ``<dt>`` elements.
    That summary belongs to the last name in the run. Reading the item page
    keeps each row on the text rustdoc wrote for that item.
    """
    path = (page_dir / href).resolve()
    if not path.is_file():
        return ""
    return blurb(path.read_text(encoding="utf-8", errors="replace"))


def bullets(page_dir: Path, base: str, rows: list[tuple[str, str, str, str]]) -> list[str]:
    lines = []
    for kind, name, href, _shared in rows:
        target = str(Path(base) / href).replace("\\", "/")
        summary = item_summary(page_dir, href)
        item = f"- {link(target, name)} ({kind})."
        if summary:
            item += f" {summary}"
        lines.append(item)
    return lines


def emit_page(lines: list[str], html_path: Path, base: str, title: str, underline: str) -> None:
    page = html_path.read_text(encoding="utf-8")
    lines.append(title)
    lines.append(underline * len(title))
    lines.append("")
    text = blurb(page)
    if text:
        lines.append(text)
        lines.append("")
    lines.append(link(f"{base}/index.html", "rustdoc"))
    lines.append("")
    for section, rows in sections(page):
        if section == "modules":
            for _kind, name, href, summary in rows:
                child = (html_path.parent / href).resolve()
                child_base = str(Path(base) / href).rsplit("/", 1)[0]
                child_title = title + "::" + name if title else name
                # The module heading sits under the crate heading.
                emit_module(lines, child, child_base, child_title, summary)
            continue
        lines.append(f".. rubric:: {section.capitalize()}")
        lines.append("")
        lines.extend(bullets(html_path.parent, base, rows))
        lines.append("")


def emit_module(
    lines: list[str], html_path: Path, base: str, title: str, summary: str
) -> None:
    if not html_path.is_file():
        return
    lines.append(title)
    lines.append("~" * len(title))
    lines.append("")
    text = summary or blurb(html_path.read_text(encoding="utf-8"))
    if text:
        lines.append(text)
        lines.append("")
    page = html_path.read_text(encoding="utf-8")
    lines.append(link(f"{base}/index.html", "rustdoc"))
    lines.append("")
    for section, rows in sections(page):
        if section == "modules":
            for _kind, name, href, child_summary in rows:
                child = (html_path.parent / href).resolve()
                child_base = str(Path(base) / href).rsplit("/", 1)[0]
                emit_module(lines, child, child_base, f"{title}::{name}", child_summary)
            continue
        lines.append(f".. rubric:: {section.capitalize()}")
        lines.append("")
        lines.extend(bullets(html_path.parent, base, rows))
        lines.append("")


def build_docs() -> None:
    env = os.environ.copy()
    flags = env.get("RUSTDOCFLAGS", "")
    extra = "-D warnings"
    env["RUSTDOCFLAGS"] = f"{flags} {extra}".strip()
    cmd = [
        "cargo",
        "doc",
        "--no-deps",
        "--locked",
        "-p",
        "deedar-deed",
        "-p",
        "deedar",
    ]
    ran = subprocess.run(cmd, cwd=ROOT, env=env)
    if ran.returncode != 0:
        raise SystemExit("cargo doc failed")


def render() -> str:
    lines = [
        ".. Generated by docs/scripts/generate_api.py from cargo doc.",
        "",
    ]
    seen_version = ""
    for _package, directory in CRATES:
        html_path = DOC / directory / "index.html"
        if not html_path.is_file():
            raise SystemExit(f"missing rustdoc page {html_path}")
        page = html_path.read_text(encoding="utf-8")
        seen_version = version(page) or seen_version
        emit_page(lines, html_path, f"rustdoc/{directory}", directory, "-")
    if seen_version:
        lines.insert(2, f"rustdoc {seen_version}.")
        lines.insert(3, "")
    text = "\n".join(lines).rstrip() + "\n"
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text


def main(argv: list[str]) -> int:
    check = argv[1:] == ["--check"]
    if not check:
        build_docs()
    text = render()
    if check:
        current = OUT.read_text(encoding="utf-8") if OUT.is_file() else ""
        if current != text:
            print("generate_api: _api_index.rst is stale", file=sys.stderr)
            return 1
        print("generate_api: _api_index.rst matches rustdoc")
        return 0
    OUT.write_text(text, encoding="utf-8")
    print(f"generate_api: wrote {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
