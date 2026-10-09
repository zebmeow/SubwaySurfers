//! The characters' own idles in the menus' 3D views (`_v.setup3D`,
//! deobfuscated.js:58122): each character's `idle-<id>.pk`
//! (`animations-character-idle`) is cut into a breathe clip and gesture
//! "flavors" (`Ep`, 39839; `data/character_idles.json`), played on the
//! avatar's own skeleton by the idle director `ov` (57480): a random
//! gesture, then one breathe, then another gesture, never cross-faded
//! (`allowMixing = false`). Props (`H_`, 57046): scene nodes such as
//! `Jake_sandwich` moved under an animated attach point.
//!
//! A character without idle data (none in this build) gets `idle.pk`'s
//! frames 0-40 at 30 fps, looped (`addAnimatedScene` `uiIdle`).

use crate::anim::{Action, Animator, ClipLib};
use crate::skin::{SkinModel, Trs};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

#[derive(Clone, Debug, Deserialize)]
pub struct Flavor {
    pub name: String,
    pub frames: [f64; 2],
}

#[derive(Clone, Debug, Deserialize)]
pub struct Idle {
    pub source: String,
    pub fps: f64,
    pub breathe: [f64; 2],
    #[serde(default)]
    pub flavors: Vec<Flavor>,
}

/// `H_`'s prop: `node` moved under `attach` (identity local); it hides the
/// `replaces` nodes while shown, and shows only on `outfits` if given.
#[derive(Clone, Debug, Deserialize)]
pub struct PropDef {
    pub node: String,
    pub attach: String,
    #[serde(default)]
    pub replaces: Vec<String>,
    #[serde(default)]
    pub outfits: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CharIdle {
    pub idle: Option<Idle>,
    #[serde(default)]
    pub props: Vec<PropDef>,
}

/// `Ep`: the characters' idle data.
pub fn table() -> &'static HashMap<String, CharIdle> {
    static T: std::sync::OnceLock<HashMap<String, CharIdle>> = std::sync::OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("../data/character_idles.json")).expect("character_idles.json"))
}

/// `Dp` / `Op`.
pub fn idle(id: &str) -> Option<&'static Idle> {
    table().get(id).and_then(|c| c.idle.as_ref())
}
pub fn props(id: &str) -> &'static [PropDef] {
    table().get(id).map_or(&[], |c| c.props.as_slice())
}

/// A small generator for the director's `Math.random()` (the menus are
/// not part of the simulation).
#[derive(Clone, Debug)]
struct Pick(u64);
impl Pick {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64 * n as f64) as usize
    }
}

/// One character's idle: its clips on its own skeleton and the director.
pub struct IdlePlayer {
    animator: Animator,
    breathe: usize,
    /// Gestures longer than 0.05 s (`addCharacterIdleScene`).
    flavors: Vec<usize>,
    /// The playing clip and its time; whether it is a gesture.
    current: usize,
    pub time: f64,
    gesture: bool,
    /// No director: the clip loops (`uiIdle`).
    looping: bool,
    pick: Pick,
}

impl IdlePlayer {
    /// The clips of `id`'s idle cut for `model` (`Y_`: tracks of the nodes
    /// the avatar has, `Root.scale` normalized); the director starts with a
    /// gesture (`ov.start`).
    pub fn new(site: &Path, model: &SkinModel, id: &str, seed: u64) -> Result<Self, String> {
        let targets: HashSet<String> = model.nodes.iter().map(|n| n.name.clone()).collect();
        let (src, looping) = match idle(id) {
            Some(i) => {
                let mut clips = serde_json::Map::new();
                clips.insert("breathe".into(), json!({ "frames": i.breathe }));
                for f in &i.flavors {
                    clips.insert(f.name.clone(), json!({ "frames": f.frames }));
                }
                (json!({ "file": i.source, "fps": i.fps, "clips": Value::Object(clips) }), false)
            }
            None => (json!({ "file": "idle", "fps": 30.0, "clips": { "breathe": { "frames": [0.0, 40.0] } } }), true),
        };
        let lib = Rc::new(ClipLib::build(site, &[&src], "", &targets)?);
        let breathe = *lib.by_name.get("breathe").ok_or(format!("{id}: no breathe clip"))?;
        let flavors: Vec<usize> = match idle(id) {
            Some(i) => i.flavors.iter().filter_map(|f| lib.by_name.get(&f.name).copied()).filter(|&c| lib.clips[c].duration > 0.05).collect(),
            None => Vec::new(),
        };
        let animator = Animator::new(lib, model, true, "");
        let mut p = Self { animator, breathe, flavors, current: breathe, time: 0.0, gesture: false, looping, pick: Pick(seed | 1) };
        p.play_gesture();
        Ok(p)
    }

    /// `ov.playGesture`: a random flavor (none: breathe).
    fn play_gesture(&mut self) {
        if self.flavors.is_empty() {
            self.play_breathe();
            return;
        }
        self.current = self.flavors[self.pick.below(self.flavors.len())];
        self.gesture = true;
        self.time = 0.0;
    }
    fn play_breathe(&mut self) {
        self.current = self.breathe;
        self.gesture = false;
        self.time = 0.0;
    }

    /// Advance by `dt` seconds (speed 1); at a clip's end the director
    /// plays the other kind (`onComplete`).
    pub fn advance(&mut self, dt: f64) {
        self.time += dt;
        let dur = self.animator.lib.clips[self.current].duration;
        if self.time >= dur {
            if self.looping {
                self.time = self.time.rem_euclid(dur.max(1e-6));
            } else if self.gesture {
                self.play_breathe();
            } else {
                self.play_gesture();
            }
        }
    }

    /// The playing clip's name.
    pub fn clip(&self) -> &str {
        &self.animator.lib.clips[self.current].name
    }

    /// The avatar's pose: bone locals (per joint) and node locals.
    pub fn pose(&mut self, model: &SkinModel) -> (Vec<Trs>, Vec<Trs>) {
        let c = self.current;
        let mut a = Action::new_public(c);
        a.time = self.time.min(self.animator.lib.clips[c].duration);
        self.animator.actions.clear();
        self.animator.actions.insert(c, a);
        self.animator.active = vec![c];
        self.animator.pose(model)
    }
}

/// `H_.update`'s visibility of each prop (by `props(id)` index) and the
/// nodes they replace, for `outfit`; `bones`: the skeleton's world
/// matrices, `attach`: each prop's attach point world (None: missing).
pub fn prop_visibility(id: &str, outfit: usize, board_mode: bool, bones: &[bevy::math::DMat4], attach: &[Option<bevy::math::DMat4>]) -> (Vec<bool>, HashSet<String>) {
    // farThreshold: 10x the skeleton's reach from its root bone, if over 1
    let root = bones.first().map(|m| m.w_axis.truncate());
    let reach = root.map_or(0.0, |r| bones.iter().map(|m| m.w_axis.truncate().distance(r)).fold(0.0, f64::max));
    let far = if reach > 1.0 { reach * 10.0 } else { 0.0 };
    let mut shown = Vec::new();
    let mut hidden = HashSet::new();
    for (p, at) in props(id).iter().zip(attach) {
        let Some(at) = at else {
            shown.push(false);
            continue;
        };
        if !p.outfits.is_empty() && !p.outfits.contains(&outfit) {
            shown.push(false);
            continue;
        }
        let mut v = true;
        if let (true, Some(r)) = (far > 0.0, root) {
            v = at.w_axis.truncate().distance(r) <= far;
        }
        if v && !p.replaces.is_empty() {
            v = !board_mode;
        }
        if v {
            hidden.extend(p.replaces.iter().cloned());
        }
        shown.push(v);
    }
    (shown, hidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_character_has_idle_data() {
        let t = table();
        assert_eq!(t.len(), 40);
        let j = idle("jake").unwrap();
        assert_eq!((j.source.as_str(), j.fps, j.breathe), ("idle-jake", 25.0, [1.0, 93.0]));
        assert_eq!(j.flavors.len(), 4);
        assert_eq!(props("jake")[0].node, "Jake_sandwich");
        assert_eq!(props("boombot").last().unwrap().replaces, vec!["Boomie_head", "Boomie_Idle_head"]);
    }
}
