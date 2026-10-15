//! Frames of Link from the sprite sheet
//! "SNES - The Legend of Zelda: A Link to the Past - Playable Characters - Link".

use egui::{ColorImage, Context, TextureHandle, TextureOptions, Vec2};
use image::RgbaImage;

/// The sheet, embedded at build time. `build.rs` checks that the file exists.
const SHEET: &[u8] = include_bytes!("../assets/link_sheet.png");

/// Background color of the sheet.
const KEY_BACKGROUND: [u8; 3] = [0x00, 0x80, 0x80];
/// Color of the box around each frame.
const KEY_FRAME_BOX: [u8; 3] = [0x00, 0x40, 0x40];

/// A rectangle on the sheet: x, y, width, height in pixels.
type SheetRect = (u32, u32, u32, u32);

/// Link facing down, standing.
const IDLE: SheetRect = (1, 3, 16, 24);

/// Walk cycle facing right.
const WALK_RIGHT: [SheetRect; 8] = [
    (1, 58, 17, 24),
    (20, 57, 17, 25),
    (38, 57, 16, 25),
    (55, 58, 17, 24),
    (73, 58, 17, 24),
    (91, 57, 17, 25),
    (109, 57, 16, 25),
    (126, 58, 16, 24),
];

/// "Item get" pose: facing down with both arms up.
const HAPPY: SheetRect = (394, 167, 16, 24);

/// One frame as a texture. `size` is the size in sheet pixels.
pub struct Frame {
    pub texture: TextureHandle,
    pub size: Vec2,
}

/// All frames that the pal uses.
pub struct Sprites {
    pub idle: Frame,
    pub walk_left: Vec<Frame>,
    pub walk_right: Vec<Frame>,
    pub happy: Frame,
}

impl Sprites {
    pub fn load(ctx: &Context) -> Self {
        let sheet = image::load_from_memory(SHEET)
            .expect("the embedded sprite sheet is a valid PNG")
            .to_rgba8();
        let frame = |name: &str, rect: SheetRect, mirror: bool| {
            let img = cut(&sheet, rect, mirror);
            let size = Vec2::new(img.width() as f32, img.height() as f32);
            let color = ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize],
                img.as_raw(),
            );
            Frame {
                texture: ctx.load_texture(name, color, TextureOptions::NEAREST),
                size,
            }
        };
        Self {
            idle: frame("idle", IDLE, false),
            walk_left: WALK_RIGHT
                .iter()
                .enumerate()
                .map(|(i, r)| frame(&format!("walk_left_{i}"), *r, true))
                .collect(),
            walk_right: WALK_RIGHT
                .iter()
                .enumerate()
                .map(|(i, r)| frame(&format!("walk_right_{i}"), *r, false))
                .collect(),
            happy: frame("happy", HAPPY, false),
        }
    }
}

/// The idle frame as a square icon of `size` × `size` pixels: scaled up by the
/// largest integer factor that fits, without smoothing, and centered on a
/// transparent canvas. Returns the width, the height and the RGBA pixels.
pub fn icon(size: u32) -> (u32, u32, Vec<u8>) {
    let sheet = image::load_from_memory(SHEET)
        .expect("the embedded sprite sheet is a valid PNG")
        .to_rgba8();
    let frame = cut(&sheet, IDLE, false);
    let scale = (size / frame.width().max(frame.height())).max(1);
    let big = image::imageops::resize(
        &frame,
        frame.width() * scale,
        frame.height() * scale,
        image::imageops::FilterType::Nearest,
    );
    let mut canvas = RgbaImage::new(size, size);
    image::imageops::overlay(
        &mut canvas,
        &big,
        (size as i64 - big.width() as i64) / 2,
        (size as i64 - big.height() as i64) / 2,
    );
    (size, size, canvas.into_raw())
}

/// Cuts one frame from the sheet, makes the key colors transparent, trims the
/// transparent border, and optionally mirrors the frame horizontally.
fn cut(sheet: &RgbaImage, (x, y, w, h): SheetRect, mirror: bool) -> RgbaImage {
    let mut img = image::imageops::crop_imm(sheet, x, y, w, h).to_image();
    for p in img.pixels_mut() {
        let rgb = [p[0], p[1], p[2]];
        if rgb == KEY_BACKGROUND || rgb == KEY_FRAME_BOX {
            p[3] = 0;
        }
    }
    let opaque = |px: u32, py: u32| img.get_pixel(px, py)[3] != 0;
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for py in 0..h {
        for px in 0..w {
            if opaque(px, py) {
                x0 = x0.min(px);
                y0 = y0.min(py);
                x1 = x1.max(px);
                y1 = y1.max(py);
            }
        }
    }
    let mut img = if x0 <= x1 {
        image::imageops::crop_imm(&img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image()
    } else {
        img
    };
    if mirror {
        image::imageops::flip_horizontal_in_place(&mut img);
    }
    img
}
