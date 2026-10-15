//! Behavior and drawing of the pal.

use std::f32::consts::TAU;

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2};

use crate::actions::{self, ACTIONS, Shot, Then};
use crate::link::{Dir, Sprites};
use crate::settings::Config;

/// Size of the pal window in sprite pixels. Link stands at the bottom center.
/// The space around him holds the swords and items of the actions, and the
/// speech bubble.
pub const CANVAS: Vec2 = vec2(52.0, 46.0);
/// Width of Link's body box, in sprite pixels. His origin is its left edge.
const BODY_WIDTH: f32 = 16.0;

/// Sprite pixels per meter. Link is about 24 sprite pixels (1.5 m) tall.
const PIXELS_PER_METER: f32 = 16.0;
/// Duration of the happy animation, in seconds.
const HAPPY_SECONDS: f64 = 1.6;
/// Smallest walk distance, in points.
const MIN_WALK: f32 = 24.0;

/// A way to walk: a factor of the configured walk speed, and the weight of
/// the gait in a random choice.
struct Gait {
    speed: f32,
    weight: u32,
    /// Seconds of running in place before the walk, as with the Pegasus Boots.
    run_up: f64,
}

static GAITS: [Gait; 4] = [
    // Stroll.
    Gait {
        speed: 0.5,
        weight: 25,
        run_up: 0.0,
    },
    // Walk.
    Gait {
        speed: 1.0,
        weight: 40,
        run_up: 0.0,
    },
    // Hurry.
    Gait {
        speed: 1.7,
        weight: 25,
        run_up: 0.0,
    },
    // Dash with the Pegasus Boots.
    Gait {
        speed: 3.5,
        weight: 10,
        run_up: 0.5,
    },
];

/// A random gait from `GAITS`, by weight.
fn random_gait() -> &'static Gait {
    let total: u32 = GAITS.iter().map(|g| g.weight).sum();
    let mut r = fastrand::u32(..total);
    for g in &GAITS {
        if r < g.weight {
            return g;
        }
        r -= g.weight;
    }
    &GAITS[1]
}

/// The usable area of the current monitor, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug)]
enum Motion {
    Stand {
        until: f64,
    },
    Walk {
        target: f32,
        /// Factor of the configured walk speed.
        speed: f32,
        /// Until this time, the pal runs in place.
        run_up_until: f64,
    },
    Fall {
        vy: f32,
    },
    Held,
    /// An action from `actions::ACTIONS`.
    Act {
        shots: Vec<Shot>,
        start: f64,
        then: Then,
    },
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
    facing: Dir,
    /// Distance of the current walk, in points.
    walk_distance: f32,
}

impl Pal {
    pub fn new(pos: Pos2) -> Self {
        Self {
            pos,
            motion: Motion::Fall { vy: 0.0 },
            mood: Mood::Calm,
            facing: Dir::Down,
            walk_distance: 0.0,
        }
    }

    pub fn window_size(scale: u32) -> Vec2 {
        CANVAS * scale as f32
    }

    pub fn is_moving(&self) -> bool {
        !matches!(self.motion, Motion::Stand { .. })
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
        if matches!(self.motion, Motion::Act { .. }) {
            self.motion = Motion::Stand {
                until: now + HAPPY_SECONDS,
            };
        }
        self.mood = Mood::Happy {
            until: now + HAPPY_SECONDS,
        };
    }

    /// Advances the behavior by `dt` seconds. `may_move` is false while the pal
    /// must stay calm: then no walk and no action starts.
    pub fn update(
        &mut self,
        now: f64,
        dt: f32,
        area: Area,
        cfg: &Config,
        sprites: &Sprites,
        may_move: bool,
    ) {
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
                let gravity = cfg.gravity * PIXELS_PER_METER * cfg.scale as f32;
                let vy = vy + gravity * dt;
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
                if may_move && now >= until {
                    let calm = self.mood == Mood::Calm;
                    if calm && cfg.actions && fastrand::f32() < cfg.action_chance {
                        self.start_action(now, sprites);
                    } else if cfg.walk {
                        self.start_walk(now, cfg, min_x, max_x, size.x);
                    } else {
                        self.motion = Motion::Stand {
                            until: now + pause(cfg),
                        };
                    }
                }
            }
            Motion::Act {
                ref shots,
                start,
                then,
            } => {
                let total: f32 = shots.iter().map(|s| s.secs).sum();
                if now - start >= total as f64 {
                    if then == Then::DropFromAbove {
                        self.pos.x = min_x + fastrand::f32() * (max_x - min_x);
                        self.pos.y = area.top - size.y;
                        self.facing = Dir::Down;
                        self.motion = Motion::Fall { vy: 0.0 };
                    } else {
                        self.motion = Motion::Stand {
                            until: now + pause(cfg),
                        };
                    }
                }
            }
            Motion::Walk {
                target,
                speed,
                run_up_until,
            } => {
                let d = target - self.pos.x;
                if !may_move || !cfg.walk {
                    self.motion = Motion::Stand {
                        until: now + pause(cfg),
                    };
                } else if now < run_up_until {
                    // Run in place: the legs move, the pal stays.
                    self.facing = if d > 0.0 { Dir::Right } else { Dir::Left };
                    self.walk_distance += cfg.walk_speed * speed * dt;
                } else {
                    let step = cfg.walk_speed * speed * dt;
                    self.facing = if d > 0.0 { Dir::Right } else { Dir::Left };
                    self.walk_distance += step.min(d.abs());
                    if d.abs() <= step {
                        self.pos.x = target;
                        if fastrand::bool() {
                            self.facing = Dir::Down;
                        }
                        self.motion = Motion::Stand {
                            until: now + pause(cfg),
                        };
                    } else {
                        self.pos.x += step * d.signum();
                    }
                }
            }
        }

        // Keep the pal inside the area and on the ground. A drop from above
        // starts above the area and falls down to the ground.
        self.pos.x = self.pos.x.clamp(min_x, max_x);
        if !matches!(self.motion, Motion::Fall { .. }) {
            if self.pos.y < ground - 0.5 {
                self.motion = Motion::Fall { vy: 0.0 };
            } else {
                self.pos.y = ground;
            }
        }
    }

    /// Starts a random action, or the action that `$PIXELPAL_ACTION` names. If
    /// the sprites lack a part of the action, the pal stands for one more second.
    fn start_action(&mut self, now: f64, sprites: &Sprites) {
        let forced = std::env::var("PIXELPAL_ACTION").ok();
        let action = forced
            .and_then(|name| ACTIONS.iter().find(|a| a.name == name))
            .unwrap_or_else(|| &ACTIONS[fastrand::usize(..ACTIONS.len())]);
        self.play(action, now, sprites);
    }

    /// Reacts to a poke with a random action from `actions::REACTIONS`, at
    /// once and also while the chat panel is open. A held or falling pal does
    /// not react.
    pub fn poke(&mut self, now: f64, sprites: &Sprites) {
        if matches!(self.motion, Motion::Held | Motion::Fall { .. }) {
            return;
        }
        let name = actions::REACTIONS[fastrand::usize(..actions::REACTIONS.len())];
        if let Some(action) = ACTIONS.iter().find(|a| a.name == name) {
            if matches!(self.mood, Mood::Happy { .. }) {
                self.mood = Mood::Calm;
            }
            self.play(action, now, sprites);
        }
    }

    fn play(&mut self, action: &actions::Action, now: f64, sprites: &Sprites) {
        let dir = actions::pick_dir(action);
        self.motion = match actions::scene(action, dir, sprites) {
            Some(shots) if !shots.is_empty() => {
                self.facing = match dir {
                    Dir::Left | Dir::Right => dir,
                    _ => Dir::Down,
                };
                Motion::Act {
                    shots,
                    start: now,
                    then: action.then,
                }
            }
            _ => Motion::Stand { until: now + 1.0 },
        };
    }

    fn start_walk(&mut self, now: f64, cfg: &Config, min_x: f32, max_x: f32, width: f32) {
        let lo = min_x + width;
        let hi = (max_x - width).max(lo);
        let target = lo + fastrand::f32() * (hi - lo);
        self.motion = if (target - self.pos.x).abs() >= MIN_WALK {
            let gait = if cfg.vary_speed {
                random_gait()
            } else {
                &GAITS[1]
            };
            Motion::Walk {
                target,
                speed: gait.speed,
                run_up_until: now + gait.run_up,
            }
        } else {
            Motion::Stand {
                until: now + pause(cfg),
            }
        };
    }

    /// The animation, direction and frame to draw. `stride` is the walked
    /// distance in sprite pixels for one frame of the walk cycle, so the walk
    /// cycle advances with the distance and not with the time.
    fn shot(&self, scale: f32, stride: f32, now: f64) -> (&'static str, Dir, usize) {
        match (&self.motion, self.mood) {
            (Motion::Walk { .. }, _) => {
                // Frames 1 to 8 of the walk strip are the walk cycle; frame 0 is
                // the standing frame.
                let i = (self.walk_distance / (stride.max(0.1) * scale)) as usize % 8;
                ("walk", self.facing, 1 + i)
            }
            (Motion::Held | Motion::Fall { .. }, _) => ("recoil", Dir::Down, 0),
            (_, Mood::Happy { .. }) => ("item_get_two_hands", Dir::Down, 0),
            (Motion::Act { shots, start, .. }, _) => {
                let mut t = (now - start) as f32;
                for s in shots {
                    if t < s.secs {
                        return (s.anim, s.dir, s.frame);
                    }
                    t -= s.secs;
                }
                ("walk", self.facing, 0)
            }
            (Motion::Stand { .. }, _) => ("walk", self.facing, 0),
        }
    }

    /// The body box of Link inside `rect`, the full pal window: 16 sprite
    /// pixels wide, from the top of his head to his feet.
    pub fn body(rect: Rect, sprites: &Sprites, scale: f32) -> Rect {
        let feet = rect.bottom();
        let left = (rect.center().x - BODY_WIDTH / 2.0 * scale).round();
        Rect::from_min_max(
            pos2(left, feet - (sprites.feet + 1.0) * scale),
            pos2(left + BODY_WIDTH * scale, feet),
        )
    }

    /// Draws the pal into `rect`, the full pal window.
    pub fn paint(&self, painter: &Painter, rect: Rect, sprites: &Sprites, cfg: &Config, now: f64) {
        let scale = cfg.scale as f32;
        let mut hop = 0.0;
        if let Mood::Happy { until } = self.mood {
            let t = (HAPPY_SECONDS - (until - now)) as f32;
            if t < 1.0 {
                hop = (t * 2.0 * TAU / 2.0).sin().abs() * 4.0 * scale;
            }
        }
        let body = Self::body(rect, sprites, scale).translate(vec2(0.0, -hop));
        let origin = pos2(body.left(), body.bottom() - sprites.feet * scale);

        let (anim, dir, frame) = self.shot(scale, cfg.walk_stride, now);
        let clip = sprites
            .clip(anim, dir)
            .or_else(|| sprites.clip("walk", Dir::Down));
        if let Some(clip) = clip
            && let Some(texture) = clip.frames.get(frame).or(clip.frames.first())
        {
            let sprite =
                Rect::from_min_size((origin - clip.origin * scale).round(), clip.size * scale);
            painter.image(
                texture.id(),
                sprite,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        match self.mood {
            Mood::Thinking => paint_bubble(painter, body, scale, now),
            Mood::Happy { .. } => paint_stars(painter, body, scale, now),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gait_occurs() {
        let mut seen = [false; GAITS.len()];
        for _ in 0..10_000 {
            let g = random_gait();
            let i = GAITS.iter().position(|x| std::ptr::eq(x, g)).unwrap();
            seen[i] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }
}
