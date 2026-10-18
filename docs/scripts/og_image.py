#!/usr/bin/env python3
"""Build the 1200x630 Open Graph image from docs/logo/icon.svg.

The card is the logo, centred, on the logo's own ground. Run from anywhere:

    python3 docs/scripts/og_image.py

Needs rsvg-convert (librsvg).
"""

from __future__ import annotations

import re
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOGO = ROOT / "docs" / "logo" / "icon.svg"
OUT = ROOT / "docs" / "source" / "_static" / "og.png"
WIDTH = 1200
HEIGHT = 630
# icon.svg is 200 by 200. 2.6 puts a 520px mark in the card.
MARK = 200
SCALE = 2.6


def logo_markup(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    text = re.sub(r"<\?xml[^?]*\?>", "", text)
    match = re.search(r"<svg\b[^>]*>(.*)</svg>\s*$", text, re.DOTALL)
    if not match:
        raise SystemExit(f"{path} has no svg root")
    return match.group(1)


def png_size(path: Path) -> tuple[int, int]:
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path} is not a png")
    width, height = struct.unpack(">II", data[16:24])
    return width, height


def main() -> int:
    rsvg = shutil.which("rsvg-convert")
    if not rsvg:
        print("og_image: need rsvg-convert (librsvg)", file=sys.stderr)
        return 1
    if not LOGO.is_file():
        print(f"og_image: missing {LOGO}", file=sys.stderr)
        return 1

    side = MARK * SCALE
    x = (WIDTH - side) / 2
    y = (HEIGHT - side) / 2
    card = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" '
        f'viewBox="0 0 {WIDTH} {HEIGHT}">\n'
        f'<rect width="{WIDTH}" height="{HEIGHT}" fill="#0B0B0A"/>\n'
        f'<g transform="translate({x:g} {y:g}) scale({SCALE:g})">\n'
        f"{logo_markup(LOGO)}\n"
        f"</g>\n</svg>\n"
    )
    with tempfile.TemporaryDirectory() as tmp:
        svg = Path(tmp) / "og.svg"
        svg.write_text(card, encoding="utf-8")
        subprocess.run(
            [rsvg, "-w", str(WIDTH), "-h", str(HEIGHT), "-o", str(OUT), str(svg)],
            check=True,
        )
    width, height = png_size(OUT)
    if (width, height) != (WIDTH, HEIGHT):
        print(f"og_image: {OUT.name} is {width}x{height}, want {WIDTH}x{HEIGHT}", file=sys.stderr)
        return 1
    print(f"og_image: wrote {OUT.relative_to(ROOT)} ({width}x{height})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
