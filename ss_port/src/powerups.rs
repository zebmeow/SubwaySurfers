//! Hero powerup components other than the pogo (which lives in
//! [`crate::hero`]): magnet `If`, 2x multiplier `Rf`, super sneakers `pp`,
//! jetpack `kf`, hoverboard `cf`; the coin/pickup attraction `ar`; the
//! upgrade levels and the HUD item timers they report to.
//!
//! See docs/js_notes/powerup_magnet_sneakers_multiplier.md,
//! powerup_jetpack.md, powerup_hoverboard.md, ui_powerups_shop.md §1.

use crate::entities::{Cls, EntityId};
use crate::game::{Game, GameState};
use bevy::math::DVec3;

// ---- upgrade levels (flow::UserData::upgrades = ShopSettings permanents) --------------------

/// Upgrade prices `g_` (54826): identical for the four upgradables.
pub const UPGRADE_COSTS: [i64; 6] = [500, 1500, 3000, 10000, 30000, 60000];

/// `calculateDuration` of magnet / multiplier / sneakers (37459, 37560, 38757).
pub fn timed_duration(level: u32) -> f64 {
    10.0 + level as f64 * 5.0
}

// ---- HUD item timers (hud.addItemTimer / updateItemTimer / removeItemTimer) ---------------

/// One meter row: name and the last reported ratio. Rows are kept in add
/// order (oldest first = bottom); re-adding moves a row to the top.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemTimer {
    pub name: &'static str,
    pub ratio: f64,
}

pub fn hud_add(g: &mut Game, name: &'static str) {
    let items = &mut g.hud.items;
    let ratio = items.iter().position(|t| t.name == name).map(|i| items.remove(i).ratio).unwrap_or(1.0);
    items.push(ItemTimer { name, ratio });
}

pub fn hud_update(g: &mut Game, name: &'static str, ratio: f64) {
    if let Some(t) = g.hud.items.iter_mut().find(|t| t.name == name) {
        t.ratio = ratio;
    }
}

pub fn hud_remove(g: &mut Game, name: &'static str) {
    g.hud.items.retain(|t| t.name != name);
}

// ---- magnet (If, 37450) ---------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Magnet {
    pub count: f64,
    pub duration: f64,
    pub frozen: bool,
}

impl Default for Magnet {
    fn default() -> Self {
        Self { count: 0.0, duration: 10.0, frozen: false }
    }
}

impl Magnet {
    pub fn is_on(&self) -> bool {
        self.count != 0.0
    }
}

/// `If.turnOn`: no "already on" guard, a re-collect restarts it.
pub fn magnet_turn_on(g: &mut Game) {
    let m = &mut g.hero.magnet;
    m.frozen = false;
    m.duration = timed_duration(g.flow.user.upgrades.magnet_tier);
    hud_add(g, "magnet");
    let m = &mut g.hero.magnet;
    m.count = m.duration;
    crate::audio::play_with(g, "special-magnet", 1.0, 1.0, true);
}

pub fn magnet_turn_off(g: &mut Game) {
    if g.hero.magnet.count != 0.0 {
        g.hero.magnet.frozen = false;
        hud_remove(g, "magnet");
        g.hero.magnet.count = 0.0;
        crate::audio::stop(g, "special-magnet");
    }
}

fn magnet_update(g: &mut Game) {
    let ds = g.delta_secs;
    let m = &mut g.hero.magnet;
    if m.count != 0.0 && !m.frozen {
        m.count -= ds;
        let r = m.count / m.duration;
        hud_update(g, "magnet", r);
        if g.hero.magnet.count <= 0.0 {
            magnet_turn_off(g);
            crate::audio::play(g, "pickup-powerdown"); // turnOff(true)
        }
    }
}

// ---- 2x multiplier (Rf, 37548) ---------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Multiplier {
    pub count: f64,
    pub duration: f64,
    pub frozen: bool,
}

impl Default for Multiplier {
    fn default() -> Self {
        Self { count: 0.0, duration: 10.0, frozen: false }
    }
}

impl Multiplier {
    pub fn is_on(&self) -> bool {
        self.count != 0.0
    }
}

/// `Rf.turnOn`: a re-collect only refills the count.
pub fn multiplier_turn_on(g: &mut Game) {
    let m = &mut g.hero.multiplier;
    m.frozen = false;
    m.duration = timed_duration(g.flow.user.upgrades.multiplier_tier);
    if m.count != 0.0 {
        m.count = m.duration;
        return;
    }
    m.count = m.duration;
    hud_add(g, "multiplier");
    g.multiplier *= 2.0;
    g.mission_multiplier *= 2.0;
}

pub fn multiplier_turn_off(g: &mut Game) {
    if g.hero.multiplier.count != 0.0 {
        g.hero.multiplier.frozen = false;
        hud_remove(g, "multiplier");
        g.multiplier /= 2.0;
        g.mission_multiplier /= 2.0;
        g.hero.multiplier.count = 0.0;
    }
}

fn multiplier_update(g: &mut Game) {
    // stats.delta: deltaSecs while RUNNING, else 0
    let sd = if g.state == GameState::Running { g.delta_secs } else { 0.0 };
    let m = &mut g.hero.multiplier;
    if m.count != 0.0 && !m.frozen {
        m.count -= sd;
        let r = m.count / m.duration;
        hud_update(g, "multiplier", r);
        if g.hero.multiplier.count <= 0.0 {
            multiplier_turn_off(g);
            crate::audio::play(g, "pickup-powerdown"); // turnOff(true)
        }
    }
}

// ---- attraction (ar, 5434) ----------------------------------------------------------------

/// Attractable component of coins (`sneakers_only` false) and pickups (true).
#[derive(Clone, Debug, Default)]
pub struct Attract {
    pub sneakers_only: bool,
    pub attracted: bool,
    pub start: DVec3,
    pub end: DVec3,
    /// Frames (`frameTime` units).
    pub time: f64,
    pub duration: f64,
}

/// `ar.update` for one entity (component update, before its movable).
pub fn attract_update(g: &mut Game, id: EntityId) {
    let Some(mut a) = g.ent(id).attract.clone() else { return };
    if !g.ent(id).active {
        return;
    }
    let running = g.state == GameState::Running;
    if a.attracted && !running {
        a.attracted = false;
        g.ent_mut(id).attract = Some(a);
        g.set_active(id, false);
        return;
    }
    if a.attracted {
        // attractionUpdate (5516)
        a.time += g.delta;
        let t = (a.time / a.duration).min(1.0);
        let hb = &g.hero.body;
        a.end = DVec3::new(hb.cx(), hb.cy(), hb.cz() + hb.vz() * 2.0);
        let s = t * t;
        let (st, en) = (a.start, a.end);
        let lerp = |x: f64, y: f64| x + (y - x) * s.clamp(0.0, 1.0);
        {
            let b = g.body(id);
            b.set_cx(lerp(st.x, en.x));
            b.set_cy(lerp(st.y, en.y));
            b.set_cz(lerp(st.z, en.z));
        }
        let done = a.time >= a.duration;
        g.ent_mut(id).attract = Some(a.clone());
        if done {
            crate::physics::collect(g, id);
            if let Some(x) = g.ent_mut(id).attract.as_mut() {
                x.attracted = false;
            }
        }
        a = g.ent(id).attract.clone().unwrap();
    }
    if !a.attracted && running {
        let airborne = !g.hero.landed();
        let hb = &g.hero.body;
        let b = g.ent(id).body.as_ref().unwrap();
        let start = if g.hero.magnet.is_on() && !a.sneakers_only {
            let d = (b.center() - hb.center()).length();
            d < 110.0
        } else if g.hero.sneakers.is_on() {
            let dx = (b.cx() - hb.cx()).abs();
            let dy = hb.cy() - b.cy();
            let dz = (hb.cz() - b.cz()).abs();
            airborne && dx < 10.0 && dy > 0.0 && dy < 50.0 && dz < 50.0
        } else {
            false
        };
        if start {
            attraction_start(g, id, &mut a);
        }
    }
    g.ent_mut(id).attract = Some(a);
}

/// `attractionStart` (5496).
fn attraction_start(g: &mut Game, id: EntityId, a: &mut Attract) {
    a.attracted = true;
    if let Some(m) = g.ent_mut(id).movable.as_mut() {
        m.speed = 0.0;
    }
    let hb = g.hero.body.clone();
    let b = g.body(id);
    // mr.reset: body.movable = false, velocity reset; then movable + ghost
    b.reset_velocity();
    b.movable = true;
    b.ghost = true;
    a.start = b.center();
    a.end = DVec3::new(hb.cx(), hb.cy(), hb.cz() + hb.vz());
    a.duration = (a.start - a.end).length() * 0.2;
    if a.duration < 4.0 {
        a.duration = 4.0;
    }
    a.time = 0.0;
}

// ---- super sneakers (pp, 38727) ------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Sneakers {
    pub time: f64,
    pub duration: f64,
    pub frozen: bool,
    pub paused: bool,
    pub activated: bool,
    /// Subscribed to `controller.onSwipeVertical`.
    pub listening: bool,
    pub mod_: f64,
    pub hit_top: f64,
    pub ceiling_drop: f64,
    pub gravity: f64,
    pub is_jumping: bool,
    pub ascending: bool,
    pub start_y: f64,
    pub start_z: f64,
    /// `setTimeout(100)` after unfreeze: replay `superRun` (virtual ms due).
    pub super_run_due: Option<f64>,
    /// The `superSneakers` mesh is shown on the hero.
    pub shown: bool,
}

impl Sneakers {
    pub fn is_on(&self) -> bool {
        self.time != 0.0
    }
}

const SNEAKERS_JUMP_HEIGHT: f64 = 40.0;
const SNEAKERS_JUMP_LENGTH: f64 = 160.0;
/// smoothDamp smoothTime `dp` and maxSpeed `fp` (38726).
const CEILING_SMOOTH_TIME: f64 = 0.3;
const CEILING_MAX_SPEED: f64 = 300.0;

/// `pp.turnOn` (38825).
pub fn sneakers_turn_on(g: &mut Game) {
    let s = &mut g.hero.sneakers;
    s.frozen = false;
    s.mod_ = 0.0;
    s.paused = false;
    crate::hero::pogo_turn_off(g);
    jetpack_turn_off(g);
    hoverboard_cancel(g);
    let s = &mut g.hero.sneakers;
    s.hit_top = 0.0;
    s.ceiling_drop = 0.0;
    s.duration = timed_duration(g.flow.user.upgrades.sneakers_tier);
    s.time = s.duration;
    s.gravity = crate::hero::GRAVITY;
    hud_add(g, "sneakers");
    g.hero.sneakers.listening = true;
    g.hero.sneakers.shown = true;
}

/// `pp.turnOff` (38843).
pub fn sneakers_turn_off(g: &mut Game) {
    if g.hero.sneakers.time == 0.0 {
        return;
    }
    sneakers_resume(g);
    g.hero.sneakers.shown = false;
    g.hero.jump.locked = false; // jf.unlock
    g.hero.jump.gravity = crate::hero::GRAVITY;
    let s = &mut g.hero.sneakers;
    s.frozen = false;
    s.time = 0.0;
    s.mod_ = 0.0;
    s.listening = false;
    g.camera.controlled = false;
    crate::hero::fsm_set(g, "empty");
    hud_remove(g, "sneakers");
    g.hero.sneakers.activated = false;
}

/// `pp.pause` (pogo turnOn).
pub fn sneakers_pause(g: &mut Game) {
    if !g.hero.sneakers.is_on() {
        return;
    }
    let s = &mut g.hero.sneakers;
    s.paused = true;
    s.activated = false;
    g.camera.controlled = false;
    sneakers_jump_end(g);
    g.hero.sneakers.shown = false;
}

/// `pp.resume` (pogo turnOff).
pub fn sneakers_resume(g: &mut Game) {
    let s = &mut g.hero.sneakers;
    if s.is_on() && s.paused {
        s.paused = false;
        s.shown = true;
    }
}

fn sneakers_activate(g: &mut Game) {
    // jf.lock: end(); locked
    g.hero.jump.is_jumping = false;
    g.hero.jump.is_double_jumping = false;
    g.hero.jump.going_upwards = false;
    g.hero.jump.locked = true;
    g.hero.sneakers.activated = true;
    crate::hero::fsm_set(g, "empty");
    g.camera.controlled = true;
}

/// `pp.jump(force)` (38884), from the swipe-up listener.
pub fn sneakers_jump(g: &mut Game, force: bool) {
    let h = &mut g.hero;
    if h.sneakers.time == 0.0 {
        return;
    }
    if !force && h.sneakers.is_jumping {
        return;
    }
    if !force && !h.can_jump() {
        return;
    }
    crate::hero::roll_cancel(h);
    h.ground_change_tolerance = 0.0;
    h.sneakers.is_jumping = true;
    h.body.set_cy(h.body.cy() + 1.0);
    h.body.set_vy(0.0);
    h.sneakers.start_y = h.body.cy();
    h.sneakers.start_z = h.body.cz();
    h.sneakers.ascending = true;
    crate::audio::play(g, "hero-sneakers-jump");
}

pub fn sneakers_jump_end(g: &mut Game) {
    let h = &mut g.hero;
    h.sneakers.hit_top = 0.0;
    h.sneakers.is_jumping = false;
    h.sneakers.ascending = false;
    h.body.set_vy(0.0);
}

fn sneakers_expo_out(p: f64) -> f64 {
    if p == 1.0 { p } else { 1.0 - 2f64.powf(p * -13.0) }
}

fn sneakers_jump_update(g: &mut Game, ft: f64) {
    let h = &mut g.hero;
    let s = &mut h.sneakers;
    if s.is_jumping && s.ascending {
        let p = (-(h.body.cz() - s.start_z) / SNEAKERS_JUMP_LENGTH) * 2.0;
        let p = if p <= 1.0 { p } else { 1.0 };
        if p >= 1.0 {
            s.ascending = false;
        }
        let dy = s.start_y + SNEAKERS_JUMP_HEIGHT * sneakers_expo_out(p) - h.body.cy();
        h.body.set_vy(if ft != 0.0 { dy / ft } else { 0.0 });
        if !s.ascending || s.hit_top != 0.0 {
            h.body.set_vy(0.0);
        }
    } else {
        let vy = h.body.vy();
        h.body.set_vy(vy - s.gravity * ft);
    }
    if h.body.bottom() <= h.ground + 0.01 && h.body.vy() <= 0.0 {
        h.body.set_bottom(h.ground);
        if h.sneakers.is_jumping {
            sneakers_jump_end(g);
        }
    }
}

/// `z.smoothDamp(cur, target, vel = 0, smoothTime, maxSpeed, dt)` (2373).
pub fn smooth_damp(cur: f64, target: f64, vel: f64, smooth_time: f64, max_speed: f64, dt: f64) -> f64 {
    let smooth_time = smooth_time.max(0.0001);
    let omega = 2.0 / smooth_time;
    let x = omega * dt;
    let exp = 1.0 / (1.0 + x + x * 0.479999989271164 * x + x * 0.234999999403954 * x * x);
    let max_change = max_speed * smooth_time;
    let change = (cur - target).clamp(-max_change, max_change);
    let orig = target;
    let target = cur - change;
    let temp = (vel + omega * change) * dt;
    let mut out = target + (change + temp) * exp;
    if (orig - cur > 0.0) == (out > orig) {
        out = orig;
    }
    out
}

/// `pp.update` (38763), hero component #5.
fn sneakers_update(g: &mut Game) {
    let ft = g.delta;
    if g.hero.sneakers.time == 0.0 {
        return;
    }
    if !g.hero.sneakers.activated && g.hero.landed() {
        sneakers_activate(g);
    }
    if !g.hero.sneakers.activated {
        return;
    }
    let ds = g.delta_secs;
    let s = &mut g.hero.sneakers;
    if !s.frozen && !s.paused {
        s.time -= ds;
    }
    sneakers_jump_update(g, ft);
    let r = g.hero.sneakers.time / g.hero.sneakers.duration;
    hud_update(g, "sneakers", r);
    let h = &mut g.hero;
    let mut m = (h.body.bottom() - h.player.camera_y) * 0.5;
    if h.sneakers.hit_top != 0.0 {
        m *= 0.5;
    }
    h.sneakers.mod_ = m;
    if h.sneakers.paused {
        return;
    }
    let (sx, sz) = (g.stats_x, g.stats_z);
    let cam_y = g.hero.player.camera_y;
    let high = g.hero.body.cy() > 80.0;
    let chunk = g.current_chunk.and_then(|id| g.chunks.iter().find(|c| c.id == id));
    let mut ceiling = f64::INFINITY;
    if !high {
        if let Some(c) = chunk {
            if c.env_tube {
                ceiling = ceiling.min(70.0);
            }
            if c.env_pillars {
                ceiling = ceiling.min(60.0);
            }
        }
    }
    let r = &mut g.camera.rig;
    r.main_x = sx * crate::camera::CAMERA_MOD_X;
    r.main_y = cam_y + crate::camera::CAMERA_POS_Y + g.hero.sneakers.mod_;
    r.main_z = sz + crate::camera::CAMERA_POS_Z;
    r.main_rot_x = crate::camera::CAMERA_ROT_X;
    r.main_rot_y = 0.0;
    let target = if ceiling == f64::INFINITY { 0.0 } else { (r.main_y - ceiling).max(0.0) };
    let s = &mut g.hero.sneakers;
    s.ceiling_drop = smooth_damp(s.ceiling_drop, target, 0.0, CEILING_SMOOTH_TIME, CEILING_MAX_SPEED, ft);
    g.camera.rig.main_y -= g.hero.sneakers.ceiling_drop;
    if g.hero.sneakers.time <= 0.0 {
        if g.hero.landed() && !g.hero.sneakers.ascending {
            sneakers_turn_off(g);
            crate::audio::play(g, "pickup-powerdown"); // turnOff(true)
        } else {
            g.hero.sneakers.time = 0.01;
        }
    }
}

/// `pp.onCollisionEnter` (listener before the player's): a ceiling hit
/// holds the rise.
pub fn sneakers_collision(g: &mut Game, flags: u32, hit_bottom: f64) {
    if flags & crate::hero::flags::TOP != 0 {
        g.hero.body.set_vy(0.0);
        g.hero.sneakers.hit_top = hit_bottom - 10.0;
    }
}

// ---- jetpack (kf) / hoverboard (cf): filled in below --------------------------------------

// ---- jetpack (kf, 36815) ---------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Jetpack {
    pub distance: f64,
    pub distance_total: f64,
    /// Target `velocity.z` (negative).
    pub speed: f64,
    pub take_off_time: f64,
    pub take_off_duration: f64,
    pub take_off_start_y: f64,
    pub take_off_end_y: f64,
    pub rig_start_y: f64,
    pub rig_start_z: f64,
    pub headstart_level: u32,
    /// Seconds that keep `isDodging()` after a barrel roll.
    pub dodge_hold: f64,
    /// The jetpack model is on the hero's back.
    pub shown: bool,
    /// Headstart pickups (`pickups`, reused by later presses).
    pub pickups: Vec<EntityId>,
}

impl Default for Jetpack {
    fn default() -> Self {
        Self {
            distance: 0.0,
            distance_total: 1.0,
            speed: 0.0,
            take_off_time: 0.0,
            take_off_duration: 0.0,
            take_off_start_y: 0.0,
            take_off_end_y: 0.0,
            rig_start_y: 0.0,
            rig_start_z: 0.0,
            headstart_level: 0,
            dodge_hold: 0.0,
            shown: false,
            pickups: Vec::new(),
        }
    }
}

impl Jetpack {
    pub fn is_on(&self) -> bool {
        self.distance != 0.0
    }
    pub fn is_dodging(&self) -> bool {
        self.dodge_hold > 0.0
    }
}

/// Jetpack coin ribbon: `R.pick(0, target)` / `R.pick(-1, 0, 1)` (37027).
pub mod jet_sites {
    use crate::rng::Site;
    pub const PICK_BACK: Site = Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at kf.spawnCoins (assets/index-QNpTjs8S.js:1:837491)");
    pub const PICK_LANE: Site = Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at kf.spawnCoins (assets/index-QNpTjs8S.js:1:837526)");
    pub const FORWARD: Site = Site("at Zo.play (assets/index-QNpTjs8S.js:1:544574) < at Vp.begin (assets/index-QNpTjs8S.js:1:945582)");
    pub const DODGE: Site = Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at Hp.begin (assets/index-QNpTjs8S.js:1:946066)");
}

/// `kf.turnOn(level)` (36941): from a pickup (mid physics substep, level 0)
/// or the headstart boost.
pub fn jetpack_turn_on(g: &mut Game, level: u32) {
    g.hero.jetpack.headstart_level = level;
    g.hero.player.dizzy = 0.0; // dizzyEnd
    sneakers_turn_off(g);
    crate::hero::pogo_turn_off(g);
    hoverboard_pause(g);
    crate::hero::jump_lock(&mut g.hero);
    crate::hero::roll_lock(&mut g.hero);
    crate::hero::fsm_set(g, "empty");
    if g.hero.player.camera_low != 0.0 {
        crate::hero::exit_camera_low(g);
    }
    g.hero.body.set_vy(0.0);
    g.hero.body.ghost = true;
    let speed = -g.stats_speed() * 2.0 - 1.0 - level as f64;
    g.hero.jetpack.speed = speed;
    let start = -g.hero.body.cz();
    let dist = if level != 0 { 2000.0 + level as f64 * 1000.0 } else { 1000.0 + speed.abs() * 200.0 + g.flow.user.upgrades.jetpack_tier as f64 * 600.0 };
    let landing = g.set_safe_landing(start + dist);
    let j = &mut g.hero.jetpack;
    j.distance_total = landing - start;
    j.distance = j.distance_total;
    j.take_off_start_y = g.hero.body.cy();
    let j = &mut g.hero.jetpack;
    j.take_off_end_y = 100.0;
    j.take_off_time = 0.0;
    j.take_off_duration = 2.0;
    g.camera.controlled = true;
    g.hero.jetpack.rig_start_y = g.camera.rig.main_y;
    g.hero.jetpack.rig_start_z = g.camera.rig.main_z;
    g.hero.jetpack.shown = true;
    crate::fx::jetpack_show(g, level != 0); // show() -> showEffects
    crate::audio::play(g, "special-jetpack-start");
    crate::audio::play_with(g, "special-jetpack", 1.0, 1.0, true);
    if level == 0 {
        hud_add(g, "jetpack");
        let d = g.hero.jetpack.distance;
        jetpack_spawn_coins(g, 100.0, d);
    } else {
        crate::boosts::spawn_pickups(g, 100.0, -landing);
    }
}

/// `kf.spawnCoins` (37013): the sky ribbon.
fn jetpack_spawn_coins(g: &mut Game, y: f64, d: f64) {
    let lead = g.stats_speed() * 350.0;
    let rem = d - lead;
    let n = rem / 30.0;
    let step = rem / n;
    let mut target = 0.0f64;
    let mut pos = 0.0f64;
    let mut cnt = 5;
    let mut i = 0.0;
    while i < n {
        let c = g.pool_get_init(Cls::Coin);
        if cnt != 0 {
            cnt -= 1;
        } else {
            target = if target != 0.0 {
                [0.0, target][crate::game::r_index(g.rng.as_mut(), jet_sites::PICK_BACK, 2)]
            } else {
                [-1.0, 0.0, 1.0][crate::game::r_index(g.rng.as_mut(), jet_sites::PICK_LANE, 3)]
            };
            cnt = 5;
        }
        if pos < target {
            pos += 0.5;
        } else if pos > target {
            pos -= 0.5;
        }
        let z = g.stats_z - step * i - lead;
        {
            let b = g.body(c);
            b.set_cx(crate::game::LANE_WIDTH * pos);
            b.set_cy(y);
            b.set_cz(z);
        }
        crate::mount::coin_awake(g, c);
        g.add_child(c);
        i += 1.0;
    }
}

/// `kf.update` (36887), hero component #3.
fn jetpack_update(g: &mut Game) {
    if g.hero.jetpack.distance == 0.0 {
        return;
    }
    let (ft, ds) = (g.delta, g.delta_secs);
    let j = &mut g.hero.jetpack;
    if j.dodge_hold > 0.0 {
        j.dodge_hold = (j.dodge_hold - ds).max(0.0);
    }
    let vz = g.hero.body.vz();
    let t = (ft * 0.1).clamp(0.0, 1.0);
    g.hero.body.set_vz(vz + (g.hero.jetpack.speed - vz) * t);
    g.hero.jetpack.distance -= g.distance_delta;
    let r = g.hero.jetpack.distance / g.hero.jetpack.distance_total;
    hud_update(g, "jetpack", r);
    let cam_y = g.stats_y + crate::camera::CAMERA_POS_Y;
    let cam_z = g.stats_z + crate::camera::CAMERA_POS_Z;
    g.hero.player.camera_y = cam_y;
    let j = &mut g.hero.jetpack;
    if j.take_off_time < j.take_off_duration {
        j.take_off_time += ds;
        if j.take_off_time > j.take_off_duration {
            j.take_off_time = j.take_off_duration;
        }
        let t = j.take_off_time / j.take_off_duration;
        let (sy, ey, ry, rz) = (j.take_off_start_y, j.take_off_end_y, j.rig_start_y, j.rig_start_z);
        g.hero.body.set_cy(crate::camera::lerp(sy, ey, t));
        g.camera.rig.main_y = crate::camera::lerp(ry, cam_y, crate::camera::ease::sine_out(t));
        g.camera.rig.main_z = crate::camera::lerp(rz, cam_z, crate::camera::ease::expo_out(t));
    } else {
        g.camera.rig.main_y = cam_y;
        g.camera.rig.main_z = cam_z;
    }
    let r = &mut g.camera.rig;
    r.main_x = crate::camera::lerp(r.main_x, g.stats_x * CAMERA_FLY_MOD_X, (ft * 0.25).min(1.0));
    r.main_rot_x = crate::camera::CAMERA_ROT_X;
    g.hero.player.camera_target_y = g.hero.body.bottom();
    if g.hero.jetpack.distance <= 0.0 {
        jetpack_turn_off(g);
        crate::audio::play(g, "pickup-powerdown"); // turnOff(true)
    }
}

/// `cameraFlyModX` (RB:1480).
const CAMERA_FLY_MOD_X: f64 = 0.9;

/// `kf.turnOff` (36978).
pub fn jetpack_turn_off(g: &mut Game) {
    if g.hero.jetpack.distance == 0.0 {
        return;
    }
    hud_remove(g, "jetpack");
    g.hero.jetpack.shown = false;
    crate::fx::jetpack_hide(g); // hide()
    crate::audio::stop(g, "special-jetpack");
    g.hero.jetpack.pickups.clear();
    g.hero.jetpack.headstart_level = 0;
    crate::boosts::lowlight_headstart(g);
    let h = &mut g.hero;
    h.body.ghost = false;
    h.body.set_vy(0.0);
    h.jump.locked = false;
    h.jump.gravity = crate::hero::GRAVITY;
    h.roll.locked = false;
    g.camera.controlled = false;
    g.hero.jetpack.distance = 0.0;
    g.hero.jetpack.dodge_hold = 0.0;
    crate::hero::restore_size(&mut g.hero);
    hoverboard_resume(g);
}

/// `Vp.begin` / `Hp.begin` (41392-41428).
pub fn jetpack_state_begin(g: &mut Game, id: &str) {
    use crate::anim::PlayOpts;
    match id {
        "jetpack" => crate::hero::hero_play_any(g, jet_sites::FORWARD, &["Jetpack_forward", "Jetpack_forward_2"], PlayOpts::looping()),
        "jetpackDodging" => {
            let clips = if g.hero.lane.abs_step < 0 {
                ["Jetpack_changeLane_left", "Jetpack_BarrelRoll_left"]
            } else {
                ["Jetpack_changeLane_right", "Jetpack_BarrelRoll_right"]
            };
            let clip = clips[crate::game::r_index(g.rng.as_mut(), jet_sites::DODGE, 2)];
            let barrel = clip.contains("BarrelRoll");
            crate::hero::hero_play(g, clip, if barrel { PlayOpts::default().sudden() } else { PlayOpts::default() });
            if barrel {
                g.hero.jetpack.dodge_hold = 0.42;
            }
        }
        _ => {}
    }
}
// ---- hoverboard (cf, 32545) -----------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Hoverboard {
    pub count: f64,
    pub duration: f64,
    pub paused: bool,
    /// Tutorial lock (`locked`).
    pub locked: bool,
    pub grind_emit_frames: u32,
    /// `setBoard`: board id, active powers, animation set.
    pub board_id: String,
    pub powers: Vec<String>,
    pub anims: crate::shop::BoardAnims,
    /// The board model is shown under the hero's feet.
    pub shown: bool,
    /// Grind index picked by `Fp.begin`.
    pub grind_index: usize,
}

impl Default for Hoverboard {
    fn default() -> Self {
        Self {
            count: 0.0,
            duration: 30.0,
            paused: false,
            locked: false,
            grind_emit_frames: 0,
            board_id: String::new(),
            powers: Vec::new(),
            anims: crate::shop::ANIMS_HOVERBOARD,
            shown: false,
            grind_index: 0,
        }
    }
}

impl Hoverboard {
    pub fn is_on(&self) -> bool {
        self.count != 0.0 && !self.paused
    }
    pub fn has(&self, power: &str) -> bool {
        self.powers.iter().any(|p| p == power)
    }
    pub fn ratio(&self) -> f64 {
        self.count / self.duration
    }
}

pub mod hb_sites {
    use crate::rng::Site;
    pub const GRIND: Site = Site("at R.range (assets/index-QNpTjs8S.js:1:56473) < at Fp.begin (assets/index-QNpTjs8S.js:1:941375)");
}

/// `cf.setBoard` (32625): the board, its powers and animation set.
fn hoverboard_set_board(g: &mut Game) {
    let user = &g.flow.user;
    let c = crate::shop::Catalog::get();
    let id = if c.board(&user.selected_board).is_some() { user.selected_board.clone() } else { "hoverboard".to_string() };
    let powers = crate::shop::board_powers(&id, &user.board_powers);
    let h = &mut g.hero.hoverboard;
    h.anims = crate::shop::board_anims(&id, &powers);
    h.powers = powers;
    h.board_id = id;
}

/// `cf.show` (32555).
fn hoverboard_show(g: &mut Game) {
    hoverboard_set_board(g);
    crate::fx::hoverboard_first_show(g);
    hud_add(g, "hoverboard");
    g.hero.hoverboard.shown = true;
    crate::hero::fsm_set(g, "empty");
}

/// `cf.turnOn` (32726), from the controller's double tap (Space).
pub fn hoverboard_turn_on(g: &mut Game) {
    let ok = !g.hero.hoverboard.is_on()
        && g.state == GameState::Running
        && !g.hero.pogo.is_on()
        && !g.hero.jetpack.is_on()
        && !g.hero.hoverboard.locked;
    if !ok {
        return;
    }
    if g.flow.user.hoverboards < 1 {
        // pause + the "buy hoverboards" popup; resume(3) on exit
        crate::flow::open_buy_boards(g, true);
        return;
    }
    hoverboard_show(g);
    // spendHoverboard (ShopSettings save)
    g.flow.user.hoverboards -= 1;
    g.flow.save_user();
    crate::missions::add_stat(g, 1, "mission-hoverboard");
    let start = g.hero.hoverboard.anims.start;
    crate::hero::hero_play(g, start, crate::anim::PlayOpts::default().sudden());
    let h = &mut g.hero.hoverboard;
    h.count = h.duration;
    h.paused = false;
    g.hero.player.dizzy = 0.0; // dizzyEnd
    crate::hero::fsm_set(g, "empty");
    apply_board_powers(g, true);
    crate::hero_fx::hoverboard_pop(g);
    crate::audio::play(g, "pickup-powerup");
}

/// Board powers on / off (`turnOn` 32749-32773, `turnOff` 32790-32806).
fn apply_board_powers(g: &mut Game, on: bool) {
    let has = |p: &str| g.hero.hoverboard.has(p);
    let (sj, dj, sd, zap, low, fast) = (has("super-jump"), has("double-jump"), has("smooth-drift"), has("zap-sideways"), has("stay-low"), has("speed-up"));
    let h = &mut g.hero;
    if sj {
        h.jump.jump_height = if on { 30.0 } else { 20.0 };
    }
    if dj {
        h.jump.double_jump_enabled = on;
    }
    if sd {
        h.jump.board_gravity = if on { 0.001 } else { crate::hero::GRAVITY };
    }
    if zap {
        h.lane.zapping = on;
    }
    if low {
        crate::hero::set_regular_height(h, if on { 5.0 } else { crate::hero::REGULAR_HEIGHT });
    }
    if fast {
        g.speed_increase = if on { (50.0, 20.0) } else { (0.0, 0.0) };
    }
}

/// `cf.hide`.
fn hoverboard_hide(g: &mut Game) {
    g.hero.hoverboard.shown = false;
    crate::fx::grind_stop(g);
}

/// `cf.turnOff(sound)` (32781).
pub fn hoverboard_turn_off(g: &mut Game) {
    hoverboard_hide(g);
    if g.hero.hoverboard.count != 0.0 {
        hud_remove(g, "hoverboard");
        g.hero.hoverboard.count = 0.0;
        crate::hero::fsm_set(g, "empty");
        apply_board_powers(g, false);
    }
}

/// `cf.cancel` (32811): turn off and refund the board.
pub fn hoverboard_cancel(g: &mut Game) {
    if g.hero.hoverboard.count != 0.0 {
        hoverboard_turn_off(g);
        g.flow.user.hoverboards += 1;
        crate::missions::add_stat(g, -1, "mission-hoverboard");
    }
}

/// `cf.pause` (32701): jetpack / pogo take over.
pub fn hoverboard_pause(g: &mut Game) {
    if g.hero.hoverboard.count != 0.0 {
        g.hero.hoverboard.paused = true;
        hoverboard_hide(g);
    }
}

/// `cf.resume` (32708).
pub fn hoverboard_resume(g: &mut Game) {
    let h = &g.hero.hoverboard;
    if h.count != 0.0 && h.paused {
        g.hero.hoverboard.paused = false;
        crate::hero::fsm_set(g, "empty");
        hoverboard_show(g);
        let clip = g.hero.hoverboard.anims.resume;
        crate::hero::hero_play(g, clip, crate::anim::PlayOpts::default());
    }
}

/// `cf.update` (32658), hero component #7.
fn hoverboard_update(g: &mut Game) {
    let sd = if g.state == GameState::Running { g.delta_secs } else { 0.0 };
    let h = &mut g.hero.hoverboard;
    if h.count != 0.0 && !h.paused {
        h.count -= sd;
        let r = h.ratio();
        hud_update(g, "hoverboard", r);
        if g.hero.hoverboard.count <= 0.0 {
            hoverboard_turn_off(g);
            crate::audio::play(g, "pickup-powerdown"); // turnOff(true)
            crate::missions::add_stat(g, 1, "mission-hoverboard-nocrash");
        }
        let h = &mut g.hero.hoverboard;
        if h.grind_emit_frames > 0 {
            h.grind_emit_frames -= 1;
            if h.grind_emit_frames == 0 {
                crate::fx::grind_stop(g);
            }
        }
    }
}

/// `cf.grinding` (32839): on a train roof.
pub fn hoverboard_grinding(g: &Game) -> bool {
    let h = &g.hero;
    h.hoverboard.is_on() && h.landed() && h.ground > 29.0 && h.ground < 29.2
}

/// Pose state `begin` in hoverboard mode (41156-41469, `of` table).
pub fn hoverboard_state_begin(g: &mut Game, id: &str) {
    use crate::anim::PlayOpts;
    use crate::hero::{hero_play, hero_play_any};
    let a = g.hero.hoverboard.anims;
    match id {
        "idle" => hero_play(g, "paintIdle", PlayOpts::looping().sudden()),
        "running" => hero_play(g, a.run, PlayOpts::looping()),
        "dodging" => {
            let clip = if g.hero.lane.abs_step < 0 { a.dodge_left } else { a.dodge_right };
            if !clip.is_empty() {
                let speed = g.animation_speed();
                hero_play(g, clip, PlayOpts::default().speed(speed).sudden());
            }
        }
        "ascending" => hero_play_any(g, crate::hero::anim_sites::ASCENDING, a.jump, PlayOpts::default()),
        "rolling" => hero_play(g, a.roll, PlayOpts::default().speed(0.5).sudden()),
        "grinding" => {
            // Fp.begin: k = R.range(0, grind.length, true); land clip then the loop
            let k = (g.rng.random(hb_sites::GRIND) * a.grind.len() as f64).floor() as usize;
            g.hero.hoverboard.grind_index = k;
            hero_play(g, a.grind_land[k], PlayOpts::looping());
            hero_play(g, a.grind[k], PlayOpts::looping());
        }
        // hangtime / descending have no board clip; dead / caught as normal
        "dead" | "caught" => {}
        _ => {}
    }
}

/// `Player.die` with the board on (37896): the crash shield. Returns true
/// when the death was absorbed.
pub fn hoverboard_shield(g: &mut Game) -> bool {
    if !g.hero.hoverboard.is_on() {
        return false;
    }
    g.hero.player.dizzy = 0.0; // dizzyEnd
    hoverboard_explode(g, false);
    crate::flow::remove_obstacles(g, 600.0);
    // setTimeout(1): due 1 ms later, so it runs after the next frame's render
    g.hero.hoverboard_shield_due = Some(g.clock.now + 1.0);
    true
}

/// `cf.updateGrinding` (32676): `config.grindParticles`, the full gameplay
/// resources: 6 frames of sparks under the hero.
pub fn hoverboard_update_grinding(g: &mut Game) {
    if g.fx.grind.is_none() {
        return;
    }
    g.hero.hoverboard.grind_emit_frames = 6;
    crate::fx::grind_arm(g);
}

/// `cf.explode` (32820): smoke burst + a forced 15-unit jump. `timer`:
/// from the shield's `setTimeout` (after the frame's render).
fn hoverboard_explode(g: &mut Game, timer: bool) {
    crate::fx::hoverboard_explode(g, timer);
    crate::hero::jump_perform(g, Some(15.0), true);
}

// ---- per-frame hooks ----------------------------------------------------------------------

/// Hero components in `Gp` add order after the pogo: sneakers (5), magnet (6),
/// hoverboard (7), multiplier (8).
/// Hero component #3 (after the pose FSM, before the pogo).
pub fn update_before_pogo(g: &mut Game) {
    jetpack_update(g);
}

pub fn update_after_pogo(g: &mut Game) {
    sneakers_update(g);
    magnet_update(g);
    hoverboard_update(g);
    multiplier_update(g);
}

/// `Gp.freezePowerUps` (41824): death stops the timers.
pub fn freeze(g: &mut Game) {
    if g.hero.magnet.is_on() {
        g.hero.magnet.frozen = true;
    }
    if g.hero.multiplier.is_on() {
        g.hero.multiplier.frozen = true;
    }
    if g.hero.sneakers.is_on() {
        g.hero.sneakers.frozen = true;
        sneakers_jump_end(g);
        g.hero.sneakers.shown = false;
    }
}

/// `Gp.unfreezePowerUps` (revive).
pub fn unfreeze(g: &mut Game) {
    g.hero.magnet.frozen = false;
    g.hero.multiplier.frozen = false;
    g.hero.sneakers.frozen = false;
    if g.hero.sneakers.is_on() {
        g.hero.sneakers.shown = true;
        g.hero.sneakers.super_run_due = Some(g.clock.now + 100.0);
    }
}

/// Timers after the frame's render (`setTimeout` callbacks).
pub fn after_render(g: &mut Game) {
    if g.hero.hoverboard_shield_due.is_some_and(|due| g.clock.now >= due) {
        g.hero.hoverboard_shield_due = None;
        // the shield's timeout: explode again, board off, exitTunnel
        hoverboard_explode(g, true);
        hoverboard_turn_off(g);
        g.camera.tunnel = false;
        crate::audio::play(g, "hero-hoverboard-crash");
    }
    if let Some(due) = g.hero.sneakers.super_run_due {
        if g.clock.now >= due {
            g.hero.sneakers.super_run_due = None;
            crate::hero::hero_play(g, "superRun", crate::anim::PlayOpts::looping().sudden());
        }
    }
}

/// `Ha.onCollect` -> `hero[type].turnOn()` for the timed powerups.
pub fn on_collect(g: &mut Game, cls: Cls) {
    use crate::entities::PickupKind as K;
    match cls {
        // Ha.onCollect: turnOn, then addStat("mission-pickup-powerups")
        Cls::Pickup(K::Magnet) => {
            magnet_turn_on(g);
            crate::missions::add_stat(g, 1, "mission-pickup-powerups");
            crate::awards::powerup(g); // onPickupPowerup
        }
        Cls::Pickup(K::Multiplier) => {
            multiplier_turn_on(g);
            crate::missions::add_stat(g, 1, "mission-pickup-powerups");
            crate::awards::powerup(g); // onPickupPowerup
        }
        Cls::Pickup(K::Sneakers) => {
            sneakers_turn_on(g);
            crate::missions::add_stat(g, 1, "mission-pickup-powerups");
            crate::awards::powerup(g); // onPickupPowerup
        }
        Cls::Pickup(K::Jetpack) => {
            jetpack_turn_on(g, 0);
            crate::missions::add_stat(g, 1, "mission-pickup-powerups");
            crate::awards::powerup(g); // onPickupPowerup
        }
        // Ya.onCollect: the prize is rolled now, opened after death
        Cls::Pickup(K::MysteryBox) => {
            let p = crate::prizes::roll(g, "mystery-box");
            g.prizes.push(p);
            crate::missions::add_stat(g, 1, "mission-pickup-mystery");
        }
        // `stats.keys += 1` (banked into the saved keys at game over)
        Cls::Pickup(K::Key) => {
            g.flow.keys += 1;
            crate::missions::add_stat(g, 1, "mission-pickup-keys");
        }
        _ => {}
    }
}
