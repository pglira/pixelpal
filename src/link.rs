//! Animations of Link, as `tools/link-extract` writes them to `assets/link/`:
//! one PNG strip per animation and direction, and `animations.json` with the
//! frame size, the origin and the playback sequences of each strip.

use std::collections::HashMap;

use egui::{ColorImage, Context, TextureHandle, TextureOptions, Vec2, vec2};
use image::RgbaImage;
use serde::Deserialize;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/link_assets.rs"));
}

/// Duration of one game frame, in seconds.
pub const GAME_FRAME: f32 = 1.0 / 60.0;

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

/// One animation in one direction.
pub struct Clip {
    pub frames: Vec<TextureHandle>,
    /// Size of each frame, in sprite pixels.
    pub size: Vec2,
    /// Position of Link's coordinate point inside a frame, in sprite pixels.
    pub origin: Vec2,
    pub sequences: HashMap<String, Sequence>,
}

/// The ground shadow of Link in one direction.
pub struct Shadow {
    pub texture: TextureHandle,
    /// Size in sprite pixels.
    pub size: Vec2,
    /// Position of Link's coordinate point, on the ground, inside the shadow.
    pub origin: Vec2,
}

pub struct Sprites {
    clips: HashMap<(String, Dir), Clip>,
    shadows: HashMap<Dir, Shadow>,
    /// Distance from Link's coordinate point down to his feet, in sprite pixels.
    pub feet: f32,
}

#[derive(Deserialize)]
struct IndexJson {
    animations: HashMap<String, AnimJson>,
    #[serde(default)]
    shadows: HashMap<String, ShadowJson>,
}

#[derive(Deserialize)]
struct ShadowJson {
    file: String,
    frame_width: u32,
    frame_height: u32,
    origin: [f32; 2],
}

#[derive(Deserialize)]
struct AnimJson {
    directions: HashMap<String, DirJson>,
}

#[derive(Deserialize)]
struct DirJson {
    frames: usize,
    frame_width: u32,
    frame_height: u32,
    origin: [f32; 2],
    sequences: HashMap<String, Sequence>,
    file: Option<String>,
    same_as: Option<String>,
}

fn index() -> IndexJson {
    serde_json::from_slice(embedded::ANIMATIONS_JSON)
        .expect("the embedded animations.json is valid")
}

fn strip_image(file: &str) -> Option<RgbaImage> {
    let (_, bytes) = embedded::STRIPS.iter().find(|(n, _)| *n == file)?;
    Some(image::load_from_memory(bytes).ok()?.to_rgba8())
}

impl Sprites {
    pub fn load(ctx: &Context) -> Self {
        let mut clips = HashMap::new();
        let mut textures: HashMap<String, Vec<TextureHandle>> = HashMap::new();
        let index = index();
        for (name, anim) in index.animations {
            for (dir, d) in anim.directions {
                let Some(dir) = Dir::parse(&dir) else {
                    continue;
                };
                let Some(file) = d.file.as_ref().or(d.same_as.as_ref()) else {
                    continue;
                };
                if !textures.contains_key(file) {
                    let Some(img) = strip_image(file) else {
                        continue;
                    };
                    let frames = (0..d.frames as u32)
                        .map(|i| {
                            let f = image::imageops::crop_imm(
                                &img,
                                i * d.frame_width,
                                0,
                                d.frame_width,
                                d.frame_height,
                            )
                            .to_image();
                            let color = ColorImage::from_rgba_unmultiplied(
                                [f.width() as usize, f.height() as usize],
                                f.as_raw(),
                            );
                            ctx.load_texture(format!("{file}#{i}"), color, TextureOptions::NEAREST)
                        })
                        .collect();
                    textures.insert(file.clone(), frames);
                }
                clips.insert(
                    (name.clone(), dir),
                    Clip {
                        frames: textures[file].clone(),
                        size: vec2(d.frame_width as f32, d.frame_height as f32),
                        origin: vec2(d.origin[0], d.origin[1]),
                        sequences: d.sequences,
                    },
                );
            }
        }
        let feet = stand_image()
            .and_then(|(img, origin_y)| Some(bottom(&img)? as f32 - origin_y))
            .unwrap_or(23.0);
        let shadows = index
            .shadows
            .into_iter()
            .filter_map(|(dir, s)| {
                let img = strip_image(&s.file)?;
                let color = ColorImage::from_rgba_unmultiplied(
                    [img.width() as usize, img.height() as usize],
                    img.as_raw(),
                );
                Some((
                    Dir::parse(&dir)?,
                    Shadow {
                        texture: ctx.load_texture(&s.file, color, TextureOptions::NEAREST),
                        size: vec2(s.frame_width as f32, s.frame_height as f32),
                        origin: vec2(s.origin[0], s.origin[1]),
                    },
                ))
            })
            .collect();
        Self {
            clips,
            shadows,
            feet,
        }
    }

    pub fn clip(&self, anim: &str, dir: Dir) -> Option<&Clip> {
        self.clips.get(&(anim.to_owned(), dir))
    }

    /// The ground shadow for direction `dir`. Animations extracted without
    /// shadows give `None`.
    pub fn shadow(&self, dir: Dir) -> Option<&Shadow> {
        self.shadows.get(&dir)
    }
}

/// The standing frame of Link facing down, and the y of his origin in it.
fn stand_image() -> Option<(RgbaImage, f32)> {
    let d = index()
        .animations
        .remove("walk")?
        .directions
        .remove("down")?;
    let img = strip_image(d.file.as_ref().or(d.same_as.as_ref())?)?;
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

/// The standing frame as a square icon of `size` × `size` pixels: trimmed,
/// scaled up by the largest integer factor that fits, without smoothing, and
/// centered on a transparent canvas. Returns the width, the height and the
/// RGBA pixels.
pub fn icon(size: u32) -> (u32, u32, Vec<u8>) {
    let mut canvas = RgbaImage::new(size, size);
    if let Some((frame, _)) = stand_image() {
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
        let sprites = Sprites::load(&Context::default());
        for dir in [Dir::Up, Dir::Down, Dir::Left, Dir::Right] {
            let s = sprites.shadow(dir).expect("shadow");
            assert!(s.size.x > 0.0 && s.size.y > 0.0);
        }
    }
}
