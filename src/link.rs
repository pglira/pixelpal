//! Animations of Link, as `tools/link-extract` writes them to `assets/link/`:
//! one directory per gear tier, each with one PNG strip per animation and
//! direction, the ground shadows, extra strips (objects, debris, Zzz), and
//! `animations.json` with the frame size, the origin and the playback
//! sequences of each strip.

use std::collections::HashMap;

use egui::{ColorImage, Context, TextureHandle, TextureOptions, Vec2, vec2};
use image::RgbaImage;
use serde::Deserialize;

/// The embedded files of one gear tier.
pub struct Tier {
    json: &'static [u8],
    strips: &'static [(&'static str, &'static [u8])],
}

mod embedded {
    use super::Tier;
    include!(concat!(env!("OUT_DIR"), "/link_assets.rs"));
}

/// Duration of one game frame, in seconds.
pub const GAME_FRAME: f32 = 1.0 / 60.0;

/// Number of embedded gear tiers. Tier 0 always exists.
pub fn tier_count() -> usize {
    embedded::TIERS.len()
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

#[derive(Deserialize)]
struct IndexJson {
    animations: HashMap<String, AnimJson>,
    #[serde(default)]
    shadows: HashMap<String, StripJson>,
    #[serde(default)]
    extras: HashMap<String, StripJson>,
}

#[derive(Deserialize)]
struct AnimJson {
    directions: HashMap<String, StripJson>,
}

#[derive(Deserialize)]
struct StripJson {
    #[serde(default = "one")]
    frames: usize,
    frame_width: u32,
    frame_height: u32,
    origin: [f32; 2],
    #[serde(default)]
    sequences: HashMap<String, Sequence>,
    file: Option<String>,
    same_as: Option<String>,
}

fn one() -> usize {
    1
}

impl StripJson {
    fn file(&self) -> Option<&String> {
        self.file.as_ref().or(self.same_as.as_ref())
    }
}

fn tier_data(tier: usize) -> &'static Tier {
    &embedded::TIERS[tier.min(embedded::TIERS.len() - 1)]
}

fn index(tier: usize) -> IndexJson {
    serde_json::from_slice(tier_data(tier).json).expect("the embedded animations.json is valid")
}

fn strip_image(tier: usize, file: &str) -> Option<RgbaImage> {
    let (_, bytes) = tier_data(tier).strips.iter().find(|(n, _)| *n == file)?;
    Some(image::load_from_memory(bytes).ok()?.to_rgba8())
}

/// Loads the textures of the strips of one tier, once per file.
struct Loader<'a> {
    ctx: &'a Context,
    tier: usize,
    textures: HashMap<String, TextureHandle>,
}

impl Loader<'_> {
    fn clip(&mut self, s: StripJson) -> Option<Clip> {
        let file = s.file()?.clone();
        if !self.textures.contains_key(&file) {
            let img = strip_image(self.tier, &file)?;
            let color = ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize],
                img.as_raw(),
            );
            let name = format!("tier{}/{file}", self.tier);
            let texture = self.ctx.load_texture(name, color, TextureOptions::NEAREST);
            self.textures.insert(file.clone(), texture);
        }
        Some(Clip {
            texture: self.textures[&file].clone(),
            frames: s.frames.max(1),
            size: vec2(s.frame_width as f32, s.frame_height as f32),
            origin: vec2(s.origin[0], s.origin[1]),
            sequences: s.sequences,
        })
    }
}

impl Sprites {
    /// Loads the sprites of gear tier `tier`, or of the last tier if `tier`
    /// is too large.
    pub fn load(ctx: &Context, tier: usize) -> Self {
        let tier = tier.min(tier_count() - 1);
        let index = index(tier);
        let mut loader = Loader {
            ctx,
            tier,
            textures: HashMap::new(),
        };
        let mut clips = HashMap::new();
        for (name, anim) in index.animations {
            for (dir, d) in anim.directions {
                if let Some(dir) = Dir::parse(&dir)
                    && let Some(clip) = loader.clip(d)
                {
                    clips.insert((name.clone(), dir), clip);
                }
            }
        }
        let shadows = index
            .shadows
            .into_iter()
            .filter_map(|(dir, s)| Some((Dir::parse(&dir)?, loader.clip(s)?)))
            .collect();
        let extras = index
            .extras
            .into_iter()
            .filter_map(|(name, s)| Some((name, loader.clip(s)?)))
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

    pub fn clip(&self, anim: &str, dir: Dir) -> Option<&Clip> {
        self.clips.get(&(anim.to_owned(), dir))
    }

    /// The ground shadow for direction `dir`. Animations extracted without
    /// shadows give `None`.
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
    let d = index(tier)
        .animations
        .remove("walk")?
        .directions
        .remove("down")?;
    let img = strip_image(tier, d.file()?)?;
    let frame = image::imageops::crop_imm(&img, 0, 0, d.frame_width, d.frame_height).to_image();
    Some((frame, d.origin[1]))
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
}
