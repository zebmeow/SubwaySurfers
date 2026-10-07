//! Skeletal animation, as the original drives it (docs/js_notes/
//! actors_animation.md): `.pk` master clips cut into named sub-clips
//! (`addSharedSourceAnimations` 24601, `ot.clipKeyframeTrack` RB:2191), one
//! three.js `AnimationMixer` per actor (actions, linear cross-fades with f32
//! fade endpoints, `LoopOnce`+clamp / `LoopRepeat`, property-mixer blending
//! toward the rest pose), and the game's `Zo` / `X` controller on top
//! (`play` 24856, `update` 24994, `X.play` RB:2318).
//!
//! The three.js routines follow the build the game ships
//! (extracted/three-dhrglqPP.js): `Interpolant.evaluate`, `LinearInterpolant`,
//! `QuaternionLinearInterpolant` (slerpFlat), `PropertyMixer.accumulate/
//! apply`, `AnimationAction._updateTime/_updateWeight/_scheduleFading`.

use crate::pk::{self, Channel, RawTrack};
use crate::rng::{Rng, Site};
use crate::skin::{SkinModel, Trs};
use bevy::math::{DQuat, DVec3};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

/// `Xo.speed`: the speed field of a clip that was never current.
pub const DEFAULT_CLIP_SPEED: f64 = 0.015;
/// `config.baseAnimSpeed`.
pub const BASE_ANIM_SPEED: f64 = 1.0 / 60.0;
/// `X._defaultMix` (0.045) at `_gameFps` 60: cross-fade length in mixer seconds.
pub const FADE: f64 = 1.0 / 0.045 / 60.0;

// ---- three.js math ------------------------------------------------------------------

/// `Quaternion.slerpFlat` (this three build: no t = 0 / 1 shortcuts).
pub fn slerp_flat(a: [f64; 4], b: [f64; 4], t: f64) -> [f64; 4] {
    let [mut s0, mut c0, mut l0, mut u0] = a;
    let [mut d, mut f, mut p, mut m] = b;
    if u0 != m || s0 != d || c0 != f || l0 != p {
        let mut e = s0 * d + c0 * f + l0 * p + u0 * m;
        if e < 0.0 {
            d = -d;
            f = -f;
            p = -p;
            m = -m;
            e = -e;
        }
        let mut s = 1.0 - t;
        let mut o = t;
        if e < 0.9995 {
            let n = e.acos();
            let r = n.sin();
            s = (s * n).sin() / r;
            o = (o * n).sin() / r;
            s0 = s0 * s + d * o;
            c0 = c0 * s + f * o;
            l0 = l0 * s + p * o;
            u0 = u0 * s + m * o;
        } else {
            s0 = s0 * s + d * o;
            c0 = c0 * s + f * o;
            l0 = l0 * s + p * o;
            u0 = u0 * s + m * o;
            let k = 1.0 / (s0 * s0 + c0 * c0 + l0 * l0 + u0 * u0).sqrt();
            s0 *= k;
            c0 *= k;
            l0 *= k;
            u0 *= k;
        }
    }
    [s0, c0, l0, u0]
}

/// A keyframe track with three's linear interpolants (f32 storage).
#[derive(Clone, Debug)]
pub struct Track {
    pub target: String,
    pub channel: Channel,
    pub times: Vec<f32>,
    pub values: Vec<f32>,
}

impl Track {
    fn size(&self) -> usize {
        self.channel.size()
    }
    fn key(&self, i: usize) -> [f64; 4] {
        let n = self.size();
        let mut v = [0.0; 4];
        for (k, x) in self.values[i * n..i * n + n].iter().enumerate() {
            v[k] = *x as f64;
        }
        v
    }
    /// `Interpolant.evaluate(t)`: clamp outside the keys, else the interval
    /// with `times[i-1] <= t < times[i]` (f64 result).
    pub fn sample(&self, t: f64) -> [f64; 4] {
        let ts = &self.times;
        let last = ts.len() - 1;
        if t < ts[0] as f64 {
            return self.key(0);
        }
        if !(t < ts[last] as f64) {
            return self.key(last);
        }
        // first index with times[i] > t
        let i = ts.partition_point(|&x| x as f64 <= t);
        let (t0, t1) = (ts[i - 1] as f64, ts[i] as f64);
        let alpha = (t - t0) / (t1 - t0);
        let (a, b) = (self.key(i - 1), self.key(i));
        match self.channel {
            Channel::Rotation => slerp_flat(a, b, alpha),
            _ => {
                let w0 = 1.0 - alpha;
                let mut r = [0.0; 4];
                for k in 0..self.size() {
                    r[k] = a[k] * w0 + b[k] * alpha;
                }
                r
            }
        }
    }
    /// The same, stored into a `Float32Array` result.
    fn sample_f32(&self, t: f64) -> Vec<f32> {
        self.sample(t)[..self.size()].iter().map(|&x| x as f32).collect()
    }
    /// `ot.clipKeyframeTrack(track, start, end)` (RB:2219).
    fn cut(&self, start: f64, end: f64) -> Option<Track> {
        if self.times.is_empty() || end <= start {
            return None;
        }
        let n = self.size();
        let mut times: Vec<f64> = Vec::new();
        let mut values: Vec<f32> = Vec::new();
        let push = |e: f64, v: &[f32], times: &mut Vec<f64>, values: &mut Vec<f32>| {
            let r = (e - start).max(0.0);
            match times.last() {
                Some(&s) if (s - r).abs() <= 1e-5 => {
                    let l = values.len() - n;
                    values.truncate(l);
                }
                _ => times.push(r),
            }
            values.extend_from_slice(v);
        };
        push(start, &self.sample_f32(start), &mut times, &mut values);
        for (k, &tk) in self.times.iter().enumerate() {
            let tk = tk as f64;
            if tk <= start || tk >= end {
                continue;
            }
            push(tk, &self.values[k * n..k * n + n], &mut times, &mut values);
        }
        push(end, &self.sample_f32(end), &mut times, &mut values);
        Some(Track { target: self.target.clone(), channel: self.channel, times: times.iter().map(|&t| t as f32).collect(), values })
    }
}

/// A named sub-clip (`ot` + its three `AnimationClip`).
#[derive(Clone, Debug)]
pub struct Clip {
    pub name: String,
    /// `end - start` (f64).
    pub duration: f64,
    pub tracks: Vec<Track>,
    /// Initial `speed` field (`source.speed || Xo.speed`).
    pub speed: f64,
}

/// All clips of one actor, in insertion order.
#[derive(Clone, Debug, Default)]
pub struct ClipLib {
    pub clips: Vec<Clip>,
    pub by_name: HashMap<String, usize>,
}

/// `isUnboundSharedHelperTrack` (24782).
const HELPERS: [&str; 4] = ["blendshape_meta", "eye_switch_meta_grp", "maya_eye_focus_point", "maya_eye_focus_point_grp"];

/// Where an animation source file lives under the site.
fn source_path(site: &Path, file: &str) -> std::path::PathBuf {
    let dir = if file == "idle" { "animations-idle" } else { "animations-basic" };
    site.join("assets").join(dir).join(format!("{file}.pk"))
}

impl ClipLib {
    /// `addSharedScene` / `addSharedAvatar`: cut every source's clips.
    /// `prefix` is the character (`jake-`) or empty (guard, dog); `targets`
    /// are the scene's node names (`getMixerTargetNames`).
    pub fn build(site: &Path, sources: &[&Value], prefix: &str, targets: &HashSet<String>) -> Result<Self, String> {
        let mut lib = ClipLib::default();
        for src in sources {
            let file = src["file"].as_str().ok_or("anim: source without file")?;
            let fps = src["fps"].as_f64().unwrap_or(24.0);
            let speed = src["speed"].as_f64().filter(|&s| s != 0.0).unwrap_or(DEFAULT_CLIP_SPEED);
            let bytes = std::fs::read(source_path(site, file)).map_err(|e| format!("anim {file}: {e}"))?;
            let pkf = pk::parse(&bytes)?;
            let anims = pkf.animations();
            // getMasterAnimation: "Take 001" | "Scene" | first
            let master = anims
                .iter()
                .find(|a| a.name == "Take 001")
                .or_else(|| anims.iter().find(|a| a.name == "Scene"))
                .or(anims.first())
                .ok_or(format!("anim {file}: no master animation"))?;
            let clips = src["clips"].as_object().ok_or("anim: no clips")?;
            let max_end = clips.values().filter_map(|c| c["frames"][1].as_f64()).map(|f| f / fps).fold(f64::MIN, f64::max);
            // getAnimationDuration: max(duration, last key of every track)
            let mut dur = master.duration;
            for t in &master.tracks {
                dur = dur.max(*t.times.last().unwrap() as f64);
            }
            let src_dur = if file == "idle" && dur > 0.0 { dur } else { dur.max(max_end) };
            // filterClipTargets + normalizeRigRootScale
            let keep = |t: &&RawTrack| !HELPERS.contains(&t.target.as_str());
            let mut tracks: Vec<RawTrack> = master.tracks.iter().filter(keep).filter(|t| targets.contains(&t.target)).cloned().collect();
            if tracks.is_empty() {
                tracks = master.tracks.iter().filter(keep).cloned().collect();
            }
            normalize_root_scale(&mut tracks);
            let tracks: Vec<Track> = tracks
                .into_iter()
                .map(|t| Track { target: t.target, channel: t.channel, times: t.times, values: t.values })
                .collect();
            for (name, c) in clips {
                let full = format!("{prefix}{name}");
                if lib.by_name.contains_key(&full) {
                    continue;
                }
                let start = c["frames"][0].as_f64().unwrap_or(0.0) / fps;
                let end = (c["frames"][1].as_f64().unwrap_or(0.0) / fps).min(src_dur);
                if end <= start {
                    continue;
                }
                let cut: Vec<Track> = tracks.iter().filter_map(|t| t.cut(start, end)).collect();
                if cut.is_empty() {
                    continue;
                }
                lib.by_name.insert(full.clone(), lib.clips.len());
                lib.clips.push(Clip { name: full, duration: end - start, tracks: cut, speed });
            }
        }
        Ok(lib)
    }
}

/// `normalizeRigRootScale` (24749): `Root.scale` (and `Root.position`)
/// rescaled so the first scale value is 0.01.
fn normalize_root_scale(tracks: &mut [RawTrack]) {
    let Some(s) = tracks.iter().find(|t| t.target == "Root" && t.channel == Channel::Scale).and_then(|t| t.values.first().copied()) else {
        return;
    };
    if s == 0.0 || ((s as f64) - 0.01).abs() < 0.000001 {
        return;
    }
    let k = 0.01 / s as f64;
    for t in tracks.iter_mut().filter(|t| t.target == "Root" && (t.channel == Channel::Scale || t.channel == Channel::Position)) {
        for v in &mut t.values {
            *v = ((*v as f64) * k) as f32;
        }
    }
}

// ---- the mixer -----------------------------------------------------------------------

/// Where a track's values go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Bone(usize),
    Node(usize),
    Unbound,
}

/// An `AnimationAction`.
#[derive(Clone, Debug)]
pub struct Action {
    pub clip: usize,
    pub time: f64,
    pub enabled: bool,
    pub paused: bool,
    /// `_effectiveWeight` (stale after stop/reset until the next update).
    pub eff_weight: f64,
    /// Fade: f32 parameter positions and values (`_weightInterpolant`).
    pub fade: Option<([f32; 2], [f32; 2])>,
    pub looping: bool,
    pub loop_count: i64,
}

impl Action {
    /// A fresh action (`clipAction`): time 0, weight 1, enabled.
    pub fn new_public(clip: usize) -> Self {
        Self::new(clip)
    }
    fn new(clip: usize) -> Self {
        Self { clip, time: 0.0, enabled: true, paused: false, eff_weight: 1.0, fade: None, looping: false, loop_count: -1 }
    }
    fn reset(&mut self) {
        self.paused = false;
        self.enabled = true;
        self.time = 0.0;
        self.loop_count = -1;
        self.fade = None;
    }
    /// `_updateTime` (LoopOnce + clampWhenFinished / LoopRepeat).
    fn update_time(&mut self, duration: f64, dt: f64) -> f64 {
        let mut t = self.time + dt;
        if dt == 0.0 {
            return t;
        }
        if !self.looping {
            if self.loop_count == -1 {
                self.loop_count = 0;
            }
            if t >= duration {
                t = duration;
            } else if t < 0.0 {
                t = 0.0;
            } else {
                self.time = t;
                return t;
            }
            self.paused = true;
            self.time = t;
        } else {
            if self.loop_count == -1 && dt >= 0.0 {
                self.loop_count = 0;
            }
            if t >= duration || t < 0.0 {
                let n = (t / duration).floor();
                t -= duration * n;
                self.loop_count += n.abs() as i64;
            }
            self.time = t;
        }
        t
    }
    /// `_updateWeight(mixerTime)`.
    fn update_weight(&mut self, now: f64) -> f64 {
        let mut w = 0.0;
        if self.enabled {
            w = 1.0;
            if let Some((pp, sv)) = self.fade {
                let (t0, t1) = (pp[0] as f64, pp[1] as f64);
                let v = if now < t0 {
                    sv[0]
                } else if !(now < t1) {
                    sv[1]
                } else {
                    let a = (now - t0) / (t1 - t0);
                    (sv[0] as f64 * (1.0 - a) + sv[1] as f64 * a) as f32
                };
                w *= v as f64;
                if now > t1 {
                    self.fade = None;
                    if v == 0.0 {
                        self.enabled = false;
                    }
                }
            }
        }
        self.eff_weight = w;
        w
    }
}

/// `Zo.play` options.
#[derive(Clone, Copy, Debug)]
pub struct PlayOpts {
    pub looping: bool,
    pub sudden: bool,
    pub speed: f64,
    pub restart: bool,
}

impl Default for PlayOpts {
    fn default() -> Self {
        Self { looping: false, sudden: false, speed: 1.0, restart: true }
    }
}

impl PlayOpts {
    pub fn looping() -> Self {
        Self { looping: true, ..Self::default() }
    }
    pub fn sudden(mut self) -> Self {
        self.sudden = true;
        self
    }
    pub fn speed(mut self, s: f64) -> Self {
        self.speed = s;
        self
    }
}

/// One actor's animation component (`Zo` + controller `X` + mixer).
#[derive(Clone, Debug)]
pub struct Animator {
    pub lib: Rc<ClipLib>,
    /// Track slot per clip per track.
    slots: Rc<Vec<Vec<Slot>>>,
    /// Per-clip `speed` field (stale between plays) and `loop`.
    pub clip_speed: Vec<f64>,
    pub clip_loop: Vec<bool>,
    pub actions: HashMap<usize, Action>,
    /// `_actions[0.._nActiveActions]` (activation order, swap-remove).
    pub active: Vec<usize>,
    pub mixer_time: f64,
    /// X: `animation`, `lastAnimation`, `_currentAction`, `_mixing`, `_mixRatio`.
    pub animation: Option<usize>,
    pub last_animation: Option<usize>,
    pub current_action: Option<usize>,
    pub mixing: bool,
    pub mix_ratio: f64,
    allow_mixing: bool,
    /// Zo: `currentAnimationName`, `currentAnimationClipName`, `currentAnimationLoop`, `speed`, `active`.
    pub name: String,
    pub clip_name: String,
    pub loop_flag: bool,
    pub speed: f64,
    pub active_flag: bool,
    /// `getAnimationName` prefix.
    pub prefix: String,
    /// The active actions as of the last `mixer.update` (what three applied
    /// to the bones, which keep it until the next update; see `pose_applied`).
    applied: Option<(Vec<usize>, HashMap<usize, Action>)>,
}

impl Animator {
    /// Bind clip tracks to a model: joints first, then (if `nodes`) other
    /// scene nodes (`Root`, attach points). Tracks with no target are unbound.
    pub fn new(lib: Rc<ClipLib>, model: &SkinModel, nodes: bool, prefix: &str) -> Self {
        let slots: Vec<Vec<Slot>> = lib
            .clips
            .iter()
            .map(|c| {
                c.tracks
                    .iter()
                    .map(|t| {
                        if let Some(k) = model.joint_index(&t.target) {
                            Slot::Bone(k)
                        } else if nodes {
                            model.nodes.iter().position(|n| n.name == t.target).map(Slot::Node).unwrap_or(Slot::Unbound)
                        } else {
                            Slot::Unbound
                        }
                    })
                    .collect()
            })
            .collect();
        Self {
            clip_speed: lib.clips.iter().map(|c| c.speed).collect(),
            clip_loop: vec![false; lib.clips.len()],
            lib,
            slots: Rc::new(slots),
            actions: HashMap::new(),
            active: Vec::new(),
            mixer_time: 0.0,
            animation: None,
            last_animation: None,
            current_action: None,
            mixing: false,
            mix_ratio: 1.0,
            allow_mixing: true,
            name: String::new(),
            clip_name: String::new(),
            loop_flag: false,
            speed: 1.0,
            active_flag: true,
            prefix: prefix.to_string(),
            applied: None,
        }
    }

    pub fn clip_index(&self, clip: &str) -> Option<usize> {
        self.lib.by_name.get(&format!("{}{clip}", self.prefix)).copied()
    }

    fn activate(&mut self, c: usize) {
        if !self.active.contains(&c) {
            self.active.push(c);
        }
    }
    fn stop(&mut self, c: usize) {
        if let Some(i) = self.active.iter().position(|&a| a == c) {
            self.active.swap_remove(i);
        }
        if let Some(a) = self.actions.get_mut(&c) {
            a.reset();
        }
    }
    fn schedule_fade(&mut self, c: usize, d: f64, from: f32, to: f32) {
        let now = self.mixer_time;
        let a = self.actions.get_mut(&c).unwrap();
        a.fade = Some(([now as f32, (now + d) as f32], [from, to]));
    }

    /// `mixer.update(dt)`: advance active actions (activation order).
    pub fn mixer_update(&mut self, dt: f64) {
        self.mixer_time += dt;
        let now = self.mixer_time;
        for c in self.active.clone() {
            let dur = self.lib.clips[c].duration;
            let a = self.actions.get_mut(&c).unwrap();
            if !a.enabled {
                a.update_weight(now);
                continue;
            }
            let scale = if a.paused { 0.0 } else { 1.0 };
            a.update_time(dur, dt * scale);
            a.update_weight(now);
        }
        let actions = self.active.iter().filter_map(|c| self.actions.get(c).map(|a| (*c, a.clone()))).collect();
        self.applied = Some((self.active.clone(), actions));
    }

    /// `X.updateAnimation(e)`: `mixer.update(e * animation.speed)`.
    pub fn update_animation(&mut self, e: f64) {
        let s = self.animation.map(|c| self.clip_speed[c]).unwrap_or(1.0);
        self.mixer_update(e * s);
    }

    /// `X.play(name, restart)`.
    fn x_play(&mut self, c: usize, restart: bool) {
        self.last_animation = self.animation;
        self.animation = Some(c);
        self.actions.entry(c).or_insert_with(|| Action::new(c));
        let others: Vec<usize> = self.actions.keys().copied().filter(|&k| k != c).collect();
        for k in others {
            if !(Some(k) == self.current_action && self.allow_mixing) {
                self.stop(k);
            }
        }
        let fade = if self.allow_mixing && self.last_animation.is_some() && self.current_action.is_some() { FADE } else { 0.0 };
        let looping = self.clip_loop[c];
        {
            let a = self.actions.get_mut(&c).unwrap();
            a.looping = looping;
            if restart {
                a.reset();
            }
        }
        match self.current_action {
            Some(cur) if fade > 0.0 && cur != c => {
                self.actions.get_mut(&c).unwrap().reset();
                self.activate(c);
                // current.crossFadeTo(new): fadeOut(current), fadeIn(new)
                self.schedule_fade(cur, fade, 1.0, 0.0);
                self.schedule_fade(c, fade, 0.0, 1.0);
                self.mixing = true;
            }
            _ => {
                self.actions.get_mut(&c).unwrap().reset();
                self.activate(c);
                self.mixing = false;
            }
        }
        self.current_action = Some(c);
        self.mix_ratio = 1.0;
    }

    /// `Zo.play(clip, opts)` for a single clip name.
    pub fn play(&mut self, clip: &str, o: PlayOpts) {
        let full = format!("{}{clip}", self.prefix);
        let Some(c) = self.lib.by_name.get(&full).copied() else { return };
        if self.name == full {
            return;
        }
        self.name = full;
        self.clip_loop[c] = o.looping;
        self.allow_mixing = !o.sudden;
        self.x_play(c, o.restart);
        self.speed = o.speed;
        if o.sudden {
            self.mix_ratio = 1.0;
            self.last_animation = None;
            self.mixing = false;
            self.update_animation(0.1);
        }
        self.clip_name = clip.to_string();
        self.loop_flag = o.looping;
    }

    /// `Zo.play([..])`: one `Math.random()` draw (at the caller's site),
    /// even if the pick is the current clip.
    pub fn play_any(&mut self, rng: &mut dyn Rng, site: Site, clips: &[&str], o: PlayOpts) {
        let i = (rng.random(site) * clips.len() as f64).floor() as usize;
        self.play(clips[i.min(clips.len() - 1)], o);
    }

    /// `Zo.updateCurrentAnimation`: replay the current clip, sudden.
    pub fn update_current(&mut self) {
        if !self.name.is_empty() {
            self.name.clear();
            let (clip, looping) = (self.clip_name.clone(), self.loop_flag);
            self.play(&clip, PlayOpts { looping, ..PlayOpts::default() }.sudden());
        }
    }

    /// `Zo.update(time)`: advance by `frameTime * clip.speed`, then the
    /// current clip's speed becomes `baseAnimSpeed * Zo.speed`.
    pub fn update(&mut self, frame_time: f64) {
        if !self.active_flag {
            return;
        }
        if self.animation.is_some() && frame_time != 0.0 {
            self.update_animation(frame_time);
        }
        if let Some(c) = self.animation {
            if frame_time != 0.0 {
                self.clip_speed[c] = BASE_ANIM_SPEED * self.speed;
            }
        }
    }

    /// The pose: rest values blended with every weighted active action
    /// (`PropertyMixer.accumulate` in activation order, then `apply` toward
    /// the original value when the cumulative weight is below 1).
    pub fn pose(&self, model: &SkinModel) -> (Vec<Trs>, Vec<Trs>) {
        self.pose_of(model, &self.active, &self.actions)
    }

    /// The pose the bones hold: as of the last mixer update (plays since
    /// then show from the next update; a sudden play updates at once).
    pub fn pose_applied(&self, model: &SkinModel) -> (Vec<Trs>, Vec<Trs>) {
        match &self.applied {
            Some((active, actions)) => self.pose_of(model, active, actions),
            None => (model.rest.clone(), model.nodes.iter().map(|n| n.local).collect()),
        }
    }

    fn pose_of(&self, model: &SkinModel, active: &[usize], actions: &HashMap<usize, Action>) -> (Vec<Trs>, Vec<Trs>) {
        let mut bones = model.rest.clone();
        let mut nodes: Vec<Trs> = model.nodes.iter().map(|n| n.local).collect();
        // (slot, channel) -> (accumulated value, cumulative weight)
        let mut acc: HashMap<(Slot, u8), ([f64; 4], f64)> = HashMap::new();
        let mut order: Vec<(Slot, u8)> = Vec::new();
        for &c in active {
            let a = &actions[&c];
            let w = a.eff_weight;
            if !(w > 0.0) || !a.enabled {
                continue;
            }
            let clip = &self.lib.clips[c];
            for (t, slot) in clip.tracks.iter().zip(&self.slots[c]) {
                if *slot == Slot::Unbound || t.channel == Channel::Morph {
                    continue;
                }
                let key = (*slot, t.channel as u8);
                let v = t.sample(a.time);
                match acc.get_mut(&key) {
                    None => {
                        acc.insert(key, (v, w));
                        order.push(key);
                    }
                    Some((buf, cw)) => {
                        *cw += w;
                        let mix = w / *cw;
                        *buf = mix_values(t.channel, *buf, v, mix);
                    }
                }
            }
        }
        for key in order {
            let (mut v, cw) = acc[&key];
            let channel = match key.1 {
                x if x == Channel::Position as u8 => Channel::Position,
                x if x == Channel::Rotation as u8 => Channel::Rotation,
                _ => Channel::Scale,
            };
            let target: &mut Trs = match key.0 {
                Slot::Bone(k) => &mut bones[k],
                Slot::Node(n) => &mut nodes[n],
                Slot::Unbound => continue,
            };
            if cw < 1.0 {
                let orig = match channel {
                    Channel::Position => [target.t.x, target.t.y, target.t.z, 0.0],
                    Channel::Rotation => [target.r.x, target.r.y, target.r.z, target.r.w],
                    _ => [target.s.x, target.s.y, target.s.z, 0.0],
                };
                v = mix_values(channel, v, orig, 1.0 - cw);
            }
            match channel {
                Channel::Position => target.t = DVec3::new(v[0], v[1], v[2]),
                Channel::Rotation => target.r = DQuat::from_xyzw(v[0], v[1], v[2], v[3]),
                _ => target.s = DVec3::new(v[0], v[1], v[2]),
            }
        }
        (bones, nodes)
    }

    /// The current action (for the trace's `anim` record).
    pub fn current(&self) -> Option<&Action> {
        self.current_action.and_then(|c| self.actions.get(&c))
    }
}

/// `PropertyMixer._mixBufferRegion`: lerp for vectors, slerpFlat for quaternions.
fn mix_values(ch: Channel, a: [f64; 4], b: [f64; 4], t: f64) -> [f64; 4] {
    match ch {
        Channel::Rotation => slerp_flat(a, b, t),
        _ => {
            let s = 1.0 - t;
            [a[0] * s + b[0] * t, a[1] * s + b[1] * t, a[2] * s + b[2] * t, 0.0]
        }
    }
}

/// The three actors' clip sets and models, loaded once.
pub struct ActorAssets {
    pub jake: SkinModel,
    pub guard: SkinModel,
    pub dog: SkinModel,
    pub jake_clips: Rc<ClipLib>,
    pub guard_clips: Rc<ClipLib>,
    pub dog_clips: Rc<ClipLib>,
}

impl ActorAssets {
    /// `refreshScenes` (avatar_jake, `Uo` scale 0.01) and `im.init` / `rm.init`.
    pub fn load(site: &Path, clips_json: &Path) -> Result<Self, String> {
        let a = site.join("assets");
        let jake = SkinModel::load(&a.join("characters-idle/avatar_jake.pk"), 0.01)?;
        let guard = SkinModel::load(&a.join("game-basic/model-guard.pk"), 1.0)?;
        let dog = SkinModel::load(&a.join("game-basic/model-dog.pk"), 1.0)?;
        let v: Value = serde_json::from_str(&std::fs::read_to_string(clips_json).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let names = |m: &SkinModel| -> HashSet<String> {
            let mut s: HashSet<String> = m.nodes.iter().map(|n| n.name.clone()).collect();
            s.extend(m.meshes.iter().map(|x| x.name.clone()));
            s
        };
        // sources: {idle, ...vp} for the avatar
        let mut avatar: Vec<&Value> = vec![&v["avatar"]["idle"]];
        avatar.extend(v["avatar"]["sources"].as_object().ok_or("anim_clips: avatar sources")?.values());
        let jake_clips = Rc::new(ClipLib::build(site, &avatar, "jake-", &names(&jake))?);
        let guard_src: Vec<&Value> = v["guard"]["sources"].as_object().ok_or("anim_clips: guard")?.values().collect();
        let dog_src: Vec<&Value> = v["dog"]["sources"].as_object().ok_or("anim_clips: dog")?.values().collect();
        let guard_clips = Rc::new(ClipLib::build(site, &guard_src, "", &names(&guard))?);
        let dog_clips = Rc::new(ClipLib::build(site, &dog_src, "", &names(&dog))?);
        Ok(Self { jake, guard, dog, jake_clips, guard_clips, dog_clips })
    }
}

/// The avatar's face controller `Go` (deobfuscated.js:12561): blinks and
/// eye-UV look offsets (docs/js_notes/actors_character_model.md §2.7).
#[derive(Clone, Debug)]
pub struct Face {
    pub blink_clock: f64,
    pub next_blink: f64,
    /// `currentBlinkAmount` (0 or 1).
    pub amount: f64,
}

/// `Go.updateBlink`'s `Math.random()`.
pub const BLINK_SITE: Site = Site("at Go.updateBlink (assets/index-QNpTjs8S.js:1:368011) < at Go.update (assets/index-QNpTjs8S.js:1:363272)");
/// `Bo`, `Vo`, `Ho` (12500): blink UV shift, look gain, look clamp.
pub const EYE_BLINK_UV: [f64; 2] = [0.0, -0.47];
pub const EYE_GAIN: [f64; 2] = [0.1, 0.1];
pub const EYE_CLAMP: [f64; 2] = [0.04, 0.025];

impl Default for Face {
    /// As pinned at the oracle's recording start (lib.mjs).
    fn default() -> Self {
        Self { blink_clock: 0.0, next_blink: 0.25, amount: 0.0 }
    }
}

impl Face {
    /// `updateBlink(frameTime)` (after the frame's animation advance).
    pub fn update(&mut self, frame_time: f64, rng: &mut dyn Rng) {
        self.blink_clock += frame_time.max(0.0) / 60.0;
        let start = self.next_blink;
        let end = start + 0.22;
        if self.blink_clock < start {
            self.amount = 0.0;
            return;
        }
        if self.blink_clock >= end {
            self.next_blink = self.blink_clock + 1.2 + rng.random(BLINK_SITE) * 1.2;
            self.amount = 0.0;
            return;
        }
        self.amount = 1.0;
    }
    /// Morph influences of a blink target (index 0 = blink).
    pub fn influences(&self, base: &[f32]) -> Vec<f32> {
        let mut v = base.to_vec();
        if let Some(b) = v.first_mut() {
            *b = (*b as f64 + (1.0 - *b as f64) * self.amount.clamp(0.0, 1.0)) as f32;
        }
        v
    }
}
