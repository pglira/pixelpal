//! Actions that the pal plays between walks. Each action is a short scene
//! made from the animations of the game.

use crate::link::{Dir, GAME_FRAME, Sprites};

/// Duration of a frame that the game holds until an event, in seconds.
const HOLD: f32 = 1.2;

/// The sequence name that selects the sequence of the action's direction.
const BY_DIR: &str = "@dir";

const SIDES: &[Dir] = &[Dir::Left, Dir::Right];
const FRONT: &[Dir] = &[Dir::Down];
const ANY: &[Dir] = &[Dir::Down, Dir::Left, Dir::Right];

#[derive(Clone, Copy)]
enum Repeat {
    Once,
    Times(u32),
    /// Plays a looping sequence, or repeats another sequence, for at least
    /// this many seconds.
    For(f32),
}

#[derive(Clone, Copy)]
enum Part {
    /// A sequence of an animation.
    Seq(&'static str, &'static str, Repeat),
    /// One frame of an animation for a number of seconds.
    Pose(&'static str, usize, f32),
}

use Part::{Pose, Seq};
use Repeat::{For, Once, Times};

/// The standing frame for a number of seconds.
const fn stand(secs: f32) -> Part {
    Pose("walk", 0, secs)
}

/// What happens after the last frame of an action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Then {
    Stand,
    /// The pal drops from above the screen, as after a fall into a pit.
    DropFromAbove,
}

pub struct Action {
    pub name: &'static str,
    dirs: &'static [Dir],
    parts: &'static [Part],
    /// Builds the scene in code, in place of `parts`.
    custom: Option<SceneBuilder>,
    pub then: Then,
}

/// Builds the shots of a scene in one direction; `None` if sprites are missing.
type SceneBuilder = fn(Dir, &Sprites) -> Option<Vec<Shot>>;

const fn action(name: &'static str, dirs: &'static [Dir], parts: &'static [Part]) -> Action {
    Action {
        name,
        dirs,
        parts,
        custom: None,
        then: Then::Stand,
    }
}

const fn custom(name: &'static str, build: SceneBuilder) -> Action {
    Action {
        name,
        dirs: SIDES,
        parts: &[],
        custom: Some(build),
        then: Then::Stand,
    }
}

/// Names of the actions that answer a middle click on the pal. None of them
/// moves the pal away.
pub const REACTIONS: &[&str] = &["electrocuted", "spin_attack", "swoon", "sword"];

/// Names of the calm actions for the night.
pub const NIGHT_ACTIONS: &[&str] = &["doze", "sleep", "read", "pray"];

/// The action that answers a tool call of Claude, by the tool name.
pub fn tool_reaction(tool: &str) -> &'static str {
    match tool {
        "Read" | "Grep" | "Glob" | "LS" | "NotebookRead" => "read",
        "Edit" | "MultiEdit" | "NotebookEdit" => "shovel",
        "Write" => "cane_of_somaria",
        "Bash" | "BashOutput" | "KillShell" => "hammer",
        "WebSearch" => "bug_net",
        "WebFetch" => "hookshot",
        "Task" | "Agent" => "magic_powder",
        "TodoWrite" => "item_get",
        _ => "boomerang",
    }
}

/// The action that answers a failed tool call.
pub const TOOL_ERROR_REACTION: &str = "electrocuted";

/// The action of a level up.
pub const LEVEL_UP: &str = "master_sword";

/// The action of waking up after an idle sleep.
pub const WAKE_UP: &str = "wake_up";

/// The action at the end of a chase of the mouse pointer.
pub const CATCH: &str = "catch";

#[rustfmt::skip]
pub const ACTIONS: &[Action] = &[
    action("sword", SIDES, &[
        Seq("sword_swing", "swing", Once), stand(0.3),
        Seq("sword_swing", "swing", Once), stand(0.15),
        Seq("sword_swing", "swing", Once), stand(0.6),
    ]),
    action("spin_attack", ANY, &[
        Pose("walk_sword_charged", 0, 1.0),
        Seq("spin_attack", BY_DIR, Once), stand(0.6),
    ]),
    action("sword_poke", SIDES, &[Seq("sword_poke_wall", "poke", For(1.2)), stand(0.4)]),
    action("swim", SIDES, &[
        Seq("swim_move", "swim", For(2.5)),
        Seq("swim_stroke", "stroke", Times(2)),
        Seq("swim_idle", "tread", For(2.0)), stand(0.4),
    ]),
    action("bow", SIDES, &[
        Seq("bow", "shoot", Once), stand(0.5),
        Seq("bow", "shoot", Once), stand(0.6),
    ]),
    action("boomerang", SIDES, &[Seq("boomerang", "throw", Once), stand(1.5)]),
    action("hammer", SIDES, &[Seq("rod_or_hammer", "hammer", Times(3)), stand(0.5)]),
    action("hookshot", SIDES, &[Pose("hookshot", 0, 1.2), stand(0.4)]),
    action("magic_powder", SIDES, &[Seq("magic_powder", "sprinkle", Once), stand(0.6)]),
    action("bug_net", SIDES, &[
        Seq("bug_net", BY_DIR, Once), stand(0.3),
        Seq("bug_net", BY_DIR, Once), stand(0.5),
    ]),
    action("shovel", SIDES, &[Seq("shovel", "dig", Times(3)), stand(0.5)]),
    action("cane_of_somaria", SIDES, &[Seq("cane", "somaria", Times(2)), stand(0.5)]),
    action("cane_of_byrna", SIDES, &[Seq("cane", "byrna", Once), stand(0.6)]),
    action("ether", FRONT, &[Seq("medallion", "ether", For(2.5)), stand(0.5)]),
    action("bombos", FRONT, &[Seq("medallion", "bombos", Once), stand(0.5)]),
    action("quake", FRONT, &[Seq("medallion", "quake", Once), stand(0.5)]),
    action("electrocuted", ANY, &[Seq("electrocuted", "shock", Times(3)), stand(0.6)]),
    action("push", SIDES, &[Seq("push", "push", For(2.0)), stand(0.4)]),
    action("pull", SIDES, &[
        Seq("grab_pull", "grab", Once),
        Seq("grab_pull", "pull", For(2.0)),
        Seq("pull_fall_back", "fall", Once), stand(0.5),
    ]),
    action("pray", FRONT, &[Seq("desert_prayer", "pray", Once), stand(0.5)]),
    action("master_sword", FRONT, &[
        Seq("pull_lever_or_master_sword", "master_sword", Once), Pose("sword_held_up", 0, 1.5),
    ]),
    action("item_get", FRONT, &[Pose("item_get_one_hand", 0, 1.5)]),
    action("sleep", FRONT, &[
        Pose("sleep_in_bed", 0, 4.0), Pose("sleep_in_bed", 1, 1.0), stand(0.3),
    ]),
    action("swoon", FRONT, &[Seq("death_swoon", "swoon", Once), stand(0.5)]),
    action("bunny", ANY, &[Seq("bunny_walk", "walk", For(2.5))]),
    action("read", &[Dir::Up], &[Pose("read_tablet", 0, 2.5), stand(0.3)]),
    action("doze", FRONT, &[
        Pose("sleep_in_bed", 0, 12.0), Pose("sleep_in_bed", 1, 1.0), stand(0.3),
    ]),
    action("wake_up", FRONT, &[Pose("sleep_in_bed", 1, 1.2), stand(0.3)]),
    action("catch", SIDES, &[Seq("bug_net", BY_DIR, Once), stand(0.4)]),
    Action {
        then: Then::DropFromAbove,
        ..action("fall_in_pit", FRONT, &[Seq("fall_in_pit", "fall", Once)])
    },
    custom("lift_rock", |dir, s| lift_and_throw("rock", dir, s)),
    custom("lift_skull", |dir, s| lift_and_throw("skull", dir, s)),
    custom("cut_bush", cut_bush),
];

/// Horizontal distance of a thrown object from Link, in sprite pixels.
const THROW_DISTANCE: f32 = 20.0;
/// Durations of the 8 debris frames, in game frames.
const DEBRIS_FRAMES: [u32; 8] = [3, 4, 4, 4, 4, 4, 4, 3];

/// Collects the shots of a custom scene.
struct Builder<'a> {
    sprites: &'a Sprites,
    dir: Dir,
    shots: Vec<Shot>,
}

impl Builder<'_> {
    /// Adds a frame of Link, for `frames` game frames, with an optional prop.
    fn add(
        &mut self,
        anim: &'static str,
        frame: usize,
        frames: u32,
        prop: Option<Prop>,
    ) -> Option<()> {
        let clip = self.sprites.clip(anim, self.dir)?;
        if frame >= clip.frames.len() {
            return None;
        }
        if let Some(p) = prop
            && p.frame >= self.sprites.extra(p.extra)?.frames.len()
        {
            return None;
        }
        self.shots.push(Shot {
            anim,
            dir: self.dir,
            frame,
            secs: frames as f32 * GAME_FRAME,
            prop,
        });
        Some(())
    }

    /// The frames and durations of the sequence `seq` of `anim`.
    fn seq(&self, anim: &str, seq: &str) -> Option<Vec<(usize, u32)>> {
        Some(
            self.sprites
                .clip(anim, self.dir)?
                .sequences
                .get(seq)?
                .frames
                .clone(),
        )
    }
}

fn prop(extra: &'static str, frame: usize, x: f32, y: f32) -> Option<Prop> {
    Some(Prop {
        extra,
        frame,
        at: (x, y),
    })
}

/// Link finds an object beside him, lifts it, carries it, throws it, and it
/// breaks. The offsets follow `SpriteModule_Carried` of the game.
fn lift_and_throw(obj: &'static str, dir: Dir, sprites: &Sprites) -> Option<Vec<Shot>> {
    let side = if dir == Dir::Right { 1.0 } else { -1.0 };
    // The object stands on the ground in front of Link.
    let ground_y = sprites.feet - 16.0;
    let mut b = Builder {
        sprites,
        dir,
        shots: Vec::new(),
    };
    let debris = if obj == "skull" {
        "skull_debris"
    } else {
        "rock_debris"
    };
    b.add("walk", 0, 36, prop(obj, 0, side * 17.0, ground_y))?;

    // Lift: the object rises in three stages.
    let lift_x = [13.0, 10.0, 5.0];
    let lift_y = [-4.0, -7.0, -10.0];
    for (i, (frame, d)) in b.seq("lift", "light")?.into_iter().enumerate() {
        let k = i.min(2);
        b.add(
            "lift",
            frame,
            d.max(1),
            prop(obj, 0, side * lift_x[k], lift_y[k]),
        )?;
    }

    // Carry: two steps in place, with the bob of the walk.
    let bob = [-12.0, -11.0, -10.0, -12.0, -11.0, -10.0];
    for _ in 0..2 {
        for (frame, d) in b.seq("carry_walk", "walk")? {
            b.add(
                "carry_walk",
                frame,
                d.max(1),
                prop(obj, 0, 0.0, bob[frame % 6]),
            )?;
        }
    }

    // Throw: the object flies forward in an arc and lands on the ground.
    for k in 0..=8 {
        let t = k as f32 / 8.0;
        let x = side * THROW_DISTANCE * t;
        let y = -12.0 + (ground_y + 12.0) * t - 8.0 * (std::f32::consts::PI * t).sin();
        let anim = if k < 3 { "throw" } else { "walk" };
        b.add(anim, 0, 2, prop(obj, 0, x, y))?;
    }
    for (frame, d) in DEBRIS_FRAMES.into_iter().enumerate() {
        b.add(
            "walk",
            0,
            d,
            prop(debris, frame, side * THROW_DISTANCE, ground_y),
        )?;
    }
    b.add("walk", 0, 24, None)?;
    Some(b.shots)
}

/// Link cuts a bush beside him with the sword; the leaves fly.
fn cut_bush(dir: Dir, sprites: &Sprites) -> Option<Vec<Shot>> {
    let side = if dir == Dir::Right { 1.0 } else { -1.0 };
    let (x, y) = (side * 17.0, sprites.feet - 16.0);
    let mut b = Builder {
        sprites,
        dir,
        shots: Vec::new(),
    };
    b.add("walk", 0, 30, prop("bush", 0, x, y))?;
    // The sword reaches the bush at frame 4 of the swing; then the leaves fly.
    let mut leaves = 0;
    for (i, (frame, d)) in b.seq("sword_swing", "swing")?.into_iter().enumerate() {
        let p = if i < 4 {
            prop("bush", 0, x, y)
        } else {
            leaves += 1;
            prop("bush_debris", leaves - 1, x, y)
        };
        b.add("sword_swing", frame, d.max(1), p)?;
    }
    for (frame, d) in DEBRIS_FRAMES.into_iter().enumerate().skip(leaves) {
        b.add("walk", 0, d, prop("bush_debris", frame, x, y))?;
    }
    b.add("walk", 0, 24, None)?;
    Some(b.shots)
}

/// One frame of a scene.
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub anim: &'static str,
    pub dir: Dir,
    pub frame: usize,
    pub secs: f32,
    /// An object drawn with Link in this frame.
    pub prop: Option<Prop>,
}

/// A frame of an extra strip (see `Sprites::extra`), placed with its top-left
/// corner at `at` relative to Link's coordinate point, in sprite pixels.
#[derive(Clone, Copy, Debug)]
pub struct Prop {
    pub extra: &'static str,
    pub frame: usize,
    pub at: (f32, f32),
}

/// A random direction of `action`.
pub fn pick_dir(action: &Action) -> Dir {
    action.dirs[fastrand::usize(..action.dirs.len())]
}

/// The direction of `action` nearest to `wanted`: `wanted` itself if the
/// action allows it, else a random allowed direction.
pub fn dir_for(action: &Action, wanted: Dir) -> Dir {
    if action.dirs.contains(&wanted) {
        wanted
    } else {
        pick_dir(action)
    }
}

/// The action with the name `name`.
pub fn find(name: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.name == name)
}

/// The frames of `action` in direction `dir`. Returns `None` if the sprites
/// lack an animation or a sequence of the action.
pub fn scene(action: &Action, dir: Dir, sprites: &Sprites) -> Option<Vec<Shot>> {
    if let Some(build) = action.custom {
        return build(dir, sprites);
    }
    let mut shots = Vec::new();
    for part in action.parts {
        match *part {
            Pose(anim, frame, secs) => {
                let clip = sprites.clip(anim, dir)?;
                if frame >= clip.frames.len() {
                    return None;
                }
                shots.push(Shot {
                    anim,
                    dir,
                    frame,
                    secs,
                    prop: None,
                });
            }
            Seq(anim, seq, repeat) => {
                let clip = sprites.clip(anim, dir)?;
                let name = if seq == BY_DIR { dir.name() } else { seq };
                let s = clip.sequences.get(name)?;
                if s.frames.is_empty() || s.frames.iter().any(|&(f, _)| f >= clip.frames.len()) {
                    return None;
                }
                let shot = |&(frame, d): &(usize, u32)| Shot {
                    anim,
                    dir,
                    frame,
                    secs: if d == 0 { HOLD } else { d as f32 * GAME_FRAME },
                    prop: None,
                };
                let one: Vec<Shot> = s.frames.iter().map(shot).collect();
                match repeat {
                    Once => shots.extend(&one),
                    Times(n) => (0..n).for_each(|_| shots.extend(&one)),
                    For(secs) => {
                        let start = shots.len();
                        let total =
                            |shots: &[Shot]| shots[start..].iter().map(|s| s.secs).sum::<f32>();
                        shots.extend(&one);
                        let looped = if s.looping {
                            &one[s.loop_from.min(one.len() - 1)..]
                        } else {
                            &one[..]
                        };
                        while total(&shots) < secs {
                            shots.extend(looped);
                        }
                    }
                }
            }
        }
    }
    Some(shots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_its_sprites() {
        for tier in 0..crate::link::tier_count() {
            let sprites = Sprites::load(&egui::Context::default(), tier);
            for action in ACTIONS {
                for &dir in action.dirs {
                    let shots = scene(action, dir, &sprites);
                    assert!(
                        shots.is_some_and(|s| !s.is_empty()),
                        "action {} lacks sprites for {dir:?} in tier {tier}",
                        action.name
                    );
                }
            }
        }
    }

    #[test]
    fn every_reaction_is_an_action() {
        let tools = [
            "Read",
            "Edit",
            "Write",
            "Bash",
            "WebSearch",
            "WebFetch",
            "Task",
            "TodoWrite",
            "x",
        ];
        let fixed = [TOOL_ERROR_REACTION, LEVEL_UP, WAKE_UP, CATCH];
        let all = REACTIONS
            .iter()
            .chain(NIGHT_ACTIONS)
            .copied()
            .chain(tools.into_iter().map(tool_reaction))
            .chain(fixed);
        for name in all {
            assert!(find(name).is_some(), "unknown reaction {name}");
        }
    }
}
