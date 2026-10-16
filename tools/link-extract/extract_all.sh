#!/bin/sh
# Builds the extractor, renders the poses of all gear tiers of pixelpal, and
# writes the sprite recipe. With a preview directory, also writes one preview
# image per tier there.
# Usage: extract_all.sh <zelda3-dir> <recipe.json> [preview-dir]
#
# A tier is a set of sword, shield and armor (see extract.c for the values).
# pixelpal switches to the next tier as the number of completed chats grows.
set -eu

zelda3=$(cd "$1" && pwd)
recipe=$2
preview=${3:-}
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

sh "$here/build.sh" "$zelda3" "$tmp/link-extract"

# tier sword shield armor
for gear in "0 1 1 0" "1 2 2 0" "2 2 2 1" "3 3 3 1" "4 4 3 2"; do
    # shellcheck disable=SC2086
    set -- $gear
    (cd "$zelda3" && "$tmp/link-extract" "$tmp/poses$1.bin" "$2" "$3" "$4")
    echo "tier$1: sword $2, shield $3, armor $4"
done

if [ -n "$preview" ]; then
    python3 "$here/pack.py" "$zelda3" "$recipe" "$tmp"/poses?.bin --preview "$preview"
else
    python3 "$here/pack.py" "$zelda3" "$recipe" "$tmp"/poses?.bin
fi
