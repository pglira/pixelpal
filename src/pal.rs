//! Behavior and drawing of the pal.

use std::f32::consts::TAU;

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2};

use crate::link::{Frame, Sprites};
use crate::settings::Config;

/// Size of the pal window in sheet pixels. The top part holds the speech bubble.
pub const CANVAS: Vec2 = vec2(28.0, 42.0);

/// Gravity after a drag, in points per second squared.
const GRAVITY: f32 = 2400.0;
/// Duration of the happy animation, in seconds.
const HAPPY_SECONDS: f64 = 1.6;
/// Smallest walk distance, in points.
const MIN_WALK: f32 = 24.0;

/// The usable area of the current monitor, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Clone, Copy, Debug)]
enum Motion {
    Stand { until: f64 },
    Walk { target: f32 },
    Fall { vy: f32 },
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mood {
    Calm,
    Thinking,
    Happy { until: f64 },
}

pub struct Pal {
    /// Top-left corner of the pal window on the screen, in points.
    pub pos: Pos2,
    motion: Motion,
    pub mood: Mood,
    facing_right: bool,
    /// Distance of the current walk, in points.
    walk_distance: f32,
}

impl Pal {
    pub fn new(pos: Pos2) -> Self {
        Self {
            pos,
            motion: Motion::Fall { vy: 0.0 },
            mood: Mood::Calm,
            facing_right: false,
            walk_distance: 0.0,
        }
    }

    pub fn window_size(scale: u32) -> Vec2 {
        CANVAS * scale as f32
    }

    pub fn is_moving(&self) -> bool {
        matches!(
            self.motion,
            Motion::Walk { .. } | Motion::Fall { .. } | Motion::Held
        )
    }

    pub fn hold(&mut self) {
        self.motion = Motion::Held;
    }

    pub fn release(&mut self) {
        self.motion = Motion::Fall { vy: 0.0 };
    }

    pub fn set_thinking(&mut self, thinking: bool) {
        if thinking {
            self.mood = Mood::Thinking;
        } else if self.mood == Mood::Thinking {
            self.mood = Mood::Calm;
        }
    }

    pub fn celebrate(&mut self, now: f64) {
        self.mood = Mood::Happy {
            until: now + HAPPY_SECONDS,
        };
    }

    /// Advances the behavior by `dt` seconds. `may_walk` is false while the pal must stay.
    pub fn update(&mut self, now: f64, dt: f32, area: Area, cfg: &Config, may_walk: bool) {
        let size = Self::window_size(cfg.scale);
        let ground = area.bottom - size.y - cfg.bottom_margin;
        let min_x = area.left;
        let max_x = (area.right - size.x).max(min_x);

        if let Mood::Happy { until } = self.mood
            && now >= until
        {
            self.mood = Mood::Calm;
        }

        match self.motion {
            Motion::Held => return,
            Motion::Fall { vy } => {
                let vy = vy + GRAVITY * dt;
                self.pos.y += vy * dt;
                if self.pos.y >= ground {
                    self.pos.y = ground;
                    self.motion = Motion::Stand {
                        until: now + pause(cfg),
                    };
                } else {
                    self.motion = Motion::Fall { vy };
                }
            }
            Motion::Stand { until } => {
                self.walk_distance = 0.0;
                if may_walk && now >= until {
                    let margin = size.x;
                    let lo = min_x + margin;
                    let hi = (max_x - margin).max(lo);
                    let target = lo + fastrand::f32() * (hi - lo);
                    if (target - self.pos.x).abs() >= MIN_WALK {
                        self.motion = Motion::Walk { target };
                    } else {
                        self.motion = Motion::Stand {
                            until: now + pause(cfg),
                        };
                    }
                }
            }
            Motion::Walk { target } => {
                if !may_walk {
                    self.motion = Motion::Stand {
                        until: now + pause(cfg),
                    };
                } else {
                    let step = cfg.walk_speed * dt;
                    let d = target - self.pos.x;
                    self.facing_right = d > 0.0;
                    self.walk_distance += step.min(d.abs());
                    if d.abs() <= step {
                        self.pos.x = target;
                        self.motion = Motion::Stand {
                            until: now + pause(cfg),
                        };
                    } else {
                        self.pos.x += step * d.signum();
                    }
                }
            }
        }

        // Keep the pal inside the area and on the ground.
        self.pos.x = self.pos.x.clamp(min_x, max_x);
        if !matches!(self.motion, Motion::Fall { .. }) {
            if self.pos.y < ground - 0.5 {
                self.motion = Motion::Fall { vy: 0.0 };
            } else {
                self.pos.y = ground;
            }
        }
    }

    /// The frame to draw. `stride` is the walked distance in sheet pixels for
    /// one frame of the walk cycle, so the walk cycle advances with the distance
    /// and not with the time.
    fn frame<'a>(&self, sprites: &'a Sprites, scale: f32, stride: f32) -> &'a Frame {
        match (self.motion, self.mood) {
            (Motion::Walk { .. }, _) => {
                let frames = if self.facing_right {
                    &sprites.walk_right
                } else {
                    &sprites.walk_left
                };
                let i = (self.walk_distance / (stride.max(0.1) * scale)) as usize % frames.len();
                &frames[i]
            }
            (Motion::Held, _) | (_, Mood::Happy { .. }) => &sprites.happy,
            _ => &sprites.idle,
        }
    }

    /// Draws the pal into `rect`, the full pal window.
    pub fn paint(&self, painter: &Painter, rect: Rect, sprites: &Sprites, cfg: &Config, now: f64) {
        let scale = cfg.scale as f32;
        let frame = self.frame(sprites, scale, cfg.walk_stride);
        let size = frame.size * scale;
        let mut hop = 0.0;
        if let Mood::Happy { until } = self.mood {
            let t = (HAPPY_SECONDS - (until - now)) as f32;
            if t < 1.0 {
                hop = (t * 2.0 * TAU / 2.0).sin().abs() * 4.0 * scale;
            }
        }
        let sprite = Rect::from_min_size(
            pos2(rect.center().x - size.x / 2.0, rect.bottom() - size.y - hop).round(),
            size,
        );
        painter.image(
            frame.texture.id(),
            sprite,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        match self.mood {
            Mood::Thinking => paint_bubble(painter, sprite, scale, now),
            Mood::Happy { .. } => paint_stars(painter, sprite, scale, now),
            Mood::Calm => {}
        }
    }
}

fn pause(cfg: &Config) -> f64 {
    let (lo, hi) = (
        cfg.pause_min.min(cfg.pause_max),
        cfg.pause_max.max(cfg.pause_min),
    );
    (lo + fastrand::f32() * (hi - lo)) as f64
}

/// A speech bubble with animated dots above the head of the sprite.
fn paint_bubble(painter: &Painter, sprite: Rect, s: f32, now: f64) {
    let outline = Color32::from_rgb(40, 40, 40);
    let bubble = Rect::from_center_size(
        pos2(sprite.center().x + 3.0 * s, sprite.top() - 7.0 * s),
        vec2(14.0 * s, 8.0 * s),
    );
    painter.rect(
        bubble,
        3.0 * s,
        Color32::WHITE,
        Stroke::new(s * 0.7, outline),
        egui::StrokeKind::Inside,
    );
    painter.circle(
        pos2(bubble.center().x - 3.0 * s, bubble.bottom() + 1.5 * s),
        1.2 * s,
        Color32::WHITE,
        Stroke::new(s * 0.5, outline),
    );
    let active = (now * 3.0) as usize % 4;
    for i in 0..3 {
        let color = if i < active {
            outline
        } else {
            Color32::from_gray(190)
        };
        painter.circle_filled(
            pos2(
                bubble.center().x + (i as f32 - 1.0) * 3.5 * s,
                bubble.center().y,
            ),
            1.0 * s,
            color,
        );
    }
}

/// Small twinkling stars around the head of the sprite.
fn paint_stars(painter: &Painter, sprite: Rect, s: f32, now: f64) {
    let spots = [(-9.0, 2.0), (9.0, 0.0), (0.0, -6.0)];
    for (i, (dx, dy)) in spots.iter().enumerate() {
        let phase = (now as f32 * 6.0 + i as f32 * 2.1).sin() * 0.5 + 0.5;
        let r = (1.5 + 1.5 * phase) * s;
        let c = pos2(sprite.center().x + dx * s, sprite.top() + dy * s);
        let color = Color32::from_rgb(255, 220, 60);
        let stroke = Stroke::new(0.8 * s, color);
        painter.line_segment([c - vec2(r, 0.0), c + vec2(r, 0.0)], stroke);
        painter.line_segment([c - vec2(0.0, r), c + vec2(0.0, r)], stroke);
        painter.circle_filled(c, 0.8 * s, Color32::WHITE);
    }
}
