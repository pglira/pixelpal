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
    pub then: Then,
}

const fn action(name: &'static str, dirs: &'static [Dir], parts: &'static [Part]) -> Action {
    Action {
        name,
        dirs,
        parts,
        then: Then::Stand,
    }
}

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
    Action {
        then: Then::DropFromAbove,
        ..action("fall_in_pit", FRONT, &[Seq("fall_in_pit", "fall", Once)])
    },
];

/// One frame of a scene.
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub anim: &'static str,
    pub dir: Dir,
    pub frame: usize,
    pub secs: f32,
}

/// A random direction of `action`.
pub fn pick_dir(action: &Action) -> Dir {
    action.dirs[fastrand::usize(..action.dirs.len())]
}

/// The frames of `action` in direction `dir`. Returns `None` if the sprites
/// lack an animation or a sequence of the action.
pub fn scene(action: &Action, dir: Dir, sprites: &Sprites) -> Option<Vec<Shot>> {
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
        let sprites = Sprites::load(&egui::Context::default());
        for action in ACTIONS {
            for &dir in action.dirs {
                let shots = scene(action, dir, &sprites);
                assert!(
                    shots.is_some_and(|s| !s.is_empty()),
                    "action {} lacks sprites for {dir:?}",
                    action.name
                );
            }
        }
    }
}
