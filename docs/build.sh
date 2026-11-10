#!/usr/bin/env bash
# Build the deedar Sphinx/Shibuya site: org → rst → html
# Usage (from repo root or this directory):
#   ./docs/build.sh
# Optional: DEEDAR_DOC_EXPORTER=emacs|pandoc  DEEDAR_DOC_VENV=path
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VENV="${DEEDAR_DOC_VENV:-$ROOT/.venv-docs}"
REQ="$ROOT/docs/requirements.txt"
SRC="$ROOT/docs/source"
BUILD="$ROOT/docs/build"

echo "==> 1/5 Open Graph image"
bash "$ROOT/.github/render-og-png.sh"

echo "==> 2/5 export org → rst"
bash "$ROOT/docs/scripts/export_org_to_rst.sh"

echo "==> 3/5 ensure Python doc deps in $VENV"
if [[ ! -d "$VENV" ]]; then
  python3 -m venv "$VENV"
fi
# shellcheck disable=SC1091
source "$VENV/bin/activate"
python -m pip install -q --upgrade pip
python -m pip install -q -r "$REQ"

echo "==> 4/5 sphinx-build (Shibuya) → $BUILD"
rm -rf "$BUILD"
sphinx-build -b html -n -W "$SRC" "$BUILD"

echo "==> 5/5 rustdoc HTML → $BUILD/rustdoc"
rm -rf "$BUILD/rustdoc"
mkdir -p "$BUILD/rustdoc"
cp -a "$ROOT/target/doc/." "$BUILD/rustdoc/"

echo ""
echo "OK: open $BUILD/index.html"
echo "    python3 -m http.server -d $BUILD 8000   # optional"
