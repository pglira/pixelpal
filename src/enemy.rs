//! Enemies of the game that appear now and then, and Link's fights with them.
//!
//! An enemy appears in a poof on the ground beside Link and comes to him. Link
//! runs to it and strikes with his sword. A hit enemy flashes and slides back;
//! a defeated enemy vanishes in the death poof of the game and can leave a
//! heart or a rupee, which Link picks up. An enemy that touches Link hurts
//! him. An Octorok spits stones, which Link's shield stops when he faces them.
//! A Buzzblob shocks Link when his sword hits it; he then uses the hammer.
//!
//! The enemy, its poof and its drop live in a window of their own, which
//! follows the enemy on the ground. The stones are drawn in the enemy window
//! and in the pal window.

use egui::{Color32, Painter, Pos2, Rect, Vec2, pos2, vec2};

use crate::actions;
use crate::link::{Clip, Dir, GAME_FRAME, Sprites};
use crate::pal::{Area, CANVAS, Pal};
use crate::settings::Config;

/// Size of the enemy window, in sprite pixels.
pub const WINDOW: Vec2 = vec2(64.0, CANVAS.y);
/// Title of the enemy window.
pub const ENEMY_TITLE: &str = "pixelpal enemy";

/// A kind of enemy. Its strips in the sprite recipe are `<name>_<animation>`.
struct Kind {
    name: &'static str,
    /// Number of hits that defeat the enemy.
    hp: u8,
    /// Walk speed, in sprite pixels per second.
    speed: f32,
    /// The enemy flies above the ground and has no shadow.
    flies: bool,
    /// The enemy stops now and then and spits a stone.
    shoots: bool,
    /// A sword hit shocks Link and does no harm to the enemy.
    shocks: bool,
    /// The enemy has walk strips for the left and the right; otherwise only
    /// `walk_down`, or `fly` for a flying enemy.
    sided: bool,
    /// Weight in the random choice of the day enemies.
    weight: u32,
}

const DAY_KINDS: &[Kind] = &[
    Kind {
        name: "octorok",
        hp: 1,
        speed: 45.0,
        flies: false,
        shoots: true,
        shocks: false,
        sided: true,
        weight: 3,
    },
    Kind {
        name: "green_soldier",
        hp: 2,
        speed: 30.0,
        flies: false,
        shoots: false,
        shocks: false,
        sided: true,
        weight: 3,
    },
    Kind {
        name: "blue_soldier",
        hp: 3,
        speed: 36.0,
        flies: false,
        shoots: false,
        shocks: false,
        sided: true,
        weight: 2,
    },
    Kind {
        name: "buzzblob",
        hp: 1,
        speed: 12.0,
        flies: false,
        shoots: false,
        shocks: true,
        sided: false,
        weight: 2,
    },
];

/// The enemy of the night.
const KEESE: Kind = Kind {
    name: "keese",
    hp: 1,
    speed: 40.0,
    flies: true,
    shoots: false,
    shocks: false,
    sided: false,
    weight: 0,
};

/// Distance from Link to a new enemy, in sprite pixels: smallest and largest.
const SPAWN_RANGE: (f32, f32) = (80.0, 200.0);
/// Shortest time between two enemies, in seconds.
const MIN_SPAWN_GAP: f64 = 30.0;
/// Duration of the poof in which an enemy appears, in seconds.
const APPEAR_SECONDS: f64 = 0.35;
/// Largest distance between the centers of Link and an enemy at which his
/// sword hits it, in sprite pixels.
const REACH: f32 = 26.0;
/// Distance at which Link stops before an enemy to strike, in sprite pixels.
const STRIKE_DISTANCE: f32 = 20.0;
/// Distance at which an enemy touches Link, in sprite pixels.
const CONTACT: f32 = 13.0;
/// Time from the start of a sword swing to the hit, in seconds.
const SWORD_HIT_DELAY: f64 = 4.0 * GAME_FRAME as f64;
/// Time from the start of a hammer blow to the hit, in seconds.
const HAMMER_HIT_DELAY: f64 = 7.0 * GAME_FRAME as f64;
/// Speed and duration of the slide of a hit enemy.
const ENEMY_KNOCK_SPEED: f32 = 80.0;
const ENEMY_KNOCK_SECONDS: f64 = 0.25;
/// The death: the damage flash, then 8 poof frames of 16/3 game frames.
const DEATH_FLASH_SECONDS: f64 = 0.25;
const POOF_FRAME: f64 = 16.0 / 3.0 * GAME_FRAME as f64;
/// Distance from Link at which an Octorok spits a stone, in sprite pixels.
const SHOOT_RANGE: (f32, f32) = (36.0, 120.0);
/// The frames of the Octorok's snout while it spits: frame, game frames. The
/// stone leaves at the start of frame 2.
const SHOOT_FRAMES: [(usize, f32); 4] = [(0, 8.0), (1, 24.0), (2, 24.0), (0, 8.0)];
/// Speed of a stone, in sprite pixels per second (44/16 pixels per frame).
const STONE_SPEED: f32 = 165.0;
/// Duration of one frame of the crumble of a stone, in game frames.
const CRUMBLE_FRAME: f32 = 32.0 / 3.0;
/// The shock of a Buzzblob: 16 frames of 2 game frames.
const SHOCK_SECONDS: f64 = 32.0 * GAME_FRAME as f64;
/// Lifetime of a drop on the ground, and its blinking at the end, in seconds.
const DROP_SECONDS: f64 = 12.0;
const DROP_BLINK_SECONDS: f64 = 3.0;
/// Distance at which Link picks up a drop, in sprite pixels.
const PICK_UP: f32 = 6.0;
/// The shine of a rupee: frame, game frames.
const RUPEE_SHINE: [(usize, f32); 6] =
    [(0, 14.0), (1, 4.0), (2, 6.0), (0, 16.0), (1, 6.0), (2, 8.0)];
/// Colors of the damage flash for strips without a `hit` strip of their
/// direction, one per game frame.
const FLASH: [Color32; 4] = [
    Color32::WHITE,
    Color32::from_rgb(255, 90, 90),
    Color32::from_rgb(120, 150, 255),
    Color32::from_rgb(255, 230, 100),
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Appear,
    Walk {
        until: f64,
    },
    Rest {
        until: f64,
    },
    Shoot {
        spat: bool,
    },
    Shock,
    Hurt {
        vx: f32,
    },
    /// The enemy dies. `flee`: it leaves without a drop.
    Dying {
        flee: bool,
    },
}

struct Enemy {
    kind: &'static Kind,
    /// Screen x of the center of the body, in points.
    x: f32,
    /// Height above the ground, in sprite pixels.
    z: f32,
    /// Direction of the walk strips: left or right.
    dir: Dir,
    state: State,
    /// Start time of the state.
    since: f64,
    hp: u8,
    /// A Buzzblob shocked Link: his next blow is with the hammer.
    shocked: bool,
    born: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum Weapon {
    Sword,
    Hammer,
}

struct Stone {
    /// Screen x of the center, in points.
    x: f32,
    /// Speed in points per second.
    vx: f32,
    /// Start of the crumble, if the stone hit something.
    crumble: Option<f64>,
}

#[derive(Clone, Copy)]
enum Prize {
    Heart,
    GreenRupee,
    BlueRupee,
}

struct Drop {
    prize: Prize,
    x: f32,
    since: f64,
}

#[derive(Default)]
pub struct Fight {
    enemy: Option<Enemy>,
    stones: Vec<Stone>,
    drop: Option<Drop>,
    /// Time of the next enemy; 0 before the first schedule.
    next_spawn: f64,
    /// A blow of Link that lands at the given time.
    blow: Option<(f64, Weapon)>,
}

/// The enemy kind that `$PIXELPAL_ENEMY` names. With it, enemies come every
/// few seconds.
fn forced_kind() -> Option<&'static Kind> {
    let name = std::env::var("PIXELPAL_ENEMY").ok()?;
    DAY_KINDS
        .iter()
        .chain(std::iter::once(&KEESE))
        .find(|k| k.name == name)
}

fn random_day_kind() -> &'static Kind {
    let total: u32 = DAY_KINDS.iter().map(|k| k.weight).sum();
    let mut r = fastrand::u32(..total);
    for k in DAY_KINDS {
        if r < k.weight {
            return k;
        }
        r -= k.weight;
    }
    &DAY_KINDS[0]
}

fn side(dx: f32) -> Dir {
    if dx < 0.0 { Dir::Left } else { Dir::Right }
}

impl Fight {
    /// True while an enemy, a stone or a drop is on the screen.
    pub fn is_active(&self) -> bool {
        self.enemy.is_some() || self.drop.is_some() || !self.stones.is_empty()
    }

    fn schedule(&mut self, now: f64, cfg: &Config) {
        self.next_spawn = if forced_kind().is_some() {
            now + 4.0
        } else {
            // Exponential gaps with the configured mean.
            let mean = cfg.enemy_minutes as f64 * 60.0;
            let gap = -mean * (1.0 - fastrand::f64()).ln();
            now + gap.max(MIN_SPAWN_GAP)
        };
    }

    /// Advances the enemies, the stones, the drops and Link's moves by `dt`
    /// seconds. `may_move` is false while the pal must stay calm: then the
    /// enemy leaves and no enemy comes. `night`: only Keese come.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        now: f64,
        dt: f32,
        pal: &mut Pal,
        sprites: &Sprites,
        cfg: &Config,
        area: Area,
        may_move: bool,
        night: bool,
    ) {
        let scale = cfg.scale as f32;
        let half = WINDOW.x / 2.0 * scale;
        let (lo, hi) = (area.left + half, (area.right - half).max(area.left + half));
        let link = pal.center_x(cfg.scale);

        if self.next_spawn == 0.0 {
            self.schedule(now, cfg);
        }
        if !cfg.enemies || !may_move {
            if let Some(e) = &mut self.enemy
                && !matches!(e.state, State::Dying { .. })
            {
                e.state = State::Dying { flee: true };
                e.since = now;
            }
            self.blow = None;
        } else if !self.is_active() && pal.is_free() && now >= self.next_spawn {
            self.spawn(now, link, lo, hi, scale, night);
            self.schedule(now, cfg);
        }

        self.update_enemy(now, dt, pal, sprites, cfg, area, link, lo, hi);
        self.update_stones(now, dt, pal, sprites, cfg, area);

        // Link goes to a drop and picks it up. A drop vanishes after a while.
        if let Some(d) = &self.drop {
            if now - d.since >= DROP_SECONDS {
                self.drop = None;
            } else if self.enemy.is_none() && may_move && pal.is_free() {
                let link = pal.center_x(cfg.scale);
                if (d.x - link).abs() <= PICK_UP * scale {
                    self.drop = None;
                } else {
                    pal.approach(d.x, cfg, area, now);
                }
            }
        }
        pal.fighting = self.enemy.is_some() || self.drop.is_some();
    }

    fn spawn(&mut self, now: f64, link: f32, lo: f32, hi: f32, scale: f32, night: bool) {
        let kind = forced_kind().unwrap_or_else(|| if night { &KEESE } else { random_day_kind() });
        let d = (SPAWN_RANGE.0 + fastrand::f32() * (SPAWN_RANGE.1 - SPAWN_RANGE.0)) * scale;
        let first = if fastrand::bool() { 1.0 } else { -1.0 };
        let Some(x) = [link + first * d, link - first * d]
            .into_iter()
            .find(|x| (lo..=hi).contains(x))
        else {
            return;
        };
        self.enemy = Some(Enemy {
            kind,
            x,
            z: if kind.flies { 12.0 } else { 0.0 },
            dir: side(link - x),
            state: State::Appear,
            since: now,
            hp: kind.hp,
            shocked: false,
            born: now,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn update_enemy(
        &mut self,
        now: f64,
        dt: f32,
        pal: &mut Pal,
        sprites: &Sprites,
        cfg: &Config,
        area: Area,
        link: f32,
        lo: f32,
        hi: f32,
    ) {
        let scale = cfg.scale as f32;
        let Some(e) = &mut self.enemy else {
            self.blow = None;
            return;
        };
        let dx = link - e.x;
        let dist = dx.abs() / scale;
        let t = now - e.since;
        let mut next = None;
        match e.state {
            State::Appear => {
                if t >= APPEAR_SECONDS {
                    next = Some(State::Walk { until: now + 1.2 });
                }
            }
            State::Walk { until } => {
                e.dir = side(dx);
                if dist > CONTACT {
                    let step = (e.kind.speed * scale * dt).min((dist - CONTACT) * scale);
                    e.x += step * dx.signum();
                }
                if e.kind.shoots && now >= until {
                    next = Some(State::Rest {
                        until: now + 0.4 + fastrand::f64() * 0.6,
                    });
                }
            }
            State::Rest { until } => {
                e.dir = side(dx);
                if now >= until {
                    next = Some(
                        if e.kind.shoots && (SHOOT_RANGE.0..=SHOOT_RANGE.1).contains(&dist) {
                            State::Shoot { spat: false }
                        } else {
                            State::Walk {
                                until: now + 0.6 + fastrand::f64() * 0.8,
                            }
                        },
                    );
                }
            }
            State::Shoot { spat } => {
                let spit_at = (SHOOT_FRAMES[0].1 + SHOOT_FRAMES[1].1) * GAME_FRAME;
                let total: f32 = SHOOT_FRAMES.iter().map(|f| f.1).sum::<f32>() * GAME_FRAME;
                if !spat && t as f32 >= spit_at {
                    let sign = if e.dir == Dir::Left { -1.0 } else { 1.0 };
                    self.stones.push(Stone {
                        x: e.x + sign * 10.0 * scale,
                        vx: sign * STONE_SPEED * scale,
                        crumble: None,
                    });
                    e.state = State::Shoot { spat: true };
                }
                if t as f32 >= total {
                    next = Some(State::Walk {
                        until: now + 0.8 + fastrand::f64() * 0.8,
                    });
                }
            }
            State::Shock => {
                if t >= SHOCK_SECONDS {
                    next = Some(State::Rest { until: now + 0.4 });
                }
            }
            State::Hurt { vx } => {
                if t < ENEMY_KNOCK_SECONDS {
                    e.x += vx * dt;
                } else if e.hp == 0 {
                    next = Some(State::Dying { flee: false });
                } else {
                    next = Some(State::Walk { until: now + 1.0 });
                }
            }
            State::Dying { flee } => {
                if t >= DEATH_FLASH_SECONDS + 8.0 * POOF_FRAME {
                    if !flee {
                        self.drop = random_prize().map(|prize| Drop {
                            prize,
                            x: e.x,
                            since: now,
                        });
                    }
                    self.enemy = None;
                    self.blow = None;
                    return;
                }
            }
        }
        if let Some(s) = next {
            e.state = s;
            e.since = now;
        }
        e.x = e.x.clamp(lo, hi);
        if e.kind.flies {
            let age = (now - e.born) as f32;
            e.z = 12.0 + 7.0 * (age * 3.1).sin();
        }

        let fighting = matches!(
            e.state,
            State::Walk { .. } | State::Rest { .. } | State::Shoot { .. }
        );
        // The enemy touches Link and hurts him, then pauses.
        if fighting && dist <= CONTACT && pal.is_vulnerable(now) {
            pal.hurt(dx.signum(), cfg, now, sprites);
            e.state = State::Rest { until: now + 0.8 };
            e.since = now;
            self.blow = None;
            return;
        }

        // Link goes to the enemy and strikes.
        let toward = side(-dx);
        if self.blow.is_none() && pal.is_free() && !matches!(e.state, State::Dying { .. }) {
            if dist > REACH * 0.85 {
                pal.approach(e.x + dx.signum() * STRIKE_DISTANCE * scale, cfg, area, now);
            } else if fighting || matches!(e.state, State::Appear) {
                let (weapon, name, delay) = if e.kind.shocks && e.shocked {
                    (Weapon::Hammer, actions::POUND, HAMMER_HIT_DELAY)
                } else {
                    (Weapon::Sword, actions::STRIKE, SWORD_HIT_DELAY)
                };
                pal.fight_move(name, toward, now, sprites);
                self.blow = Some((now + delay, weapon));
            }
        }

        // The blow lands if the enemy is still in reach.
        if let Some((at, weapon)) = self.blow
            && now >= at
        {
            self.blow = None;
            if fighting && dist <= REACH * 1.3 {
                if e.kind.shocks && weapon == Weapon::Sword {
                    pal.fight_move("electrocuted", toward, now, sprites);
                    e.shocked = true;
                    e.state = State::Shock;
                } else {
                    e.hp = e.hp.saturating_sub(1);
                    e.state = State::Hurt {
                        vx: -dx.signum() * ENEMY_KNOCK_SPEED * scale,
                    };
                }
                e.since = now;
            }
        }
    }

    fn update_stones(
        &mut self,
        now: f64,
        dt: f32,
        pal: &mut Pal,
        sprites: &Sprites,
        cfg: &Config,
        area: Area,
    ) {
        let scale = cfg.scale as f32;
        let link = pal.center_x(cfg.scale);
        for s in &mut self.stones {
            if s.crumble.is_some() {
                continue;
            }
            s.x += s.vx * dt;
            if (s.x - link).abs() <= 8.0 * scale {
                // The shield stops a stone that comes from the front.
                let from = side(s.x - link);
                if pal.is_shielding() && pal.facing() == from {
                    s.crumble = Some(now);
                } else if pal.is_vulnerable(now) {
                    pal.hurt((link - s.x).signum(), cfg, now, sprites);
                    s.crumble = Some(now);
                }
            }
        }
        let crumble_secs = 4.0 * CRUMBLE_FRAME as f64 * GAME_FRAME as f64;
        self.stones.retain(|s| match s.crumble {
            Some(t) => now - t < crumble_secs,
            None => s.x > area.left && s.x < area.right,
        });
    }

    /// The top-left corner of the enemy window on the screen, while an enemy
    /// or a drop is there.
    pub fn window_pos(&self, cfg: &Config, area: Area) -> Option<Pos2> {
        let x = self
            .enemy
            .as_ref()
            .map(|e| e.x)
            .or(self.drop.as_ref().map(|d| d.x))?;
        let size = WINDOW * cfg.scale as f32;
        let ground = area.bottom - cfg.bottom_margin;
        Some(pos2(x - size.x / 2.0, ground - size.y).round())
    }

    /// Draws the enemy, its poof, the drop and the stones into `rect`, a
    /// window whose top-left corner is at `window` on the screen. `stones_only`
    /// draws only the stones.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &self,
        painter: &Painter,
        rect: Rect,
        window: Pos2,
        sprites: &Sprites,
        cfg: &Config,
        area: Area,
        night: bool,
        now: f64,
        stones_only: bool,
    ) {
        let scale = cfg.scale as f32;
        let ground = area.bottom - cfg.bottom_margin - window.y + rect.top();
        let base = if night {
            crate::pal::NIGHT_TINT
        } else {
            Color32::WHITE
        };
        let to_local = |x: f32| x - window.x + rect.left();
        // Draws frame `frame` of `clip` with its origin at `at`.
        let draw = |clip: &Clip, frame: usize, at: Pos2, tint: Color32| {
            let (i, n) = (frame.min(clip.frames - 1) as f32, clip.frames as f32);
            painter.image(
                clip.texture.id(),
                Rect::from_min_size((at - clip.origin * scale).round(), clip.size * scale),
                Rect::from_min_max(pos2(i / n, 0.0), pos2((i + 1.0) / n, 1.0)),
                tint * base,
            );
        };
        // The origin of a sprite whose reference strip stands on the ground,
        // with its body centered at the screen x `x`, `z` sprite pixels high.
        let origin = |reference: &Clip, x: f32, z: f32| {
            let feet = reference.bottoms.first().copied().unwrap_or(16.0) - reference.origin.y;
            pos2(to_local(x) - 8.0 * scale, ground - (feet + z) * scale)
        };

        if let (Some(fly), Some(crumble)) = (
            sprites.extra("octorok_stone_idle"),
            sprites.extra("octorok_stone_crumble"),
        ) {
            for s in &self.stones {
                let at = origin(fly, s.x, 0.0);
                match s.crumble {
                    Some(t) => {
                        let i = ((now - t) / GAME_FRAME as f64 / CRUMBLE_FRAME as f64) as usize;
                        draw(crumble, i, at, Color32::WHITE);
                    }
                    None => draw(fly, 0, at, Color32::WHITE),
                }
            }
        }
        if stones_only {
            return;
        }

        if let Some(d) = &self.drop {
            let left = DROP_SECONDS - (now - d.since);
            let blink = left < DROP_BLINK_SECONDS && (now * 15.0) as i64 % 2 == 0;
            let t = ((now - d.since) / GAME_FRAME as f64) as f32;
            let shown = match d.prize {
                Prize::Heart => {
                    // The heart falls from above and sways.
                    let z = (16.0 - t * 0.5).max(0.0);
                    let idle = sprites.extra("heart_idle");
                    let fall = sprites.extra("heart_fall");
                    idle.zip(fall).map(|(idle, fall)| {
                        if z > 0.0 {
                            (fall, (t / 8.0) as usize % 2, origin(idle, d.x, z))
                        } else {
                            (idle, 0, origin(idle, d.x, 0.0))
                        }
                    })
                }
                Prize::GreenRupee | Prize::BlueRupee => {
                    let name = if matches!(d.prize, Prize::GreenRupee) {
                        "green_rupee_idle"
                    } else {
                        "blue_rupee_idle"
                    };
                    sprites
                        .extra(name)
                        .map(|c| (c, shine_frame(t), origin(c, d.x, 0.0)))
                }
            };
            if let Some((clip, frame, at)) = shown
                && !blink
            {
                draw(clip, frame, at, Color32::WHITE);
            }
        }

        let Some(e) = &self.enemy else {
            return;
        };
        let name = e.kind.name;
        let strip = |anim: &str| sprites.extra(&format!("{name}_{anim}"));
        let side_name = if e.dir == Dir::Left { "left" } else { "right" };
        let walk = if e.kind.flies {
            strip("fly")
        } else if e.kind.sided {
            strip(&format!("walk_{side_name}"))
        } else {
            strip("walk_down")
        };
        let Some(walk) = walk else {
            return;
        };
        let at = origin(walk, e.x, e.z);
        let t = now - e.since;
        let gf = (t / GAME_FRAME as f64) as usize;
        let age = ((now - e.born) / GAME_FRAME as f64) as usize;
        let walk_frame = if e.kind.flies {
            age / 4 % 2
        } else {
            age / 8 % 4
        };
        let flash = |clip: &Clip, frame: usize| {
            // Strips without a `hit` strip flash with a tint.
            match strip("hit").filter(|_| !e.kind.sided) {
                Some(hit) => draw(hit, gf % 8, at, Color32::WHITE),
                None => draw(clip, frame, at, FLASH[gf % FLASH.len()]),
            }
        };
        match e.state {
            State::Appear => {
                // The cloud of the poof, backwards, then the enemy.
                if let Some(poof) = sprites.extra("poof_poof") {
                    let i = (t / (APPEAR_SECONDS / 4.0)) as usize;
                    draw(poof, 7 - i.min(3), at, Color32::WHITE);
                }
            }
            State::Walk { .. } => draw(walk, walk_frame, at, Color32::WHITE),
            State::Rest { .. } => draw(
                walk,
                if e.kind.flies { walk_frame } else { 0 },
                at,
                Color32::WHITE,
            ),
            State::Shoot { .. } => {
                let mut left = t as f32 / GAME_FRAME;
                let mut frame = 0;
                for (f, d) in SHOOT_FRAMES {
                    frame = f;
                    if left < d {
                        break;
                    }
                    left -= d;
                }
                match strip(&format!("shoot_{side_name}")) {
                    Some(shoot) => draw(shoot, frame, at, Color32::WHITE),
                    None => draw(walk, 0, at, Color32::WHITE),
                }
            }
            State::Shock => match strip("shock") {
                Some(shock) => draw(shock, gf / 2 % 16, at, Color32::WHITE),
                None => draw(walk, 0, at, Color32::WHITE),
            },
            State::Hurt { .. } => flash(walk, walk_frame),
            State::Dying { flee } => {
                let poof_t = t - DEATH_FLASH_SECONDS;
                // The body stays under the poof while the poof is small.
                if poof_t < 6.0 * POOF_FRAME {
                    if flee {
                        draw(walk, walk_frame, at, Color32::WHITE);
                    } else {
                        flash(walk, walk_frame);
                    }
                }
                if poof_t >= 0.0
                    && let Some(poof) = sprites.extra("poof_poof")
                {
                    draw(poof, (poof_t / POOF_FRAME) as usize, at, Color32::WHITE);
                }
            }
        }
    }
}

/// A random drop of a defeated enemy, or none.
fn random_prize() -> Option<Prize> {
    match fastrand::u32(..10) {
        0..=2 => Some(Prize::Heart),
        3..=5 => Some(Prize::GreenRupee),
        6 => Some(Prize::BlueRupee),
        _ => None,
    }
}

/// The frame of the shine of a rupee after `t` game frames.
fn shine_frame(t: f32) -> usize {
    let total: f32 = RUPEE_SHINE.iter().map(|s| s.1).sum();
    let mut left = t % total;
    for (frame, d) in RUPEE_SHINE {
        if left < d {
            return frame;
        }
        left -= d;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_enemy_has_its_strips() {
        let sprites = Sprites::load(&egui::Context::default(), 0);
        for kind in DAY_KINDS.iter().chain(std::iter::once(&KEESE)) {
            let anims: &[&str] = if kind.flies {
                &["fly", "hit"]
            } else if kind.sided {
                &["walk_left", "walk_right"]
            } else {
                &["walk_down", "hit"]
            };
            for anim in anims {
                let name = format!("{}_{anim}", kind.name);
                assert!(sprites.extra(&name).is_some(), "missing {name}");
            }
        }
        for name in [
            "octorok_shoot_left",
            "octorok_shoot_right",
            "octorok_stone_idle",
            "octorok_stone_crumble",
            "buzzblob_shock",
            "poof_poof",
            "heart_idle",
            "heart_fall",
            "green_rupee_idle",
            "blue_rupee_idle",
        ] {
            assert!(sprites.extra(name).is_some(), "missing {name}");
        }
    }

    const AREA: Area = Area {
        left: 0.0,
        top: 0.0,
        right: 3000.0,
        bottom: 1000.0,
    };

    fn quiet_config() -> Config {
        Config {
            walk: false,
            actions: false,
            hops: false,
            chase: false,
            idle_sleep: false,
            ..Config::default()
        }
    }

    /// Runs Link and the fight for `secs` seconds at 60 frames per second.
    fn run(
        fight: &mut Fight,
        pal: &mut Pal,
        sprites: &Sprites,
        cfg: &Config,
        from: f64,
        secs: f64,
    ) {
        let dt = 1.0 / 60.0;
        let senses = crate::pal::Senses::default();
        let mut now = from;
        while now < from + secs {
            pal.update(now, dt as f32, AREA, cfg, sprites, true, &senses);
            fight.update(now, dt as f32, pal, sprites, cfg, AREA, true, false);
            now += dt;
        }
    }

    fn enemy_at(kind: &'static Kind, x: f32, now: f64) -> Enemy {
        Enemy {
            kind,
            x,
            z: 0.0,
            dir: Dir::Left,
            state: State::Appear,
            since: now,
            hp: kind.hp,
            shocked: false,
            born: now,
        }
    }

    #[test]
    fn link_defeats_every_enemy() {
        let sprites = Sprites::load(&egui::Context::default(), 0);
        let cfg = quiet_config();
        let scale = cfg.scale as f32;
        for kind in DAY_KINDS.iter().chain(std::iter::once(&KEESE)) {
            let mut pal = Pal::new(pos2(1200.0, 0.0));
            let mut fight = Fight {
                next_spawn: f64::INFINITY,
                ..Fight::default()
            };
            // Link lands on the ground.
            run(&mut fight, &mut pal, &sprites, &cfg, 0.0, 3.0);
            let x = pal.center_x(cfg.scale) + 120.0 * scale;
            fight.enemy = Some(enemy_at(kind, x, 3.0));
            run(&mut fight, &mut pal, &sprites, &cfg, 3.0, 30.0);
            assert!(fight.enemy.is_none(), "{} survives", kind.name);
        }
    }

    #[test]
    fn the_shield_stops_a_stone_from_the_front() {
        let sprites = Sprites::load(&egui::Context::default(), 0);
        let cfg = quiet_config();
        let scale = cfg.scale as f32;
        for (facing, blocked) in [(Dir::Right, true), (Dir::Left, false)] {
            let mut pal = Pal::new(pos2(1200.0, 0.0));
            let mut fight = Fight {
                next_spawn: f64::INFINITY,
                ..Fight::default()
            };
            run(&mut fight, &mut pal, &sprites, &cfg, 0.0, 3.0);
            pal.stand_facing(facing, 100.0);
            fight.stones.push(Stone {
                x: pal.center_x(cfg.scale) + 40.0 * scale,
                vx: -STONE_SPEED * scale,
                crumble: None,
            });
            run(&mut fight, &mut pal, &sprites, &cfg, 3.0, 0.5);
            assert!(fight.stones.iter().all(|s| s.crumble.is_some()));
            assert_eq!(pal.is_vulnerable(3.5), blocked, "facing {facing:?}");
        }
    }

    #[test]
    fn the_rupee_shine_loops() {
        assert_eq!(shine_frame(0.0), 0);
        assert_eq!(shine_frame(14.0), 1);
        assert_eq!(shine_frame(54.0), 0);
    }
}
