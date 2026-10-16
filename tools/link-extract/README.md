# link-extract

Makes the sprite recipe `assets/link.json` of pixelpal. The tool uses the
drawing code and the data tables of [zelda3](https://github.com/snesrev/zelda3),
a C reimplementation of *A Link to the Past*. It renders each of the 511 poses
of Link exactly as the game composes them: the body, the sword, the shield and
the item pieces. It also renders the ground shadows, the liftable objects with
their debris, and the Zzz.

The recipe holds no graphics. For each 8×8 tile of each pose, it names the
place of the pixels and of the colors in the ROM, and the position of the tile.
pixelpal reads the user's ROM and draws the strips from the recipe. Only the
recipe goes into the repository. The rendered poses and the previews contain
the graphics of Nintendo: never commit them.

## Requirements

- A built zelda3 tree (`make` in the zelda3 directory), with the ROM
  `zelda3.sfc` in it. The tool links against the object files, reads
  `zelda3_assets.dat`, and uses `assets/tables.py` and `assets/util.py`.
- `gcc`, `sdl2-config` and Python 3 with Pillow.

## Usage

One command builds the tool, renders all gear tiers and writes the recipe:

```sh
tools/link-extract/extract_all.sh <zelda3-dir> assets/link.json [preview-dir]
```

With a preview directory, the command also writes `tier0.png` … `tier4.png`
there: an overview of all strips of a tier, with their names.

| Tier | Sword | Shield | Armor |
|---|---|---|---|
| 0 | Fighter's Sword | Fighter's Shield | Green Mail |
| 1 | Master Sword | Fire Shield | Green Mail |
| 2 | Master Sword | Fire Shield | Blue Mail |
| 3 | Tempered Sword | Mirror Shield | Blue Mail |
| 4 | Golden Sword | Mirror Shield | Red Mail |

The steps of the command:

1. `build.sh <zelda3-dir> <binary>` builds `link-extract`.
2. `link-extract <poses.bin> <sword> <shield> <armor>`, run in the zelda3
   directory, renders the poses of one set of gear: sword 0-4 (0 = none,
   1 = fighter's sword), shield 0-3 (0 = none, 1 = fighter's shield) and armor
   0-2 (0 = green mail). Each pose has its pixels and the list of its drawn
   8×8 tiles, with their 4bpp data and palette.
3. `pack.py <zelda3-dir> <recipe.json> <poses>...` finds the ROM source of
   each tile and color, groups the poses into strips, and writes the recipe.
   Then it draws every strip from the recipe and the ROM, and stops with an
   error if a strip differs from the rendered pixels. It writes a checksum of
   the pixels of each tier into the recipe. The tests of pixelpal compare the
   strips that pixelpal draws with these checksums.

## Tile sources

- The 4bpp sheet of Link at ROM offset 0x80000, uncompressed.
- The 3bpp sprite sheets of zelda3's `kCompSpritePtrs`: sheets 0-11 are
  uncompressed, the other sheets are compressed. The sword, the shield, the
  items, the objects, the shadow and the Zzz come from these sheets. The
  "high" expansion adds 8 to the color index of each opaque pixel, as
  `Do3To4High16Bit` does.
- The colors come from the sprite palette tables of the ROM.

## Recipe

`about` in the recipe describes the format. In short:

- `tiers`: per tier the strips: one strip per animation and direction, the
  ground shadows (`shadows`), and the extras (`extras`). Each strip has the
  frame size, the origin, the crop rectangle in the poses, and the pose of
  each frame. The origin of an animation frame is Link's coordinate point. The
  origin of a shadow is the same point with Link on the ground. The origin of
  an extra is the top-left corner of the object (or of the Zzz tile).
- `sequences`: per animation the playback sequences, as `[frame index,
  duration]` pairs. The durations are in game frames at 60 fps; a duration of
  0 means that the game holds the frame until an event. A sequence keyed by a
  direction name applies to that direction only.

The extras:

- `rock`, `bush`, `skull`: the liftable objects, as the game draws them while
  Link carries them (sprite 0xEC). The rock and the bush come from the
  light-world sprite pack 1. The skull pot comes from the dungeon pack 10,
  which holds no plain pot.
- `rock_debris`, `bush_debris`, `skull_debris`: the 8 frames of the debris
  when an object breaks (`Garnish16_ThrownItemDebris`).
- `zzz`: the 3 sizes of the Zzz of a sleeping Link (`Ancilla21_Snore`).

The sequences come from the timer tables and state handlers in zelda3's
`player.c` and `messaging.c`. The durations of the spiral stairs, the pull
levers, the Quake jump and the first frame of the desert prayer are estimates.
Types 1 and 28 exist in the pose tables, but the game never shows them.
