//! Camera: the rig `om` (deobfuscated.js:42439-42575), the camera system
//! `sm` (42576-42791) and the intro tween `hg`/`mg` (47978-48145).
//!
//! See docs/js_notes/hero_pogo_camera.md §3. The rig is a chain of nodes
//! with one transform each:
//! `rig [x,y = shake] -> _idle [xyz] -> _idleRotY -> _idleRotX -> _main [xyz]
//!  -> _mainRotY -> _mainRotX -> _tunnel [y, rx] -> camera`.

use crate::game::Game;
use crate::rng::Site;
use bevy::math::{DMat4, DQuat, DVec3};

/// `z.DEG_TO_RAD` (2410) is a rounded constant, not PI/180.
pub const DEG_TO_RAD: f64 = 0.0174533;
pub const CAMERA_FOV: f64 = 68.0;
pub const CAMERA_POS_Y: f64 = 33.8;
pub const CAMERA_POS_Z: f64 = 33.0;
pub const CAMERA_MOD_X: f64 = 0.75;
pub const CAMERA_ROT_X: f64 = -0.375;

pub const SHAKE_Y: Site = Site("at R.range (assets/index-QNpTjs8S.js:1:56473) < at sm.updateShake (assets/index-QNpTjs8S.js:1:989404)");
pub const SHAKE_X: Site = Site("at R.range (assets/index-QNpTjs8S.js:1:56473) < at sm.updateShake (assets/index-QNpTjs8S.js:1:989478)");

/// `z.lerp(a, b, t)` (2287): t clamped to [0, 1].
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Easing functions `Ra` (10720).
pub mod ease {
    use std::f64::consts::PI;
    pub fn sine_in(t: f64) -> f64 {
        let c = (t * PI * 0.5).cos();
        if c.abs() < 1e-14 {
            1.0
        } else {
            1.0 - c
        }
    }
    pub fn sine_out(t: f64) -> f64 {
        (t * PI / 2.0).sin()
    }
    pub fn sine_in_out(t: f64) -> f64 {
        ((PI * t).cos() - 1.0) * -0.5
    }
    pub fn expo_out(t: f64) -> f64 {
        if t == 1.0 {
            t
        } else {
            1.0 - 2f64.powf(t * -10.0)
        }
    }
}

/// `z.smoothDamp` (2373), Unity's SmoothDamp; the camera passes a zero
/// current velocity every call.
pub fn smooth_damp(current: f64, target: f64, vel: f64, smooth_time: f64, max_speed: f64, dt: f64) -> f64 {
    let smooth_time = smooth_time.max(0.0001);
    let omega = 2.0 / smooth_time;
    let x = omega * dt;
    let exp = 1.0 / (1.0 + x + x * 0.479999989271164 * x + x * 0.234999999403954 * x * x);
    let max_change = max_speed * smooth_time;
    let change = (current - target).clamp(-max_change, max_change);
    let orig = target;
    let target = current - change;
    let temp = (vel + omega * change) * dt;
    let mut out = target + (change + temp) * exp;
    if (orig - current > 0.0) == (out > orig) {
        out = orig;
    }
    out
}

/// The rig's tweakable levels (`pg` snapshot fields + tunnel + shake).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rig {
    /// Rig root position (shake offsets).
    pub x: f64,
    pub y: f64,
    pub idle_x: f64,
    pub idle_y: f64,
    pub idle_z: f64,
    pub idle_rot_x: f64,
    pub idle_rot_y: f64,
    pub main_x: f64,
    pub main_y: f64,
    pub main_z: f64,
    pub main_rot_x: f64,
    pub main_rot_y: f64,
    pub tunnel_y: f64,
    pub tunnel_rx: f64,
    /// Degrees.
    pub fov: f64,
}

impl Rig {
    /// World matrix of the camera (child of `_tunnel`, identity local).
    pub fn world_matrix(&self) -> DMat4 {
        DMat4::from_translation(DVec3::new(self.x, self.y, 0.0))
            * DMat4::from_translation(DVec3::new(self.idle_x, self.idle_y, self.idle_z))
            * DMat4::from_rotation_y(self.idle_rot_y)
            * DMat4::from_rotation_x(self.idle_rot_x)
            * DMat4::from_translation(DVec3::new(self.main_x, self.main_y, self.main_z))
            * DMat4::from_rotation_y(self.main_rot_y)
            * DMat4::from_rotation_x(self.main_rot_x)
            * DMat4::from_translation(DVec3::new(0.0, self.tunnel_y, 0.0))
            * DMat4::from_rotation_x(self.tunnel_rx)
    }
    pub fn pose(&self) -> (DVec3, DQuat) {
        let (_, r, t) = self.world_matrix().to_scale_rotation_translation();
        (t, r)
    }
}

/// `mg`: one intro tween step.
#[derive(Clone, Debug)]
pub struct Tween {
    pub time: f64,
    pub duration: f64,
    pub playing: bool,
    pub from: Rig,
    pub to: Rig,
    pub curve: fn(f64) -> f64,
}

fn linear(t: f64) -> f64 {
    t
}

impl Tween {
    fn new(rig: &Rig, duration: f64) -> Self {
        Self { time: 0.0, duration, playing: false, from: *rig, to: *rig, curve: linear }
    }
    fn play(&mut self, rig: &Rig) {
        self.playing = true;
        self.time = 0.0;
        self.from = *rig;
    }
    /// Returns true when the step completed this update.
    fn update(&mut self, rig: &mut Rig, ft: f64) -> bool {
        if !self.playing {
            return false;
        }
        self.time += ft;
        if self.time >= self.duration {
            self.time = self.duration;
        }
        let e = (self.curve)(self.time / self.duration);
        let (f, t) = (&self.from, &self.to);
        rig.idle_x = lerp(f.idle_x, t.idle_x, e);
        rig.idle_y = lerp(f.idle_y, t.idle_y, e);
        rig.idle_z = lerp(f.idle_z, t.idle_z, e);
        rig.idle_rot_x = lerp(f.idle_rot_x, t.idle_rot_x, e);
        rig.idle_rot_y = lerp(f.idle_rot_y, t.idle_rot_y, e);
        rig.main_x = lerp(f.main_x, t.main_x, e);
        rig.main_y = lerp(f.main_y, t.main_y, e);
        rig.main_z = lerp(f.main_z, t.main_z, e);
        rig.main_rot_x = lerp(f.main_rot_x, t.main_rot_x, e);
        rig.main_rot_y = lerp(f.main_rot_y, t.main_rot_y, e);
        rig.fov = lerp(f.fov, t.fov, e);
        if self.time >= self.duration {
            self.playing = false;
            self.time = self.duration;
            return true;
        }
        false
    }
}

/// Intro system `hg`.
#[derive(Clone, Debug, Default)]
pub struct Intro {
    pub time: f64,
    pub playing: bool,
    pub steps: Vec<Tween>,
}

pub const INTRO_DURATION: f64 = 73.0;

/// Camera system `sm` + rig.
#[derive(Clone, Debug)]
pub struct Camera {
    pub rig: Rig,
    pub running: bool,
    pub tunnel: bool,
    pub shake_power: f64,
    pub controlled: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Self { rig: Rig::default(), running: true, tunnel: false, shake_power: 0.0, controlled: false }
    }
}

impl Camera {
    /// `sm.idle` (onIdle).
    pub fn idle(&mut self) {
        // rig.reset() zeroes the levels, not the root (shake) or fov
        let Rig { x, y, fov, .. } = self.rig;
        self.rig = Rig { x, y, fov, ..Rig::default() };
        self.running = false;
        self.shake_power = 0.0;
        self.tunnel = false;
    }
    /// `sm.run` (game.run / onRun).
    pub fn run(&mut self) {
        self.rig.tunnel_y = 0.0;
        self.rig.tunnel_rx = 0.0;
        self.running = true;
        self.tunnel = false;
    }
    pub fn shake(&mut self, power: f64) {
        self.shake_power = power;
    }
    /// `updateIdle`: the menu pose.
    pub fn update_idle(&mut self) {
        if self.controlled {
            return;
        }
        let r = &mut self.rig;
        r.idle_x = -25.0;
        r.idle_y = -13.06444;
        r.idle_z = 15.0;
        r.idle_rot_x = DEG_TO_RAD * 16.37991;
        r.idle_rot_y = DEG_TO_RAD * 77.0;
        r.idle_rot_y += 0.001;
        r.main_x = 0.0;
        r.main_y = CAMERA_POS_Y;
        r.main_z = CAMERA_POS_Z;
        r.main_rot_x = CAMERA_ROT_X;
        r.main_rot_y = 0.0;
        r.fov = 71.99513;
    }
}

/// `sm.update` (system update #0).
pub fn update(g: &mut Game) {
    let ft = g.delta;
    if g.camera.running {
        update_running(g, ft);
    } else {
        g.camera.update_idle();
    }
}

fn update_running(g: &mut Game, ft: f64) {
    if !g.camera.controlled {
        let tx = g.stats_x * CAMERA_MOD_X;
        let ty = g.hero.player.camera_y + CAMERA_POS_Y;
        let r = &mut g.camera.rig;
        r.idle_x = 0.0;
        r.idle_y = 0.0;
        r.idle_z = 0.0;
        r.idle_rot_x = 0.0;
        r.idle_rot_y = 0.0;
        r.main_x = lerp(r.main_x, tx, ft * 0.3);
        r.main_y = lerp(r.main_y, ty, ft * 0.3);
        r.main_z = g.stats_z + CAMERA_POS_Z;
        r.main_rot_x = lerp(r.main_rot_x, CAMERA_ROT_X, ft * 0.1);
        r.main_rot_y = 0.0;
        r.fov = CAMERA_FOV;
    }
    update_tunnel(g, ft);
    update_shake(g, ft);
}

/// `updateTunnel` (42698). The early-out tests the rig root's y (the shake
/// offset), as in the original.
fn update_tunnel(g: &mut Game, ft: f64) {
    let tunnel = g.camera.tunnel;
    if !tunnel && g.camera.rig.tunnel_rx == 0.0 && g.camera.rig.y == 0.0 {
        return;
    }
    let tilt = DEG_TO_RAD * -4.621953;
    let ty = if tunnel { CAMERA_POS_Y - 18.30177 } else { 0.0 };
    let trx = if tunnel { CAMERA_ROT_X - tilt } else { 0.0 };
    let s = g.stats_speed() * 0.25 + 0.75;
    let r = &mut g.camera.rig;
    r.tunnel_y = smooth_damp(r.tunnel_y, -ty, 0.0, 0.2, s * 1.2, ft);
    r.tunnel_rx = smooth_damp(r.tunnel_rx, -trx, 0.0, 0.05, s * 0.1, ft);
}

/// `updateShake` (42726): `R.range` draws, y first.
fn update_shake(g: &mut Game, ft: f64) {
    let p = g.camera.shake_power;
    if p != 0.0 || g.camera.rig.x != 0.0 || g.camera.rig.y != 0.0 {
        let y = -p + (p - -p) * g.rng.random(SHAKE_Y);
        let x = -p + (p - -p) * g.rng.random(SHAKE_X);
        g.camera.rig.y = y;
        g.camera.rig.x = x;
        g.camera.shake_power -= ft * 0.5;
        if g.camera.shake_power < 0.0 {
            g.camera.shake_power = 0.0;
        }
    }
}

/// `hg.play` (after the intro starts): take the rig, idle pose, three tween
/// steps (40% / 20% / 40% of 73 frames).
pub fn intro_play(g: &mut Game) {
    g.intro.playing = true;
    g.camera.controlled = true;
    // updateIdle(0) while controlled is a no-op
    g.camera.update_idle();
    g.intro.time = 0.0;
    let rig = g.camera.rig;
    let mut s1 = Tween::new(&rig, INTRO_DURATION * 0.4);
    s1.to.idle_x = -16.49361;
    s1.to.idle_y = -8.666094;
    s1.to.idle_z = 12.52404;
    s1.to.idle_rot_x = DEG_TO_RAD * 23.52661;
    s1.to.idle_rot_y = DEG_TO_RAD * 55.26425;
    s1.to.main_x = 0.0;
    s1.to.main_y = CAMERA_POS_Y;
    s1.to.main_z = CAMERA_POS_Z;
    s1.to.main_rot_x = CAMERA_ROT_X;
    s1.to.main_rot_y = 0.0;
    s1.curve = ease::sine_in_out;
    let mut s2 = Tween::new(&rig, INTRO_DURATION * 0.2);
    s2.to.idle_x = -16.49361;
    s2.to.idle_y = -8.666094;
    s2.to.idle_z = 12.52404;
    s2.to.idle_rot_x = DEG_TO_RAD * 23.30551;
    s2.to.idle_rot_y = DEG_TO_RAD * 55.47934;
    s2.to.main_x = 0.0;
    s2.to.main_y = CAMERA_POS_Y;
    s2.to.main_z = CAMERA_POS_Z;
    s2.to.main_rot_x = CAMERA_ROT_X;
    s2.to.main_rot_y = 0.0;
    let mut s3 = Tween::new(&rig, INTRO_DURATION * 0.4);
    s3.to.idle_x = 0.0;
    s3.to.idle_y = 0.0;
    s3.to.idle_z = 0.0;
    s3.to.idle_rot_x = 0.0;
    s3.to.idle_rot_y = 0.0;
    s3.to.main_x = 0.0;
    s3.to.main_y = CAMERA_POS_Y;
    s3.to.main_z = CAMERA_POS_Z;
    s3.to.main_rot_x = CAMERA_ROT_X;
    s3.to.main_rot_y = 0.0;
    s3.to.fov = CAMERA_FOV;
    s3.curve = ease::sine_in;
    s1.play(&rig);
    g.intro.steps = vec![s1, s2, s3];
}

/// `hg.preupdate`: returns true when the intro completes this frame.
pub fn intro_preupdate(g: &mut Game) -> bool {
    if !g.intro.playing {
        return false;
    }
    g.intro.time += g.delta;
    if g.intro.time >= INTRO_DURATION {
        g.intro.time = INTRO_DURATION;
    }
    if g.intro.time >= INTRO_DURATION {
        // complete(): playing = false, releaseControl, then runFromIntro
        g.intro.playing = false;
        g.camera.controlled = false;
        return true;
    }
    false
}

/// `hg.update`: step tweens in order; a completed step starts the next one,
/// which is updated in the same frame.
pub fn intro_update(g: &mut Game) {
    if !g.intro.playing {
        return;
    }
    let ft = g.delta;
    for i in 0..g.intro.steps.len() {
        let done = g.intro.steps[i].update(&mut g.camera.rig, ft);
        if done && i + 1 < g.intro.steps.len() {
            let rig = g.camera.rig;
            g.intro.steps[i + 1].play(&rig);
        }
    }
}
