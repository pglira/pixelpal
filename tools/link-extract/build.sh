#!/bin/sh
# Builds the pose extractor against a built zelda3 tree.
# Usage: build.sh <zelda3-dir> <output-binary>
set -eu

zelda3=$(cd "$1" && pwd)
out=$2
here=$(cd "$(dirname "$0")" && pwd)

objs=$(ls "$zelda3"/src/*.o "$zelda3"/snes/*.o \
  "$zelda3"/third_party/gl_core/*.o "$zelda3"/third_party/opus-1.3.1-stripped/*.o |
  grep -v -e '/src/main\.o$' -e '/src/player_oam\.o$')

# shellcheck disable=SC2086
gcc -O1 -w -I "$zelda3" $(sdl2-config --cflags) \
  -o "$out" "$here/extract.c" $objs $(sdl2-config --libs) -lm
