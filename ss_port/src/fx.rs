//! The original's particle effects around the run (docs/js_notes/particles.md):
//!
//! - the FX rigs `Tf` (36563) / `Df` (36749): per side a node following an
//!   anchor on the hero's prop (wobbling), Unity particle systems under it
//!   and a ribbon trail `Sf` (36406); the pogo (`Kf.showEffects`, config
//!   `pogo-wGRmkqbh.js`), the jetpack and headstart (`kf.showEffects`,
//!   `uf` / `lf`, plus the boosted flames);
//! - the Bali theme's systems on scenery: gate sparkles (`handleGates*`,
//!   `Ur`) and swamp fog (epic start, station end; `Hr`).
//!
//! Every `An` updates in the component phase in `allEntities` order (the
//! order their entities joined the scene), which `Fx::order` keeps: they
//! all draw from the same random sites. The rigs update in postupdate
//! (their runner joins `scene.onPostupdate` after the systems).

use crate::entities::EntityId;
use crate::game::Game;
use crate::particles::{config_from, Gradient, System};
use crate::scene::ObjId;
use bevy::math::{DMat4, DQuat, DVec3, EulerRot};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::OnceLock;

/// `Cf`
const DEG: f64 = PI / 180.0;
/// `mf`: trail capacity.
const TRAIL_MAX: usize = 48;

/// Blending of a system / trail (`an`): normal (non-premultiplied) or add.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Add,
}

/// What a system draws with (`Jn` uniforms + the bridge state).
#[derive(Clone, Debug)]
pub struct SysLook {
    pub texture: String,
    pub blend: Blend,
    pub multiplier: f64,
    /// `uSizeScale` (the rig particle's x / y scale).
    pub size_scale: [f64; 2],
    /// three `renderOrder` (rig materials' `renderQueue`; 0 otherwise).
    pub render_order: i32,
}

/// Where a system's entity sits.
#[derive(Clone, Debug)]
pub enum Owner {
    /// A scenery object of the port's scene (world matrix from the graph).
    Scene(ObjId),
    /// Under a rig side node: `world` is set by the rig.
    Rig,
    /// An entity of its own in the scene (hoverboard grind sparks and crash
    /// smoke, revive smoke): `world` is its transform.
    Free,
}

#[derive(Clone, Debug)]
pub struct SysSlot {
    pub sys: System,
    pub owner: Owner,
    pub look: SysLook,
    /// Rig systems: the entity's world matrix after the last rig update.
    pub world: DMat4,
    /// Drawn (the rig's `visible`; scenery: while its entity is in the scene).
    pub visible: bool,
}

// ---- trails (`Sf`) ----------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurveKey {
    pub time: f64,
    pub value: f64,
    pub in_tangent: f64,
    pub out_tangent: f64,
}

/// `vf(curve, t)`: Unity's Hermite keys (the first key's value when a
/// tangent is not finite).
pub fn eval_keys(keys: &[CurveKey], t: f64) -> f64 {
    if keys.is_empty() {
        return 1.0;
    }
    if t <= keys[0].time {
        return keys[0].value;
    }
    let last = &keys[keys.len() - 1];
    if t >= last.time {
        return last.value;
    }
    let mut i = 1;
    while keys[i].time < t {
        i += 1;
    }
    let (a, b) = (&keys[i - 1], &keys[i]);
    let d = b.time - a.time;
    if !a.out_tangent.is_finite() || !b.in_tangent.is_finite() {
        return a.value;
    }
    let x = (t - a.time) / d;
    let (x2, x3) = (x * x, x * x * x);
    (x3 * 2.0 - x2 * 3.0 + 1.0) * a.value + (x3 - x2 * 2.0 + x) * d * a.out_tangent + (x3 * -2.0 + x2 * 3.0) * b.value + (x3 - x2) * d * b.in_tangent
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrailData {
    pub time: f64,
    pub min_vertex_distance: f64,
    pub width_multiplier: f64,
    pub width_curve: Vec<CurveKey>,
    pub color: Gradient,
}

/// `yf(times, values, t)`: piecewise linear, clamped.
fn piecewise(times: &[f64], values: &[f64], t: f64) -> f64 {
    if t <= times[0] {
        return values[0];
    }
    for i in 1..times.len() {
        if t <= times[i] {
            return values[i - 1] + (values[i] - values[i - 1]) * (t - times[i - 1]) / (times[i] - times[i - 1]);
        }
    }
    values[values.len() - 1]
}

impl TrailData {
    /// `xf(bf(color), t)`: rgba.
    pub fn color_at(&self, t: f64) -> [f64; 4] {
        let g = &self.color;
        let ch = |f: fn(&crate::particles::Rgba) -> f64| piecewise(&g.color_times, &g.color_values.iter().map(f).collect::<Vec<_>>(), t);
        [ch(|c| c.r), ch(|c| c.g), ch(|c| c.b), piecewise(&g.alpha_times, &g.alpha_values, t)]
    }
}

/// An FX material (`pf`): texture, scroll, scale, multiplier, blending.
#[derive(Clone, Debug)]
pub struct FxLook {
    pub texture: String,
    pub additive: bool,
    pub multiplier: f64,
    pub scroll_speed: [f64; 2],
    pub uv_scale: [f64; 2],
    pub render_order: i32,
}

impl FxLook {
    /// `pf.setTime(t)`: `uUvScroll = (t / 20 * speed) % 1`.
    pub fn uv_scroll(&self, t: f64) -> [f64; 2] {
        let k = t / 20.0;
        [(k * self.scroll_speed[0]) % 1.0, (k * self.scroll_speed[1]) % 1.0]
    }
}

#[derive(Clone, Debug)]
pub struct Trail {
    pub data: TrailData,
    pub look: FxLook,
    /// (world position, clock) oldest first.
    pub points: Vec<(DVec3, f64)>,
    /// The drawn polyline, newest first (built each update).
    pub chain: Vec<DVec3>,
    pub emitting: bool,
    pub clock: f64,
    pub visible: bool,
    /// `clear()` empties the draw range until the next build.
    pub drawn: bool,
    /// Offset of the trail anchor in the side node.
    pub anchor_local: DVec3,
}

impl Trail {
    fn clear(&mut self) {
        self.points.clear();
        self.drawn = false;
    }

    /// `Sf.update(dt)` with the anchor's world position.
    fn update(&mut self, dt: f64, anchor: DVec3) {
        self.clock += dt;
        let last = self.points.last().map(|p| p.0);
        if self.emitting && last.is_none_or(|l| l.distance(anchor) >= self.data.min_vertex_distance) {
            self.points.push((anchor, self.clock));
        }
        while !self.points.is_empty() && (self.clock - self.points[0].1 > self.data.time || self.points.len() > TRAIL_MAX - 1) {
            self.points.remove(0);
        }
        self.chain.clear();
        if self.emitting && self.points.last().is_none_or(|p| p.0.distance_squared(anchor) > 0.000001) {
            self.chain.push(anchor);
        }
        for p in self.points.iter().rev() {
            self.chain.push(p.0);
        }
        self.drawn = self.chain.len().min(TRAIL_MAX) >= 2;
    }
}

// ---- rigs (`Tf`, `Df`) ------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RigKind {
    Pogo,
    Jetpack,
    Headstart,
}

#[derive(Clone, Debug)]
pub struct Side {
    pub key: String,
    /// Anchor position in the prop view (`Ef` / `Wf`).
    pub anchor_local: DVec3,
    rotation: [f64; 3],
    wobble: [f64; 3],
    /// The side node's world matrix (`Tf` x node).
    pub node_world: DMat4,
    /// (system index, entity matrix in the node).
    systems: Vec<(usize, DMat4)>,
    pub trail: Trail,
    /// `Df`: the boost node's world matrix and the flame's matrix in it.
    pub boost_world: DMat4,
    pub flame: Option<DMat4>,
}

#[derive(Clone, Debug, Deserialize)]
struct Boost {
    time: f64,
    rotation: [f64; 3],
    scale: [f64; 3],
    curve: Vec<CurveKey>,
}

#[derive(Clone, Debug)]
pub struct Rig {
    pub kind: RigKind,
    pub sides: Vec<Side>,
    pub clock: f64,
    pub visible: bool,
    /// Added to the scene (its systems joined the update order).
    in_scene: bool,
    boost: Option<Boost>,
    boost_time: f64,
    boost_duration: f64,
    /// `Df.flameMaterial` (`common_Jetpack_VFX`).
    pub flame_look: Option<FxLook>,
}

/// `Wf` (pogo) and `Ef` (jetpack sides).
fn anchors(kind: RigKind) -> Vec<(&'static str, DVec3)> {
    match kind {
        RigKind::Pogo => vec![("base", DVec3::new(0.0, -0.12, 0.0))],
        _ => vec![("left", DVec3::new(1.58, -3.96, -1.4)), ("right", DVec3::new(-1.46, -3.96, -1.43))],
    }
}

fn rig_json(kind: RigKind) -> &'static Value {
    static P: OnceLock<Value> = OnceLock::new();
    static J: OnceLock<Value> = OnceLock::new();
    static H: OnceLock<Value> = OnceLock::new();
    let (cell, src) = match kind {
        RigKind::Pogo => (&P, include_str!("../data/fx/pogo.json")),
        RigKind::Jetpack => (&J, include_str!("../data/fx/jetpack.json")),
        RigKind::Headstart => (&H, include_str!("../data/fx/headstart.json")),
    };
    cell.get_or_init(|| serde_json::from_str(src).expect("fx json"))
}

fn arr3(v: &Value) -> [f64; 3] {
    let a: Vec<f64> = v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default();
    [a.first().copied().unwrap_or(0.0), a.get(1).copied().unwrap_or(0.0), a.get(2).copied().unwrap_or(0.0)]
}

/// three `rotation.set(x, y, z, "YXZ")`.
fn euler_yxz(r: [f64; 3]) -> DQuat {
    DQuat::from_euler(EulerRot::YXZ, r[1], r[0], r[2])
}

fn fx_look(m: &Value) -> FxLook {
    let pair = |k: &str, d: f64| -> [f64; 2] {
        m[k].as_array().map_or([d, d], |a| [a.first().and_then(Value::as_f64).unwrap_or(d), a.get(1).and_then(Value::as_f64).unwrap_or(d)])
    };
    FxLook {
        texture: m["texture"].as_str().unwrap_or("").to_string(),
        additive: m["additive"].as_bool().unwrap_or(false),
        multiplier: m["multiplier"].as_f64().unwrap_or(1.0),
        scroll_speed: pair("scrollSpeed", 0.0),
        uv_scale: pair("uvScale", 1.0),
        render_order: m["renderQueue"].as_i64().unwrap_or(0) as i32,
    }
}

// ---- the registry -----------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Fx {
    pub systems: Vec<SysSlot>,
    /// System indices in update (`allEntities`) order.
    pub order: Vec<usize>,
    /// Scenery systems by owning entity (join / leave with it).
    by_entity: HashMap<EntityId, Vec<usize>>,
    pub rigs: Vec<Rig>,
    rig_index: HashMap<RigKind, usize>,
    /// `scene.onPostupdate` items that are rigs, in order (`start` moves a
    /// rig to the end, `stop` removes it).
    runner: Vec<usize>,
    /// `cf.grindSparks`, `cf.collisionSmoke` (built on the board's first show).
    pub grind: Option<usize>,
    pub hoverboard_smoke: Option<usize>,
    /// `Gp.reviveSmoke` (built on the first revive).
    pub revive_smoke: Option<usize>,
    /// Placements made by timers after a frame's render: three's matrices
    /// pick them up at the next render.
    deferred: Vec<(usize, DMat4)>,
}

/// A free system (`H.unityParticle` added to the scene): joins the update
/// order now.
fn add_free(fx: &mut Fx, src: &str, tex: &str, blend: Blend, rx: f64) -> usize {
    let cfg: Value = serde_json::from_str(src).expect("fx json");
    let i = fx.systems.len();
    fx.systems.push(SysSlot {
        sys: System::new(config_from(&cfg)),
        owner: Owner::Free,
        look: SysLook { texture: tex.into(), blend, multiplier: 1.0, size_scale: [1.0, 1.0], render_order: 0 },
        world: DMat4::from_rotation_x(rx),
        visible: true,
    });
    fx.order.push(i);
    i
}

/// `H.unityParticle(cfg, tex, blend)` for a scenery object.
fn scenery_config(kind: &str) -> (Value, &'static str, Blend) {
    static SPARK: OnceLock<Value> = OnceLock::new();
    static FOG: OnceLock<Value> = OnceLock::new();
    match kind {
        "spark" => (SPARK.get_or_init(|| serde_json::from_str(include_str!("../data/fx/bali_sparkles.json")).unwrap()).clone(), "spark", Blend::Add),
        "fog" => (FOG.get_or_init(|| serde_json::from_str(include_str!("../data/fx/bali_fog.json")).unwrap()).clone(), "fog", Blend::Normal),
        k => panic!("scenery particles {k}"),
    }
}

impl Fx {
    /// A scenery system (`createSparkles` / `createSwampFog`) on object
    /// `obj` of entity `owner`; it joins the update order with the entity.
    pub fn add_scenery(&mut self, owner: EntityId, obj: ObjId, kind: &str) -> usize {
        let (cfg, tex, blend) = scenery_config(kind);
        let i = self.systems.len();
        self.systems.push(SysSlot {
            sys: System::new(config_from(&cfg)),
            owner: Owner::Scene(obj),
            look: SysLook { texture: tex.into(), blend, multiplier: 1.0, size_scale: [1.0, 1.0], render_order: 0 },
            world: DMat4::IDENTITY,
            visible: false,
        });
        self.by_entity.entry(owner).or_default().push(i);
        i
    }

    /// `allEntities.add` of an entity (and its particle children).
    pub fn entity_added(&mut self, id: EntityId) {
        if let Some(v) = self.by_entity.get(&id) {
            for &i in v {
                if !self.order.contains(&i) {
                    self.order.push(i);
                }
                self.systems[i].visible = true;
            }
        }
    }

    /// `allEntities.remove`.
    pub fn entity_removed(&mut self, id: EntityId) {
        if let Some(v) = self.by_entity.get(&id) {
            for &i in v {
                self.order.retain(|&k| k != i);
                self.systems[i].visible = false;
            }
        }
    }

    /// The rig of `kind` (`new Tf` / `new Df` on first use).
    fn rig(&mut self, kind: RigKind) -> usize {
        if let Some(&i) = self.rig_index.get(&kind) {
            return i;
        }
        let j = rig_json(kind);
        let materials = &j["materials"];
        let mut sides = Vec::new();
        for (key, anchor) in anchors(kind) {
            let s = &j["sides"][key];
            let mut systems = Vec::new();
            for p in s["particles"].as_array().into_iter().flatten() {
                let mut cfg = p["config"].clone();
                // non-looping systems with a curve rate burst 6..9 at once
                if cfg["main"]["loop"] == Value::Bool(false) && cfg["emission"]["rateOverTime"]["mode"] == 2 {
                    cfg["emission"]["rateOverTime"] = serde_json::json!({"mode": 0, "values": [0], "range": 1});
                    cfg["emission"]["bursts"] = serde_json::json!([{"time": 0, "count": {"mode": 3, "values": [6, 9], "range": 1}}]);
                }
                let mname = p["material"].as_str().unwrap_or("");
                let m = &materials[mname];
                let pos = arr3(&p["position"]);
                let rot = arr3(&p["rotation"]);
                let scale = p.get("scale").filter(|v| v.is_array()).map(arr3);
                let local = DMat4::from_translation(DVec3::from_array(pos))
                    * DMat4::from_quat(euler_yxz([rot[0] * DEG, rot[1] * DEG, rot[2] * DEG]))
                    * DMat4::from_scale(DVec3::from_array(scale.unwrap_or([1.0; 3])));
                let i = self.systems.len();
                self.systems.push(SysSlot {
                    sys: System::new(config_from(&cfg)),
                    owner: Owner::Rig,
                    look: SysLook {
                        texture: m["texture"].as_str().unwrap_or("").into(),
                        blend: if m["additive"].as_bool() == Some(true) { Blend::Add } else { Blend::Normal },
                        multiplier: m["multiplier"].as_f64().unwrap_or(1.0),
                        size_scale: scale.map_or([1.0, 1.0], |s| [s[0], s[1]]),
                        render_order: m["renderQueue"].as_i64().unwrap_or(0) as i32,
                    },
                    world: DMat4::IDENTITY,
                    visible: false,
                });
                systems.push((i, local));
            }
            let t = &s["trail"];
            let trail = Trail {
                data: serde_json::from_value(t["data"].clone()).expect("trail data"),
                look: fx_look(&materials[t["material"].as_str().unwrap_or("")]),
                points: Vec::new(),
                chain: Vec::new(),
                emitting: true,
                clock: 0.0,
                visible: false,
                drawn: false,
                anchor_local: DVec3::from_array(arr3(&t["position"])),
            };
            let flame = s.get("flame").filter(|f| f.is_object()).map(|f| {
                let sc = f["scale"].as_f64().unwrap_or(1.0);
                let r = arr3(&f["rotation"]);
                DMat4::from_translation(DVec3::from_array(arr3(&f["position"])))
                    * DMat4::from_quat(euler_yxz([r[0] * DEG, r[1] * DEG, r[2] * DEG]))
                    * DMat4::from_scale(DVec3::new(-sc, sc, sc))
            });
            sides.push(Side {
                key: key.into(),
                anchor_local: anchor,
                rotation: arr3(&s["rotation"]),
                wobble: [s["wobble"]["offset"].as_f64().unwrap_or(0.0), s["wobble"]["speed"].as_f64().unwrap_or(0.0), s["wobble"]["intensity"].as_f64().unwrap_or(0.0)],
                node_world: DMat4::IDENTITY,
                systems,
                trail,
                boost_world: DMat4::IDENTITY,
                flame,
            });
        }
        let boost: Option<Boost> = j.get("boost").and_then(|b| serde_json::from_value(b.clone()).ok());
        let flame_look = (kind != RigKind::Pogo).then(|| fx_look(&materials["common_Jetpack_VFX"]));
        let i = self.rigs.len();
        self.rigs.push(Rig { kind, sides, clock: 0.0, visible: false, in_scene: false, boost, boost_time: 0.0, boost_duration: 0.0, flame_look });
        self.rig_index.insert(kind, i);
        i
    }
}

// ---- the hero's prop anchors --------------------------------------------------------------

/// The pogo view (`attachPoint1` x (y -0.12, rx -pi/2)) and the jetpack view
/// (`LowerSpine_jnt` x (0, 0.53, 0.75), ry pi, x0.6) in world space, from
/// Jake's skeleton in the current pose at the hero's render position.
pub fn prop_views(g: &Game) -> Option<(DMat4, DMat4)> {
    let assets = g.actors.as_ref()?;
    let anim = g.hero_anim.as_ref()?;
    let jake = &assets.jake;
    let hero_world = DMat4::from_translation(g.hero.position) * DMat4::from_rotation_y(g.hero.ry);
    let y = -crate::hero::REGULAR_HEIGHT * 0.5 + 1.0;
    let root = hero_world * DMat4::from_translation(DVec3::new(0.0, y, 0.0)) * DMat4::from_rotation_y(PI) * DMat4::from_scale(DVec3::splat(100.0));
    // the bones as of the last mixer update (not the animator's state
    // after later plays)
    let (bones, nodes) = anim.pose_applied(jake);
    let ap = jake.node_index("attachPoint1").map(|n| jake.node_world_posed(root, n, &nodes))?;
    let pogo = ap * DMat4::from_translation(DVec3::new(0.0, -0.12, 0.0)) * DMat4::from_rotation_x(-PI / 2.0);
    let worlds = jake.bone_worlds_posed(root, &nodes, &bones);
    let spine = jake.joint_index("LowerSpine_jnt").map(|j| worlds[j])?;
    let jet = spine * DMat4::from_translation(DVec3::new(0.0, 0.53, 0.75)) * DMat4::from_rotation_y(PI) * DMat4::from_scale(DVec3::splat(0.6));
    Some((pogo, jet))
}

/// `Tf.scale (1, 1, -1)` at the scene root.
fn tf_matrix() -> DMat4 {
    DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0))
}

/// `Tf.update(dt)` (and `Df.animate`).
fn rig_update(g: &mut Game, r: usize, dt: f64) {
    if !g.fx.rigs[r].visible {
        return;
    }
    let Some((pogo_view, jet_view)) = prop_views(g) else { return };
    let rig = &mut g.fx.rigs[r];
    let view = if rig.kind == RigKind::Pogo { pogo_view } else { jet_view };
    rig.clock += dt;
    // Df.animate: the boost curve scales and tilts the flames
    let mut k = 0.0;
    if let Some(b) = &rig.boost {
        if rig.boost_time > 0.0 {
            rig.boost_time = (rig.boost_time - dt).max(0.0);
            k = eval_keys(&b.curve, 1.0 - rig.boost_time / rig.boost_duration);
        }
    }
    let boost_m = rig.boost.as_ref().map_or(DMat4::IDENTITY, |b| {
        DMat4::from_rotation_x(b.rotation[0] * k * DEG) * DMat4::from_scale(DVec3::new(1.0, 1.0, 1.0 + b.scale[2] * k))
    });
    let tf = tf_matrix();
    let clock = rig.clock;
    let mut worlds = Vec::new();
    for side in &mut rig.sides {
        let y = side.rotation[1] + (side.wobble[0] + clock * side.wobble[1]).sin() * side.wobble[2];
        let anchor = view.transform_point3(side.anchor_local);
        let local_pos = tf.inverse().transform_point3(anchor);
        let node = DMat4::from_translation(local_pos) * DMat4::from_quat(euler_yxz([side.rotation[0] * DEG, y * DEG, side.rotation[2] * DEG]));
        side.node_world = tf * node;
        side.boost_world = side.node_world * boost_m;
        for &(i, local) in &side.systems {
            worlds.push((i, side.node_world * local));
        }
        let ta = side.node_world.transform_point3(side.trail.anchor_local);
        side.trail.update(dt, ta);
    }
    for (i, w) in worlds {
        g.fx.systems[i].world = w;
    }
}

/// `Tf.start(scene)`: join the scene (first time), clear the trails, reset
/// and arm the systems, show, `update(0)`.
fn rig_start(g: &mut Game, r: usize) {
    let rig = &mut g.fx.rigs[r];
    let first = !rig.in_scene;
    rig.in_scene = true;
    let ids: Vec<usize> = rig.sides.iter().flat_map(|s| s.systems.iter().map(|x| x.0)).collect();
    for s in &mut rig.sides {
        s.trail.clear();
        s.trail.visible = true;
    }
    rig.visible = true;
    if first {
        g.fx.order.extend(ids.iter().copied());
    }
    g.fx.runner.retain(|&x| x != r);
    g.fx.runner.push(r);
    for &i in &ids {
        let s = &mut g.fx.systems[i];
        s.sys.reset();
        s.sys.set_emitting(true);
        s.visible = true;
    }
    rig_update(g, r, 0.0);
}

/// `Tf.stop()`.
fn rig_stop(g: &mut Game, r: usize) {
    g.fx.runner.retain(|&x| x != r);
    let rig = &mut g.fx.rigs[r];
    rig.visible = false;
    let ids: Vec<usize> = rig.sides.iter().flat_map(|s| s.systems.iter().map(|x| x.0)).collect();
    for s in &mut rig.sides {
        s.trail.visible = false;
        s.trail.clear();
    }
    for i in ids {
        g.fx.systems[i].sys.set_emitting(false);
        g.fx.systems[i].visible = false;
    }
}

/// `Kf.showEffects` (from `show` in `turnOn`).
pub fn pogo_show(g: &mut Game) {
    let r = g.fx.rig(RigKind::Pogo);
    rig_start(g, r);
    crate::audio::play_with(g, "special-jetpack", 1.0, 1.0, true);
}

/// `Kf.hideEffects` (hangtime, `hide`).
pub fn pogo_hide(g: &mut Game) {
    if let Some(&r) = g.fx.rig_index.get(&RigKind::Pogo) {
        if g.fx.rigs[r].visible {
            rig_stop(g, r);
            crate::audio::stop(g, "special-jetpack");
        }
    }
}

/// `kf.showEffects`: the headstart or jetpack rig; boost 1.7 s if already on.
pub fn jetpack_show(g: &mut Game, headstart: bool) {
    let kind = if headstart { RigKind::Headstart } else { RigKind::Jetpack };
    let r = g.fx.rig(kind);
    let rig = &mut g.fx.rigs[r];
    if rig.visible {
        rig.boost_duration = 1.7;
        rig.boost_time = 1.7;
        return;
    }
    // Df.start: boostFor(boost.time), then Tf.start
    let t = rig.boost.as_ref().map_or(0.0, |b| b.time);
    rig.boost_duration = t;
    rig.boost_time = t;
    rig_start(g, r);
}

/// `kf.hide`: both jetpack rigs stop.
pub fn jetpack_hide(g: &mut Game) {
    for kind in [RigKind::Jetpack, RigKind::Headstart] {
        if let Some(&r) = g.fx.rig_index.get(&kind) {
            rig_stop(g, r);
        }
    }
}

// ---- per frame --------------------------------------------------------------------------

/// Component phase: every system in `allEntities` order.
pub fn update_systems(g: &mut Game) {
    for (i, w) in std::mem::take(&mut g.fx.deferred) {
        g.fx.systems[i].world = w;
    }
    let delta_ms = g.clock.delta_ms;
    let order = g.fx.order.clone();
    for i in order {
        // gravity: world -Y in the entity's local space
        let world = match g.fx.systems[i].owner {
            Owner::Scene(obj) => g.scene.world_matrix(obj),
            Owner::Rig | Owner::Free => g.fx.systems[i].world,
        };
        let down = world.inverse().transform_vector3(DVec3::NEG_Y).normalize_or_zero();
        let slot = &mut g.fx.systems[i];
        slot.sys.update(delta_ms, &mut *g.rng, down);
    }
}

/// The rigs in `scene.onPostupdate`, copied before the postupdate runs (a
/// rig started during physics waits for the next frame).
pub fn runner_snapshot(g: &Game) -> Vec<usize> {
    g.fx.runner.clone()
}

/// Postupdate (after physics and chunk placement): the runner's rigs.
pub fn update_rigs(g: &mut Game, runner: &[usize]) {
    let dt = g.clock.delta_ms / 1000.0;
    for &r in runner {
        rig_update(g, r, dt);
    }
}

// ---- hoverboard and revive (`cf`, `Gp.revive`) ------------------------------------------------

/// `cf.show` on the first time: `createGrindingSparks` (rx pi - 17.51 deg,
/// emitting off) and `createCollisionSmoke` (rx -45 deg), both added to the
/// scene in that order.
pub fn hoverboard_first_show(g: &mut Game) {
    if g.fx.grind.is_some() {
        return;
    }
    let grind = add_free(&mut g.fx, include_str!("../data/fx/grind.json"), "grindSpark", Blend::Add, PI - DEG * 17.5133495);
    g.fx.systems[grind].sys.set_emitting(false);
    g.fx.grind = Some(grind);
    let smoke = add_free(&mut g.fx, include_str!("../data/fx/revive_hoverboard.json"), "spray-splash", Blend::Normal, DEG * -45.0);
    g.fx.hoverboard_smoke = Some(smoke);
}

fn place(g: &mut Game, i: usize, pos: DVec3) {
    let rot = DMat4::from_quat(DQuat::from_mat4(&g.fx.systems[i].world));
    g.fx.systems[i].world = DMat4::from_translation(pos) * rot;
}

/// `cf.updateGrinding`: emit for 6 more frames from under the hero.
pub fn grind_arm(g: &mut Game) {
    let Some(i) = g.fx.grind else { return };
    g.fx.systems[i].sys.set_emitting(true);
    let p = g.hero.position;
    place(g, i, DVec3::new(p.x - 1.0, p.y - 4.5, p.z - 3.0));
}

/// `grindSparks.emitting = false` (`hide`, the 6 frames ran out).
pub fn grind_stop(g: &mut Game) {
    if let Some(i) = g.fx.grind {
        g.fx.systems[i].sys.set_emitting(false);
    }
}

/// `cf.explode`: the crash smoke bursts at the hero (`timer`: called from a
/// `setTimeout`, after this frame's render).
pub fn hoverboard_explode(g: &mut Game, timer: bool) {
    let Some(i) = g.fx.hoverboard_smoke else { return };
    let p = g.hero.position;
    if timer {
        let rot = DMat4::from_quat(DQuat::from_mat4(&g.fx.systems[i].world));
        g.fx.deferred.push((i, DMat4::from_translation(DVec3::new(p.x + 0.47, p.y + 7.58, p.z - 3.02)) * rot));
    } else {
        place(g, i, DVec3::new(p.x + 0.47, p.y + 7.58, p.z - 3.02));
    }
    let s = &mut g.fx.systems[i].sys;
    s.reset();
    s.playing = true;
}

/// `Gp.revive`: the revive smoke (built and added on the first revive)
/// 16 ahead of the camera and 0.8 below its axis, turned -153.78 deg about
/// its x, replays. `after_render`: the ad revive lands in a microtask after
/// the frame, so the placement shows from the next render.
pub fn revive_smoke(g: &mut Game, after_render: bool) {
    let i = match g.fx.revive_smoke {
        Some(i) => i,
        None => {
            let i = add_free(&mut g.fx, include_str!("../data/fx/revive_smoke.json"), "spray-splash", Blend::Normal, 0.0);
            g.fx.revive_smoke = Some(i);
            i
        }
    };
    let (_, q, t) = g.camera.rig.world_matrix().to_scale_rotation_translation();
    let pos = t + (q * DVec3::NEG_Z) * 16.0 + (q * DVec3::Y) * -0.8;
    let rot = q * DQuat::from_axis_angle(DVec3::X, DEG * -153.78);
    let w = DMat4::from_rotation_translation(rot, pos);
    if after_render {
        g.fx.deferred.push((i, w));
    } else {
        g.fx.systems[i].world = w;
    }
    let s = &mut g.fx.systems[i].sys;
    s.reset();
    s.playing = true;
}
