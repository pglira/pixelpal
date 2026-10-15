#!/usr/bin/env python3
"""Packs the poses that `extract` writes into one PNG strip per animation and
direction, plus an `animations.json` index and a `preview.png` overview.

Usage: pack.py <poses.bin> <out-dir>

All frames of one strip share one bounding box, so that the frames stay
aligned. `origin` in the index is the position of Link's coordinate point
(the top-left corner of his 16x16 body box) inside a frame.
"""

import json
import struct
import sys
from pathlib import Path

from PIL import Image, ImageDraw

DIRECTIONS = ["up", "down", "left", "right"]

HOLD = 0  # A duration of 0 means: the game holds this frame until an event.


def seq(rts, durations, loop=False, loop_from=0):
    """A playback sequence: frame indices into the strip, with durations in
    game frames (60 per second)."""
    if isinstance(durations, int):
        durations = [durations] * len(rts)
    assert len(rts) == len(durations)
    out = {"frames": [[r, d] for r, d in zip(rts, durations)], "loop": loop}
    if loop and loop_from:
        out["loop_from"] = loop_from
    return out


def per_dir(rts_by_dir, durations):
    return {d: seq(r, durations) for d, r in zip(DIRECTIONS, rts_by_dir)}


# Name and playback sequences of each animation type `yt` of LinkOam_Main()
# in zelda3's player_oam.c. The sequences follow the timer tables and state
# handlers of player.c. A sequence keyed by a direction name applies only to
# that direction; the other sequences apply to all directions.
ANIMATIONS = {
    0: ("walk", {"stand": seq([0], HOLD), "walk": seq(range(1, 9), [2, 3, 2, 2, 2, 3, 2, 1], True),
                 "ice": seq(range(1, 9), [1, 2, 1, 1, 1, 2, 1, 2], True),
                 "dash": seq(range(1, 9), 1, True)}),
    1: ("unused_1", {}),
    2: ("walk_sword_charged", {"walk": seq(range(6), 4, True)}),
    3: ("sword_poke_wall", {"poke": seq([0, 1, 2], [3, 4, 4], True)}),
    4: ("fall_in_pit", {"fall": seq(range(6), 10), "land": seq([6, 7, 8, 9], 4, True)}),
    5: ("recoil", {"recoil": seq([0], HOLD)}),
    6: ("rod_or_hammer", {"rod": seq([0, 1, 2], [3, 4, 6]), "hammer": seq([0, 1, 2], [3, 4, 17])}),
    7: ("bow", {"shoot": seq([0, 1, 2], [3, 4, 9])}),
    8: ("magic_powder", {"sprinkle": seq(range(9), [2, 2, 2, 4, 3, 3, 3, 3, 7])}),
    9: ("boomerang", {"throw": seq([0, 1], [7, 6])}),
    10: ("walk_grass_or_shallow_water", {"walk": seq(range(6), 4, True)}),
    11: ("desert_prayer", {"pray": seq([0, 1, 2, 3], [1, 23, 23, HOLD])}),
    12: ("shovel", {"dig": seq([0, 1, 2], [7, 19, 17])}),
    13: ("carry_walk", {"walk": seq(range(6), 4, True)}),
    14: ("throw", {"throw": seq([0], 5)}),
    15: ("spin_attack", per_dir([
        [10, 11, 10, 6, 7, 8, 9, 2, 3, 4, 5, 10],
        [0, 1, 0, 2, 3, 4, 5, 6, 7, 8, 9, 0],
        [12, 13, 12, 4, 5, 6, 7, 8, 9, 2, 3, 12],
        [14, 15, 14, 8, 9, 2, 3, 4, 5, 6, 7, 14],
    ], [3, 6] + [2] * 9 + [6])),
    16: ("lift", {"light": seq([0, 1, 2], [6, 7, 7]),
                  "heavy": seq([0, 0, 1, 0, 1, 0, 1, 2], [6, 8, 24, 8, 32, 6, 8, 13])}),
    17: ("swim_idle", {"tread": seq([0, 1], 16, True)}),
    18: ("swim_stroke", {"stroke": seq([0, 1, 2, 3], [7, 8, 8, 8])}),
    19: ("swim_move", {"swim": seq([2, 0, 1, 0], 8, True)}),
    20: ("electrocuted", {"shock": seq([0, 1, 2, 3, 0, 1, 2, 3], [2, 3, 3, 3, 3, 3, 3, 3])}),
    21: ("medallion", {
        "ether": seq([0, 1, 2, 3, 0, 1, 2, 3, 4, 5, 6, 7], [6] * 8 + [8, 8, 4, 4], True, 10),
        "bombos": seq([0, 1, 2, 3, 0, 1, 2, 3, 8, 9, 10, 11, 12, 10, 8, 13, 14, 15, 16, 17],
                      [6] * 8 + [4] * 5 + [8] + [2] * 5 + [HOLD]),
        "quake": seq([0, 1, 2, 3, 0, 1, 2, 3, 18, 19, 20, 21, 22], [6] * 10 + [8, 8, HOLD]),
    }),
    22: ("push", {"push": seq(range(5), 8, True)}),
    23: ("pull_lever_or_master_sword", {"master_sword": seq([9, 10], [HOLD, 17]),
                                        "lever_down": seq([0, 1, 2, 0, 3, 4], 10),
                                        "lever_up": seq([5, 6, 7, 8], 10)}),
    24: ("grab_pull", {"grab": seq([0], HOLD), "pull": seq([1, 2, 3], [6, 6, 13], True)}),
    25: ("spiral_stairs_up", {"climb": seq([0, 1, 2], 3, True)}),
    26: ("spiral_stairs_down", {"climb": seq([0, 1, 2], 3, True)}),
    27: ("death_swoon", {"swoon": seq([0, 1, 2, 3] * 3 + [4, 5], [6] * 12 + [4, HOLD])}),
    28: ("unused_28", {}),
    29: ("item_get_one_hand", {"hold": seq([0], HOLD)}),
    30: ("item_get_two_hands", {"hold": seq([0], HOLD)}),
    31: ("sleep_in_bed", {"asleep": seq([0], HOLD), "awake": seq([1], HOLD)}),
    32: ("hookshot", {"hold": seq([0], HOLD)}),
    33: ("bunny_walk", {"walk": seq(range(4), 4, True)}),
    34: ("cane", {"somaria": seq([0, 1, 2], [3, 4, 6]), "byrna": seq([0, 1, 2], [19, 8, 14])}),
    35: ("bug_net", per_dir([
        [11, 6, 7, 8, 1, 2, 3, 4, 5, 6],
        [1, 2, 3, 4, 5, 6, 7, 8, 1, 2],
        [9, 4, 5, 6, 7, 8, 1, 2, 3, 4],
        [10, 8, 1, 2, 3, 4, 5, 6, 7, 8],
    ], [3] + [4] * 9)),
    36: ("sword_held_up", {"hold": seq([0], HOLD)}),
    37: ("read_tablet", {"hold": seq([0], HOLD)}),
    38: ("pull_fall_back", {"fall": seq([0, 1, 2, 3, 4, 0, 1, 2, 3], 3)}),
    39: ("sword_swing", {"swing": seq(range(9), [1, 1, 1, 1, 1, 4, 1, 1, 2])}),
}


def read_poses(path):
    data = Path(path).read_bytes()
    header, rest = data.split(b"\n", 1)
    _, size, origin = header.split()
    size, origin = int(size), int(origin)
    record = 16 + size * size * 4
    poses = []
    for i in range(0, len(rest), record):
        d, yt, rt, r2 = struct.unpack_from("<4i", rest, i)
        img = Image.frombytes("RGBA", (size, size), rest[i + 16 : i + record])
        poses.append({"dir": d, "yt": yt, "rt": rt, "pose": r2, "image": img})
    return size, origin, poses


def main():
    src, out = sys.argv[1], Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    size, origin, poses = read_poses(src)

    groups = {}
    for p in poses:
        groups.setdefault((p["yt"], p["dir"]), []).append(p)

    index = {
        "fps": 60,
        "notes": [
            "Each strip holds the frames of one animation and direction, left to right.",
            "origin: Link's coordinate point inside a frame; align it across frames.",
            "sequences: [frame index, duration in game frames]; duration 0 = hold until an event.",
            "same_as: the strip of another direction holds the identical frames.",
        ],
        "animations": {},
    }
    seen = {}
    rows = []
    for (yt, d), frames in sorted(groups.items()):
        name, sequences = ANIMATIONS[yt]
        key = tuple(f["pose"] for f in frames)
        boxes = [f["image"].getbbox() for f in frames]
        boxes = [b for b in boxes if b]
        if not boxes:
            continue
        x0 = min(b[0] for b in boxes)
        y0 = min(b[1] for b in boxes)
        x1 = max(b[2] for b in boxes)
        y1 = max(b[3] for b in boxes)
        w, h = x1 - x0, y1 - y0
        anim = index["animations"].setdefault(name, {"type": yt, "directions": {}})
        entry = {
            "frames": len(frames),
            "frame_width": w,
            "frame_height": h,
            "origin": [origin - x0, origin - y0],
            "game_poses": list(key),
            "sequences": {
                k: v for k, v in sequences.items()
                if k not in DIRECTIONS or k == DIRECTIONS[d]
            },
        }
        if key in seen:
            entry["same_as"] = seen[key]
        else:
            file = f"{name}_{DIRECTIONS[d]}.png"
            seen[key] = file
            strip = Image.new("RGBA", (w * len(frames), h))
            for i, f in enumerate(frames):
                strip.paste(f["image"].crop((x0, y0, x1, y1)), (i * w, 0))
            strip.save(out / file)
            entry["file"] = file
            rows.append((f"{yt:2d} {name} {DIRECTIONS[d]}", strip, w))
        anim["directions"][DIRECTIONS[d]] = entry

    (out / "animations.json").write_text(json.dumps(index, indent=1) + "\n")

    scale, label_w, pad = 2, 260, 4
    width = label_w + max(s.width for _, s, _ in rows) * scale + pad
    height = sum(s.height * scale + pad for _, s, _ in rows)
    sheet = Image.new("RGBA", (width, height), (40, 120, 120, 255))
    draw = ImageDraw.Draw(sheet)
    y = 0
    for label, strip, w in rows:
        big = strip.resize((strip.width * scale, strip.height * scale), Image.NEAREST)
        draw.text((4, y + 2), label, fill=(255, 255, 255, 255))
        sheet.alpha_composite(big, (label_w, y))
        for i in range(1, strip.width // w):
            draw.line([(label_w + i * w * scale, y), (label_w + i * w * scale, y + big.height)], fill=(20, 80, 80, 255))
        y += big.height + pad
    sheet.save(out / "preview.png")


if __name__ == "__main__":
    main()
