//! The hero `Gp` (deobfuscated.js:41470-41918) and its gameplay components:
//! input controller `hm`, lane `Pf`, jump `jf`, roll `tp`, pogo `Kf`, player
//! `Hf` and the pose state machine `gp`.
//!
//! See docs/js_notes/hero_core.md, hero_controllers.md, hero_pogo_camera.md.
//! Body vectors are f32-stored (every write rounds), arithmetic is f64.
//! Components not exercised here (jetpack, sneakers, magnet, hoverboard,
//! multiplier) are treated as off.

use crate::camera::{self, ease, lerp, CAMERA_MOD_X, CAMERA_POS_Y, CAMERA_POS_Z};
use crate::anim::PlayOpts;
use crate::entities::{Aabb, Body, EntityId};
use crate::game::{Game, GameState, LANE_WIDTH};
use bevy::math::DVec3;
use std::f64::consts::PI;

pub const GRAVITY: f64 = 0.055;
pub const REGULAR_HEIGHT: f64 = 11.0;
/// `config.dizzyDuration * 60`.
pub const DIZZY_FRAMES: f64 = 180.0;
/// Pogo settings (`Kf`).
const POGO_JUMP_HEIGHT: f64 = 160.0;
const POGO_JUMP_DISTANCE: f64 = 300.0;
const POGO_HANGTIME_POSITION: f64 = 0.5;

/// Gameplay keys (the `pm` keyboard bindings: arrows and WASD, space).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Action,
    /// `v` / `c`: the headstart / score booster buttons (`Wm`).
    Headstart,
    ScoreBooster,
    /// Escape: the pause button (`fg.btnPause`, key "Escape").
    Pause,
    /// `k`: Save me's keys button (`btnReviveKeys.key = "k"`).
    ReviveKeys,
}

impl Key {
    /// DOM `KeyboardEvent.key` / `code` names as recorded by the oracle.
    pub fn from_dom(name: &str) -> Option<Key> {
        Some(match name {
            "ArrowUp" | "w" | "W" | "KeyW" => Key::Up,
            "ArrowDown" | "s" | "S" | "KeyS" => Key::Down,
            "ArrowLeft" | "a" | "A" | "KeyA" => Key::Left,
            "ArrowRight" | "d" | "D" | "KeyD" => Key::Right,
            " " | "Space" => Key::Action,
            "v" => Key::Headstart,
            "c" => Key::ScoreBooster,
            "Escape" => Key::Pause,
            "k" | "K" | "KeyK" => Key::ReviveKeys,
            _ => return None,
        })
    }
}

/// Controller system `hm` (43122-43326): keydown sets a flag, the system
/// update dispatches one signal per frame (vertical > horizontal > action).
#[derive(Clone, Debug, Default)]
pub struct Controller {
    pub vertical: i32,
    pub horizontal: i32,
    pub action: i32,
    /// `keyboard.enabled` (false until `onRun`).
    pub enabled: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Lane {
    pub lane: i32,
    pub last: i32,
    pub abs_step: i32,
    pub changing: bool,
    pub start_x: f64,
    pub end_x: f64,
    pub duration: f64,
    pub time: f64,
    pub queued_step: i32,
    pub queued_duration: f64,
    pub zapping: bool,
    pub on: bool,
}

#[derive(Clone, Debug)]
pub struct Jump {
    pub locked: bool,
    pub is_jumping: bool,
    pub is_double_jumping: bool,
    pub going_upwards: bool,
    pub gravity: f64,
    pub board_gravity: f64,
    pub time: f64,
    pub duration: f64,
    pub start_y: f64,
    pub end_y: f64,
    pub jump_height: f64,
    pub should_jump_again: bool,
    pub double_jump_enabled: bool,
    pub smooth_drift_tick: f64,
    pub on: bool,
}

impl Default for Jump {
    fn default() -> Self {
        Self {
            locked: false,
            is_jumping: false,
            is_double_jumping: false,
            going_upwards: false,
            gravity: GRAVITY,
            board_gravity: GRAVITY,
            time: 0.0,
            duration: 0.0,
            start_y: 0.0,
            end_y: 0.0,
            jump_height: 20.0,
            should_jump_again: false,
            double_jump_enabled: false,
            smooth_drift_tick: 0.0,
            on: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Roll {
    pub locked: bool,
    pub is_rolling: bool,
    pub duration: f64,
    pub time: f64,
    pub rolling_height: f64,
    pub on: bool,
}

impl Default for Roll {
    fn default() -> Self {
        Self { locked: false, is_rolling: false, duration: 30.0, time: 0.0, rolling_height: 5.0, on: false }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Pogo {
    pub count: u32,
    pub hangtime: bool,
    /// Body center when turned on.
    pub position: DVec3,
    pub position_end: f64,
    pub camera_start_y: f64,
    /// Subscribed to `roll.onStart`.
    pub roll_listener: bool,
}

impl Pogo {
    pub fn is_on(&self) -> bool {
        self.count != 0
    }
}

#[derive(Clone, Debug, Default)]
pub struct Player {
    pub dizzy: f64,
    /// The dizzy stars (`$o`) are on: from a stumble's `dizzyStart` while
    /// `dizzy` runs (not the dizzy period `Player.run` starts with).
    pub dizzy_fx: bool,
    /// `$o.spin`: += frameTime * 0.05 each update while the stars show.
    pub dizzy_spin: f64,
    /// The tutorial's rewind: the checkpoint to reach (`rewindEndPoint`)
    /// and the death's 1 s timeout (`crate::tutorial`).
    pub rewind_to: Option<bevy::math::DVec3>,
    pub rewind_due: Option<f64>,
    pub running: bool,
    pub camera_y: f64,
    pub camera_target_y: f64,
    pub camera_low: f64,
    pub tunnel: bool,
    pub dead: bool,
    pub death_cause: String,
    /// `catchMode` (set when the guard catches the hero).
    pub catch_mode: String,
    pub bump_count: f64,
}

/// Pose FSM params (`Gp.onStateUpdate`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P {
    Playing,
    Landed,
    Ascending,
    Descending,
    Dodging,
    Rolling,
    Special,
    Dead,
    Catch,
    Jetpack,
    Grinding,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Params {
    pub playing: bool,
    pub landed: bool,
    pub ascending: bool,
    pub descending: bool,
    pub dodging: bool,
    pub rolling: bool,
    pub hoverboard: bool,
    pub dizzy: bool,
    pub special: bool,
    pub dead: bool,
    pub catch: bool,
    pub jetpack: bool,
    pub grinding: bool,
}

impl Params {
    fn get(&self, p: P) -> bool {
        match p {
            P::Playing => self.playing,
            P::Landed => self.landed,
            P::Ascending => self.ascending,
            P::Descending => self.descending,
            P::Dodging => self.dodging,
            P::Rolling => self.rolling,
            P::Special => self.special,
            P::Dead => self.dead,
            P::Catch => self.catch,
            P::Jetpack => self.jetpack,
            P::Grinding => self.grinding,
        }
    }
}

/// States in `Kp` insertion order with the param keys each defines (41156-41469).
const STATES: &[(&str, &[(P, bool)])] = {
    use P::*;
    &[
        ("idle", &[(Playing, false), (Dead, false)]),
        ("dead", &[(Dead, true), (Catch, false)]),
        ("caught", &[(Dead, true), (Catch, true)]),
        ("running", &[(Playing, true), (Landed, true), (Rolling, false), (Dodging, false), (Grinding, false)]),
        (
            "grinding",
            &[(Playing, true), (Landed, true), (Rolling, false), (Dodging, false), (Grinding, true), (Ascending, false), (Descending, false)],
        ),
        ("dodging", &[(Playing, true), (Landed, true), (Rolling, false), (Dodging, true)]),
        ("ascending", &[(Playing, true), (Landed, false), (Special, false), (Rolling, false), (Ascending, true), (Descending, false)]),
        ("hangtime", &[(Playing, true), (Landed, false), (Special, false), (Rolling, false), (Ascending, false), (Descending, false)]),
        ("descending", &[(Playing, true), (Landed, false), (Special, false), (Rolling, false), (Ascending, false), (Descending, true)]),
        ("rolling", &[(Playing, true), (Rolling, true)]),
        ("jetpack", &[(Jetpack, true), (Dodging, false)]),
        ("jetpackDodging", &[(Jetpack, true), (Dodging, true)]),
    ]
};

/// `Kp` addTransition calls (41863-41955): (from, to, both ways).
const TRANSITIONS: &[(&str, &str, bool)] = &[
    ("all", "idle", false),
    ("all", "caught", false),
    ("idle", "running", false),
    ("running", "dodging", false),
    ("running", "rolling", false),
    ("running", "airborne", false),
    ("running", "jetpack", false),
    ("running", "grinding", true),
    ("dodging", "rolling", false),
    ("dodging", "running", false),
    ("dodging", "hangtime", false),
    ("dodging", "jetpack", false),
    ("airborne", "rolling", false),
    ("airborne", "running", false),
    ("airborne", "dodging", false),
    ("airborne", "jetpack", false),
    ("ascending", "rolling", true),
    ("ascending", "running", true),
    ("ascending", "dodging", true),
    ("ascending", "jetpack", true),
    ("hangtime", "ascending", true),
    ("hangtime", "rolling", true),
    ("hangtime", "running", true),
    ("hangtime", "dodging", true),
    ("hangtime", "jetpack", true),
    ("descending", "ascending", true),
    ("descending", "hangtime", true),
    ("descending", "rolling", true),
    ("descending", "running", true),
    ("descending", "dodging", true),
    ("descending", "jetpack", true),
    ("rolling", "running", false),
    ("rolling", "airborne", false),
    ("rolling", "dodging", false),
    ("rolling", "jetpack", false),
    ("jetpack", "ascending", false),
    ("jetpack", "hangtime", false),
    ("jetpack", "descending", false),
    ("jetpack", "airborne", false),
    ("jetpack", "jetpackDodging", false),
    ("jetpackDodging", "jetpack", false),
    ("jetpackDodging", "airborne", false),
    ("jetpackDodging", "ascending", false),
    ("jetpackDodging", "hangtime", false),
    ("jetpackDodging", "descending", false),
    ("all", "dead", false),
    ("dead", "idle", false),
    ("dead", "running", false),
    ("grinding", "dodging", true),
    ("grinding", "rolling", true),
    ("grinding", "airborne", true),
    ("grinding", "jetpack", true),
    ("grinding", "ascending", true),
    ("grinding", "descending", true),
];

/// Pose state machine `gp` (39048-39139).
#[derive(Clone, Debug)]
pub struct Fsm {
    pub current: &'static str,
    /// `statesReady` (Kp ran).
    pub ready: bool,
    pub params: Params,
    /// `transitionMap`: (from, to) pairs; "all" as from means any.
    edges: std::collections::HashSet<(&'static str, &'static str)>,
}

impl Default for Fsm {
    fn default() -> Self {
        Self { current: "empty", ready: false, params: Params::default(), edges: Default::default() }
    }
}

impl Fsm {
    fn add_transition(&mut self, a: &'static str, b: &'static str, both: bool) {
        self.edges.insert((a, b));
        if both {
            self.edges.insert((b, a));
        }
    }
    /// `Kp`: register states (each `add` links empty <-> state) and transitions.
    fn init(&mut self) {
        self.params = Params::default();
        for (id, _) in STATES {
            self.add_transition("empty", id, true);
        }
        for &(a, b, both) in TRANSITIONS {
            self.add_transition(a, b, both);
        }
        self.ready = true;
    }
    pub fn can(&self, to: &str) -> bool {
        if self.current == to {
            return false;
        }
        self.edges.iter().any(|&(a, b)| b == to && (a == "all" || a == self.current))
    }
    /// `state.set(id)` (begin/end callbacks only drive animations).
    pub fn set(&mut self, to: &'static str) {
        if self.ready && self.can(to) {
            self.current = to;
        }
    }
}

/// The hero entity.
#[derive(Clone, Debug)]
pub struct Hero {
    pub body: Body,
    pub ground: f64,
    pub ground_before: f64,
    pub ground_change_tolerance: f64,
    /// Ground sensor box (1 x 100 x 1 below the center).
    pub sensor: Aabb,
    /// Trigger bodies currently overlapped (`body.colliding`).
    pub colliding: Vec<EntityId>,
    /// Entity transform position (`Body.render` copy, pre-physics).
    pub position: DVec3,
    /// Entity `ry`: the lean during a lane change (`Pf.update`).
    pub ry: f64,
    pub lane: Lane,
    pub jump: Jump,
    pub roll: Roll,
    pub pogo: Pogo,
    pub sneakers: crate::powerups::Sneakers,
    pub magnet: crate::powerups::Magnet,
    pub multiplier: crate::powerups::Multiplier,
    pub hoverboard: crate::powerups::Hoverboard,
    pub jetpack: crate::powerups::Jetpack,
    /// The shield's `setTimeout(1)`: virtual ms it is due (fires after
    /// the render of the first frame at or past it, i.e. the next frame).
    pub hoverboard_shield_due: Option<f64>,
    /// The running state's sneakers footsteps (`Pp.soundSteps`, `count`, `alt`).
    pub steps: Steps,
    /// `Gp.regularHeight` (11; 5 on a stay-low board).
    pub regular_height: f64,
    pub player: Player,
    pub fsm: Fsm,
}

impl Hero {
    /// After `Game.idle()` -> `player.reset(0, 1.2)`: the menu position.
    pub fn new() -> Self {
        let mut body = Body::new(4.0, REGULAR_HEIGHT, 4.0);
        body.movable = true;
        body.set_cx(1.2);
        body.set_cz(0.0);
        body.set_bottom(0.0);
        body.origin = body.bx;
        let mut sensor = Aabb::default();
        sensor.s = [1.0, 100.0, 1.0];
        Self {
            position: body.center(),
            ry: 0.0,
            body,
            ground: 0.0,
            ground_before: 0.0,
            ground_change_tolerance: 0.0,
            sensor,
            colliding: Vec::new(),
            lane: Lane::default(),
            jump: Jump::default(),
            roll: Roll::default(),
            pogo: Pogo::default(),
            sneakers: Default::default(),
            magnet: Default::default(),
            multiplier: Default::default(),
            hoverboard: Default::default(),
            jetpack: Default::default(),
            hoverboard_shield_due: None,
            steps: Steps::default(),
            regular_height: REGULAR_HEIGHT,
            player: Player::default(),
            fsm: Fsm::default(),
        }
    }

    pub fn landed(&self) -> bool {
        self.body.bottom() <= self.ground + 1.0 && !self.body.ghost
    }
    pub fn ascending(&self) -> bool {
        self.body.origin.y() < self.body.bx.y()
    }
    pub fn descending(&self) -> bool {
        self.body.origin.y() > self.body.bx.y()
    }
    pub fn hangtime(&self) -> bool {
        let vy = self.body.vy();
        !self.landed() && vy > -0.2 && vy < 0.2
    }
    pub fn can_jump(&self) -> bool {
        self.ground_change_tolerance != 0.0 || self.landed()
    }
    /// `body.height = h` (size only, center kept).
    fn set_height(&mut self, h: f64) {
        self.body.set_sy(h);
    }

    /// `Body.move` for the hero: integrate, ground clamp, sensor follows,
    /// ground-change tolerance counts down.
    pub fn move_body(&mut self, sd: f64) {
        self.body.integrate(sd, self.ground);
        self.sensor_follow();
        if self.ground_change_tolerance != 0.0 {
            self.ground_change_tolerance -= sd;
            if self.ground_change_tolerance < 0.0 {
                self.ground_change_tolerance = 0.0;
            }
        }
    }
    fn sensor_follow(&mut self) {
        self.sensor.c = [self.body.bx.c[0], (self.body.bx.y() - 50.0) as f32, self.body.bx.c[2]];
    }
    /// `matchPosition(box)` after a push.
    pub fn match_position(&mut self) {
        self.body.origin.c = self.body.bx.c;
        self.sensor_follow();
    }
}

impl Default for Hero {
    fn default() -> Self {
        Self::new()
    }
}

// ---- run flow ------------------------------------------------------------------------

/// `Gp.playIntro` (41694): back to x = 0 for the intro run.
pub fn play_intro(g: &mut Game) {
    let h = &mut g.hero;
    lane_reset(&mut h.lane);
    h.body.set_cz(0.0);
    h.body.set_cx(0.0);
    h.body.set_bottom(0.0);
    // entity transform (overwritten by the next Body.render)
    h.position = DVec3::new(-1.0, h.body.bottom() + h.body.sy() * 0.5, 0.0);
    hero_play(g, "introRun", PlayOpts::default().sudden());
}

/// `Gp.run` + `Player.run` (41722, 37723).
pub fn run(g: &mut Game) {
    // refreshScenes -> anim.updateCurrentAnimation (current clip replayed)
    if let Some(a) = g.hero_anim.as_mut() {
        a.update_current();
    }
    let speed = g.stats_speed();
    g.hero.body.set_vz(-speed);
    player_run(g);
}

/// `Player.run(0)` (37723): dizzy for 3 s, controls on, `run2`.
pub fn player_run(g: &mut Game) {
    crate::hero_fx::shadow(g, true);
    let h = &mut g.hero;
    // Player.run: velocity.z = -config.speed (0)
    h.body.set_vz(-0.0);
    let rh = h.regular_height;
    h.set_height(rh);
    h.body.set_bottom(0.0);
    h.body.movable = true;
    h.player.running = true;
    h.player.dizzy = DIZZY_FRAMES;
    h.player.dizzy_fx = false;
    h.lane.on = true;
    h.jump.on = true;
    h.roll.on = true;
    hero_play(g, "run2", PlayOpts::looping().sudden());
    crate::audio::stop(g, "special-jetpack");
}

/// `lane.reset()` (the tutorial's rewind).
pub fn lane_reset_public(h: &mut Hero) {
    lane_reset(&mut h.lane);
}

fn lane_reset(l: &mut Lane) {
    let zapping = l.zapping;
    let on = l.on;
    *l = Lane { zapping, on, ..Lane::default() };
}

// ---- input -----------------------------------------------------------------------------

/// Keydown (before the frame's update): `hm.pressX`.
pub fn press(g: &mut Game, key: Key) {
    if !g.controller.enabled {
        return;
    }
    match key {
        Key::Up => g.controller.vertical = 1,
        Key::Down => g.controller.vertical = -1,
        Key::Left => g.controller.horizontal = -1,
        Key::Right => g.controller.horizontal = 1,
        Key::Action => {
            if g.state == GameState::Running {
                g.controller.action = 1;
            }
        }
        Key::Headstart | Key::ScoreBooster | Key::Pause | Key::ReviveKeys => {}
    }
}

/// `hm.show()` (onRun).
pub fn controller_show(g: &mut Game) {
    g.controller = Controller { enabled: true, ..Controller::default() };
}

/// `hm.update` (system update).
pub fn controller_update(g: &mut Game) {
    if g.state == GameState::Running {
        let (v, h, a) = (g.controller.vertical, g.controller.horizontal, g.controller.action);
        if v == 1 || v == -1 {
            // onSwipeVertical listeners: jump, then roll
            if g.hero.jump.on && v == 1 {
                jump_perform(g, None, false);
            }
            if g.hero.roll.on && v == -1 {
                roll_perform(g);
            }
            if g.hero.sneakers.listening && v == 1 {
                crate::powerups::sneakers_jump(g, false);
            }
        } else if h != 0 {
            if g.hero.lane.on {
                lane_change(g, h);
            }
        } else if a != 0 {
            // onDoubleTap -> hoverboard.turnOn
            crate::powerups::hoverboard_turn_on(g);
        }
    }
    g.controller.vertical = 0;
    g.controller.horizontal = 0;
    g.controller.action = 0;
}

// ---- lane (Pf) -------------------------------------------------------------------------

/// `Pf.change(step)` (37328).
pub fn lane_change(g: &mut Game, step: i32) {
    let x = g.hero.body.cx();
    if !g.hero.jetpack.is_on() && !g.hero.pogo.is_on() {
        if g.hero.lane.lane == 1 && step > 0 {
            if x > LANE_WIDTH - 1.0 {
                player_stumble(g, "wall", "lower", false);
            }
            return;
        }
        if g.hero.lane.lane == -1 && step < 0 {
            if x < -LANE_WIDTH + 1.0 {
                player_stumble(g, "wall", "lower", false);
            }
            return;
        }
    }
    let per_lane = 0.2 + (1.0 - g.speed_ratio()) * 0.1;
    let speed = g.stats_speed();
    let h = &mut g.hero;
    let new_lane = (h.lane.lane + step).clamp(-1, 1);
    let target = new_lane as f64 * LANE_WIDTH;
    let dist = (target - x).abs();
    if dist > LANE_WIDTH {
        h.lane.queued_step = step;
        h.lane.queued_duration = per_lane;
        return;
    }
    h.fsm.set("empty");
    let l = &mut h.lane;
    l.abs_step = if step < 0 { -1 } else { 1 };
    l.last = l.lane;
    l.lane = new_lane;
    l.queued_step = 0;
    l.queued_duration = 0.0;
    l.changing = true;
    l.start_x = x;
    l.end_x = target;
    l.duration = (per_lane * dist / LANE_WIDTH).max(0.1);
    if h.jetpack.is_on() {
        let min_speed = 110.0 / 60.0;
        l.duration = ((0.54 / (speed * 2.0 / min_speed).max(1.0)) * dist / LANE_WIDTH).max(0.1);
    }
    l.time = 0.0;
    crate::audio::play(g, "hero-dodge");
    crate::awards::lane_changed(g); // onLaneChanged
}

/// `Pf.update` (37384).
fn lane_update(g: &mut Game) {
    if !g.hero.lane.changing {
        return;
    }
    let (ft, ds) = (g.delta, g.delta_secs);
    let h = &mut g.hero;
    let l = &mut h.lane;
    l.time += ds * if l.zapping { 2.0 } else { 1.0 };
    if l.time > l.duration {
        l.time = l.duration;
    }
    let k = (l.time / l.duration).clamp(0.0, 1.0);
    let tgt = lerp(l.start_x, l.end_x, k);
    let vx = if ft != 0.0 { (tgt - h.body.cx()) / ft } else { 0.0 };
    h.body.set_vx(vx);
    let lean = -(h.lane.end_x - h.body.cx()) * 0.05;
    h.ry = if h.jetpack.is_on() || h.pogo.is_on() || h.hoverboard.is_on() { 0.0 } else { lean };
    if h.lane.time >= h.lane.duration {
        lane_change_end(g);
    }
}

fn lane_change_end(g: &mut Game) {
    let h = &mut g.hero;
    h.body.set_cx(h.lane.lane as f64 * LANE_WIDTH);
    h.lane.changing = false;
    h.ry = 0.0;
    h.body.set_vx(0.0);
    if h.lane.queued_duration != 0.0 {
        let step = h.lane.queued_step;
        lane_change(g, step);
        g.hero.lane.queued_step = 0;
        g.hero.lane.queued_duration = 0.0;
    }
}

fn lane_change_cancel(h: &mut Hero) {
    h.lane.changing = false;
    h.ry = 0.0;
    h.lane.queued_step = 0;
    h.lane.queued_duration = 0.0;
    h.body.set_vx(0.0);
}

/// `Pf.bump` (37438): back toward the previous lane after a side hit.
fn lane_bump(g: &mut Game, who: &str, step: i32) {
    lane_change_cancel(&mut g.hero);
    lane_change(g, step);
    // onBumpSideways -> Player.onBumpSideways
    player_stumble(g, who, "lower", false);
    g.hero.body.set_vx(0.0);
}

// ---- jump (jf) -------------------------------------------------------------------------

/// `jf.perform(height, force)` (37171).
pub fn jump_perform(g: &mut Game, height: Option<f64>, force: bool) {
    let h = &mut g.hero;
    if !force {
        let double = h.jump.double_jump_enabled && !h.jump.is_double_jumping;
        if h.jump.locked {
            return;
        } else if h.jump.is_jumping {
            if double {
                h.jump.is_double_jumping = true;
            } else {
                if h.body.vy() < 0.0 {
                    h.jump.should_jump_again = true;
                }
                return;
            }
        } else if !h.can_jump() {
            return;
        }
    }
    roll_end(h);
    h.ground_change_tolerance = 0.0;
    h.jump.is_jumping = true;
    h.body.set_vy(0.0);
    h.body.set_cy(h.body.cy() + 1.0);
    h.jump.start_y = h.body.cy();
    h.jump.end_y = (h.jump.start_y + height.unwrap_or(h.jump.jump_height) - 1.0).min(70.0 + h.ground);
    h.jump.time = 0.0;
    h.jump.duration = 0.41;
    h.jump.going_upwards = true;
    h.jump.smooth_drift_tick = 0.0;
    crate::audio::play(g, "hero-jump");
    crate::awards::jumped(g); // onJump
    crate::missions::add_stat(g, 1, "mission-jump");
    g.hero.jump.gravity = g.hero.jump.board_gravity;
}

fn jump_end(h: &mut Hero) {
    h.jump.is_jumping = false;
    h.jump.is_double_jumping = false;
    h.jump.going_upwards = false;
}

/// `jf.update` (37128): ascent curve or gravity, then the ground snap.
fn jump_update(g: &mut Game) {
    if g.hero.jump.locked {
        return;
    }
    if g.hero.jump.should_jump_again && !g.hero.jump.is_jumping {
        g.hero.jump.should_jump_again = false;
        jump_perform(g, None, false);
    }
    let (ft, ds) = (g.delta, g.delta_secs);
    let h = &mut g.hero;
    if h.jump.is_jumping && h.jump.going_upwards {
        h.jump.time += ds;
        if h.jump.time > h.jump.duration {
            h.jump.time = h.jump.duration;
        }
        let r = h.jump.time / h.jump.duration;
        let y = lerp(h.jump.start_y, h.jump.end_y, ease::sine_out(r));
        h.body.set_vy(if ft != 0.0 { (y - h.body.cy()) / ft } else { 0.0 });
        if r >= 1.0 {
            h.body.set_vy(0.0);
            h.jump.going_upwards = false;
        }
    } else {
        h.body.set_vy(h.body.vy() - h.jump.gravity * ft);
    }
    if h.jump.is_jumping && !h.jump.going_upwards {
        h.jump.smooth_drift_tick += ds * 0.001;
        h.jump.gravity = lerp(h.jump.gravity, GRAVITY, h.jump.smooth_drift_tick);
    }
    if h.body.bottom() <= h.ground + 0.01 {
        h.body.set_vy(0.0);
        h.body.set_bottom(h.ground);
        if h.jump.is_jumping {
            jump_end(h);
        }
    }
}

// ---- roll (tp) -------------------------------------------------------------------------

/// `tp.perform` (38537).
pub fn roll_perform(g: &mut Game) {
    if g.hero.roll.locked || g.hero.roll.is_rolling {
        return;
    }
    // onStart -> pogo.onRollStart -> turnOff
    if g.hero.pogo.roll_listener {
        pogo_turn_off(g);
    }
    jump_end(&mut g.hero);
    if g.hero.sneakers.is_on() {
        crate::powerups::sneakers_jump_end(g);
    }
    let h = &mut g.hero;
    h.roll.is_rolling = true;
    h.roll.time = 0.0;
    if !h.landed() {
        h.body.set_vy(-2.0);
    }
    let b = h.body.bottom();
    h.set_height(h.roll.rolling_height);
    h.body.set_bottom(b);
    crate::audio::play(g, "hero-roll");
    crate::awards::jumped(g); // roll.onStart
    crate::missions::add_stat(g, 1, "mission-roll");
}

/// `Gp.regularHeight = h` (41539): also the body height.
pub fn set_regular_height(h: &mut Hero, v: f64) {
    h.regular_height = v;
    h.set_height(v);
}

/// `tp.cancel`.
pub fn roll_cancel(h: &mut Hero) {
    roll_end(h);
}

/// `tp.lock` (38574): unconditional `end()`, then locked.
pub fn roll_lock(h: &mut Hero) {
    roll_end(h);
    h.roll.locked = true;
}

/// `jf.lock` (37254): `end()`, then locked.
pub fn jump_lock(h: &mut Hero) {
    jump_end(h);
    h.jump.locked = true;
}

/// `Gp.restoreSize` (41742): 4 x regularHeight x 4, center kept.
pub fn restore_size(h: &mut Hero) {
    h.body.set_sx(4.0);
    let rh = h.regular_height;
    h.set_height(rh);
    h.body.set_sz(4.0);
}

/// `tp.end` (also `cancel`): height back around the same center, clamped
/// to the ground.
fn roll_end(h: &mut Hero) {
    h.roll.is_rolling = false;
    let rh = h.regular_height;
    h.set_height(rh);
    if h.body.bottom() < h.ground {
        h.body.set_bottom(h.ground);
    }
    h.roll.time = 0.0;
}

fn roll_update(g: &mut Game) {
    let ft = g.delta;
    let h = &mut g.hero;
    if h.roll.is_rolling {
        h.roll.time += ft;
        if h.roll.time > h.roll.duration {
            h.roll.time = h.roll.duration;
        }
        if h.roll.time == h.roll.duration {
            roll_end(h);
        }
    }
}

// ---- pogo (Kf) -------------------------------------------------------------------------

/// `Kf.turnOn` (38102), called when the pogo pickup is collected (inside a
/// physics sub-step).
pub fn pogo_turn_on(g: &mut Game) {
    g.hero.pogo.hangtime = false;
    crate::powerups::sneakers_pause(g);
    crate::powerups::jetpack_turn_off(g);
    let h = &mut g.hero;
    h.player.dizzy = 0.0; // player.dizzyEnd()
    crate::powerups::hoverboard_pause(g);
    let h = &mut g.hero;
    roll_end(h); // roll.cancel()
    jump_end(h); // jump.lock()
    h.jump.locked = true;
    if h.player.camera_low != 0.0 {
        exit_camera_low(g);
    }
    g.camera.controlled = true;
    let h = &mut g.hero;
    h.body.ghost = true;
    hero_play_any(g, anim_sites::POGO_ON, &["pogostick_kicking"], PlayOpts::default());
    crate::fx::pogo_show(g); // show() -> showEffects
    let h = &mut g.hero;
    h.pogo.position = h.body.center();
    h.pogo.position_end = h.pogo.position.z - POGO_JUMP_DISTANCE;
    h.pogo.count = 1;
    let start = h.body.center();
    crate::mount::pogo_turn_on(g, start); // spawnCoins
    g.hero.pogo.camera_start_y = g.camera.rig.main_y;
    g.hero.pogo.roll_listener = true;
}

/// `Kf.turnOff` (38186).
pub fn pogo_turn_off(g: &mut Game) {
    crate::fx::pogo_hide(g); // hide() -> hideEffects, always
    if g.hero.pogo.count == 0 {
        return;
    }
    let h = &mut g.hero;
    h.pogo.roll_listener = false;
    h.body.ghost = false;
    h.jump.locked = false; // jump.unlock()
    h.jump.gravity = GRAVITY;
    g.camera.controlled = false;
    crate::powerups::hoverboard_resume(g);
    crate::powerups::sneakers_resume(g);
    g.hero.pogo.count = 0;
}

/// `Kf.update` (38129): the body follows the jump curve and the pogo drives
/// the camera rig.
fn pogo_update(g: &mut Game) {
    if g.hero.pogo.count == 0 {
        return;
    }
    let ft = g.delta;
    let bz = g.hero.body.cz();
    let speed = g.stats_speed();
    if bz > g.hero.pogo.position_end {
        let p = &g.hero.pogo;
        let t = -(bz - speed * ft - p.position.z) / POGO_JUMP_DISTANCE;
        let bottom = p.position.y + crate::mount::curve_eval(&crate::mount::POGO_CURVE, t) * POGO_JUMP_HEIGHT;
        g.hero.body.set_bottom(bottom);
        if t > POGO_HANGTIME_POSITION && !g.hero.pogo.hangtime {
            g.hero.pogo.hangtime = true;
            crate::fx::pogo_hide(g);
            hero_play_any(
                g,
                anim_sites::POGO_HANGTIME,
                &["pogostick_Hangtime_flying", "pogostick_Hangtime_kick", "pogostick_Hangtime_front_flip1"],
                PlayOpts::default(),
            );
        }
        let k = ease::sine_in(t) * 0.8;
        let cam_x = g.stats_x * CAMERA_MOD_X;
        let cam_ty = g.stats_y + CAMERA_POS_Y * k;
        let cam_z = g.stats_z + CAMERA_POS_Z;
        let e = ease::expo_out(t);
        g.hero.player.camera_y = cam_ty;
        let r = &mut g.camera.rig;
        r.main_x = cam_x;
        r.main_y = lerp(g.hero.pogo.camera_start_y, cam_ty, e);
        r.main_z = cam_z;
        let dy = r.main_y - g.stats_y;
        let dz = r.main_z - g.stats_z + 50.0;
        r.main_rot_x = dz.atan2(dy) - PI * 0.5;
        g.hero.player.camera_target_y = bottom;
        g.hero.player.camera_y = bottom;
        g.hero.body.set_vy(0.0);
    } else {
        pogo_turn_off(g);
    }
}

// ---- player (Hf) -----------------------------------------------------------------------

/// `Hf.stumble(name, cause, bounce)` (37861).
pub fn player_stumble(g: &mut Game, who: &str, cause: &str, bounce: bool) {
    let p = &g.hero.player;
    if p.bump_count == 0.0 || !bounce {
        g.camera.shake(3.0);
        crate::missions::process_bump(g, who);
        if g.hero.player.dizzy != 0.0 {
            player_die(g, cause);
        } else {
            g.hero.player.dizzy = DIZZY_FRAMES; // dizzyStart
            g.hero.player.dizzy_fx = true; // dizzy.turnOn
            crate::audio::play(g, "hero-stumble");
            g.hero.player.bump_count = 20.0;
        }
    }
}

/// `Hf.crash` (37875).
pub fn player_crash(g: &mut Game, who: &str, cause: &str) {
    g.hero.player.camera_low = 0.0;
    g.hero.player.tunnel = false;
    g.camera.shake(5.0);
    crate::missions::process_bump(g, who);
    player_die(g, cause);
}

/// `Hf.die` (37894): a no-op with `god`. Otherwise the run ends (the
/// game-over flow after this is not ported).
pub fn player_die(g: &mut Game, cause: &str) {
    if g.god {
        return;
    }
    if crate::powerups::hoverboard_shield(g) {
        return;
    }
    let h = &mut g.hero;
    h.body.set_cz(h.body.cz() + 5.0);
    h.player.dead = true;
    h.player.death_cause = cause.to_string();
    h.player.dizzy = 0.0;
    h.lane.on = false;
    h.jump.on = false;
    h.roll.on = false;
    crate::audio::stop(g, "special-jetpack");
    crate::audio::play(g, "hero-death");
    if cause == "train" {
        crate::audio::play_later(g, "hero-death-hitcam", 600.0);
    }
    pogo_turn_off(g);
    crate::powerups::jetpack_turn_off(g);
    crate::hero_fx::shadow(g, false);
    crate::powerups::freeze(g);
    // on the tutorial route: back to the last checkpoint 1 s later
    if g.is_tutorial() {
        crate::tutorial::died(g);
        return;
    }
    // game.gameover(): state GAMEOVER, hero.player.stop()
    g.state = GameState::Gameover;
    g.hero.player.running = false;
    g.hero.body.set_vz(0.0);
    g.camera.tunnel = false;
    crate::flow::gameover(g);
}

/// `Gp.reset` (41722): body at z 0, x 1; every powerup off; the notifier's
/// component resets (body, anim, lane, jump, roll, player).
pub fn hero_reset(g: &mut Game) {
    crate::hero_fx::halo_stop(g); // Gp.init: reviveHalo.stop()
    {
        let b = &mut g.hero.body;
        b.set_cz(0.0);
        b.set_cx(1.0);
        b.set_bottom(0.0);
    }
    g.hero.position = DVec3::new(1.0, g.hero.position.y, 0.0);
    pogo_turn_off(g);
    crate::powerups::magnet_turn_off(g);
    crate::powerups::jetpack_turn_off(g);
    crate::powerups::sneakers_turn_off(g);
    g.hero.player.dizzy = 0.0;
    crate::powerups::multiplier_turn_off(g);
    crate::powerups::hoverboard_turn_off(g);
    let h = &mut g.hero;
    // U.reset
    h.ground = 0.0;
    h.colliding.clear();
    h.body.origin = h.body.bx;
    h.body.reset_velocity();
    h.ground_change_tolerance = 0.0;
    // Pf.reset, jf.reset, tp.reset
    h.lane = Lane::default();
    h.ry = 0.0;
    h.jump = Jump::default();
    roll_end(h);
    h.roll = Roll::default();
    let rh = h.regular_height;
    h.set_height(rh);
    // Zo.reset: currentAction = null; stop() only logs
    player_reset(g, 0.0, 0.0);
}

/// `Player.reset(z, x, lane)` (37693) with `Body.reset`, as used by revive.
pub fn player_reset(g: &mut Game, z: f64, x: f64) {
    let h = &mut g.hero;
    h.ground = 0.0;
    h.colliding.clear();
    h.body.reset_velocity();
    h.body.origin = h.body.bx;
    h.ground_change_tolerance = 0.0;
    h.body.set_cx(x);
    h.body.set_cz(z);
    h.body.set_bottom(0.0);
    h.body.movable = true;
    h.body.ghost = false;
    h.position = h.body.center();
    h.ry = 0.0;
    let p = &mut h.player;
    p.dizzy = 0.0;
    p.running = false;
    p.camera_y = 0.0;
    p.camera_target_y = 0.0;
    p.camera_low = 0.0;
    p.tunnel = false;
    p.dead = false;
    p.death_cause.clear();
    p.catch_mode.clear();
    p.rewind_to = None;
}

pub fn exit_camera_low(g: &mut Game) {
    g.hero.player.camera_low = 0.0;
    g.hero.player.tunnel = false;
    g.camera.tunnel = false; // game.exitTunnel -> sm.exitTunnel
}

/// Collision flags `rn` (2635).
pub mod flags {
    pub const LEFT: u32 = 4;
    pub const TOP: u32 = 8;
    pub const RIGHT: u32 = 16;
    pub const BOTTOM: u32 = 32;
    pub const FRONT: u32 = 64;
    pub const BACK: u32 = 128;
    pub const SLOPE: u32 = 512;
}

/// `Body.collisionEnter` then `Player.onCollisionEnter` (5154, 37946).
/// `who`: the other entity's class name, lower case (`constructor.name`).
pub fn collision_enter(g: &mut Game, who: &str, other_movable: bool, f: u32, hit: &Aabb) {
    use flags::*;
    // listeners in add order: sneakers (pp, 38751) before the player
    crate::powerups::sneakers_collision(g, f, hit.y() - hit.height() * 0.5);
    let b = &mut g.hero.body;
    if f & LEFT != 0 || f & RIGHT != 0 {
        b.set_vx(0.0);
    }
    if f & TOP != 0 || f & BOTTOM != 0 {
        b.set_vy(0.0);
    }
    if (f & FRONT != 0 || f & BACK != 0) && hit.width() > 3.5 {
        b.set_vz(0.0);
    }
    if f & FRONT != 0 {
        g.hero.body.set_vz(0.0);
        if g.hero.lane.changing || hit.height() < 1.0 {
            player_stumble(g, who, "bounce", true);
        } else if other_movable {
            player_crash(g, who, "train");
        } else if hit.height() > 6.0 {
            player_crash(g, who, "bounce");
        } else if hit.y() > g.hero.body.cy() {
            player_crash(g, who, "upper");
        } else {
            player_crash(g, who, "lower");
        }
    } else if f & LEFT != 0 || f & RIGHT != 0 {
        let step = -g.hero.lane.abs_step;
        lane_bump(g, who, step);
    } else if f & SLOPE != 0 {
        player_stumble(g, who, "bounce", true);
        let vz = g.hero.body.vz();
        g.hero.body.set_vz(vz * 0.5);
    }
}

/// `Player.onTriggerEnter` / `onTriggerExit` (37970-37995).
pub fn trigger(g: &mut Game, other: EntityId, enter: bool) {
    // tutorialTrigger (`No`): enterTrigger / exitTrigger(type)
    if g.ent(other).cls == crate::entities::Cls::No {
        let t = g.ent(other).tutorial_type.clone();
        if enter {
            crate::tutorial::enter_trigger(g, &t);
        } else {
            crate::tutorial::exit_trigger(g, &t);
        }
        return;
    }
    if enter && g.ent(other).cls == crate::entities::Cls::Rr {
        crate::missions::add_stat(g, 1, "mission-dodge");
        return;
    }
    let low_camera = matches!(g.ent(other).cls, crate::entities::Cls::Gate(_) | crate::entities::Cls::Li);
    if !low_camera {
        return;
    }
    if enter {
        if g.hero.jetpack.is_on() {
            return;
        }
        g.hero.player.camera_low = -15.0;
        g.hero.player.tunnel = true;
        g.camera.tunnel = true; // game.enterTunnel -> sm.enterTunnel
    } else {
        exit_camera_low(g);
    }
}

// ---- frame phases ------------------------------------------------------------------------

/// `Gp.onStateUpdate` (41754) + `gp.update` (39088).
fn state_update(g: &mut Game) {
    if !g.hero.fsm.ready {
        g.hero.fsm.init();
    }
    let running = g.state == GameState::Running;
    let h = &mut g.hero;
    let hangtime = h.hangtime();
    let p = Params {
        landed: h.landed(),
        ascending: !hangtime && h.ascending(),
        descending: !hangtime && h.descending(),
        rolling: h.roll.is_rolling,
        dead: !h.player.death_cause.is_empty(),
        catch: !h.player.catch_mode.is_empty(),
        hoverboard: h.hoverboard.is_on(),
        dodging: h.lane.changing || h.jetpack.is_dodging(),
        playing: running && h.player.death_cause.is_empty(),
        jetpack: h.jetpack.is_on(),
        special: h.jetpack.is_on() || h.pogo.is_on(),
        grinding: false,
        dizzy: h.fsm.params.dizzy,
    };
    let p = Params { grinding: crate::powerups::hoverboard_grinding(g), ..p };
    let h = &mut g.hero;
    h.fsm.params = p;
    let next = STATES.iter().find(|&&(id, keys)| keys.iter().all(|&(k, v)| p.get(k) == v) && h.fsm.current != id && h.fsm.can(id)).map(|&(id, _)| id);
    if let Some(id) = next {
        fsm_set(g, id);
    }
    // Fp.update: on the rail's grind loop, the sparks emit
    if g.hero.fsm.current == "grinding" {
        let h = &g.hero.hoverboard;
        let clip = h.anims.grind.get(h.grind_index).copied().unwrap_or("");
        if g.hero_anim.as_ref().is_some_and(|a| a.clip_name == clip) {
            crate::powerups::hoverboard_update_grinding(g);
        }
    }
    // the current state's update: running keeps the clip at animationSpeed (Pp.update)
    if g.hero.fsm.current == "running" {
        let s = if g.hero.hoverboard.is_on() { 1.0 } else { g.animation_speed() };
        if let Some(a) = g.hero_anim.as_mut() {
            a.speed = s;
        }
        // in sneakers mode, a step sound every 25 frame units, left / right
        let st = &mut g.hero.steps;
        if st.on {
            st.count -= g.delta;
            if st.count <= 0.0 {
                st.count = 25.0;
                st.alt = !st.alt;
                let id = if st.alt { "hero-sneakers-foot-l" } else { "hero-sneakers-foot-r" };
                crate::audio::play(g, id);
            }
        }
    }
}

/// RNG call sites of clip choices (`Zo.play` with an array).
pub mod anim_sites {
    use crate::rng::Site;
    pub const DESCENDING: Site = Site("at Zo.play (assets/index-QNpTjs8S.js:1:544574) < at zp.begin (assets/index-QNpTjs8S.js:1:944730)");
    pub const ASCENDING: Site = Site("at Zo.play (assets/index-QNpTjs8S.js:1:544574) < at Lp.begin (assets/index-QNpTjs8S.js:1:943524)");
    pub const HANGTIME: Site = Site("at Zo.play (assets/index-QNpTjs8S.js:1:544574) < at Rp.begin (assets/index-QNpTjs8S.js:1:944166)");
    pub const POGO_ON: Site = Site("at Zo.play (assets/index-QNpTjs8S.js:1:544574) < at Kf.turnOn (assets/index-QNpTjs8S.js:1:875176)");
    pub const POGO_HANGTIME: Site = Site("at Zo.play (assets/index-QNpTjs8S.js:1:544574) < at Kf.update (assets/index-QNpTjs8S.js:1:876480)");
}

/// Play a hero clip (`hero.anim.play`).
pub fn hero_play(g: &mut Game, clip: &str, o: PlayOpts) {
    if let Some(a) = g.hero_anim.as_mut() {
        a.play(clip, o);
    }
}

/// Play one of several hero clips (one draw at `site`).
pub fn hero_play_any(g: &mut Game, site: crate::rng::Site, clips: &[&str], o: PlayOpts) {
    if let Some(a) = g.hero_anim.as_mut() {
        a.play_any(g.rng.as_mut(), site, clips, o);
    }
}

/// `state.set(id)`: on a change, the new state's `begin` (its clip).
pub fn fsm_set(g: &mut Game, id: &'static str) {
    if !(g.hero.fsm.ready && g.hero.fsm.can(id)) {
        return;
    }
    g.hero.fsm.set(id);
    // Pp.begin: count 0, steps only in sneakers mode
    if id == "running" {
        let on = mode(g) == Mode::Sneakers;
        g.hero.steps.on = on;
        g.hero.steps.count = 0.0;
    }
    state_begin(g, id);
}

/// `Pp`'s footstep state.
#[derive(Clone, Debug, Default)]
pub struct Steps {
    pub on: bool,
    pub count: f64,
    pub alt: bool,
}

/// `Hf.getMode()` (37820): hoverboard, then sneakers, else normal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Sneakers,
    Hoverboard,
}

pub fn mode(g: &Game) -> Mode {
    if g.hero.hoverboard.is_on() {
        Mode::Hoverboard
    } else if g.hero.sneakers.is_on() {
        Mode::Sneakers
    } else {
        Mode::Normal
    }
}

/// Pose state `begin` (41156-41469).
fn state_begin(g: &mut Game, id: &str) {
    let speed = g.animation_speed();
    let mode = mode(g);
    if id == "jetpack" || id == "jetpackDodging" {
        crate::powerups::jetpack_state_begin(g, id);
        return;
    }
    if mode == Mode::Hoverboard {
        crate::powerups::hoverboard_state_begin(g, id);
        return;
    }
    match id {
        "idle" => hero_play(g, "paintIdle", PlayOpts::looping().sudden()),
        "running" if mode == Mode::Sneakers => hero_play(g, "superRun", PlayOpts::looping()),
        "running" => hero_play(g, "run3", PlayOpts::looping()),
        "hangtime" if mode == Mode::Sneakers => hero_play_any(g, anim_sites::HANGTIME, &["hangtime", "hangtime2", "hangtime3"], PlayOpts::looping()),
        "dodging" => {
            let clip = if g.hero.lane.abs_step < 0 { "dodgeLeft" } else { "dodgeRight" };
            hero_play(g, clip, PlayOpts::default().speed(speed).sudden());
        }
        "ascending" => hero_play_any(g, anim_sites::ASCENDING, &["jump", "jump2", "jump3", "jump_salto"], PlayOpts::default()),
        "descending" => hero_play_any(g, anim_sites::DESCENDING, &["hangtime", "hangtime2", "hangtime3"], PlayOpts::looping()),
        "rolling" => hero_play(g, "roll", PlayOpts::default().speed(1.0).sudden()),
        "dead" => {
            let clip = match g.hero.player.death_cause.as_str() {
                "upper" => "death_upper",
                "lower" => "death_lower",
                "train" => "death_movingTrain",
                _ => "death_bounce",
            };
            hero_play(g, clip, PlayOpts::default().sudden());
        }
        "caught" => {
            let clip = format!("Avatar{}", g.hero.player.catch_mode);
            hero_play(g, &clip, PlayOpts::default().sudden());
        }
        // hangtime plays nothing in normal mode; empty is transitional
        _ => {}
    }
}

/// Hero component updates, in `Gp` add order.
pub fn update(g: &mut Game) {
    // anim (Zo.update) runs before the pose FSM
    let ft = g.delta;
    if let Some(a) = g.hero_anim.as_mut() {
        a.update(ft);
        // the face controller follows the advance (Zo.update)
        if a.active_flag {
            g.hero_face.update(ft, g.rng.as_mut());
        }
    }
    state_update(g);
    // the dizzy stars' component (`$o.update`)
    let p = &mut g.hero.player;
    if p.dizzy_fx && p.dizzy > 0.0 {
        p.dizzy_spin += ft * 0.05;
    }
    crate::powerups::update_before_pogo(g);
    pogo_update(g);
    crate::powerups::update_after_pogo(g);
    lane_update(g);
    jump_update(g);
    roll_update(g);
    crate::hero_fx::update(g);
}

/// Component render: `Body.render` (position copy), then `Player.render`.
pub fn render(g: &mut Game) {
    crate::tutorial::rewind(g);
    let ft = g.delta;
    let speed = g.stats_speed();
    let h = &mut g.hero;
    h.position = h.body.center();
    let p = &h.player;
    if p.running && !p.dead {
        if !h.jetpack.is_on() {
            let vz = h.body.vz();
            h.body.set_vz(lerp(vz, -speed, ft * 0.1));
        }
        let (ground, bottom, landed) = (h.ground, h.body.bottom(), h.landed());
        if ground >= h.player.camera_target_y && landed {
            h.player.camera_target_y = ground;
        } else if bottom < h.player.camera_target_y {
            h.player.camera_target_y = bottom;
        }
    } else {
        h.body.set_vz(0.0);
    }
    let p = &mut h.player;
    p.camera_y = lerp(p.camera_y, p.camera_target_y, ft * 0.2);
    if p.camera_y > h.body.bottom() + 3.0 {
        p.camera_y = h.body.bottom() + 3.0;
    }
    if p.dizzy != 0.0 {
        p.dizzy -= ft;
        if p.dizzy <= 0.0 {
            p.dizzy = 0.0;
        }
    }
    if p.bump_count != 0.0 && p.running && !p.dead {
        p.bump_count -= ft;
        if p.bump_count <= 0.0 {
            p.bump_count = 0.0;
        }
    }
}

/// Short state name for HUDs.
pub fn describe(g: &Game) -> String {
    format!("{} lane {} z {:.1}", g.hero.fsm.current, g.hero.lane.lane, g.hero.body.cz())
}

// keep `camera` referenced for docs links
#[allow(unused_imports)]
use camera as _camera_docs;
