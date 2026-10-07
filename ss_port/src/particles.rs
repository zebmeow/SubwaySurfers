//! The original's Unity particle runtime `An` ("UnityParticles",
//! deobfuscated.js:3339-3968) with its curve / gradient helpers `V`, `En`,
//! `Dn` (3189-3330). See docs/js_notes/particles.md.
//!
//! A system simulates in its entity's local space: `offset` (the
//! `aOffset` instance attribute, f32 like the JS `Float32Array`), `data`
//! (size, rotation x/y/z), `color` (rgba) and the sprite-sheet tile per
//! particle; the renderer places them with the entity's world matrix.
//! Every `Math.random()` goes through the oracle's per-call-site streams
//! (`sites`). All systems share those sites, so the draw order between
//! systems (the component update order, `crate::fx::Fx::order`) matters.

use crate::rng::{Rng, Site};
use bevy::math::{DMat4, DQuat, DVec3, EulerRot};
use serde::Deserialize;

/// Call sites of `Math.random()` (and `yt`) in `An` (index-QNpTjs8S.js).
pub mod sites {
    use super::Site;
    /// `spawn`: `V(rateOverTime, time, Math.random())`, every spawn call.
    pub const RATE: Site = Site("at An.spawn (assets/index-QNpTjs8S.js:1:95670) < at An.updateSpawns (assets/index-QNpTjs8S.js:1:95336)");
    /// `spawn`: `V(burst.count, t, Math.random())`.
    pub const BURST: Site = Site("at An.spawn (assets/index-QNpTjs8S.js:1:97062) < at An.updateSpawns (assets/index-QNpTjs8S.js:1:95336)");
    /// `emit`'s draws, by the `spawn` loop that called it.
    pub struct Emit {
        pub random: Site,
        pub cone_angle: Site,
        pub cone_theta: Site,
        pub cone_radius: Site,
        pub circle_theta: Site,
        pub circle_radius: Site,
        pub box_x: Site,
        pub box_y: Site,
        pub box_z: Site,
        pub random_color: Site,
    }
    /// From the rate loop (`spawn` col 96133).
    pub const EMIT_RATE: Emit = Emit {
        random: Site("at An.emit (assets/index-QNpTjs8S.js:1:97912) < at An.spawn (assets/index-QNpTjs8S.js:1:96133)"),
        cone_angle: Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at An.emit (assets/index-QNpTjs8S.js:1:100165)"),
        cone_theta: Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at An.emit (assets/index-QNpTjs8S.js:1:100243)"),
        cone_radius: Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at An.emit (assets/index-QNpTjs8S.js:1:100545)"),
        circle_theta: Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at An.emit (assets/index-QNpTjs8S.js:1:100829)"),
        circle_radius: Site("at An.emit (assets/index-QNpTjs8S.js:1:101076) < at An.spawn (assets/index-QNpTjs8S.js:1:96133)"),
        box_x: Site("at An.emit (assets/index-QNpTjs8S.js:1:101618) < at An.spawn (assets/index-QNpTjs8S.js:1:96133)"),
        box_y: Site("at An.emit (assets/index-QNpTjs8S.js:1:101647) < at An.spawn (assets/index-QNpTjs8S.js:1:96133)"),
        box_z: Site("at An.emit (assets/index-QNpTjs8S.js:1:101729) < at An.spawn (assets/index-QNpTjs8S.js:1:96133)"),
        random_color: Site("at Dn (assets/index-QNpTjs8S.js:1:87941) < at An.emit (assets/index-QNpTjs8S.js:1:98828)"),
    };
    /// From the burst loop (`spawn` col 97151): the frames above `emit` are
    /// the same, the direct `Math.random()` keys name the other caller.
    pub const EMIT_BURST: Emit = Emit {
        random: Site("at An.emit (assets/index-QNpTjs8S.js:1:97912) < at An.spawn (assets/index-QNpTjs8S.js:1:97151)"),
        cone_angle: Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at An.emit (assets/index-QNpTjs8S.js:1:100165)"),
        cone_theta: Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at An.emit (assets/index-QNpTjs8S.js:1:100243)"),
        cone_radius: Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at An.emit (assets/index-QNpTjs8S.js:1:100545)"),
        circle_theta: Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at An.emit (assets/index-QNpTjs8S.js:1:100829)"),
        circle_radius: Site("at An.emit (assets/index-QNpTjs8S.js:1:101076) < at An.spawn (assets/index-QNpTjs8S.js:1:97151)"),
        box_x: Site("at An.emit (assets/index-QNpTjs8S.js:1:101618) < at An.spawn (assets/index-QNpTjs8S.js:1:97151)"),
        box_y: Site("at An.emit (assets/index-QNpTjs8S.js:1:101647) < at An.spawn (assets/index-QNpTjs8S.js:1:97151)"),
        box_z: Site("at An.emit (assets/index-QNpTjs8S.js:1:101729) < at An.spawn (assets/index-QNpTjs8S.js:1:97151)"),
        random_color: Site("at Dn (assets/index-QNpTjs8S.js:1:87941) < at An.emit (assets/index-QNpTjs8S.js:1:98828)"),
    };
    /// `updateParticles`: `Dn(colorOverLifetime.color, ...)` (RandomColor only).
    pub const COLOR_OVER_LIFETIME: Site = Site("at Dn (assets/index-QNpTjs8S.js:1:87941) < at An.updateParticles (assets/index-QNpTjs8S.js:1:105466)");
}

/// `_0xda0724`: three's DEG2RAD.
const DEG2RAD: f64 = std::f64::consts::PI / 180.0;

// ---- config (the Unity export, data/fx/*.json) ------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Curve {
    #[serde(default)]
    pub mode: u8,
    #[serde(default)]
    pub values: Vec<f64>,
    #[serde(default)]
    pub range: f64,
    #[serde(default)]
    pub boost: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    #[serde(default = "one")]
    pub a: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gradient {
    /// `En` reads `.fixed`, which the export never sets (it writes
    /// `fixedGradient`): always false, as in the original.
    #[serde(default)]
    pub fixed: bool,
    pub color_times: Vec<f64>,
    pub color_values: Vec<Rgba>,
    pub alpha_times: Vec<f64>,
    pub alpha_values: Vec<f64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MinMaxColor {
    #[serde(default)]
    pub mode: u8,
    #[serde(default)]
    pub colors: Vec<Rgba>,
    #[serde(default)]
    pub gradients: Vec<Gradient>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Xyz {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Main {
    pub duration: f64,
    #[serde(rename = "loop")]
    pub looping: bool,
    #[serde(default)]
    pub start_delay: f64,
    pub play_on_awake: bool,
    pub start_lifetime: Curve,
    pub start_speed: Curve,
    pub start_size: Curve,
    #[serde(default)]
    pub start_rotation: Curve,
    #[serde(default, rename = "startRotation3D")]
    pub start_rotation_3d: bool,
    #[serde(default)]
    pub start_rotation_x: Curve,
    #[serde(default)]
    pub start_rotation_y: Curve,
    #[serde(default)]
    pub start_rotation_z: Curve,
    pub gravity_modifier: Curve,
    pub start_color: MinMaxColor,
    pub simulation_speed: f64,
    pub max_particles: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Burst {
    pub time: f64,
    pub count: Curve,
    #[serde(default)]
    pub cycles: Option<f64>,
    #[serde(default)]
    pub interval: Option<f64>,
    /// Runtime: times fired this cycle.
    #[serde(default)]
    pub triggered: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Emission {
    pub enabled: bool,
    pub rate_over_time: Curve,
    #[serde(default)]
    pub bursts: Vec<Burst>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    pub shape_type: u8,
    #[serde(default)]
    pub angle: f64,
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub radius_thickness: f64,
    pub position: Xyz,
    pub rotation: Xyz,
    pub scale: Xyz,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VelocityOverLifetime {
    pub enabled: bool,
    #[serde(default)]
    pub linear: Vec<Curve>,
    #[serde(default)]
    pub orbital: Vec<Curve>,
    #[serde(default)]
    pub orbital_offset: Vec<Curve>,
    #[serde(default)]
    pub radial: Option<Curve>,
    #[serde(default)]
    pub speed_modifier: Option<Curve>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitVelocity {
    pub enabled: bool,
    #[serde(default)]
    pub speed: Curve,
    #[serde(default)]
    pub dampen: f64,
    #[serde(default)]
    pub drag: Curve,
    #[serde(default)]
    pub multiply_drag_by_particle_size: bool,
    #[serde(default)]
    pub multiply_drag_by_particle_velocity: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ColorOverLifetime {
    pub enabled: bool,
    #[serde(default)]
    pub color: MinMaxColor,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SizeOverLifetime {
    pub enabled: bool,
    #[serde(default)]
    pub size: Curve,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationOverLifetime {
    pub enabled: bool,
    #[serde(default)]
    pub separate_axes: bool,
    #[serde(default)]
    pub x: Curve,
    #[serde(default)]
    pub y: Curve,
    #[serde(default)]
    pub z: Curve,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureSheet {
    pub enabled: bool,
    #[serde(default = "one")]
    pub num_tiles_x: f64,
    #[serde(default = "one")]
    pub num_tiles_y: f64,
    #[serde(default)]
    pub animation: u8,
    #[serde(default)]
    pub time_mode: u8,
    #[serde(default)]
    pub frame_over_time: Curve,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Renderer {
    #[serde(default)]
    pub mode: u8,
    #[serde(default)]
    pub alignment: u8,
    #[serde(default)]
    pub length_scale: Option<f64>,
    #[serde(default)]
    pub speed_scale: Option<f64>,
    #[serde(default)]
    pub material_name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub main: Main,
    pub emission: Emission,
    pub shape: Shape,
    #[serde(default)]
    pub velocity_over_lifetime: VelocityOverLifetime,
    #[serde(default)]
    pub limit_velocity_over_lifetime: LimitVelocity,
    #[serde(default)]
    pub color_over_lifetime: ColorOverLifetime,
    #[serde(default)]
    pub size_over_lifetime: SizeOverLifetime,
    #[serde(default)]
    pub rotation_over_lifetime: RotationOverLifetime,
    #[serde(default)]
    pub texture_sheet_animation: TextureSheet,
    #[serde(default)]
    pub renderer: Renderer,
    #[serde(default)]
    pub noise: Option<serde_json::Value>,
}

impl Config {
    fn noise_enabled(&self) -> bool {
        self.noise.as_ref().is_some_and(|n| n["enabled"].as_bool() == Some(true))
    }
}

/// `_n`
pub mod shape {
    pub const CONE: u8 = 4;
    pub const BOX: u8 = 5;
    pub const CIRCLE: u8 = 10;
    pub const RECTANGLE: u8 = 18;
}

/// `bn` (renderer modes).
pub const MODE_BILLBOARD: u8 = 0;
pub const MODE_STRETCH: u8 = 1;

// ---- curves -----------------------------------------------------------------------------

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// `V(curve, t, random)` (3189): Constant, TwoConstants, Curve (evenly
/// spaced samples over `range`), TwoCurves (pairs; the original's
/// segment fraction `x - 2 floor(x)` is kept).
pub fn v(c: &Curve, t: f64, random: f64) -> f64 {
    let t = t / c.range;
    match c.mode {
        0 => c.values[0],
        3 => lerp(c.values[0], c.values[1], random),
        1 => {
            let n = c.values.len() - 1;
            if t >= 1.0 {
                return c.values[n];
            }
            let x = t * n as f64;
            let i = x.floor() as usize;
            lerp(c.values[i], c.values[i + 1], x - i as f64)
        }
        2 => {
            let n = c.values.len() / 2 - 1;
            if t >= 1.0 {
                return lerp(c.values[n], c.values[n + 1], random);
            }
            let x = t * n as f64;
            let i = (x.floor() as usize) * 2;
            lerp(lerp(c.values[i], c.values[i + 1], random), lerp(c.values[i + 2], c.values[i + 3], random), x - i as f64)
        }
        m => panic!("Unsupported curve mode {m}"),
    }
}

/// `Tn(times, t)`: the bracketing keys and the fraction between them.
fn bracket(times: &[f64], t: f64) -> (usize, usize, f64) {
    if t < times[0] {
        return (0, 0, 0.0);
    }
    let last = times.len() - 1;
    for i in 0..last {
        if times[i + 1] > t {
            return (i, i + 1, (t - times[i]) / (times[i + 1] - times[i]));
        }
    }
    (last, last, 0.0)
}

/// `En(gradient, t, out)`: rgb into `out`, returns alpha.
fn en(g: &Gradient, t: f64, out: &mut [f64; 3]) -> f64 {
    let (a, b, k) = bracket(&g.color_times, t);
    let (ca, cb) = (g.color_values[a], g.color_values[b]);
    if g.fixed {
        *out = [ca.r, ca.g, ca.b];
    } else {
        *out = [lerp(ca.r, cb.r, k), lerp(ca.g, cb.g, k), lerp(ca.b, cb.b, k)];
    }
    let (a, b, k) = bracket(&g.alpha_times, t);
    lerp(g.alpha_values[a], g.alpha_values[b], k)
}

/// `Dn(color, t, out, random)`: rgb into `out`, returns alpha.
fn dn(c: &MinMaxColor, t: f64, out: &mut [f64; 3], random: f64, rng: &mut dyn Rng, site: Site) -> f64 {
    match c.mode {
        0 => {
            let k = c.colors[0];
            *out = [k.r, k.g, k.b];
            k.a
        }
        1 => en(&c.gradients[0], t, out),
        4 => {
            let r = rng.random(site);
            en(&c.gradients[0], r, out)
        }
        2 => {
            let (a, b) = (c.colors[0], c.colors[1]);
            *out = [lerp(a.r, b.r, random), lerp(a.g, b.g, random), lerp(a.b, b.b, random)];
            lerp(a.a, b.a, random)
        }
        3 => {
            let x = en(&c.gradients[0], t, out);
            let mut o2 = [0.0; 3];
            let y = en(&c.gradients[1], t, &mut o2);
            for k in 0..3 {
                out[k] = lerp(out[k], o2[k], random);
            }
            lerp(x, y, random)
        }
        m => panic!("Unsupported color mode {m}"),
    }
}

/// `yt(a, b)`: `a + Math.random() * (b - a)`.
fn yt(rng: &mut dyn Rng, site: Site, a: f64, b: f64) -> f64 {
    a + rng.random(site) * (b - a)
}

// ---- the system -------------------------------------------------------------------------

/// `Cn`.
#[derive(Clone, Debug, Default)]
pub struct Particle {
    pub velocity: DVec3,
    pub position: DVec3,
    pub color: [f64; 3],
    pub alpha: f64,
    pub lifetime: f64,
    pub start_lifetime: f64,
    pub start_size: f64,
    pub random: f64,
    pub alive: bool,
}

/// `An`.
#[derive(Clone, Debug)]
pub struct System {
    pub cfg: Config,
    pub playing: bool,
    pub gravity: f64,
    pub override_emit_factor: f64,
    pub count: f64,
    pub time: f64,
    pub particles: Vec<Particle>,
    pub offset: Vec<[f32; 3]>,
    pub data: Vec<[f32; 4]>,
    pub color: Vec<[f32; 4]>,
    pub tile: Vec<f32>,
    /// Stretched billboards only.
    pub velocity_buf: Option<Vec<[f32; 3]>>,
    pub instance_count: usize,
    /// `material.uvTiles`, set by `spawn` while the texture sheet is on.
    pub uv_tiles: [f64; 2],
    /// `On(shape)`: the shape's matrix (Box / Rectangle).
    shape_matrix: Option<DMat4>,
}

fn f32x3(v: [f64; 3]) -> [f32; 3] {
    [v[0] as f32, v[1] as f32, v[2] as f32]
}

impl System {
    pub fn new(cfg: Config) -> Self {
        assert!(cfg.main.max_particles > 0);
        let n = cfg.main.max_particles;
        let stretch = cfg.renderer.mode == MODE_STRETCH || cfg.renderer.alignment == 4;
        Self {
            time: -cfg.main.start_delay,
            playing: cfg.main.play_on_awake,
            gravity: 9.81,
            override_emit_factor: 1.0,
            count: 0.0,
            particles: vec![Particle::default(); n],
            offset: vec![[0.0; 3]; n],
            data: vec![[0.0; 4]; n],
            color: vec![[0.0; 4]; n],
            tile: vec![0.0; n],
            velocity_buf: stretch.then(|| vec![[0.0; 3]; n]),
            instance_count: 0,
            uv_tiles: [1.0, 1.0],
            shape_matrix: None,
            cfg,
        }
    }

    pub fn emitting(&self) -> bool {
        self.cfg.emission.enabled
    }
    pub fn set_emitting(&mut self, on: bool) {
        self.cfg.emission.enabled = on;
    }

    /// `An.reset`: the clock and the bursts (not `count`, not the particles).
    pub fn reset(&mut self) {
        self.time = -self.cfg.main.start_delay;
        for b in &mut self.cfg.emission.bursts {
            b.triggered = 0.0;
        }
    }

    /// `An.update(time)`: `delta_ms` is `time.deltaTime`; `gravity_local`
    /// the world's -Y in the entity's local space (normalized).
    pub fn update(&mut self, delta_ms: f64, rng: &mut dyn Rng, gravity_local: DVec3) {
        if !self.playing {
            return;
        }
        let dt = (if delta_ms > 0.0 { delta_ms } else { 16.6667 }) / 1000.0 * self.cfg.main.simulation_speed;
        self.update_spawns(dt, rng);
        self.update_particles(dt, gravity_local, rng);
    }

    fn update_spawns(&mut self, dt: f64, rng: &mut dyn Rng) {
        let before = self.time.max(0.0);
        self.time += dt;
        if self.time < 0.0 {
            return;
        }
        let dt = self.time - before;
        let (duration, looping) = (self.cfg.main.duration, self.cfg.main.looping);
        if looping && self.time > duration {
            self.time %= duration;
            for b in &mut self.cfg.emission.bursts {
                b.triggered = 0.0;
            }
        }
        if self.time <= duration {
            self.spawn(dt, rng);
        }
    }

    fn spawn(&mut self, dt: f64, rng: &mut dyn Rng) {
        if !self.cfg.emission.enabled {
            return;
        }
        let max = self.cfg.main.max_particles;
        let rate = v(&self.cfg.emission.rate_over_time, self.time, rng.random(sites::RATE)) * self.cfg.emission.rate_over_time.boost.unwrap_or(1.0);
        let mut next = self.count + rate * dt * self.override_emit_factor;
        let end = next.floor();
        self.instance_count = (end.max(0.0) as usize).min(max);
        let mut i = self.count.floor();
        while i < end {
            self.emit((i as usize) % max, rng, &sites::EMIT_RATE);
            i += 1.0;
        }
        let progress = self.time / self.cfg.main.duration;
        if self.cfg.texture_sheet_animation.enabled {
            self.uv_tiles = [self.cfg.texture_sheet_animation.num_tiles_x, self.cfg.texture_sheet_animation.num_tiles_y];
        }
        for k in 0..self.cfg.emission.bursts.len() {
            let b = &self.cfg.emission.bursts[k];
            let cycles = b.cycles.unwrap_or(1.0);
            let interval = b.interval.unwrap_or(0.01);
            let at = b.time + b.triggered * interval;
            if b.triggered < cycles && self.time > at {
                let n = v(&b.count.clone(), progress, rng.random(sites::BURST)).min(max as f64);
                let mut j = 0.0;
                while j < n {
                    self.emit(((end + j) as usize) % max, rng, &sites::EMIT_BURST);
                    j += 1.0;
                }
                self.cfg.emission.bursts[k].triggered += 1.0;
                next += n;
            }
        }
        self.count = next;
    }

    fn shape_matrix(&mut self) -> DMat4 {
        *self.shape_matrix.get_or_insert_with(|| {
            let s = &self.cfg.shape;
            let q = DQuat::from_euler(EulerRot::XYZ, s.rotation.x, s.rotation.y, s.rotation.z);
            DMat4::from_scale_rotation_translation(DVec3::new(s.scale.x, s.scale.y, s.scale.z), q, DVec3::new(s.position.x, s.position.y, s.position.z))
        })
    }

    fn emit(&mut self, i: usize, rng: &mut dyn Rng, site: &sites::Emit) {
        let t = self.time;
        let main = &self.cfg.main;
        let random = rng.random(site.random);
        let start_lifetime = v(&main.start_lifetime, t, random);
        let start_size = v(&main.start_size, t, random);
        let rot = if main.start_rotation_3d {
            [v(&main.start_rotation_x, t, random), v(&main.start_rotation_y, t, random), v(&main.start_rotation_z, t, random)]
        } else {
            [0.0, 0.0, v(&main.start_rotation, t, random)]
        };
        let mut col = [0.0; 3];
        let alpha = dn(&main.start_color, t / main.duration, &mut col, random, rng, site.random_color);
        let start_speed = v(&main.start_speed, t, random);
        self.data[i] = [start_size as f32, rot[0] as f32, rot[1] as f32, rot[2] as f32];
        self.color[i] = [col[0] as f32, col[1] as f32, col[2] as f32, alpha as f32];
        let shape_type = self.cfg.shape.shape_type;
        let (velocity, offset) = match shape_type {
            shape::CONE => {
                let s = &self.cfg.shape;
                let c = yt(rng, site.cone_angle, (s.angle * std::f64::consts::PI / 180.0).cos(), 1.0);
                let th = yt(rng, site.cone_theta, 0.0, std::f64::consts::PI * 2.0);
                let (ct, st) = (th.cos(), th.sin());
                let sn = (1.0 - c * c).sqrt();
                let vel = DVec3::new(sn * ct, sn * st, c).normalize();
                let r = yt(rng, site.cone_radius, s.radius_thickness, 1.0) * s.radius;
                (vel, [ct * r, st * r, 0.0])
            }
            shape::CIRCLE => {
                let s = &self.cfg.shape;
                let th = yt(rng, site.circle_theta, 0.0, std::f64::consts::PI * 2.0);
                let (ct, st) = (th.cos(), th.sin());
                let (radius, thick) = (s.radius, s.radius_thickness);
                let r = radius * lerp(1.0 - thick, 1.0, rng.random(site.circle_radius).sqrt());
                (DVec3::new(ct, st, 0.0), [ct * r, st * r, 0.0])
            }
            shape::BOX | shape::RECTANGLE => {
                let m = self.shape_matrix();
                let vel = m.transform_vector3(DVec3::Z).normalize();
                let x = rng.random(site.box_x) - 0.5;
                let y = rng.random(site.box_y) - 0.5;
                let z = if shape_type == shape::BOX { rng.random(site.box_z) - 0.5 } else { 0.0 };
                let p = m.transform_point3(DVec3::new(x, y, z));
                (vel, [p.x, p.y, p.z])
            }
            s => panic!("Unsupported shape type {s}"),
        };
        self.offset[i] = f32x3(offset);
        if let Some(vb) = self.velocity_buf.as_mut() {
            vb[i] = f32x3(velocity.to_array());
        }
        let p = &mut self.particles[i];
        p.random = random;
        p.start_lifetime = start_lifetime;
        p.lifetime = start_lifetime;
        p.start_size = start_size;
        p.alive = true;
        p.color = col;
        p.alpha = alpha;
        p.velocity = velocity * start_speed;
        let o = self.offset[i];
        p.position = DVec3::new(o[0] as f64, o[1] as f64, o[2] as f64);
    }

    fn update_particles(&mut self, dt: f64, gravity_local: DVec3, rng: &mut dyn Rng) {
        let cfg = &self.cfg;
        assert!(!cfg.noise_enabled(), "particle noise is not ported (no config uses it)");
        for i in 0..self.particles.len() {
            let p = &mut self.particles[i];
            if !p.alive {
                continue;
            }
            if p.start_lifetime == 0.0 || p.lifetime < 0.0 {
                self.data[i] = [0.0; 4];
                p.alive = false;
                continue;
            }
            let mut vel = p.velocity;
            let age = (1.0 - p.lifetime / p.start_lifetime).clamp(0.0, 1.0);
            if cfg.color_over_lifetime.enabled {
                let mut c = [0.0; 3];
                let a = dn(&cfg.color_over_lifetime.color, age, &mut c, p.random, rng, sites::COLOR_OVER_LIFETIME);
                self.color[i] = [(p.color[0] * c[0]) as f32, (p.color[1] * c[1]) as f32, (p.color[2] * c[2]) as f32, (a * p.alpha) as f32];
            }
            if cfg.size_over_lifetime.enabled {
                self.data[i][0] = (v(&cfg.size_over_lifetime.size, age, p.random) * p.start_size) as f32;
            }
            let rol = &cfg.rotation_over_lifetime;
            if rol.enabled {
                // degrees per frame (no dt); without separate axes the
                // z angle is set, not accumulated (as in the original)
                if rol.separate_axes {
                    self.data[i][1] = (self.data[i][1] as f64 + v(&rol.x, age, p.random) * DEG2RAD) as f32;
                    self.data[i][2] = (self.data[i][2] as f64 + v(&rol.y, age, p.random) * DEG2RAD) as f32;
                    self.data[i][3] = (self.data[i][3] as f64 + v(&rol.z, age, p.random) * DEG2RAD) as f32;
                } else {
                    self.data[i][3] = (v(&rol.z, age, p.random) * DEG2RAD) as f32;
                }
            }
            let vol = &cfg.velocity_over_lifetime;
            if vol.enabled {
                vel.x += v(&vol.linear[0], age, p.random);
                vel.y += v(&vol.linear[1], age, p.random);
                vel.z += v(&vol.linear[2], age, p.random);
                let o = self.offset[i];
                let un = DVec3::new(
                    o[0] as f64 - v(&vol.orbital_offset[0], age, p.random) * 0.5,
                    o[1] as f64 - v(&vol.orbital_offset[1], age, p.random) * 0.5,
                    o[2] as f64 - v(&vol.orbital_offset[2], age, p.random) * 0.5,
                );
                let orb = [v(&vol.orbital[0], age, p.random), v(&vol.orbital[1], age, p.random), v(&vol.orbital[2], age, p.random)];
                for (k, axis) in [DVec3::X, DVec3::Y, DVec3::Z].into_iter().enumerate() {
                    if orb[k] != 0.0 {
                        vel += un.cross(axis * orb[k]);
                    }
                }
                let radial = vol.radial.as_ref().filter(|c| !c.values.is_empty()).map_or(0.0, |c| v(c, age, p.random));
                if radial != 0.0 {
                    let u = DVec3::new(o[0] as f64, o[1] as f64, o[2] as f64);
                    if u.length_squared() > 1e-8 {
                        vel += u.normalize() * radial;
                    }
                }
                if let Some(m) = vol.speed_modifier.as_ref().filter(|c| !c.values.is_empty()) {
                    vel *= v(m, age, p.random);
                }
            }
            let lim = &cfg.limit_velocity_over_lifetime;
            if lim.enabled {
                let max = v(&lim.speed, age, p.random);
                let mut speed = p.velocity.length();
                if speed > 0.0 {
                    let over = (speed - max).max(0.0);
                    speed -= over * lim.dampen;
                    let mut drag = v(&lim.drag, age, p.random);
                    if lim.multiply_drag_by_particle_size {
                        drag *= self.data[i][0] as f64;
                    }
                    if lim.multiply_drag_by_particle_velocity {
                        drag *= speed;
                    }
                    let drag = (drag * dt * dt).clamp(0.0, 1.0);
                    p.velocity = p.velocity.normalize() * (speed * (1.0 - drag));
                }
            }
            let tsa = &cfg.texture_sheet_animation;
            if tsa.enabled && tsa.animation == 0 && tsa.time_mode == 0 {
                let f = v(&tsa.frame_over_time, age, p.random);
                self.tile[i] = (f * tsa.num_tiles_x * tsa.num_tiles_y).floor() as f32;
            }
            let g = v(&cfg.main.gravity_modifier, age, p.random);
            p.velocity += gravity_local * (self.gravity * dt * g);
            let o = &mut self.offset[i];
            o[0] = (o[0] as f64 + vel.x * dt) as f32;
            o[1] = (o[1] as f64 + vel.y * dt) as f32;
            o[2] = (o[2] as f64 + vel.z * dt) as f32;
            if let Some(vb) = self.velocity_buf.as_mut() {
                let w = &mut vb[i];
                w[0] = (vel.x + (w[0] as f64 - vel.x) * 0.8) as f32;
                w[1] = (vel.y + (w[1] as f64 - vel.y) * 0.8) as f32;
                w[2] = (vel.z + (w[2] as f64 - vel.z) * 0.8) as f32;
            }
            p.position = DVec3::new(o[0] as f64, o[1] as f64, o[2] as f64);
            p.lifetime -= dt;
        }
    }
}

/// Load a system config (a whole file, or `config` of a side particle).
pub fn config_from(v: &serde_json::Value) -> Config {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("particle config: {e}"))
}
