//! A CPU canvas for the windows of the pal: sprite frames with nearest-pixel
//! scaling, and smooth shapes from `tiny-skia`.
//!
//! The pixels are RGBA with premultiplied alpha.

use std::cell::RefCell;

use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

use crate::geom::{Color32, Pos2, Rect};

/// An image of sprites, with premultiplied RGBA pixels.
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[u8; 4]>,
}

impl Texture {
    /// A texture from RGBA pixels with straight alpha.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Self {
        let pixels = rgba
            .chunks_exact(4)
            .map(|p| {
                let a = u16::from(p[3]);
                let pm = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
                [pm(p[0]), pm(p[1]), pm(p[2]), p[3]]
            })
            .collect();
        Self {
            width,
            height,
            pixels,
        }
    }
}

/// A line width and a color.
#[derive(Clone, Copy, Debug)]
pub struct Stroke {
    pub width: f32,
    pub color: Color32,
}

impl Stroke {
    pub fn new(width: f32, color: Color32) -> Self {
        Self { width, color }
    }
}

/// The pixels of one window. The drawing functions take `&self`, so that
/// drawing closures can share the canvas.
pub struct Canvas {
    pixmap: RefCell<Pixmap>,
}

impl Canvas {
    /// A transparent canvas. A size of zero gives a canvas of one pixel.
    pub fn new(width: u32, height: u32) -> Self {
        let pixmap = Pixmap::new(width.max(1), height.max(1)).expect("canvas size is not zero");
        Self {
            pixmap: RefCell::new(pixmap),
        }
    }

    pub fn width(&self) -> u32 {
        self.pixmap.borrow().width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.borrow().height()
    }

    /// The full canvas as a rectangle with its top-left corner at zero.
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(
            Pos2::ZERO,
            crate::geom::vec2(self.width() as f32, self.height() as f32),
        )
    }

    /// Makes all pixels transparent.
    pub fn clear(&self) {
        self.pixmap.borrow_mut().fill(tiny_skia::Color::TRANSPARENT);
    }

    /// The premultiplied RGBA pixels, row by row.
    pub fn with_pixels<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R {
        f(self.pixmap.borrow().data())
    }

    /// Draws the part `uv` of `texture` into `rect`, with each pixel
    /// multiplied by `tint`. `uv` is in fractions of the texture size. Each
    /// pixel of the canvas takes the nearest texture pixel, so that pixel art
    /// stays sharp.
    pub fn image(&self, texture: &Texture, rect: Rect, uv: Rect, tint: Color32) {
        let mut pixmap = self.pixmap.borrow_mut();
        let (cw, ch) = (pixmap.width() as i32, pixmap.height() as i32);
        let (tw, th) = (texture.width as f32, texture.height as f32);
        if rect.width() <= 0.0 || rect.height() <= 0.0 || tw == 0.0 || th == 0.0 {
            return;
        }
        let x0 = (rect.left().round() as i32).max(0);
        let x1 = (rect.right().round() as i32).min(cw);
        let y0 = (rect.top().round() as i32).max(0);
        let y1 = (rect.bottom().round() as i32).min(ch);
        let tint = [tint.r, tint.g, tint.b, tint.a].map(u16::from);
        let data = pixmap.data_mut();
        for y in y0..y1 {
            let v = uv.top() + (y as f32 + 0.5 - rect.top()) / rect.height() * uv.height();
            let sy = ((v * th) as i32).clamp(0, th as i32 - 1) as u32;
            for x in x0..x1 {
                let u = uv.left() + (x as f32 + 0.5 - rect.left()) / rect.width() * uv.width();
                let sx = ((u * tw) as i32).clamp(0, tw as i32 - 1) as u32;
                let src = texture.pixels[(sy * texture.width + sx) as usize];
                if src[3] == 0 {
                    continue;
                }
                // The tint scales the color, and its alpha scales the whole
                // premultiplied pixel.
                let s = [0, 1, 2, 3].map(|i| {
                    let c = (u16::from(src[i]) * tint[i] + 127) / 255;
                    if i < 3 { (c * tint[3] + 127) / 255 } else { c }
                });
                let i = ((y * cw + x) * 4) as usize;
                let inv = 255 - s[3];
                for c in 0..4 {
                    let d = u16::from(data[i + c]);
                    data[i + c] = (s[c] + (d * inv + 127) / 255).min(255) as u8;
                }
            }
        }
    }

    fn fill_path(&self, path: &tiny_skia::Path, color: Color32) {
        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = true;
        self.pixmap.borrow_mut().fill_path(
            path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    fn stroke_path(&self, path: &tiny_skia::Path, stroke: Stroke) {
        if stroke.width <= 0.0 {
            return;
        }
        let mut paint = Paint::default();
        let c = stroke.color;
        paint.set_color_rgba8(c.r, c.g, c.b, c.a);
        paint.anti_alias = true;
        let s = tiny_skia::Stroke {
            width: stroke.width,
            ..Default::default()
        };
        self.pixmap
            .borrow_mut()
            .stroke_path(path, &paint, &s, Transform::identity(), None);
    }

    /// A rectangle with rounded corners of the radius `radius`, filled with
    /// `fill`, and with the line `stroke` inside its edge.
    pub fn rounded_rect(&self, rect: Rect, radius: f32, fill: Color32, stroke: Stroke) {
        if let Some(path) = rounded_rect_path(rect, radius) {
            self.fill_path(&path, fill);
        }
        let inner = rect.expand(-stroke.width / 2.0);
        if let Some(path) = rounded_rect_path(inner, (radius - stroke.width / 2.0).max(0.0)) {
            self.stroke_path(&path, stroke);
        }
    }

    pub fn circle(&self, center: Pos2, radius: f32, fill: Color32, stroke: Stroke) {
        if let Some(path) = PathBuilder::from_circle(center.x, center.y, radius) {
            self.fill_path(&path, fill);
            self.stroke_path(&path, stroke);
        }
    }

    pub fn circle_filled(&self, center: Pos2, radius: f32, fill: Color32) {
        if let Some(path) = PathBuilder::from_circle(center.x, center.y, radius) {
            self.fill_path(&path, fill);
        }
    }

    pub fn line_segment(&self, points: [Pos2; 2], stroke: Stroke) {
        let mut pb = PathBuilder::new();
        pb.move_to(points[0].x, points[0].y);
        pb.line_to(points[1].x, points[1].y);
        if let Some(path) = pb.finish() {
            self.stroke_path(&path, stroke);
        }
    }
}

/// The outline of a rectangle with rounded corners. The radius is at most
/// half of the shorter side.
fn rounded_rect_path(rect: Rect, radius: f32) -> Option<tiny_skia::Path> {
    let r = radius
        .min(rect.width() / 2.0)
        .min(rect.height() / 2.0)
        .max(0.0);
    let (l, t, rt, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    // The control points of a cubic curve that approximates a quarter circle.
    let k = r * 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(l + r, t);
    pb.line_to(rt - r, t);
    pb.cubic_to(rt - r + k, t, rt, t + r - k, rt, t + r);
    pb.line_to(rt, b - r);
    pb.cubic_to(rt, b - r + k, rt - r + k, b, rt - r, b);
    pb.line_to(l + r, b);
    pb.cubic_to(l + r - k, b, l, b - r + k, l, b - r);
    pb.line_to(l, t + r);
    pb.cubic_to(l, t + r - k, l + r - k, t, l + r, t);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{pos2, vec2};

    fn pixel(c: &Canvas, x: u32, y: u32) -> [u8; 4] {
        c.with_pixels(|p| {
            let i = ((y * c.width() + x) * 4) as usize;
            [p[i], p[i + 1], p[i + 2], p[i + 3]]
        })
    }

    #[test]
    fn image_scales_without_smoothing_and_tints() {
        // Two pixels: opaque red and transparent.
        let tex = Texture::from_rgba(2, 1, &[255, 0, 0, 255, 0, 0, 0, 0]);
        let c = Canvas::new(8, 4);
        let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        c.image(
            &tex,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(8.0, 4.0)),
            full,
            Color32::WHITE,
        );
        assert_eq!(pixel(&c, 3, 2), [255, 0, 0, 255]);
        assert_eq!(pixel(&c, 4, 2), [0, 0, 0, 0]);
        c.clear();
        c.image(
            &tex,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(8.0, 4.0)),
            full,
            Color32::from_gray(128),
        );
        assert_eq!(pixel(&c, 0, 0), [128, 0, 0, 255]);
    }

    #[test]
    fn shapes_draw_inside_their_bounds() {
        let c = Canvas::new(20, 20);
        c.rounded_rect(
            Rect::from_min_size(pos2(2.0, 2.0), vec2(16.0, 10.0)),
            3.0,
            Color32::WHITE,
            Stroke::new(1.0, Color32::from_gray(40)),
        );
        assert_eq!(pixel(&c, 10, 7), [255, 255, 255, 255]);
        assert_eq!(pixel(&c, 10, 2)[3], 255);
        assert_eq!(pixel(&c, 10, 15), [0, 0, 0, 0]);
    }
}
