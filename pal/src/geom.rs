//! 2D points, vectors, rectangles and colors of the pal and its windows.
//!
//! All coordinates are in physical pixels of the screen, as `f32`.

use std::ops::{Add, AddAssign, Div, Mul, Sub, SubAssign};

/// A position.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pos2 {
    pub x: f32,
    pub y: f32,
}

/// A distance or a size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

pub const fn pos2(x: f32, y: f32) -> Pos2 {
    Pos2 { x, y }
}

pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

impl Pos2 {
    pub const ZERO: Pos2 = pos2(0.0, 0.0);

    pub fn round(self) -> Self {
        pos2(self.x.round(), self.y.round())
    }
}

impl Vec2 {
    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }
}

impl Add<Vec2> for Pos2 {
    type Output = Pos2;
    fn add(self, v: Vec2) -> Pos2 {
        pos2(self.x + v.x, self.y + v.y)
    }
}

impl AddAssign<Vec2> for Pos2 {
    fn add_assign(&mut self, v: Vec2) {
        *self = *self + v;
    }
}

impl Sub<Vec2> for Pos2 {
    type Output = Pos2;
    fn sub(self, v: Vec2) -> Pos2 {
        pos2(self.x - v.x, self.y - v.y)
    }
}

impl SubAssign<Vec2> for Pos2 {
    fn sub_assign(&mut self, v: Vec2) {
        *self = *self - v;
    }
}

impl Sub for Pos2 {
    type Output = Vec2;
    fn sub(self, o: Pos2) -> Vec2 {
        vec2(self.x - o.x, self.y - o.y)
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        vec2(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        vec2(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, f: f32) -> Vec2 {
        vec2(self.x * f, self.y * f)
    }
}

impl Div<f32> for Vec2 {
    type Output = Vec2;
    fn div(self, f: f32) -> Vec2 {
        vec2(self.x / f, self.y / f)
    }
}

/// An axis-aligned rectangle from `min` (top left) to `max` (bottom right).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub min: Pos2,
    pub max: Pos2,
}

impl Rect {
    pub fn from_min_max(min: Pos2, max: Pos2) -> Self {
        Self { min, max }
    }

    pub fn from_min_size(min: Pos2, size: Vec2) -> Self {
        Self {
            min,
            max: min + size,
        }
    }

    pub fn from_center_size(center: Pos2, size: Vec2) -> Self {
        Self::from_min_size(center - size / 2.0, size)
    }

    pub fn left(&self) -> f32 {
        self.min.x
    }

    pub fn right(&self) -> f32 {
        self.max.x
    }

    pub fn top(&self) -> f32 {
        self.min.y
    }

    pub fn bottom(&self) -> f32 {
        self.max.y
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn center(&self) -> Pos2 {
        pos2(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
        )
    }

    pub fn translate(self, v: Vec2) -> Self {
        Self::from_min_max(self.min + v, self.max + v)
    }

    /// The rectangle with a margin of `d` on each side.
    pub fn expand(self, d: f32) -> Self {
        Self::from_min_max(self.min - vec2(d, d), self.max + vec2(d, d))
    }
}

/// An opaque or translucent color with straight alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color32 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color32 {
    pub const WHITE: Color32 = Color32::from_rgb(255, 255, 255);

    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn from_gray(v: u8) -> Self {
        Self::from_rgb(v, v, v)
    }
}

/// The product of two 8-bit channels, as a fraction of 255.
fn mul_channel(a: u8, b: u8) -> u8 {
    ((u16::from(a) * u16::from(b) + 127) / 255) as u8
}

/// The component-wise product: a tint of a tint.
impl Mul for Color32 {
    type Output = Color32;
    fn mul(self, o: Color32) -> Color32 {
        Color32 {
            r: mul_channel(self.r, o.r),
            g: mul_channel(self.g, o.g),
            b: mul_channel(self.b, o.b),
            a: mul_channel(self.a, o.a),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_math() {
        let r = Rect::from_center_size(pos2(10.0, 10.0), vec2(4.0, 2.0));
        assert_eq!(r.min, pos2(8.0, 9.0));
        assert_eq!((r.width(), r.height()), (4.0, 2.0));
        assert_eq!(r.expand(1.0).width(), 6.0);
        assert_eq!(
            Color32::WHITE * Color32::from_gray(190),
            Color32::from_gray(190)
        );
    }
}
