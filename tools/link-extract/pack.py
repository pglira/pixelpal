#!/usr/bin/env python3
"""Packs the poses that `extract` writes for each gear tier into the sprite
recipe of pixelpal, and optionally into preview images.

Usage: pack.py <zelda3-dir> <recipe.json> <poses-tier0.bin> ... [--preview <dir>]

The recipe holds no graphics. It names, for every 8x8 tile of every pose, the
place of its pixels and colors in the ROM (zelda3.sfc of <zelda3-dir>), the
position of the tile in the pose, and the strips that pixelpal builds from the
poses. pixelpal reads the user's ROM and draws the strips from this recipe.

The packer draws every strip from the recipe and the ROM, and stops with an
error if a strip differs from the pixels that `extract` rendered.
"""

import hashlib
import json
import struct
import sys
from pathlib import Path

from PIL import Image, ImageDraw

DIRECTIONS = ["up", "down", "left", "right"]

# Names of the extra strips, by the extra id of extract.c (enum kExtra...).
EXTRAS = ["skull", "rock", "bush", "skull_debris", "rock_debris", "bush_debris", "zzz"]

HOLD = 0  # A duration of 0 means: the game holds this frame until an event.

# The 4bpp graphics of Link: ROM offset and length.
LINK_SHEET = (0x80000, 0x7000)


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


class Rom:
    """The ROM and the graphics sheets of zelda3's tables."""

    def __init__(self, zelda3):
        sys.path.insert(0, str(Path(zelda3) / "assets"))
        import tables
        import util

        data = (Path(zelda3) / "zelda3.sfc").read_bytes()
        if len(data) % 1024 == 512:
            data = data[512:]
        self.data = data
        self.sha1 = hashlib.sha1(data).hexdigest()
        self.sheet_ptrs = tables.kCompSpritePtrs
        self.decomp = util.decomp

    @staticmethod
    def offset(snes):
        """The file offset of a LoROM address."""
        return (snes >> 16 & 0x7F) * 0x8000 + (snes & 0x7FFF)

    def sheet(self, i):
        """Sheet `i` of 3bpp sprite graphics: 0x600 bytes, compressed from
        sheet 12 on. Returns the data and whether it is compressed."""
        if i < 12:
            o = self.offset(self.sheet_ptrs[i])
            return self.data[o:o + 0x600], False
        return self.decomp(self.sheet_ptrs[i], lambda a: self.data[self.offset(a)], False), True


def expand_3bpp(t, high):
    """A 3bpp tile (24 bytes) as a 4bpp tile (32 bytes) in SNES format. With
    `high`, every opaque pixel gets color index + 8, as Do3To4High16Bit does."""
    out = bytearray(32)
    for r in range(8):
        p0, p1, p2 = t[r * 2], t[r * 2 + 1], t[16 + r]
        out[r * 2], out[r * 2 + 1], out[16 + r * 2] = p0, p1, p2
        if high:
            out[17 + r * 2] = p0 | p1 | p2
    return bytes(out)


def tile_pixels(data):
    """The 64 color indices of a 4bpp tile in SNES format, row by row."""
    px = []
    for y in range(8):
        p = [data[y * 2], data[y * 2 + 1], data[16 + y * 2], data[17 + y * 2]]
        for x in range(8):
            b = 7 - x
            px.append(sum((p[k] >> b & 1) << k for k in range(4)))
    return px


class Sources:
    """Finds the ROM source of a tile and of a palette color."""

    def __init__(self, rom):
        self.rom = rom
        self.by_data = {}
        o, n = LINK_SHEET
        for k in range(n // 32):
            self.by_data.setdefault(rom.data[o + k * 32:o + k * 32 + 32], (-1, k, False))
        for i in range(len(rom.sheet_ptrs)):
            sheet, _ = rom.sheet(i)
            for k in range(len(sheet) // 24):
                t = sheet[k * 24:k * 24 + 24]
                for high in (True, False):
                    self.by_data.setdefault(expand_3bpp(t, high), (i, k, high))
        # Colors: the sprite palette tables first, then the whole ROM.
        self.by_color = {}
        for start, end in ((0xDD218, 0xDEC00), (0, len(rom.data) - 1)):
            for o in range(start, end, 2):
                c = struct.unpack_from("<H", rom.data, o)[0] & 0x7FFF
                self.by_color.setdefault(c, o)
        self.sheets, self.tiles, self.palettes = [], [], []
        self.tile_ids, self.palette_ids = {}, {}

    def tile(self, data):
        sheet, k, high = self.by_data[data]
        if sheet >= 0 and sheet not in [s[0] for s in self.sheets]:
            _, compressed = self.rom.sheet(sheet)
            self.sheets.append((sheet, self.rom.offset(self.rom.sheet_ptrs[sheet]), compressed))
        slot = -1 if sheet < 0 else [s[0] for s in self.sheets].index(sheet)
        return self.tile_ids.setdefault((slot, k, high), len(self.tile_ids))

    def palette(self, colors):
        refs = tuple(0 if i == 0 else self.by_color[c & 0x7FFF] for i, c in enumerate(colors))
        return self.palette_ids.setdefault(refs, len(self.palette_ids))


def read_poses(path):
    data = Path(path).read_bytes()
    header, rest = data.split(b"\n", 1)
    _, size, origin = header.split()
    size, origin = int(size), int(origin)
    poses, i = [], 0
    while i < len(rest):
        d, yt, rt, r2 = struct.unpack_from("<4i", rest, i)
        img = Image.frombytes("RGBA", (size, size), rest[i + 16:i + 16 + size * size * 4])
        i += 16 + size * size * 4
        (n,) = struct.unpack_from("<i", rest, i)
        i += 4
        tiles = []
        for _ in range(n):
            x, y, flags = struct.unpack_from("<hhB", rest, i)
            tiles.append((x, y, flags, rest[i + 8:i + 40], struct.unpack_from("<16H", rest, i + 40)))
            i += 72
        poses.append({"dir": d, "yt": yt, "rt": rt, "pose": r2, "image": img, "tiles": tiles})
    return size, origin, poses


def bbox(frames):
    boxes = [b for b in (f["image"].getbbox() for f in frames) if b]
    if not boxes:
        return None
    return (min(b[0] for b in boxes), min(b[1] for b in boxes),
            max(b[2] for b in boxes), max(b[3] for b in boxes))


class Packer:
    def __init__(self, zelda3):
        self.rom = Rom(zelda3)
        self.src = Sources(self.rom)
        self.poses, self.pose_ids = [], {}
        self.size = self.origin = None

    def pose(self, p):
        """The id of the placement list of pose `p`. A placement is 5 bytes:
        x, y, flips | palette << 2, tile (16 bit, little endian)."""
        b = bytearray()
        for x, y, flags, data, colors in p["tiles"]:
            tile = self.src.tile(data)
            pal = self.src.palette(colors)
            assert 0 <= x < 256 and 0 <= y < 256 and pal < 64
            b += bytes([x, y, flags | pal << 2, tile & 0xFF, tile >> 8])
        key = b.hex()
        if key not in self.pose_ids:
            self.pose_ids[key] = len(self.poses)
            self.poses.append(key)
        return self.pose_ids[key]

    def strip(self, name, frames):
        box = bbox(frames)
        if box is None:
            return None, None
        x0, y0, x1, y1 = box
        w, h = x1 - x0, y1 - y0
        image = Image.new("RGBA", (w * len(frames), h))
        for i, f in enumerate(frames):
            image.paste(f["image"].crop(box), (i * w, 0))
        entry = {
            "strip": name,
            "frames": len(frames),
            "frame_width": w,
            "frame_height": h,
            "origin": [self.origin - x0, self.origin - y0],
            "crop": [x0, y0],
            "poses": [self.pose(f) for f in frames],
        }
        return entry, image

    def tier(self, path):
        size, origin, poses = read_poses(path)
        assert self.size in (None, size) and self.origin in (None, origin)
        self.size, self.origin = size, origin
        groups, shadows, extras = {}, [], {}
        for p in poses:
            if p["yt"] == -1:
                shadows.append(p)
            elif p["yt"] == -2:
                extras.setdefault(EXTRAS[p["dir"]], []).append(p)
            else:
                groups.setdefault((p["yt"], p["dir"]), []).append(p)

        tier = {"animations": {}, "shadows": {}, "extras": {}}
        images = {}
        for name, frames in extras.items():
            frames.sort(key=lambda f: f["rt"])
            entry, image = self.strip(name, frames)
            if entry:
                tier["extras"][name] = entry
                images[name] = image
        for s in shadows:
            name = f"shadow_{DIRECTIONS[s['dir']]}"
            entry, image = self.strip(name, [s])
            if entry:
                tier["shadows"][DIRECTIONS[s["dir"]]] = entry
                images[name] = image
        seen = {}
        for (yt, d), frames in sorted(groups.items()):
            anim = ANIMATIONS[yt][0]
            key = tuple(f["pose"] for f in frames)
            name = seen.setdefault(key, f"{anim}_{DIRECTIONS[d]}")
            entry, image = self.strip(name, frames)
            if entry:
                tier["animations"].setdefault(anim, {})[DIRECTIONS[d]] = entry
                images[name] = image
        return tier, images


class Renderer:
    """Draws strips from the recipe and the ROM, as pixelpal does."""

    def __init__(self, rom, recipe):
        sheets = []
        for offset, compressed in recipe["sheets"]:
            i = [rom.offset(p) for p in rom.sheet_ptrs].index(offset)
            sheets.append(rom.sheet(i)[0])
        self.tiles = []
        o = recipe["link_sheet"][0]
        for slot, k, high in recipe["tiles"]:
            if slot < 0:
                data = rom.data[o + k * 32:o + k * 32 + 32]
            else:
                data = expand_3bpp(sheets[slot][k * 24:k * 24 + 24], high)
            self.tiles.append(tile_pixels(data))
        self.palettes = []
        for refs in recipe["palettes"]:
            pal = []
            for r in refs:
                c = struct.unpack_from("<H", rom.data, r)[0]
                pal.append(((c & 31) * 255 // 31, (c >> 5 & 31) * 255 // 31, (c >> 10 & 31) * 255 // 31, 255))
            self.palettes.append(pal)
        self.poses = [bytes.fromhex(p) for p in recipe["poses"]]

    def strip(self, e):
        w, h = e["frame_width"], e["frame_height"]
        x0, y0 = e["crop"]
        out = bytearray(w * len(e["poses"]) * h * 4)
        pitch = w * len(e["poses"])
        for i, pose in enumerate(e["poses"]):
            p = self.poses[pose]
            for j in range(0, len(p), 5):
                x, y, flags, tile = p[j], p[j + 1], p[j + 2], p[j + 3] | p[j + 4] << 8
                px, pal = self.tiles[tile], self.palettes[flags >> 2]
                for ty in range(8):
                    for tx in range(8):
                        sx = 7 - tx if flags & 1 else tx
                        sy = 7 - ty if flags & 2 else ty
                        c = px[sy * 8 + sx]
                        fx, fy = x + tx - x0, y + ty - y0
                        if c and 0 <= fx < w and 0 <= fy < h:
                            o = (fy * pitch + i * w + fx) * 4
                            out[o:o + 4] = bytes(pal[c])
        return Image.frombytes("RGBA", (pitch, h), bytes(out))


def fnv1a(data, h=0xCBF29CE484222325):
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def strips_of(tier):
    """Every strip entry of a tier, by strip name."""
    out = {}
    for dirs in tier["animations"].values():
        for e in dirs.values():
            out[e["strip"]] = e
    for e in list(tier["shadows"].values()) + list(tier["extras"].values()):
        out[e["strip"]] = e
    return out


def preview(images, path):
    scale, label_w, pad = 2, 200, 4
    rows = sorted(images.items())
    width = label_w + max(s.width for _, s in rows) * scale + pad
    height = sum(s.height * scale + pad for _, s in rows)
    sheet = Image.new("RGBA", (width, height), (40, 120, 120, 255))
    draw = ImageDraw.Draw(sheet)
    y = 0
    for label, strip in rows:
        big = strip.resize((strip.width * scale, strip.height * scale), Image.NEAREST)
        draw.text((4, y + 2), label, fill=(255, 255, 255, 255))
        sheet.alpha_composite(big, (label_w, y))
        y += big.height + pad
    sheet.save(path)


def main():
    args = sys.argv[1:]
    preview_dir = None
    if "--preview" in args:
        i = args.index("--preview")
        preview_dir = Path(args[i + 1])
        del args[i:i + 2]
    zelda3, out, tier_files = args[0], Path(args[1]), args[2:]

    packer = Packer(zelda3)
    tiers, all_images = [], []
    for path in tier_files:
        tier, images = packer.tier(path)
        tiers.append(tier)
        all_images.append(images)

    sequences = {name: seqs for name, seqs in ANIMATIONS.values() if seqs}
    recipe = {
        "about": [
            "The sprite recipe of pixelpal. It holds no graphics: pixelpal reads the",
            "pixels and colors from the ROM with the SHA-1 in 'rom' and draws the strips.",
            "link_sheet: [file offset, length] of the 4bpp graphics of Link.",
            "sheets: [file offset, compressed] of each 3bpp sheet of 0x600 bytes.",
            "tiles: [sheet index or -1 for link_sheet, tile index, high]; 'high' adds 8",
            "  to the color index of each opaque pixel of a 3bpp tile.",
            "palettes: 16 file offsets of BGR555 colors; color 0 is transparent.",
            "poses: placements of 5 bytes in drawing order, as hex: x, y,",
            "  flips (1 = horizontal, 2 = vertical) | palette << 2, tile (little endian).",
            "  x and y are in the pose; Link's coordinate point is at (pose_origin, pose_origin).",
            "tiers: the strips of each gear tier. Frame i of a strip is the rectangle",
            "  (crop, frame_width, frame_height) of pose poses[i]. origin: Link's coordinate",
            "  point in a frame; for a shadow, the same point with Link on the ground; for",
            "  an extra, the top-left corner of the object (or of the Zzz tile).",
            "sequences: [frame index, duration in game frames at 60 fps]; duration 0 = hold",
            "  until an event. A sequence keyed by a direction applies to that direction only.",
            "check: FNV-1a 64 of the RGBA bytes of all strips of a tier, by strip name.",
        ],
        "rom": {"sha1": packer.rom.sha1, "size": len(packer.rom.data)},
        "pose_origin": packer.origin,
        "link_sheet": list(LINK_SHEET),
        "sheets": [[o, c] for _, o, c in packer.src.sheets],
        "tiles": [list(k) for k in packer.src.tile_ids],
        "palettes": [list(p) for p in packer.src.palette_ids],
        "poses": packer.poses,
        "sequences": sequences,
        "tiers": tiers,
    }

    renderer = Renderer(packer.rom, recipe)
    for n, (tier, images) in enumerate(zip(tiers, all_images)):
        h = 0xCBF29CE484222325
        for name, entry in sorted(strips_of(tier).items()):
            drawn = renderer.strip(entry)
            if drawn.tobytes() != images[name].tobytes():
                sys.exit(f"tier{n}: strip {name} differs from the extracted pixels")
            h = fnv1a(drawn.tobytes(), h)
        tier["check"] = f"{h:016x}"
        if preview_dir:
            preview_dir.mkdir(parents=True, exist_ok=True)
            preview(images, preview_dir / f"tier{n}.png")
        print(f"tier{n}: {len(images)} strips, check {tier['check']}")

    text = json.dumps(recipe, separators=(",", ":"))
    # One line per pose and per top-level key keeps diffs readable.
    text = text.replace('","', '",\n"').replace(',"tiers":', ',\n"tiers":')
    out.write_text(text + "\n")


if __name__ == "__main__":
    main()
