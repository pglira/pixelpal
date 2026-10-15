//! Behavior and drawing of the pal.

use std::f32::consts::TAU;

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2};

use crate::actions::{self, ACTIONS, Shot, Then};
use crate::chat::ToolActivity;
use crate::link::{Clip, Dir, Sprites};
use crate::settings::Config;

/// Size of the pal window in sprite pixels. Link stands at the bottom center.
/// The space around him holds the swords, items and thrown objects of the
/// actions, and the speech bubble.
pub const CANVAS: Vec2 = vec2(72.0, 46.0);
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
        /// The walk follows the mouse pointer and ends with a catch.
        chase: Option<Chase>,
    },
    /// Asleep while the user is away.
    Sleep,
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
    /// The current hop, if any.
    hop: Option<Hop>,
    /// True during the night hours of the settings.
    night: bool,
    /// The earliest time of the next chase of the mouse pointer.
    next_chase: f64,
}

/// What the pal perceives in one frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Senses {
    /// The mouse pointer, in screen points.
    pub pointer: Option<Pos2>,
    /// Time since the last input of the user.
    pub idle: Option<std::time::Duration>,
    /// True during the night hours.
    pub night: bool,
}

#[derive(Clone, Copy, Debug)]
struct Chase {
    until: f64,
}

/// Longest chase of the mouse pointer, in seconds.
const CHASE_SECONDS: f64 = 6.0;
/// Shortest time between two chases, in seconds.
const CHASE_COOLDOWN: f64 = 30.0;
/// Speed of a chase, as a factor of the walk speed.
const CHASE_SPEED: f32 = 2.2;
/// Horizontal distance of the pointer from Link that starts a chase, in sprite pixels.
const CHASE_RANGE: (f32, f32) = (24.0, 160.0);
/// Horizontal distance at which Link catches the pointer, in sprite pixels.
const CATCH_DISTANCE: f32 = 14.0;
/// Animations whose poses lie on the ground.
const LYING: &[&str] = &["sleep_in_bed"];
/// Tint of the sprites during the night.
const NIGHT_TINT: Color32 = Color32::from_rgb(160, 170, 235);

/// A hop during a walk: Link leaves the ground, and his shadow stays on it.
#[derive(Clone, Copy, Debug)]
struct Hop {
    start: f64,
    /// Upward start speed, in sprite pixels per second.
    v0: f32,
}

impl Hop {
    /// Height above the ground at `now`, in sprite pixels, for the gravity
    /// `g` in sprite pixels per second². Negative after the landing.
    fn height(&self, now: f64, g: f32) -> f32 {
        let t = (now - self.start) as f32;
        self.v0 * t - g * t * t / 2.0
    }
}

/// Mean number of hops per second of walking.
const HOP_RATE: f32 = 0.35;
/// Range of the hop heights, in sprite pixels.
const HOP_HEIGHT: (f32, f32) = (5.0, 11.0);

impl Pal {
    pub fn new(pos: Pos2) -> Self {
        Self {
            pos,
            motion: Motion::Fall { vy: 0.0 },
            mood: Mood::Calm,
            facing: Dir::Down,
            walk_distance: 0.0,
            hop: None,
            night: false,
            next_chase: 0.0,
        }
    }

    /// True if the pal can start a reaction now: he is on the ground and awake.
    fn can_react(&self) -> bool {
        !matches!(
            self.motion,
            Motion::Held | Motion::Fall { .. } | Motion::Sleep
        )
    }

    /// Plays the action `name` at once, in the direction nearest to `dir`.
    fn react(&mut self, name: &str, dir: Dir, now: f64, sprites: &Sprites) {
        if let Some(action) = actions::find(name) {
            if matches!(self.mood, Mood::Happy { .. }) {
                self.mood = Mood::Calm;
            }
            self.play_dir(action, actions::dir_for(action, dir), now, sprites);
        }
    }

    /// Reacts to a tool event of Claude with the item of the tool. A failed
    /// tool interrupts the current action; a tool call waits for its end.
    pub fn react_to_tool(&mut self, activity: &ToolActivity, now: f64, sprites: &Sprites) {
        if !self.can_react() {
            return;
        }
        let busy = matches!(self.motion, Motion::Act { .. });
        let side = if fastrand::bool() {
            Dir::Left
        } else {
            Dir::Right
        };
        match activity {
            ToolActivity::Error => self.react(actions::TOOL_ERROR_REACTION, side, now, sprites),
            ToolActivity::Use(tool) if !busy => {
                self.react(actions::tool_reaction(tool), side, now, sprites)
            }
            ToolActivity::Use(_) => {}
        }
    }

    /// Shows a level up: Link pulls the Master Sword and holds it up.
    pub fn level_up(&mut self, now: f64, sprites: &Sprites) {
        if self.can_react() {
            self.react(actions::LEVEL_UP, Dir::Down, now, sprites);
        }
    }

    /// Height of the current hop above the ground, in sprite pixels.
    fn hop_height(&self, now: f64, cfg: &Config) -> f32 {
        self.hop
            .map_or(0.0, |h| h.height(now, cfg.gravity * PIXELS_PER_METER))
            .max(0.0)
    }

    pub fn window_size(scale: u32) -> Vec2 {
        CANVAS * scale as f32
    }

    /// Seconds until the pal looks different, so that the app repaints only
    /// then. `ppp` is the number of physical pixels per point. `idle` is the
    /// time since the last input of the user.
    pub fn next_change(
        &self,
        now: f64,
        cfg: &Config,
        ppp: f32,
        may_move: bool,
        idle: Option<std::time::Duration>,
    ) -> f64 {
        const FRAME: f64 = 1.0 / 60.0;
        let mut t = match &self.motion {
            Motion::Fall { .. } | Motion::Held => FRAME,
            Motion::Walk { .. } if self.hop.is_some() => FRAME,
            Motion::Walk { speed, .. } => {
                // The window moves by one physical pixel.
                let px_per_second = cfg.walk_speed * speed * ppp.max(0.1);
                (1.0 / px_per_second.max(1.0)) as f64
            }
            Motion::Act { shots, start, .. } => {
                // The next shot starts.
                let mut end = 0.0;
                let elapsed = (now - start) as f32;
                for s in shots {
                    end += s.secs;
                    if end > elapsed {
                        break;
                    }
                }
                (end - elapsed).max(0.0) as f64
            }
            Motion::Stand { until } if self.hop.is_some() => FRAME.min(until - now),
            Motion::Stand { until } => until - now,
            // The Zzz rise half a sprite pixel per game frame. After a long
            // absence, they slow down to save power.
            Motion::Sleep if idle.is_some_and(|d| d.as_secs() >= 30 * 60) => 1.0,
            Motion::Sleep => 4.0 * FRAME,
        };
        match self.mood {
            // The dots of the bubble change three times per second.
            Mood::Thinking => t = t.min(1.0 / 3.0 - now.rem_euclid(1.0 / 3.0)),
            Mood::Happy { .. } => t = FRAME,
            Mood::Calm => {}
        }
        // A pal that may chase checks the pointer five times per second.
        if self.wants_pointer(now, cfg, may_move) {
            t = t.min(0.2);
        }
        t.max(FRAME)
    }

    /// True if the pal uses the pointer position now: for a chase, or to see
    /// whether one can start.
    pub fn wants_pointer(&self, now: f64, cfg: &Config, may_move: bool) -> bool {
        match self.motion {
            Motion::Walk { chase: Some(_), .. } => true,
            Motion::Stand { .. } => cfg.chase && may_move && now >= self.next_chase,
            _ => false,
        }
    }

    pub fn is_held(&self) -> bool {
        matches!(self.motion, Motion::Held)
    }

    pub fn hold(&mut self) {
        self.motion = Motion::Held;
        self.hop = None;
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
    /// must stay calm: then no walk, no chase and no action starts.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        now: f64,
        dt: f32,
        area: Area,
        cfg: &Config,
        sprites: &Sprites,
        may_move: bool,
        senses: &Senses,
    ) {
        let size = Self::window_size(cfg.scale);
        let ground = area.bottom - size.y - cfg.bottom_margin;
        let min_x = area.left;
        let max_x = (area.right - size.x).max(min_x);
        self.night = cfg.day_night && senses.night;

        if let Mood::Happy { until } = self.mood
            && now >= until
        {
            self.mood = Mood::Calm;
        }

        // Sleep while the user is away, and wake up when the user returns.
        let away = cfg.idle_sleep
            && senses
                .idle
                .is_some_and(|d| d.as_secs_f32() >= cfg.idle_minutes * 60.0);
        match self.motion {
            Motion::Sleep if !away => self.react(actions::WAKE_UP, Dir::Down, now, sprites),
            Motion::Stand { .. } | Motion::Walk { .. } | Motion::Act { .. }
                if away && self.mood != Mood::Thinking =>
            {
                self.motion = Motion::Sleep;
                self.mood = Mood::Calm;
                self.facing = Dir::Down;
                self.hop = None;
            }
            _ => {}
        }

        match self.motion {
            Motion::Held | Motion::Sleep => return,
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
                let chase_target = (may_move && cfg.chase && now >= self.next_chase)
                    .then(|| self.pointer_target(senses.pointer, area, cfg))
                    .flatten();
                if let Some(target) = chase_target {
                    self.motion = Motion::Walk {
                        target,
                        speed: CHASE_SPEED,
                        run_up_until: now,
                        chase: Some(Chase {
                            until: now + CHASE_SECONDS,
                        }),
                    };
                } else if may_move && now >= until {
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
                mut target,
                speed,
                run_up_until,
                chase,
            } => {
                if let Some(c) = chase {
                    // Follow the pointer; give up when it leaves the range or
                    // after a while.
                    match self.pointer_target(senses.pointer, area, cfg) {
                        Some(t) if now < c.until && may_move => {
                            target = t;
                            if let Motion::Walk { target: old, .. } = &mut self.motion {
                                *old = t;
                            }
                        }
                        _ => {
                            self.next_chase = now + CHASE_COOLDOWN;
                            self.motion = Motion::Stand {
                                until: now + pause(cfg),
                            };
                            return;
                        }
                    }
                    let scale = cfg.scale as f32;
                    if (target - self.pos.x).abs() <= CATCH_DISTANCE * scale {
                        self.next_chase = now + CHASE_COOLDOWN;
                        self.react(actions::CATCH, self.facing, now, sprites);
                        return;
                    }
                }
                let d = target - self.pos.x;
                if !may_move || !(cfg.walk || chase.is_some()) {
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
                        if cfg.hops && self.hop.is_none() && fastrand::f32() < HOP_RATE * dt {
                            let h = HOP_HEIGHT.0 + fastrand::f32() * (HOP_HEIGHT.1 - HOP_HEIGHT.0);
                            self.hop = Some(Hop {
                                start: now,
                                v0: (2.0 * cfg.gravity * PIXELS_PER_METER * h).sqrt(),
                            });
                        }
                    }
                }
            }
        }

        // A hop ends at the landing, and with any motion other than a walk or a
        // stand. A hop at the end of a walk lands in the stand.
        if let Some(h) = self.hop
            && (h.height(now, cfg.gravity * PIXELS_PER_METER) < 0.0
                || !matches!(self.motion, Motion::Walk { .. } | Motion::Stand { .. }))
        {
            self.hop = None;
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
    /// At night, the action comes from `actions::NIGHT_ACTIONS`.
    fn start_action(&mut self, now: f64, sprites: &Sprites) {
        let forced = std::env::var("PIXELPAL_ACTION").ok();
        let night = actions::NIGHT_ACTIONS[fastrand::usize(..actions::NIGHT_ACTIONS.len())];
        let action = forced
            .as_deref()
            .and_then(actions::find)
            .or_else(|| self.night.then(|| actions::find(night)).flatten())
            .unwrap_or_else(|| &ACTIONS[fastrand::usize(..ACTIONS.len())]);
        self.play(action, now, sprites);
    }

    /// Reacts to a poke with a random action from `actions::REACTIONS`, at
    /// once and also while the chat panel is open. A held or falling pal does
    /// not react.
    pub fn poke(&mut self, now: f64, sprites: &Sprites) {
        if !self.can_react() {
            return;
        }
        let name = actions::REACTIONS[fastrand::usize(..actions::REACTIONS.len())];
        if let Some(action) = actions::find(name) {
            if matches!(self.mood, Mood::Happy { .. }) {
                self.mood = Mood::Calm;
            }
            self.play(action, now, sprites);
        }
    }

    fn play(&mut self, action: &actions::Action, now: f64, sprites: &Sprites) {
        self.play_dir(action, actions::pick_dir(action), now, sprites);
    }

    fn play_dir(&mut self, action: &actions::Action, dir: Dir, now: f64, sprites: &Sprites) {
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

    /// The window x at which Link stands just before the mouse pointer, if the
    /// pointer is in the chase range: near the ground, beside Link, and not on him.
    fn pointer_target(&self, pointer: Option<Pos2>, area: Area, cfg: &Config) -> Option<f32> {
        let p = pointer?;
        let scale = cfg.scale as f32;
        let size = Self::window_size(cfg.scale);
        let center = self.pos.x + size.x / 2.0;
        let feet = self.pos.y + size.y;
        let dx = p.x - center;
        let near_ground = p.y >= feet - 60.0 * scale && p.y <= area.bottom + 4.0;
        let in_range = dx.abs() >= CHASE_RANGE.0 * scale && dx.abs() <= CHASE_RANGE.1 * scale;
        let chasing = matches!(self.motion, Motion::Walk { chase: Some(_), .. });
        // The target stays inside the area, so that a pointer near the screen
        // edge can be caught.
        let max_x = (area.right - size.x).max(area.left);
        (near_ground && (in_range || (chasing && dx.abs() <= CHASE_RANGE.1 * scale)))
            .then(|| (p.x - size.x / 2.0 - dx.signum() * 6.0 * scale).clamp(area.left, max_x))
    }

    fn start_walk(&mut self, now: f64, cfg: &Config, min_x: f32, max_x: f32, width: f32) {
        let lo = min_x + width;
        let hi = (max_x - width).max(lo);
        let target = lo + fastrand::f32() * (hi - lo);
        self.motion = if (target - self.pos.x).abs() >= MIN_WALK {
            // At night, Link only strolls.
            let gait = if self.night {
                &GAITS[0]
            } else if cfg.vary_speed {
                random_gait()
            } else {
                &GAITS[1]
            };
            Motion::Walk {
                target,
                speed: gait.speed,
                run_up_until: now + gait.run_up,
                chase: None,
            }
        } else {
            Motion::Stand {
                until: now + pause(cfg),
            }
        };
    }

    /// The frame to draw, with its prop. `stride` is the walked distance in
    /// sprite pixels for one frame of the walk cycle, so the walk cycle
    /// advances with the distance and not with the time.
    fn shot(&self, scale: f32, stride: f32, now: f64) -> Shot {
        let pose = |anim, dir, frame| Shot {
            anim,
            dir,
            frame,
            secs: 0.0,
            prop: None,
        };
        match (&self.motion, self.mood) {
            (Motion::Walk { .. }, _) => {
                // Frames 1 to 8 of the walk strip are the walk cycle; frame 0 is
                // the standing frame.
                let i = (self.walk_distance / (stride.max(0.1) * scale)) as usize % 8;
                pose("walk", self.facing, 1 + i)
            }
            (Motion::Held | Motion::Fall { .. }, _) => pose("recoil", Dir::Down, 0),
            (Motion::Sleep, _) => pose("sleep_in_bed", Dir::Down, 0),
            (_, Mood::Happy { .. }) => pose("item_get_two_hands", Dir::Down, 0),
            (Motion::Act { shots, start, .. }, _) => {
                let mut t = (now - start) as f32;
                for s in shots {
                    if t < s.secs {
                        return *s;
                    }
                    t -= s.secs;
                }
                pose("walk", self.facing, 0)
            }
            (Motion::Stand { .. }, _) => pose("walk", self.facing, 0),
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

    /// The area of `rect`, the full pal window, that takes clicks: Link's body
    /// with a small margin, and the space of the highest hop above his head.
    pub fn click_rect(rect: Rect, sprites: &Sprites, scale: f32) -> Rect {
        let body = Self::body(rect, sprites, scale).expand(2.0 * scale);
        Rect::from_min_max(body.min - vec2(0.0, HOP_HEIGHT.1 * scale), body.max)
    }

    /// Draws the pal into `rect`, the full pal window.
    pub fn paint(&self, painter: &Painter, rect: Rect, sprites: &Sprites, cfg: &Config, now: f64) {
        let scale = cfg.scale as f32;
        let mut hop = self.hop_height(now, cfg) * scale;
        if let Mood::Happy { until } = self.mood {
            let t = (HAPPY_SECONDS - (until - now)) as f32;
            if t < 1.0 {
                hop += (t * 2.0 * TAU / 2.0).sin().abs() * 4.0 * scale;
            }
        }
        let ground = Self::body(rect, sprites, scale);
        let body = ground.translate(vec2(0.0, -hop));
        let mut origin = pos2(body.left(), body.bottom() - sprites.feet * scale);

        let shot = self.shot(scale, cfg.walk_stride, now);
        // In the game, a lying Link rests in a bed, which is a background tile.
        // Without the bed, his pose moves down onto the ground.
        if LYING.contains(&shot.anim)
            && let Some(clip) = sprites.clip(shot.anim, shot.dir)
            && let Some(&bottom) = clip.bottoms.get(shot.frame)
        {
            let gap = sprites.feet - (bottom - clip.origin.y);
            origin.y += gap.max(0.0) * scale;
        }
        let tint = if self.night {
            NIGHT_TINT
        } else {
            Color32::WHITE
        };
        // Draws frame `frame` of `clip` with the clip origin at `at`. The
        // frames lie side by side in the texture of the strip.
        let draw = |clip: &Clip, frame: usize, at: Pos2| {
            let (i, n) = (frame.min(clip.frames - 1) as f32, clip.frames as f32);
            painter.image(
                clip.texture.id(),
                Rect::from_min_size((at - clip.origin * scale).round(), clip.size * scale),
                Rect::from_min_max(pos2(i / n, 0.0), pos2((i + 1.0) / n, 1.0)),
                tint,
            );
        };

        // In the air, the shadow stays on the ground below Link.
        if hop > 0.0
            && let Some(shadow) = sprites.shadow(shot.dir)
        {
            let ground_origin = pos2(ground.left(), ground.bottom() - sprites.feet * scale);
            draw(shadow, 0, ground_origin);
        }
        if let Some(clip) = sprites
            .clip(shot.anim, shot.dir)
            .or_else(|| sprites.clip("walk", Dir::Down))
        {
            draw(clip, shot.frame, origin);
        }
        if let Some(p) = shot.prop
            && let Some(clip) = sprites.extra(p.extra)
        {
            draw(clip, p.frame, origin + vec2(p.at.0, p.at.1) * scale);
        }
        if (shot.anim, shot.frame) == ("sleep_in_bed", 0)
            && let Some(zzz) = sprites.extra("zzz")
        {
            paint_zzz(&draw, zzz, origin, scale, now);
        }

        match self.mood {
            Mood::Thinking => paint_bubble(painter, body, scale, now),
            Mood::Happy { .. } => paint_stars(painter, body, scale, now),
            Mood::Calm => {}
        }
    }
}

/// The Zzz of a sleeping Link, as `Ancilla21_Snore` of the game draws them: a
/// new Zzz every 32 game frames, rising 0.5 pixels per frame from beside his
/// head and swaying, for 56 frames. All Zzz show the frame of the newest one,
/// which grows from small to large in 16 frames.
fn paint_zzz(draw: &impl Fn(&Clip, usize, Pos2), zzz: &Clip, origin: Pos2, scale: f32, now: f64) {
    let frames = now * 60.0;
    let newest = (frames % 32.0) as f32;
    let frame = ((newest / 8.0) as usize).min(zzz.frames - 1);
    for age in [newest, newest + 32.0] {
        if age >= 56.0 {
            continue;
        }
        let x = 16.0 + 2.5 * (age / 32.0 * TAU).sin();
        let y = 4.0 - 0.5 * age;
        draw(zzz, frame, origin + vec2(x, y) * scale);
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
    fn no_pose_floats_above_the_ground() {
        let sprites = Sprites::load(&egui::Context::default(), 0);
        for (anim, dirs) in sprites.clip_names() {
            // Link shrinks into the hole of a pit; lying poses move down.
            if anim == "fall_in_pit" || LYING.contains(&anim.as_str()) {
                continue;
            }
            for dir in dirs {
                let clip = sprites.clip(&anim, dir).unwrap();
                for (i, &bottom) in clip.bottoms.iter().enumerate() {
                    let gap = sprites.feet - (bottom - clip.origin.y);
                    assert!(gap <= 3.0, "{anim} {dir:?} frame {i} floats {gap} px");
                }
            }
        }
    }

    #[test]
    fn hop_reaches_its_height_and_lands() {
        let g = 9.81 * PIXELS_PER_METER;
        let h = 8.0;
        let hop = Hop {
            start: 0.0,
            v0: (2.0 * g * h).sqrt(),
        };
        let top = (hop.v0 / g) as f64;
        assert!((hop.height(top, g) - h).abs() < 1e-3);
        assert!(hop.height(2.0 * top + 0.01, g) < 0.0);
    }

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
