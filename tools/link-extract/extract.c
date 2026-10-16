// Renders every pose of Link with the drawing code and data of the zelda3
// reimplementation (https://github.com/snesrev/zelda3).
//
// The program includes player_oam.c to reach its static pose tables, and links
// against the other object files of a built zelda3 tree. It loads
// zelda3_assets.dat from the current directory.
//
// Usage: extract <out.bin> [sword 0-4] [shield 0-3] [armor 0-2]
//
// Output: a header line, then one record per pose, one shadow record per
// direction with yt = -1, the extra records of WriteExtras() with yt = -2, and
// the enemy records of WriteEnemies() with yt = -3:
//   int32 dir, yt, rt, r2; POSE_SIZE * POSE_SIZE RGBA pixels; int32 n; then
//   n TileLog entries, one per drawn 8x8 tile, in drawing order.
// Link's position (link_x_coord, link_y_coord) is at pixel (ORIGIN, ORIGIN).

#include "src/player_oam.c"

#include "src/assets.h"
#include "src/config.h"
#include "src/load_gfx.h"
#include "src/nmi.h"
#include "src/overworld.h"
#include "src/sprite.h"
#include "src/sprite_main.h"
#include "snes/ppu.h"

#include <stdio.h>
#include <stdlib.h>

enum { POSE_SIZE = 96, ORIGIN = 40, SCREEN_POS = 0x80 };

const uint8 *g_asset_ptrs[kNumberOfAssets];
uint32 g_asset_sizes[kNumberOfAssets];

// Symbols that zelda3's main.c defines for the other modules.
MemBlk FindInAssetArray(int asset, int idx) {
  return FindIndexInMemblk((MemBlk) { g_asset_ptrs[asset], g_asset_sizes[asset] }, idx);
}
void ZeldaApuLock(void) {}
void ZeldaApuUnlock(void) {}
void NORETURN Die(const char *error) {
  fprintf(stderr, "Error: %s\n", error);
  exit(1);
}

static uint8 *ReadFile(const char *name, size_t *length) {
  FILE *f = fopen(name, "rb");
  if (!f)
    return NULL;
  fseek(f, 0, SEEK_END);
  *length = ftell(f);
  rewind(f);
  uint8 *data = malloc(*length);
  if (fread(data, 1, *length, f) != *length) {
    free(data);
    data = NULL;
  }
  fclose(f);
  return data;
}

// Same layout check and parsing as LoadAssets() in zelda3's main.c.
static void LoadAssetsFile(void) {
  size_t length = 0;
  uint8 *data = ReadFile("zelda3_assets.dat", &length);
  if (!data) {
    fprintf(stderr, "cannot read zelda3_assets.dat\n");
    exit(1);
  }
  static const char kAssetsSig[] = { kAssets_Sig };
  if (length < 88 + kNumberOfAssets * 4 || memcmp(data, kAssetsSig, 48) != 0 ||
      *(uint32 *)(data + 80) != kNumberOfAssets) {
    fprintf(stderr, "invalid zelda3_assets.dat\n");
    exit(1);
  }
  uint32 offset = 88 + kNumberOfAssets * 4 + *(uint32 *)(data + 84);
  for (size_t i = 0; i < kNumberOfAssets; i++) {
    uint32 size = *(uint32 *)(data + 88 + i * 4);
    offset = (offset + 3) & ~3;
    g_asset_sizes[i] = size;
    g_asset_ptrs[i] = data + offset;
    offset += size;
  }
}

// The part of LinkOam_Main() after the selection of the animation type `yt`
// and the frame `rt`: the auxiliary item pieces, the sword, the shield and the
// body. The shadow is not drawn.
static void DrawPose(int dir, int yt, int rt, bool want_sword) {
  uint8 xcoord = SCREEN_POS, ycoord = SCREEN_POS;
  link_direction_facing = dir * 2;
  link_direction_facing_mirror = link_direction_facing;
  oam_priority_value = 0x2000;
  sort_sprites_offset_into_oam_buffer = 0;
  link_palette_bits_of_oam = 0xe00;
  link_dma_var1 = link_dma_var2 = 0;

  int r2 = kPlayerOamOtherOffs[dir * 40 + yt] + rt;
  int r4loc = kPlayerOamSpriteLocs[r2];

  int xt = FindInByteArray(kPlayerOam_Tab5, yt, 7);
  if (xt >= 0) {
    int j = kPlayerOam_Tab6[xt + dir * 7] + rt;
    uint8 bank1 = kPlayerOam_Spr1Bank[j];
    if (bank1 != 0xff) {
      link_dma_var1 = bank1 * 2;
      int oam_pos = kPlayerOam_Tab19A[r4loc] >> 2;
      oam_buf[oam_pos].y = kPlayerOam_Spr1Y[j] + ycoord;
      oam_buf[oam_pos].x = kPlayerOam_Spr1X[j] + xcoord;
      uint16 q = WORD(kPlayerOam_Prio[bank1 >> 1]);
      q = (bank1 & 1) ? q << 4 : q;
      WORD(oam_buf[oam_pos].charnum) = (q & 0xc000) | oam_priority_value | link_palette_bits_of_oam | 4;
      bytewise_extended_oam[oam_pos] = 0;
    }
    uint8 bank2 = kPlayerOam_Spr2Bank[j];
    if (bank2 != 0xff) {
      link_dma_var2 = bank2 * 2;
      int oam_pos = kPlayerOam_Tab20A[r4loc] >> 2;
      oam_buf[oam_pos].y = kPlayerOam_Spr2Y[j] + ycoord;
      oam_buf[oam_pos].x = kPlayerOam_Spr2X[j] + xcoord;
      uint16 q = WORD(kPlayerOam_Prio[bank2 >> 1]);
      q = (bank2 & 1) ? q << 4 : q;
      WORD(oam_buf[oam_pos].charnum) = (q & 0xc000) | oam_priority_value | link_palette_bits_of_oam | 0x14;
      bytewise_extended_oam[oam_pos] = 0;
    }
  }

  SwordResult sr;
  if (want_sword && link_sword_type != 0 && !LinkOam_SetWeaponVRAMOffsets(r2, &sr)) {
    uint8 oam_y = kDrawSword_y[r2] + ycoord;
    uint8 oam_x = kDrawSword_x[r2] + xcoord;
    int oam_pos = kSwordStuff_oam_index_ptrs_0[r4loc] >> 2;
    int j = sr.r6 * 3;
    for (int i = 0; i != 3; i++, j++) {
      uint16 td = kSwordTiledata[j];
      if (td != 0xffff) {
        td = (td & ~0x3000) | oam_priority_value;
        WORD(oam_buf[oam_pos].charnum) = td;
        oam_buf[oam_pos].x = oam_x;
        oam_buf[oam_pos].y = oam_y;
        bytewise_extended_oam[oam_pos] = sr.r12;
        oam_pos++;
      }
      oam_x += 8;
      if (i == 1)
        oam_x -= 16, oam_y += 8;
    }
  }

  if (link_shield_type && !LinkOam_SetEquipmentVRAMOffsets(r2, &sr)) {
    uint8 oam_y = kShieldStuff_y[r2] + ycoord - 1;
    uint8 oam_x = kShieldStuff_x[r2] + xcoord;
    uint16 oam_pal = 0xa00;
    int oam_pos = kShieldStuff_oam_index_ptrs_0[r4loc] >> 2;
    int j = sr.r6 * 3;
    for (int i = 0; i != 3; i++, j++) {
      uint16 td = kShieldStuff_OamData[j];
      if (td == 0xffff)
        continue;
      td = (td & 0xc1ff) | oam_pal | oam_priority_value;
      WORD(oam_buf[oam_pos].charnum) = td;
      WORD(oam_buf[oam_pos].x) = oam_x | oam_y << 8;
      bytewise_extended_oam[oam_pos] = sr.r12;
      oam_x += 8;
      if (i == 1)
        oam_x -= 16, oam_y += 8;
    }
  }

  {
    int oam_pos = kLinkBody_oam_index_0[r4loc] >> 2;
    int j = kLinkDmaGraphicsIndices[r2];
    link_dma_graphics_index = j * 2;
    const LinkSpriteBody *sp = &kLinkSpriteBodys[j];
    uint8 oam_y = ycoord + sp->y;
    uint8 oam_x = xcoord + sp->x;
    uint16 td = sp->tile << 8;
    if ((td & 0xf000) != 0xf000) {
      WORD(oam_buf[oam_pos].charnum) = td & 0xf000 | oam_priority_value | link_palette_bits_of_oam;
      WORD(oam_buf[oam_pos].x) = oam_x | oam_y << 8;
      bytewise_extended_oam[oam_pos] = 2;
    }
    if ((td << 4 & 0xf000) != 0xf000) {
      WORD(oam_buf[oam_pos + 1].charnum) = td << 4 & 0xf000 | oam_priority_value | link_palette_bits_of_oam | 2;
      WORD(oam_buf[oam_pos + 1].x) = xcoord | (ycoord + 8) << 8;
      bytewise_extended_oam[oam_pos + 1] = 2;
    }
  }
}

// The ground shadow of Link, as the shadow part of LinkOam_Main() draws it:
// two 8x8 tiles, the second one mirrored.
static void DrawShadow(int dir) {
  uint8 oam_x = SCREEN_POS + kOffsToShadowGivenDir_X[dir];
  uint8 oam_y = SCREEN_POS + kOffsToShadowGivenDir_Y[dir];
  uint16 td = kLinkShadows_Chardata[0] & ~0x3000 | 0x2000;
  WORD(oam_buf[0].charnum) = td;
  WORD(oam_buf[1].charnum) = td & ~0xC000 | 0x4000;
  WORD(oam_buf[0].x) = oam_x | oam_y << 8;
  WORD(oam_buf[1].x) = (uint8)(oam_x + 8) | oam_y << 8;
  bytewise_extended_oam[0] = 0;
  bytewise_extended_oam[1] = 0;
}

static uint32 Bgr555ToRgba(uint16 c) {
  uint32 r = (c & 31) * 255 / 31, g = (c >> 5 & 31) * 255 / 31, b = (c >> 10 & 31) * 255 / 31;
  return r | g << 8 | b << 16 | 0xffu << 24;
}

// One drawn 8x8 tile: its position in the pose, the flips (1 = horizontal,
// 2 = vertical), its 4bpp data in SNES format, and its 16 palette colors.
typedef struct TileLog {
  int16 x, y;
  uint8 flags, pad[3];
  uint16 data[16];
  uint16 colors[16];
} TileLog;

static TileLog g_tiles[1024];
static int g_num_tiles;

// Draws one 8x8 tile of the object name table into `out`, and logs it to
// g_tiles if it has an opaque pixel inside the pose.
static void DrawTile(uint32 *out, int tile, int px, int py, int pal, bool hflip, bool vflip) {
  const uint16 *t = &g_zenv.vram[0x4000 + (tile & 0x1ff) * 16];
  bool drawn = false;
  for (int y = 0; y < 8; y++) {
    int row = vflip ? 7 - y : y;
    uint16 p01 = t[row], p23 = t[row + 8];
    for (int x = 0; x < 8; x++) {
      int bit = hflip ? x : 7 - x;
      int c = (p01 >> bit & 1) | (p01 >> (bit + 8) & 1) << 1 |
              (p23 >> bit & 1) << 2 | (p23 >> (bit + 8) & 1) << 3;
      int ox = px + x, oy = py + y;
      if (c && ox >= 0 && oy >= 0 && ox < POSE_SIZE && oy < POSE_SIZE) {
        out[oy * POSE_SIZE + ox] = Bgr555ToRgba(main_palette_buffer[128 + pal * 16 + c]);
        drawn = true;
      }
    }
  }
  if (drawn && g_num_tiles < 1024) {
    TileLog *l = &g_tiles[g_num_tiles++];
    memset(l, 0, sizeof(*l));
    l->x = px, l->y = py;
    l->flags = hflip | vflip << 1;
    memcpy(l->data, t, 32);
    memcpy(l->colors, &main_palette_buffer[128 + pal * 16], 32);
  }
}

// Writes a record: the header, the pixels, and the tiles of g_tiles in
// drawing order.
static void WritePose(FILE *f, int32_t hdr[4], uint32 *pixels) {
  fwrite(hdr, sizeof(int32_t), 4, f);
  fwrite(pixels, POSE_SIZE * POSE_SIZE * 4, 1, f);
  int32_t n = g_num_tiles;
  fwrite(&n, sizeof(n), 1, f);
  fwrite(g_tiles, sizeof(TileLog), n, f);
}

// Draws the OAM entries in hardware priority order: a lower index is on top.
static void RenderOam(uint32 *out) {
  memset(out, 0, POSE_SIZE * POSE_SIZE * 4);
  g_num_tiles = 0;
  for (int i = 127; i >= 0; i--) {
    OamEnt *e = &oam_buf[i];
    if (e->y == 0xf0)
      continue;
    uint8 ext = bytewise_extended_oam[i];
    int x = e->x | (ext & 1) << 8;
    if (x >= 256)
      x -= 512;
    int y = e->y;
    uint16 cw = WORD(e->charnum);
    int tile = cw & 0x1ff, pal = cw >> 9 & 7;
    bool hflip = cw >> 14 & 1, vflip = cw >> 15 & 1;
    int n = (ext & 2) ? 2 : 1;
    int bx = x - SCREEN_POS + ORIGIN, by = y - SCREEN_POS + ORIGIN;
    for (int ty = 0; ty < n; ty++) {
      for (int tx = 0; tx < n; tx++) {
        int sx = hflip ? n - 1 - tx : tx, sy = vflip ? n - 1 - ty : ty;
        int t = (tile & 0x100) | ((tile + sx) & 0x0f) | ((tile & 0xf0) + sy * 16) & 0xf0;
        DrawTile(out, t, bx + tx * 8, by + ty * 8, pal, hflip, vflip);
      }
    }
  }
}

// Extra strips: liftable objects, their debris, and the Zzz of a sleeping
// Link. Each record has the animation type -2, the extra id as `dir`, and the
// frame as `rt`. The origin of an object or debris frame is the top-left corner
// of the object; the origin of a Zzz frame is the top-left corner of its tile.
enum { kExtraSkull, kExtraRock, kExtraBush, kExtraSkullDebris, kExtraRockDebris,
       kExtraBushDebris, kExtraZzz };

// One 8x8 debris piece: offset from the object's top-left corner, tile, and
// flips (1 = horizontal, 2 = vertical), from Garnish16_ThrownItemDebris().
typedef struct DebrisPiece { int8 dx, dy; uint8 tile, flip; } DebrisPiece;

static const DebrisPiece kShardDebris[8][4] = {
  {{0, 0, 0x58, 2}, {8, 0, 0x58, 0}, {0, 8, 0x58, 2}, {8, 8, 0x58, 1}},
  {{-2, 0, 0x58, 2}, {9, -1, 0x58, 1}, {-1, 10, 0x58, 2}, {9, 10, 0x58, 0}},
  {{-4, 0, 0x58, 0}, {9, -3, 0x58, 3}, {-1, 11, 0x58, 0}, {10, 7, 0x58, 2}},
  {{-6, 1, 0x58, 2}, {9, -4, 0x58, 1}, {-1, 12, 0x58, 2}, {12, 8, 0x58, 0}},
  {{-7, 1, 0x48, 2}, {9, -4, 0x58, 3}, {-2, 13, 0x58, 0}, {13, 9, 0x58, 2}},
  {{-9, 2, 0x48, 0}, {9, -4, 0x58, 0}, {-3, 16, 0x58, 2}, {14, 10, 0x48, 0}},
  {{-4, 14, 0x48, 2}, {-4, 14, 0x48, 2}, {9, -4, 0x58, 2}, {15, 11, 0x48, 2}},
  {{-3, 16, 0x48, 0}, {-3, 16, 0x48, 0}, {-3, 16, 0x48, 0}, {9, -1, 0x48, 0}},
};

static const DebrisPiece kBushDebris[8][4] = {
  {{-4, 2, 0x59, 1}, {4, -5, 0x59, 1}, {6, 5, 0x59, 1}, {10, 1, 0x59, 0}},
  {{-1, 3, 0x59, 1}, {4, -7, 0x59, 1}, {6, 8, 0x59, 1}, {7, 2, 0x59, 0}},
  {{0, 4, 0x59, 1}, {2, -8, 0x59, 1}, {4, 4, 0x59, 0}, {7, 10, 0x59, 0}},
  {{1, -9, 0x59, 2}, {1, 4, 0x59, 0}, {5, 4, 0x59, 1}, {7, 12, 0x59, 1}},
  {{0, -10, 0x59, 1}, {-2, 4, 0x59, 0}, {8, 8, 0x59, 1}, {9, 14, 0x59, 1}},
  {{-1, -12, 0x59, 1}, {-6, 4, 0x59, 1}, {9, 8, 0x59, 1}, {10, 15, 0x59, 1}},
  {{-2, -15, 0x59, 1}, {-7, 3, 0x59, 1}, {12, 8, 0x59, 0}, {11, 17, 0x59, 0}},
  {{-3, -17, 0x59, 1}, {-9, 1, 0x59, 0}, {4, 18, 0x59, 0}, {6, 15, 0x59, 0}},
};

static void ClearOam(void) {
  for (int i = 0; i < 128; i++)
    oam_buf[i].y = 0xf0;
  memset(bytewise_extended_oam, 0, 128);
}

// Sets OAM entry `i` to a tile at an offset from the screen point of Link.
static void PutTile(int i, int dx, int dy, uint16 charnum, bool big) {
  WORD(oam_buf[i].charnum) = charnum;
  oam_buf[i].x = SCREEN_POS + dx;
  oam_buf[i].y = SCREEN_POS + dy;
  bytewise_extended_oam[i] = big ? 2 : 0;
}

static void WriteRecord(FILE *f, uint32 *pixels, int id, int frame) {
  RenderOam(pixels);
  int32_t hdr[4] = { id, -2, frame, -1 };
  WritePose(f, hdr, pixels);
}

// Writes an object (palette 6) and its 8 debris frames. The game draws the
// pieces from the last one to the first, so the last piece is on top.
static void WriteObject(FILE *f, uint32 *pixels, int obj_id, uint8 tile,
                        int debris_id, const DebrisPiece (*debris)[4]) {
  ClearOam();
  PutTile(0, 0, 0, 0x2c00 | tile, true);
  WriteRecord(f, pixels, obj_id, 0);
  for (int frame = 0; frame < 8; frame++) {
    ClearOam();
    for (int k = 0; k < 4; k++) {
      const DebrisPiece *p = &debris[frame][k];
      uint16 flips = (p->flip & 1 ? 0x4000 : 0) | (p->flip & 2 ? 0x8000 : 0);
      PutTile(3 - k, p->dx, p->dy, 0x2c00 | p->tile | flips, false);
    }
    WriteRecord(f, pixels, debris_id, frame);
  }
}

static void LoadSpritePalette6(void) {
  memcpy(&main_palette_buffer[128 + 6 * 16], &aux_palette_buffer[128 + 6 * 16], 16 * sizeof(uint16));
}

static void WriteExtras(FILE *f, uint32 *pixels) {
  // Indoors: the common sprite pack 10 holds the skull pot of the dungeons.
  misc_sprites_graphics_index = 10;
  LoadCommonSprites();
  palette_sp6r_indoors = 10;
  Palette_Load_SpriteEnvironment_Dungeon();
  LoadSpritePalette6();
  WriteObject(f, pixels, kExtraSkull, 0x44, kExtraSkullDebris, kShardDebris);

  // Light world: the common sprite pack 1 holds the rock and the bush.
  misc_sprites_graphics_index = 1;
  LoadCommonSprites();
  Palette_MiscSprite_Outdoors();
  LoadSpritePalette6();
  WriteObject(f, pixels, kExtraRock, 0x44, kExtraRockDebris, kShardDebris);
  WriteObject(f, pixels, kExtraBush, 0x46, kExtraBushDebris, kBushDebris);

  // The Zzz: tile 9, uploaded from the item graphics in 3 sizes.
  static const uint8 kZzzDma[3] = { 0x44, 0x43, 0x42 };
  for (int frame = 0; frame < 3; frame++) {
    link_dma_var5 = kZzzDma[frame];
    NMI_PrepareSprites();
    nmi_disable_core_updates = 0;
    NMI_DoUpdates();
    ClearOam();
    PutTile(0, 0, 0, 0x2409, false);
    WriteRecord(f, pixels, kExtraZzz, frame);
  }
}

// Enemy strips. Each record has the animation type -3, the enemy id as `dir`,
// the frame as `rt`, and the animation id as `r2`. The sprite coordinate of
// the enemy (the top-left corner of its 16x16 body box) is at pixel
// (ORIGIN, ORIGIN). Every frame is drawn by the sprite routine of the game,
// with the game paused so that the routine draws and does not move.
enum { kEnemyOctorok, kEnemyOctorokStone, kEnemyBuzzblob, kEnemyKeese,
       kEnemyBlueSoldier, kEnemyGreenSoldier, kEnemyPoof, kEnemyHeart,
       kEnemyGreenRupee, kEnemyBlueRupee };

// Animation ids. A walk or shoot animation adds the direction sprite_D
// (0 = right, 1 = left, 2 = down, 3 = up) to its base id.
enum { kAnimWalk = 0, kAnimShoot = 4, kAnimHit = 8, kAnimIdle = 9, kAnimFly = 10,
       kAnimShock = 11, kAnimCrumble = 12, kAnimPoof = 13, kAnimFall = 14 };

enum { kEnemySlot = 0 };

// Sets up sprite slot kEnemySlot for `type` as SpritePrep_LoadProperties()
// does, active, on the lower floor, at the screen point of Link.
static void PrepEnemy(int type) {
  int k = kEnemySlot;
  sprite_type[k] = type;
  SpritePrep_LoadProperties(k);
  sprite_state[k] = 9;
  sprite_floor[k] = 0;
  sprite_x_lo[k] = SCREEN_POS, sprite_x_hi[k] = 0;
  sprite_y_lo[k] = SCREEN_POS, sprite_y_hi[k] = 0;
  sprite_obj_prio[k] = 0x20;
}

// Draws the sprite in slot kEnemySlot with `draw`, after the OAM allocation
// of Sprite_TimersAndOam(), and writes the record.
static void WriteEnemyFrame(FILE *f, uint32 *pixels, int id, int anim, int frame,
                            void (*draw)(int k)) {
  int k = kEnemySlot;
  ClearOam();
  Oam_ResetRegionBases();
  Sprite_Get16BitCoords(k);
  Oam_AllocateFromRegionA(((sprite_flags2[k] & 0x1f) + 1) * 4);
  draw(k);
  RenderOam(pixels);
  int32_t hdr[4] = { id, -3, frame, anim };
  WritePose(f, hdr, pixels);
}

// The damage flash of Sprite_TimersAndOam(): while sprite_hit_timer counts
// down, its low 3 bits select the palette (XOR) of the sprite, one per frame.
static void WriteEnemyHit(FILE *f, uint32 *pixels, int id) {
  uint8 prio = sprite_obj_prio[kEnemySlot];
  for (int i = 0; i < 8; i++) {
    sprite_obj_prio[kEnemySlot] = prio & ~0xe | (7 - i) * 2;
    WriteEnemyFrame(f, pixels, id, kAnimHit, i, SpriteActive_Main);
  }
  sprite_obj_prio[kEnemySlot] = prio;
}

// Loads the sprite graphics set `gfx` into VRAM 0x5000..0x5fff, the common
// sprite pack `misc`, and the sprite palettes of a light world area with the
// sprite palette set `spr_pal`.
static void LoadEnemyGraphics(int gfx, int misc, int area, int spr_pal) {
  sprite_graphics_index = gfx;
  misc_sprites_graphics_index = misc;
  InitializeTilesets();
  overworld_screen_index = area;
  palette_sp0l = 1;
  Palette_Load_SpriteMain();
  Palette_Load_Sp0L();
  Palette_MiscSprite_Outdoors();
  Overworld_LoadPalettes(GetOverworldBgPalette(area), spr_pal);
  // Sprite palettes 0..6; palette 7 holds Link.
  memcpy(&main_palette_buffer[128], &aux_palette_buffer[128], 112 * sizeof(uint16));
}

static void DrawPoof(int k) {
  SpriteDeath_DrawPoof(k);
}

static void WriteEnemies(FILE *f, uint32 *pixels) {
  int k = kEnemySlot;
  uint8 sub_bak = submodule_index;
  submodule_index = 1;

  // Octorok, its stone and the Buzzblob: Lake Hylia NW (area 0x35), sprite
  // graphics set 4, sprite palette set 3.
  LoadEnemyGraphics(4, 1, 0x35, 3);

  // Octorok walk: sprite_graphics = subtype2 >> 3 & 3 | (D & 2) << 1.
  for (int d = 0; d < 4; d++) {
    for (int i = 0; i < 4; i++) {
      PrepEnemy(0x08);
      sprite_D[k] = d;
      sprite_graphics[k] = i | (d & 2) << 1;
      WriteEnemyFrame(f, pixels, kEnemyOctorok, kAnimWalk + d, i, SpriteActive_Main);
    }
  }
  // Octorok shoot: sprite_C (the snout) is 0, 1, 2 and back to 0.
  for (int d = 0; d < 4; d++) {
    for (int i = 0; i < 3; i++) {
      PrepEnemy(0x08);
      sprite_D[k] = d;
      sprite_graphics[k] = (d & 2) << 1;
      sprite_C[k] = i;
      WriteEnemyFrame(f, pixels, kEnemyOctorok, kAnimShoot + d, i, SpriteActive_Main);
    }
  }
  PrepEnemy(0x08);
  sprite_D[k] = 2;
  sprite_graphics[k] = 4;
  WriteEnemyHit(f, pixels, kEnemyOctorok);

  // Octorok stone: in flight, and crumbling with sprite_delay_main from 31 down.
  PrepEnemy(0x0c);
  WriteEnemyFrame(f, pixels, kEnemyOctorokStone, kAnimIdle, 0, SpriteActive_Main);
  for (int i = 0; i < 4; i++) {
    PrepEnemy(0x0c);
    sprite_state[k] = 6;
    sprite_delay_main[k] = 31 - i * 8;
    WriteEnemyFrame(f, pixels, kEnemyOctorokStone, kAnimCrumble, i, SpriteActive_Main);
  }

  // Buzzblob: sprite_graphics = {0, 1, 0, 2}[subtype2 >> 3 & 3], plus 3 while
  // sprite_delay_aux1 (the shock, from 64 down) flashes the palette.
  static const uint8 kBuzzblobGfx[4] = { 0, 1, 0, 2 };
  for (int i = 0; i < 4; i++) {
    PrepEnemy(0x0d);
    sprite_graphics[k] = kBuzzblobGfx[i];
    WriteEnemyFrame(f, pixels, kEnemyBuzzblob, kAnimWalk + 2, i, SpriteActive_Main);
  }
  for (int i = 0; i < 16; i++) {
    PrepEnemy(0x0d);
    sprite_delay_aux1[k] = 64 - i * 2;
    sprite_graphics[k] = kBuzzblobGfx[i >> 2 & 3] + 3;
    WriteEnemyFrame(f, pixels, kEnemyBuzzblob, kAnimShock, i, SpriteActive_Main);
  }
  PrepEnemy(0x0d);
  WriteEnemyHit(f, pixels, kEnemyBuzzblob);

  // Soldiers: Lost Woods Outskirts (area 0x10), sprite graphics set 6,
  // sprite palette set 1. The body frame is kSoldier_Gfx2[D * 4 + (subtype2
  // >> 3 & 3)] of Guard_TickAndUpdateBody(), and the head looks along D.
  LoadEnemyGraphics(6, 1, 0x10, 1);
  static const uint8 kSoldierGfx2[16] = { 11, 12, 13, 12, 4, 5, 6, 5, 0, 1, 2, 3, 7, 8, 9, 10 };
  for (int s = 0; s < 2; s++) {
    int id = s ? kEnemyGreenSoldier : kEnemyBlueSoldier;
    int type = s ? 0x42 : 0x41;
    for (int d = 0; d < 4; d++) {
      for (int i = 0; i < 4; i++) {
        PrepEnemy(type);
        sprite_D[k] = sprite_head_dir[k] = d;
        sprite_graphics[k] = kSoldierGfx2[d * 4 + i];
        WriteEnemyFrame(f, pixels, id, kAnimWalk + d, i, SpriteActive_Main);
      }
    }
    PrepEnemy(type);
    sprite_D[k] = sprite_head_dir[k] = 2;
    sprite_graphics[k] = kSoldierGfx2[8];
    WriteEnemyHit(f, pixels, id);
  }

  // Keese: a dungeon enemy, in sprite graphics set 0x41 (the set of the
  // enemy block 1). It rests with sprite_graphics 0 and flies with
  // sprite_graphics = (frame_counter >> 2 & 1) + 1.
  LoadEnemyGraphics(0x41, 10, 0x10, 1);
  PrepEnemy(0x6f);
  WriteEnemyFrame(f, pixels, kEnemyKeese, kAnimIdle, 0, SpriteActive_Main);
  for (int i = 0; i < 2; i++) {
    PrepEnemy(0x6f);
    sprite_graphics[k] = i + 1;
    WriteEnemyFrame(f, pixels, kEnemyKeese, kAnimFly, i, SpriteActive_Main);
  }
  PrepEnemy(0x6f);
  sprite_graphics[k] = 1;
  WriteEnemyHit(f, pixels, kEnemyKeese);

  // The death poof of SpriteDeath_DrawPoof(), light world, with
  // sprite_delay_main from 31 down: one frame per 4 counts.
  LoadEnemyGraphics(4, 1, 0x35, 3);
  LoadDefaultGraphics();
  for (int i = 0; i < 8; i++) {
    PrepEnemy(0x08);
    sprite_state[k] = 6;
    sprite_delay_main[k] = 31 - i * 4;
    WriteEnemyFrame(f, pixels, kEnemyPoof, kAnimPoof, i, DrawPoof);
  }

  // Drops: the heart on the ground (sprite_graphics 0) and in its fall
  // (sprite_graphics 1, mirrored while it glides to the right), and the rupees.
  PrepEnemy(0xd8);
  WriteEnemyFrame(f, pixels, kEnemyHeart, kAnimIdle, 0, SpriteActive_Main);
  for (int i = 0; i < 2; i++) {
    PrepEnemy(0xd8);
    sprite_graphics[k] = 1;
    sprite_oam_flags[k] = sprite_oam_flags[k] & ~0x40 | (i ? 0x40 : 0);
    WriteEnemyFrame(f, pixels, kEnemyHeart, kAnimFall, i, SpriteActive_Main);
  }
  // The rupees use tiles 0x0b and 0x1b, which NMI_DoUpdates() uploads from
  // the item graphics at 0xb280 + {0, 0x20, 0x40}: the shine of the rupee.
  for (int i = 0; i < 3; i++) {
    NMI_PrepareSprites();
    dma_source_addr_9 = 0xb280 + i * 0x20;
    dma_source_addr_14 = dma_source_addr_9 + 0x60;
    nmi_disable_core_updates = 0;
    NMI_DoUpdates();
    PrepEnemy(0xd9);
    WriteEnemyFrame(f, pixels, kEnemyGreenRupee, kAnimIdle, i, SpriteActive_Main);
    PrepEnemy(0xda);
    WriteEnemyFrame(f, pixels, kEnemyBlueRupee, kAnimIdle, i, SpriteActive_Main);
  }

  sprite_state[k] = 0;
  submodule_index = sub_bak;
}

int main(int argc, char **argv) {
  if (argc < 2) {
    fprintf(stderr, "usage: %s <out.bin> [sword 0-4] [shield 0-3] [armor 0-2]\n", argv[0]);
    return 1;
  }
  LoadAssetsFile();
  ZeldaInitialize();

  link_sword_type = argc > 2 ? atoi(argv[2]) : 1;
  link_shield_type = argc > 3 ? atoi(argv[3]) : 1;
  link_armor = argc > 4 ? atoi(argv[4]) : 0;
  link_x_coord = link_y_coord = SCREEN_POS;
  BG2HOFS_copy2 = BG2VOFS_copy2 = 0;

  LoadItemGFXIntoWRAM4BPPBuffer();
  DecompressSwordGraphics();
  DecompressShieldGraphics();
  Palette_Load_SpriteMain();
  Palette_Load_Sp0L();
  Palette_Load_Sp5L();
  Palette_Load_Sp6L();
  Palette_MiscSprite_Outdoors();
  Palette_Load_Sword();
  Palette_Load_Shield();
  Palette_Load_LinkArmorAndGloves();
  // Some loaders write only the auxiliary buffer, which the game copies to
  // the main buffer when the screen fades in.
  memcpy(&main_palette_buffer[128], &aux_palette_buffer[128], 128 * sizeof(uint16));
  // The common sprites hold the tiles of the shadow. Pack 10 is the set that
  // the dungeons use.
  misc_sprites_graphics_index = 10;
  LoadCommonSprites();

  FILE *f = fopen(argv[1], "wb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  fprintf(f, "LINKPOSES %d %d\n", POSE_SIZE, ORIGIN);

  // A pose range starts at each distinct offset and ends at the next one.
  uint16 starts[160];
  int nstarts = 0;
  for (int i = 0; i < 160; i++) {
    bool seen = false;
    for (int k = 0; k < nstarts; k++)
      seen |= starts[k] == kPlayerOamOtherOffs[i];
    if (!seen)
      starts[nstarts++] = kPlayerOamOtherOffs[i];
  }

  static uint32 pixels[POSE_SIZE * POSE_SIZE];
  for (int yt = 0; yt < 40; yt++) {
    for (int dir = 0; dir < 4; dir++) {
      int start = kPlayerOamOtherOffs[dir * 40 + yt], end = 511;
      for (int k = 0; k < nstarts; k++)
        if (starts[k] > start && starts[k] < end)
          end = starts[k];
      for (int rt = 0; rt < end - start; rt++) {
        for (int i = 0; i < 128; i++)
          oam_buf[i].y = 0xf0;
        memset(bytewise_extended_oam, 0, 128);
        DrawPose(dir, yt, rt, true);
        NMI_PrepareSprites();
        nmi_disable_core_updates = 0;
        NMI_DoUpdates();
        RenderOam(pixels);
        int32_t hdr[4] = { dir, yt, rt, start + rt };
        WritePose(f, hdr, pixels);
      }
    }
  }

  // One shadow record per direction, with the animation type -1.
  for (int dir = 0; dir < 4; dir++) {
    for (int i = 0; i < 128; i++)
      oam_buf[i].y = 0xf0;
    memset(bytewise_extended_oam, 0, 128);
    DrawShadow(dir);
    RenderOam(pixels);
    int32_t hdr[4] = { dir, -1, 0, -1 };
    WritePose(f, hdr, pixels);
  }

  WriteExtras(f, pixels);
  WriteEnemies(f, pixels);
  fclose(f);
  return 0;
}
