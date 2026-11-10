"""Sphinx configuration for the deedar project site (Shibuya theme)."""

from __future__ import annotations

import re
from html import escape
from pathlib import Path

_DOCS = Path(__file__).resolve().parent
_ROOT = _DOCS.parent.parent

project = "deedar"
copyright = "deedar developers"
author = "deedar developers"
# |version| is major.minor. |release| is the full crate version.
# cog writes both from [workspace.package] in the workspace Cargo.toml.
# [[[cog import versions; cog.outl(f'release = "{versions.version()}"'); cog.outl(f'version = "{versions.minor()}"') ]]]
release = "0.6.1"
version = "0.6"
# [[[end]]]

extensions = [
    "sphinx.ext.mathjax",
    "sphinx_copybutton",
]

templates_path = ["_templates"]
# Included fragments. Sphinx still includes them; it does not publish them
# as their own pages, so their headings do not take a second set of labels.
exclude_patterns = ["_*.rst"]

html_theme = "shibuya"
html_static_path = ["_static"]
html_favicon = "_static/favicon.svg"
html_logo = "_static/mark.svg"
html_title = "deedar"
html_css_files = ["custom.css"]

# Absolute links for the canonical URL and for the copied page source.
# Shibuya's view-source item uses html_baseurl plus the _sources file when
# html_copy_source is on. That file is the page's rst, which the build writes.
html_baseurl = "https://leidarljos.github.io/deedar/"
html_copy_source = True

html_context = {
    "source_type": "github",
    "source_user": "leidarljos",
    "source_repo": "deedar",
    "source_version": "main",
    "source_docs_path": "/docs/source/",
    "source_view_template": "https://leidarljos.github.io/deedar/_sources/{0}",
}

# The document title Sphinx would otherwise take from the first section.
# The home page's title is the site name.
PAGE_TITLES = {
    "index": "deedar",
    "install": "Installation",
    "getting-started": "Getting started",
    "howto": "How-to",
    "concepts": "Concepts",
    "reference": "Reference",
    "explanation": "Explanation",
    "seat": "In the seat",
    "api": "API reference",
    "changelog": "Changelog",
    "search": "Search",
    "genindex": "Index",
}

# One sentence from the page itself, for the description and the link preview.
PAGE_DESCRIPTIONS = {
    "index": (
        "A deed is the frozen product of one unit of work, appended to a Merkle log "
        "and cited by accession."
    ),
    "install": (
        "Install deedar and deedar-mcp on Linux, macOS, and Windows from a release "
        "archive, cargo binstall, crates.io, or a checkout."
    ),
    "getting-started": (
        "Mint a deed in a scratch store, supersede it, hand it over with its receipt, "
        "and check it on another machine."
    ),
    "howto": (
        "Sign a handover, keep a head between them, audit a store, backfill a log, "
        "and demand attestation."
    ),
    "concepts": (
        "Bytes, deed, log, head, bag, and citation: the parts a deedar store is made of."
    ),
    "reference": (
        "Verbs, kinds, accessions, the store, the log, and the environment variables."
    ),
    "explanation": (
        "Why a deed is frozen, what the log adds, and what a signature covers."
    ),
    "seat": (
        "This crate is one habitat. Citing a deed names it; the bytes stay in deedar."
    ),
    "api": "Public items of the deed and deedar crates, generated from rustdoc.",
    "changelog": "What each version of deedar changed.",
    "search": "Search the deedar documentation.",
    "genindex": "Index of the deedar documentation.",
}

OG_IMAGE = "https://leidarljos.github.io/deedar/_static/og.png"

# Pages whose sections are all h1s. Seat already has one h1, its own title.
_DEMOTE_H1 = {
    "index",
    "install",
    "getting-started",
    "howto",
    "concepts",
    "reference",
    "explanation",
    "api",
    "changelog",
}
_HEADERLINK = re.compile(
    r'<a\b[^>]*class="headerlink"[^>]*>.*?</a>',
    re.DOTALL,
)
_H1_OPEN = re.compile(r"<h1\b([^>]*)>")
_H1_BLOCK = re.compile(r"<h1\b[^>]*>(.*?)</h1>", re.DOTALL)

html_theme_options = {
    "accent_color": "gold",
    "color_mode": "auto",
    "dark_code": True,
    "open_in_chatgpt": False,
    "open_in_claude": False,
    "github_url": "https://github.com/leidarljos/deedar",
    "nav_links": [
        {"title": "Install", "url": "install"},
        {"title": "Tutorial", "url": "getting-started"},
        {"title": "How-to", "url": "howto"},
        {"title": "Concepts", "url": "concepts"},
        {"title": "Reference", "url": "reference"},
        {"title": "Search", "url": "search"},
    ],
}

# Offline builds must not reach for an inventory.
intersphinx_mapping: dict = {}

copybutton_prompt_text = r"\$ "
copybutton_prompt_is_regexp = True


def _heading_text(inner: str) -> str:
    text = _HEADERLINK.sub("", inner)
    text = re.sub(r"<[^>]+>", "", text)
    return re.sub(r"\s+", " ", text).strip()


def _one_page_title(body: str, title: str) -> str:
    """Leave a single h1, the page title, and keep section headings as h2.

    The h2s carry a class so the stylesheet can give them the size the
    sections had as h1s. The title h1 is clipped in that same stylesheet.
    """
    blocks = _H1_BLOCK.findall(body)
    if len(blocks) == 1 and _heading_text(blocks[0]) == title:
        return body
    body = _H1_OPEN.sub(r'<h2 class="section-title"\1>', body)
    body = body.replace("</h1>", "</h2>")
    return f'<h1 class="page-title">{escape(title)}</h1>\n' + body


def _page_meta(app, pagename, templatename, context, doctree):
    del app, templatename, doctree
    title = PAGE_TITLES.get(pagename)
    if title:
        context["title"] = title
    description = PAGE_DESCRIPTIONS.get(
        pagename, PAGE_DESCRIPTIONS["index"]
    )
    meta = context.get("meta")
    if not isinstance(meta, dict):
        meta = {}
        context["meta"] = meta
    meta["description"] = description
    meta["cover"] = OG_IMAGE
    context["page_description"] = description
    if pagename in _DEMOTE_H1 and context.get("body"):
        context["body"] = _one_page_title(context["body"], title or pagename)


def setup(app):
    app.connect("html-page-context", _page_meta)
