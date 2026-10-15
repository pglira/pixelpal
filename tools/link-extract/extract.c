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
// direction with yt = -1, and the extra records of WriteExtras() with yt = -2:
//   int32 dir, yt, rt, r2; then POSE_SIZE * POSE_SIZE RGBA pixels.
// Link's position (link_x_coord, link_y_coord) is at pixel (ORIGIN, ORIGIN).

#include "src/player_oam.c"

#include "src/assets.h"
#include "src/config.h"
#include "src/load_gfx.h"
#include "src/nmi.h"
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

// Draws one 8x8 tile of the object name table into `out`.
static void DrawTile(uint32 *out, int tile, int px, int py, int pal, bool hflip, bool vflip) {
  const uint16 *t = &g_zenv.vram[0x4000 + (tile & 0x1ff) * 16];
  for (int y = 0; y < 8; y++) {
    int row = vflip ? 7 - y : y;
    uint16 p01 = t[row], p23 = t[row + 8];
    for (int x = 0; x < 8; x++) {
      int bit = hflip ? x : 7 - x;
      int c = (p01 >> bit & 1) | (p01 >> (bit + 8) & 1) << 1 |
              (p23 >> bit & 1) << 2 | (p23 >> (bit + 8) & 1) << 3;
      int ox = px + x, oy = py + y;
      if (c && ox >= 0 && oy >= 0 && ox < POSE_SIZE && oy < POSE_SIZE)
        out[oy * POSE_SIZE + ox] = Bgr555ToRgba(main_palette_buffer[128 + pal * 16 + c]);
    }
  }
}

// Draws the OAM entries in hardware priority order: a lower index is on top.
static void RenderOam(uint32 *out) {
  memset(out, 0, POSE_SIZE * POSE_SIZE * 4);
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
  fwrite(hdr, sizeof(int32_t), 4, f);
  fwrite(pixels, POSE_SIZE * POSE_SIZE * 4, 1, f);
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
        fwrite(hdr, sizeof(hdr), 1, f);
        fwrite(pixels, sizeof(pixels), 1, f);
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
    fwrite(hdr, sizeof(hdr), 1, f);
    fwrite(pixels, sizeof(pixels), 1, f);
  }

  WriteExtras(f, pixels);
  fclose(f);
  return 0;
}
