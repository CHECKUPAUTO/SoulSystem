#!/usr/bin/env bash
# Purge des fichiers .js inutiles dans crates/avid-cobalt/src
#
# Contexte : 65 fichiers .js (~584 KB, 8400 LOC) ont été déposés dans
# `crates/avid-cobalt/src/` comme "référence" depuis cobalt.tools.
# Cargo les ignore complètement (il ne compile que les .rs), et aucun
# code Rust du crate ne les lit (aucun spawn de subprocess Node.js).
# Donc ce sont littéralement des fichiers morts qui :
#   1. Gonflent le repo (584 KB)
#   2. Polluent grep / fd / IDE indexing
#   3. Trompent les futurs lecteurs en suggérant qu'ils sont utilisés
#
# Ce script les déplace vers `vendor/cobalt-js-reference/` (hors src/)
# pour qu'ils restent disponibles en lecture comme référence sans
# polluer la crate Rust.
#
# Usage :
#   bash scripts/cobalt_purge_js.sh [--dry-run]

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

DRY_RUN=0
if [[ "${1:-}" == "--dry-run" ]]; then
    DRY_RUN=1
fi

SRC="crates/avid-cobalt/src"
DST="vendor/cobalt-js-reference"

# Find all .js files and the dirs that contain ONLY .js (no .rs).
js_files=$(find "$SRC" -name '*.js' 2>/dev/null || true)
if [[ -z "$js_files" ]]; then
    echo "No .js files found in $SRC — nothing to do."
    exit 0
fi

count=$(echo "$js_files" | wc -l)
size=$(echo "$js_files" | xargs du -ch 2>/dev/null | tail -1 | awk '{print $1}')
echo "Found $count .js files in $SRC (total $size)"

if [[ $DRY_RUN -eq 1 ]]; then
    echo "[DRY-RUN] Would move them to $DST/"
    echo "$js_files" | head -20
    [[ $count -gt 20 ]] && echo "...and $((count - 20)) more"
    exit 0
fi

# Move files preserving directory structure
mkdir -p "$DST"
echo "$js_files" | while read -r jsfile; do
    rel="${jsfile#$SRC/}"
    target="$DST/$rel"
    mkdir -p "$(dirname "$target")"
    git mv -f "$jsfile" "$target" 2>/dev/null || mv -f "$jsfile" "$target"
done

# Clean up empty directories that used to contain only .js
find "$SRC" -type d -empty -delete 2>/dev/null || true

echo "Done. Moved $count files to $DST/"
echo "Verify with: cargo check -p avid-cobalt"
