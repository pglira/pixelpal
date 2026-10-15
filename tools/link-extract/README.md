# link-extract

Extracts all animations of Link from *A Link to the Past*. The tool uses the
drawing code and the data tables of [zelda3](https://github.com/snesrev/zelda3),
a C reimplementation of the game. It renders each of the 511 poses exactly as
the game composes them: the body, the sword, the shield and the item pieces.

The graphics belong to Nintendo. The output goes to `assets/link/`, which git
ignores. Never commit the output.

## Requirements

- A built zelda3 tree (`make` in the zelda3 directory). The tool links against
  its object files and reads its `zelda3_assets.dat`, which zelda3 extracts from
  your own ROM.
- `gcc`, `sdl2-config` and Python 3 with Pillow.

## Usage

```sh
tools/link-extract/build.sh <zelda3-dir> /tmp/link-extract
(cd <zelda3-dir> && /tmp/link-extract /tmp/poses.bin 1 1 0)
python3 tools/link-extract/pack.py /tmp/poses.bin assets/link
```

The arguments after the output file of `link-extract` select the equipment:
sword 0-4 (0 = none, 1 = fighter's sword), shield 0-3 (0 = none, 1 = fighter's
shield) and armor 0-2 (0 = green mail).

## Output

- `<animation>_<direction>.png`: one strip per animation and direction. The
  frames are side by side and have the same size.
- `animations.json`: per animation and direction the strip file, the frame size,
  the origin of Link inside a frame, and the playback sequences. A sequence is a
  list of `[frame index, duration]` pairs. The durations are in game frames at
  60 fps; a duration of 0 means that the game holds the frame until an event.
  `same_as` marks a direction that uses the strip of another direction.
- `shadow_<direction>.png`: the ground shadow of Link, as the game draws it
  below him. `animations.json` lists it under `shadows`, with its origin: the
  position of Link's coordinate point when he stands on the ground.
- `preview.png`: an overview of all strips with their names.

The sequences come from the timer tables and state handlers in zelda3's
`player.c` and `messaging.c`. The durations of the spiral stairs, the pull
levers, the Quake jump and the first frame of the desert prayer are estimates.
Types 1 and 28 exist in the pose tables, but the game never shows them.
