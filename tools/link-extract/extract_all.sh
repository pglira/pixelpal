#!/bin/sh
# Builds the extractor and writes the animations of all gear tiers of pixelpal
# to <out-dir>/tier0 ... <out-dir>/tier4.
# Usage: extract_all.sh <zelda3-dir> <out-dir>
#
# A tier is a set of sword, shield and armor (see extract.c for the values).
# pixelpal switches to the next tier as the number of completed chats grows.
set -eu

zelda3=$(cd "$1" && pwd)
mkdir -p "$2"
out=$(cd "$2" && pwd)
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

sh "$here/build.sh" "$zelda3" "$tmp/link-extract"

# tier sword shield armor
for gear in "0 1 1 0" "1 2 2 0" "2 2 2 1" "3 3 3 1" "4 4 3 2"; do
    set -- $gear
    (cd "$zelda3" && "$tmp/link-extract" "$tmp/poses.bin" "$2" "$3" "$4")
    rm -rf "$out/tier$1"
    python3 "$here/pack.py" "$tmp/poses.bin" "$out/tier$1"
    echo "tier$1: sword $2, shield $3, armor $4"
done
