//! Animations of Link, drawn from the ROM of the game with the sprite recipe
//! `assets/link.json` that `tools/link-extract` writes. The recipe has one set
//! of strips per gear tier: one strip per animation and direction, the ground
//! shadows, and extra strips (objects, debris, Zzz), each with the frame size,
//! the origin and the playback sequences.

use std::collections::HashMap;
use std::sync::OnceLock;

use egui::{ColorImage, Context, TextureHandle, TextureOptions, Vec2, vec2};
use image::RgbaImage;
use serde::Deserialize;

/// The sprite recipe. It holds no graphics: it names the places of the pixels
/// and colors in the ROM.
const RECIPE: &str = include_str!("../assets/link.json");

/// Duration of one game frame, in seconds.
pub const GAME_FRAME: f32 = 1.0 / 60.0;

/// The tiles and the palettes of the recipe, read from the ROM.
struct Graphics {
    /// The 64 color indices of each tile, row by row.
    tiles: Vec<[u8; 64]>,
    /// The RGBA colors of each palette.
    palettes: Vec<[[u8; 4]; 16]>,
    tier_count: usize,
}

static GRAPHICS: OnceLock<Graphics> = OnceLock::new();

/// Reads the tiles and the palettes of the recipe from `rom`, the checked ROM
/// without a copier header. Call it once before the first `Sprites::load`.
pub fn init(rom: &[u8]) -> Result<(), String> {
    let graphics = Graphics::new(&recipe(), rom)?;
    let _ = GRAPHICS.set(graphics);
    Ok(())
}

#[cfg(not(test))]
fn graphics() -> &'static Graphics {
    GRAPHICS.get().expect("link::init ran")
}

/// The graphics of the ROM in `$PIXELPAL_ROM`, or else of the stored ROM.
#[cfg(test)]
fn graphics() -> &'static Graphics {
    GRAPHICS.get_or_init(|| {
        let path = std::env::var_os("PIXELPAL_ROM")
            .map(std::path::PathBuf::from)
            .or_else(crate::rom::stored_path)
            .expect("a ROM path");
        let rom = crate::rom::read(&path).unwrap_or_else(|e| {
            panic!("The tests need the ROM. Import it or set PIXELPAL_ROM.\n{e}")
        });
        Graphics::new(&recipe(), &rom).expect("the ROM fits the recipe")
    })
}

/// Number of gear tiers. Tier 0 always exists.
pub fn tier_count() -> usize {
    graphics().tier_count
}

#[derive(Deserialize)]
struct Recipe {
    rom: RomJson,
    /// File offset and length of the 4bpp sheet of Link.
    link_sheet: (usize, usize),
    /// File offset and compression of each 3bpp sheet.
    sheets: Vec<(usize, bool)>,
    /// Sheet index (or -1 for the sheet of Link), tile index, and `high`.
    tiles: Vec<(i32, usize, bool)>,
    /// File offsets of the 16 colors of each palette.
    palettes: Vec<[usize; 16]>,
    /// The tile placements of each pose, as hex.
    poses: Vec<String>,
    /// The playback sequences of each animation.
    sequences: HashMap<String, HashMap<String, Sequence>>,
    tiers: Vec<TierJson>,
}

#[derive(Deserialize)]
struct RomJson {
    sha1: String,
}

#[derive(Deserialize)]
struct TierJson {
    animations: HashMap<String, HashMap<String, StripJson>>,
    shadows: HashMap<String, StripJson>,
    extras: HashMap<String, StripJson>,
    /// FNV-1a 64 of the RGBA bytes of all strips, by strip name.
    #[cfg_attr(not(test), allow(dead_code))]
    check: String,
}

#[derive(Deserialize)]
struct StripJson {
    strip: String,
    frame_width: u32,
    frame_height: u32,
    origin: [f32; 2],
    /// The top-left corner of each frame in its pose.
    crop: [i32; 2],
    /// The pose of each frame.
    poses: Vec<usize>,
}

fn recipe() -> Recipe {
    serde_json::from_str(RECIPE).expect("the embedded recipe is valid")
}

impl Graphics {
    fn new(recipe: &Recipe, rom: &[u8]) -> Result<Self, String> {
        if recipe.rom.sha1 != crate::rom::SHA1 {
            return Err("The sprite recipe is for a different ROM.".into());
        }
        let bytes = |offset: usize, len: usize| {
            rom.get(offset..offset + len)
                .ok_or_else(|| format!("The ROM ends before offset {offset:#x}."))
        };
        let sheets = recipe
            .sheets
            .iter()
            .map(|&(offset, compressed)| {
                if compressed {
                    decompress(rom, offset)
                } else {
                    bytes(offset, 0x600).map(<[u8]>::to_vec)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (link_offset, link_len) = recipe.link_sheet;
        let link = bytes(link_offset, link_len)?;
        let tiles = recipe
            .tiles
            .iter()
            .map(|&(sheet, index, high)| {
                if sheet < 0 {
                    let t = link
                        .get(index * 32..index * 32 + 32)
                        .ok_or("A tile is outside of the sheet of Link.")?;
                    Ok(decode_4bpp(t))
                } else {
                    let t = sheets
                        .get(sheet as usize)
                        .and_then(|s| s.get(index * 24..index * 24 + 24))
                        .ok_or("A tile is outside of its sheet.")?;
                    Ok(decode_3bpp(t, high))
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        let palettes = recipe
            .palettes
            .iter()
            .map(|offsets| {
                let mut pal = [[0; 4]; 16];
                for (color, &offset) in pal.iter_mut().zip(offsets).skip(1) {
                    let b = bytes(offset, 2)?;
                    *color = bgr555(u16::from_le_bytes([b[0], b[1]]));
                }
                Ok(pal)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            tiles,
            palettes,
            tier_count: recipe.tiers.len(),
        })
    }

    /// Draws a strip: frame `i` is the rectangle at `crop` of pose `poses[i]`.
    /// A later tile of a pose covers an earlier one.
    fn strip(&self, poses: &[Vec<u8>], s: &StripJson) -> RgbaImage {
        let (w, h) = (s.frame_width, s.frame_height);
        let mut img = RgbaImage::new(w * s.poses.len() as u32, h);
        for (i, &pose) in s.poses.iter().enumerate() {
            let placements = poses.get(pose).map(Vec::as_slice).unwrap_or_default();
            for p in placements.chunks_exact(5) {
                let (x, y, flags) = (p[0] as i32, p[1] as i32, p[2]);
                let tile = u16::from_le_bytes([p[3], p[4]]) as usize;
                let (Some(tile), Some(pal)) = (
                    self.tiles.get(tile),
                    self.palettes.get((flags >> 2) as usize),
                ) else {
                    continue;
                };
                for ty in 0..8 {
                    for tx in 0..8 {
                        let sx = if flags & 1 != 0 { 7 - tx } else { tx };
                        let sy = if flags & 2 != 0 { 7 - ty } else { ty };
                        let c = tile[(sy * 8 + sx) as usize];
                        let (fx, fy) = (x + tx - s.crop[0], y + ty - s.crop[1]);
                        if c != 0 && (0..w as i32).contains(&fx) && (0..h as i32).contains(&fy) {
                            let px = image::Rgba(pal[c as usize]);
                            img.put_pixel(i as u32 * w + fx as u32, fy as u32, px);
                        }
                    }
                }
            }
        }
        img
    }
}

/// Decompresses the graphics at file offset `offset`, as zelda3's
/// Decompress() does.
fn decompress(rom: &[u8], offset: usize) -> Result<Vec<u8>, String> {
    let err = || format!("The compressed graphics at {offset:#x} are damaged.");
    let mut src = rom.get(offset..).ok_or_else(err)?.iter().copied();
    let mut next = || src.next().ok_or_else(err);
    let mut out: Vec<u8> = Vec::new();
    loop {
        let mut cmd = next()?;
        if cmd == 0xff {
            return Ok(out);
        }
        let len;
        if cmd & 0xe0 != 0xe0 {
            len = (cmd & 0x1f) as usize + 1;
            cmd &= 0xe0;
        } else {
            len = next()? as usize + ((cmd as usize & 3) << 8) + 1;
            cmd = (cmd << 3) & 0xe0;
        }
        if cmd == 0 {
            for _ in 0..len {
                out.push(next()?);
            }
        } else if cmd & 0x80 != 0 {
            let from = next()? as usize | (next()? as usize) << 8;
            for k in from..from + len {
                out.push(*out.get(k).ok_or_else(err)?);
            }
        } else if cmd & 0x40 == 0 {
            let v = next()?;
            out.extend(std::iter::repeat_n(v, len));
        } else if cmd & 0x20 == 0 {
            let (lo, hi) = (next()?, next()?);
            out.extend((0..len).map(|k| if k % 2 == 0 { lo } else { hi }));
        } else {
            let v = next()?;
            out.extend((0..len).map(|k| v.wrapping_add(k as u8)));
        }
        if out.len() > 0x10000 {
            return Err(err());
        }
    }
}

/// The 64 color indices of a 4bpp tile in SNES format.
fn decode_4bpp(t: &[u8]) -> [u8; 64] {
    let mut px = [0; 64];
    for y in 0..8 {
        let planes = [t[y * 2], t[y * 2 + 1], t[16 + y * 2], t[17 + y * 2]];
        for x in 0..8 {
            px[y * 8 + x] = (0..4).map(|k| (planes[k] >> (7 - x) & 1) << k).sum();
        }
    }
    px
}

/// The 64 color indices of a 3bpp tile in SNES format. With `high`, every
/// opaque pixel gets color index + 8.
fn decode_3bpp(t: &[u8], high: bool) -> [u8; 64] {
    let mut px = [0; 64];
    for y in 0..8 {
        let planes = [t[y * 2], t[y * 2 + 1], t[16 + y]];
        for x in 0..8 {
            let c: u8 = (0..3).map(|k| (planes[k] >> (7 - x) & 1) << k).sum();
            px[y * 8 + x] = if high && c != 0 { c + 8 } else { c };
        }
    }
    px
}

/// An SNES color as RGBA.
fn bgr555(c: u16) -> [u8; 4] {
    let ch = |shift: u16| ((c >> shift & 31) as u32 * 255 / 31) as u8;
    [ch(0), ch(5), ch(10), 255]
}

fn decode_hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2).unwrap_or("00"), 16).unwrap_or(0))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub fn name(self) -> &'static str {
        match self {
            Dir::Up => "up",
            Dir::Down => "down",
            Dir::Left => "left",
            Dir::Right => "right",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        [Dir::Up, Dir::Down, Dir::Left, Dir::Right]
            .into_iter()
            .find(|d| d.name() == s)
    }
}

/// A playback sequence: frame indices with durations in game frames. A
/// duration of 0 means that the game holds the frame until an event.
#[derive(Clone, Debug, Deserialize)]
pub struct Sequence {
    pub frames: Vec<(usize, u32)>,
    #[serde(rename = "loop", default)]
    pub looping: bool,
    /// First entry of the loop part, for a looping sequence with an intro.
    #[serde(default)]
    pub loop_from: usize,
}

/// One animation in one direction, or one extra strip.
pub struct Clip {
    /// The whole strip: the frames side by side.
    pub texture: TextureHandle,
    /// Number of frames.
    pub frames: usize,
    /// Size of each frame, in sprite pixels.
    pub size: Vec2,
    /// For an animation: position of Link's coordinate point inside a frame.
    /// For a shadow: the same point with Link on the ground. For an extra: the
    /// top-left corner of the object. In sprite pixels.
    pub origin: Vec2,
    pub sequences: HashMap<String, Sequence>,
    /// For each frame, the row below its lowest opaque pixel, in sprite
    /// pixels from the top of the frame.
    pub bottoms: Vec<f32>,
}

pub struct Sprites {
    /// The gear tier of these sprites.
    pub tier: usize,
    clips: HashMap<(String, Dir), Clip>,
    shadows: HashMap<Dir, Clip>,
    extras: HashMap<String, Clip>,
    /// Distance from Link's coordinate point down to his feet, in sprite pixels.
    pub feet: f32,
}

/// Draws the strips of one tier and makes their textures, once per strip.
struct Loader<'a> {
    ctx: &'a Context,
    tier: usize,
    poses: Vec<Vec<u8>>,
    textures: HashMap<String, (TextureHandle, Vec<f32>)>,
}

impl Loader<'_> {
    fn clip(&mut self, s: &StripJson, sequences: HashMap<String, Sequence>) -> Option<Clip> {
        if s.poses.is_empty() || s.frame_width == 0 || s.frame_height == 0 {
            return None;
        }
        if !self.textures.contains_key(&s.strip) {
            let img = graphics().strip(&self.poses, s);
            let bottoms = (0..s.poses.len() as u32)
                .map(|i| {
                    let frame = image::imageops::crop_imm(
                        &img,
                        i * s.frame_width,
                        0,
                        s.frame_width,
                        s.frame_height,
                    )
                    .to_image();
                    bottom(&frame).unwrap_or(s.frame_height) as f32
                })
                .collect();
            let color = ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize],
                img.as_raw(),
            );
            let name = format!("tier{}/{}", self.tier, s.strip);
            let texture = self.ctx.load_texture(name, color, TextureOptions::NEAREST);
            self.textures.insert(s.strip.clone(), (texture, bottoms));
        }
        let (texture, bottoms) = &self.textures[&s.strip];
        Some(Clip {
            texture: texture.clone(),
            bottoms: bottoms.clone(),
            frames: s.poses.len(),
            size: vec2(s.frame_width as f32, s.frame_height as f32),
            origin: vec2(s.origin[0], s.origin[1]),
            sequences,
        })
    }
}

/// The sequences of animation `anim` that apply to direction `dir`: a sequence
/// keyed by a direction name applies only to that direction.
fn sequences_for(
    all: &HashMap<String, HashMap<String, Sequence>>,
    anim: &str,
    dir: Dir,
) -> HashMap<String, Sequence> {
    all.get(anim)
        .map(|seqs| {
            seqs.iter()
                .filter(|(k, _)| Dir::parse(k).is_none_or(|d| d == dir))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default()
}

impl Sprites {
    /// Loads the sprites of gear tier `tier`, or of the last tier if `tier`
    /// is too large.
    pub fn load(ctx: &Context, tier: usize) -> Self {
        let tier = tier.min(tier_count() - 1);
        let mut recipe = recipe();
        let index = recipe.tiers.swap_remove(tier);
        let mut loader = Loader {
            ctx,
            tier,
            poses: recipe.poses.iter().map(|p| decode_hex(p)).collect(),
            textures: HashMap::new(),
        };
        let mut clips = HashMap::new();
        for (name, dirs) in &index.animations {
            for (dir, s) in dirs {
                if let Some(dir) = Dir::parse(dir)
                    && let Some(clip) = loader.clip(s, sequences_for(&recipe.sequences, name, dir))
                {
                    clips.insert((name.clone(), dir), clip);
                }
            }
        }
        let shadows = index
            .shadows
            .iter()
            .filter_map(|(dir, s)| Some((Dir::parse(dir)?, loader.clip(s, HashMap::new())?)))
            .collect();
        let extras = index
            .extras
            .iter()
            .filter_map(|(name, s)| Some((name.clone(), loader.clip(s, HashMap::new())?)))
            .collect();
        let feet = stand_image(tier)
            .and_then(|(img, origin_y)| Some(bottom(&img)? as f32 - origin_y))
            .unwrap_or(23.0);
        Self {
            tier,
            clips,
            shadows,
            extras,
            feet,
        }
    }

    /// The names of all animations, each with its directions.
    #[cfg(test)]
    pub fn clip_names(&self) -> HashMap<String, Vec<Dir>> {
        let mut names: HashMap<String, Vec<Dir>> = HashMap::new();
        for (name, dir) in self.clips.keys() {
            names.entry(name.clone()).or_default().push(*dir);
        }
        names
    }

    pub fn clip(&self, anim: &str, dir: Dir) -> Option<&Clip> {
        self.clips.get(&(anim.to_owned(), dir))
    }

    /// The ground shadow for direction `dir`.
    pub fn shadow(&self, dir: Dir) -> Option<&Clip> {
        self.shadows.get(&dir)
    }

    /// The extra strip `name`, for example `rock` or `zzz`.
    pub fn extra(&self, name: &str) -> Option<&Clip> {
        self.extras.get(name)
    }
}

/// The standing frame of Link facing down in tier `tier`, and the y of his
/// origin in it.
fn stand_image(tier: usize) -> Option<(RgbaImage, f32)> {
    let recipe = recipe();
    let s = recipe
        .tiers
        .get(tier)?
        .animations
        .get("walk")?
        .get("down")?;
    let poses: Vec<Vec<u8>> = s
        .poses
        .iter()
        .take(1)
        .map(|&p| decode_hex(recipe.poses.get(p).map_or("", String::as_str)))
        .collect();
    let first = StripJson {
        strip: s.strip.clone(),
        poses: vec![0],
        ..*s
    };
    Some((graphics().strip(&poses, &first), s.origin[1]))
}

/// The row below the lowest opaque pixel.
fn bottom(img: &RgbaImage) -> Option<u32> {
    (0..img.height())
        .rev()
        .find(|&y| (0..img.width()).any(|x| img.get_pixel(x, y)[3] != 0))
        .map(|y| y + 1)
}

/// The standing frame of tier 0 as a square icon of `size` × `size` pixels:
/// trimmed, scaled up by the largest integer factor that fits, without
/// smoothing, and centered on a transparent canvas. Returns the width, the
/// height and the RGBA pixels.
pub fn icon(size: u32) -> (u32, u32, Vec<u8>) {
    let mut canvas = RgbaImage::new(size, size);
    if let Some((frame, _)) = stand_image(0) {
        let (mut x0, mut y0, mut x1, mut y1) = (frame.width(), frame.height(), 0, 0);
        for (x, y, p) in frame.enumerate_pixels() {
            if p[3] != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
        if x0 < x1 {
            let frame = image::imageops::crop_imm(&frame, x0, y0, x1 - x0, y1 - y0).to_image();
            let scale = (size / frame.width().max(frame.height())).max(1);
            let big = image::imageops::resize(
                &frame,
                frame.width() * scale,
                frame.height() * scale,
                image::imageops::FilterType::Nearest,
            );
            image::imageops::overlay(
                &mut canvas,
                &big,
                (size as i64 - big.width() as i64) / 2,
                (size as i64 - big.height() as i64) / 2,
            );
        }
    }
    (size, size, canvas.into_raw())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_direction_has_a_shadow() {
        let sprites = Sprites::load(&Context::default(), 0);
        for dir in [Dir::Up, Dir::Down, Dir::Left, Dir::Right] {
            let s = sprites.shadow(dir).expect("shadow");
            assert!(s.size.x > 0.0 && s.size.y > 0.0);
        }
    }

    #[test]
    fn every_tier_has_the_extras() {
        for tier in 0..tier_count() {
            let sprites = Sprites::load(&Context::default(), tier);
            assert_eq!(sprites.tier, tier);
            for name in [
                "skull",
                "rock",
                "bush",
                "skull_debris",
                "rock_debris",
                "bush_debris",
                "zzz",
            ] {
                assert!(sprites.extra(name).is_some(), "tier {tier} lacks {name}");
            }
        }
    }

    /// The strips are identical to the strips that the extractor rendered
    /// with the drawing code of the game.
    #[test]
    fn strips_match_the_checksums_of_the_recipe() {
        let recipe = recipe();
        let poses: Vec<Vec<u8>> = recipe.poses.iter().map(|p| decode_hex(p)).collect();
        for (n, tier) in recipe.tiers.iter().enumerate() {
            let mut strips: Vec<&StripJson> = tier
                .animations
                .values()
                .flat_map(HashMap::values)
                .chain(tier.shadows.values())
                .chain(tier.extras.values())
                .collect();
            strips.sort_by(|a, b| a.strip.cmp(&b.strip));
            strips.dedup_by(|a, b| a.strip == b.strip);
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for s in strips {
                for &b in graphics().strip(&poses, s).as_raw() {
                    h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
            assert_eq!(format!("{h:016x}"), tier.check, "tier {n}");
        }
    }

    #[test]
    fn decompress_handles_every_command() {
        // A literal, a byte fill, a word fill, an increment, a copy, the end.
        let rom = [
            0x01, 7, 8, 0x22, 9, 0x42, 1, 2, 0x62, 5, 0x81, 0x01, 0x00, 0xff,
        ];
        let out = decompress(&rom, 0).unwrap();
        assert_eq!(out, [7, 8, 9, 9, 9, 1, 2, 1, 5, 6, 7, 8, 9]);
    }
}
