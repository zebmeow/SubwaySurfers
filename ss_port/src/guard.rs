//! The pursuers: guard `im` (deobfuscated.js:42359) with its chaser
//! component `nm` (42190) and state machine `qp` (41919), and the dog `rm`
//! (42308). See docs/js_notes/actors_guard.md.
//!
//! The guard follows `stats` (the hero's render position): x eases toward
//! `stats.x`, z = `stats.z + distance`, and the distance tweens to the
//! current state's target (near 10 while the hero is dizzy, far 70, goAway
//! 100, catch 0). Bodies are deco: physics never moves or hits them.

use crate::anim::PlayOpts;
use crate::camera::{ease, lerp};
use crate::game::{Game, GameState};
use crate::rng::Site;

/// `tm.enter`'s `R.pick(...["_Caught_Shoulder"])`.
pub const CATCH_PICK: Site = Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at tm.enter (assets/index-QNpTjs8S.js:1:973005)");

/// Guard body height (6 x 14 x 6): center = bottom + 7.
const HALF_HEIGHT: f64 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Disabled,
    Intro,
    Near,
    Far,
    GoAway,
    Catch,
}

impl State {
    /// Trace / JS state name.
    pub fn name(self) -> &'static str {
        match self {
            State::Disabled => "disabled",
            State::Intro => "intro",
            State::Near => "near",
            State::Far => "far",
            State::GoAway => "goAway",
            State::Catch => "catch",
        }
    }
    /// (target distance, duration in seconds, curve) of the state's tween.
    fn tween(self) -> (f64, f64, Curve) {
        match self {
            State::Disabled => (9999.0, 0.1, Curve::Linear),
            State::Intro => (10.0, 0.01, Curve::Linear),
            State::Near => (10.0, 0.6, Curve::SineOut),
            State::Far => (70.0, 3.0, Curve::SineIn),
            State::GoAway => (100.0, 0.5, Curve::SineIn),
            State::Catch => (0.0, 0.3, Curve::SineOut),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    Linear,
    SineIn,
    SineOut,
}


#[derive(Clone, Debug)]
pub struct Guard {
    pub enabled: bool,
    pub state: State,
    pub time: f64,
    pub duration: f64,
    pub start: f64,
    pub end: f64,
    pub curve: Curve,
    pub distance: f64,
    /// Body center, f32 stores.
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub last_ground: f64,
    /// Entity active (in view at all).
    pub active: bool,
    /// Guard mesh + its mixer: `distance < 30` (or forced by catch).
    pub model_active: bool,
    pub dog_active: bool,
    /// Frames until the dog's 100 ms scale timer fires (it sits at 0.01).
    pub dog_scale_timer: Option<u32>,
    pub dog_scale: f64,
}

impl Default for Guard {
    /// `im` construction + `nm.reset()`.
    fn default() -> Self {
        Self {
            enabled: false,
            state: State::Disabled,
            time: 0.0,
            duration: 0.1,
            start: 0.0,
            end: 9999.0,
            curve: Curve::Linear,
            distance: 0.0,
            x: 0.0,
            y: HALF_HEIGHT as f32,
            z: 999.0,
            last_ground: 0.0,
            active: false,
            model_active: false,
            dog_active: false,
            dog_scale_timer: None,
            dog_scale: 1.0,
        }
    }
}

/// `guard.anim.play` and `guard.dog.anim.play`.
fn play(g: &mut Game, guard: &str, dog: &str, looping: bool, dog_looping: bool, sudden: bool) {
    let o = |l: bool| PlayOpts { looping: l, sudden, ..PlayOpts::default() };
    if let Some(a) = g.guard_anim.as_mut() {
        a.play(guard, o(looping));
    }
    if let Some(a) = g.dog_anim.as_mut() {
        a.play(dog, o(dog_looping));
    }
}

/// `distance` setter: z follows `stats.z` rigidly; the guard mesh shows
/// only within 30 units.
fn set_distance(g: &mut Game, d: f64) {
    let z = g.stats_z + d;
    let gd = &mut g.guard;
    gd.distance = d;
    gd.z = z as f32;
    gd.model_active = d < 30.0;
}

/// `qp.set`: enter a state (`Yp.enter` + the state's side effects).
fn set_state(g: &mut Game, s: State) {
    if g.guard.state == s {
        return;
    }
    let (end, duration, curve) = s.tween();
    let gd = &mut g.guard;
    gd.state = s;
    gd.start = gd.distance;
    gd.end = end;
    gd.duration = duration;
    gd.curve = curve;
    gd.time = 0.0;
    match s {
        State::Intro => {
            crate::audio::play(g, "guard-start");
            play(g, "Guard_playIntro", "Dog_playIntro", false, false, true)
        }
        State::Near => crate::audio::play(g, "guard-proximity"),
        State::Catch => {
            crate::audio::play(g, "guard-catch");
            let anims = ["_Caught_Shoulder"];
            let i = crate::game::r_index(g.rng.as_mut(), CATCH_PICK, anims.len());
            g.guard.model_active = true;
            let (gc, dc) = (format!("Guard{}", anims[i]), format!("Dog{}", anims[i]));
            play(g, &gc, &dc, false, false, true);
            g.hero.player.catch_mode = anims[i].to_string();
            let hc = format!("Avatar{}", anims[i]);
            crate::hero::hero_play(g, &hc, PlayOpts::default().sudden());
            crate::missions::add_stat(g, 1, "mission-get-caught");
        }
        _ => {}
    }
}

/// `im.playIntro` (in the run-with-intro continuation, after the hero's).
pub fn play_intro(g: &mut Game) {
    g.guard.active = true;
    g.guard.dog_active = true;
    // rm.playIntro: scale 0.01, a 100 ms timer back to 1 (fires after f13)
    g.guard.dog_scale = 0.01;
    g.guard.dog_scale_timer = Some(7);
    set_distance(g, 50.0);
    g.guard.time = 0.01;
    g.guard.duration = 0.01;
    g.guard.enabled = true;
    set_state(g, State::Intro);
}

/// `im.reset` / `nm.reset` (revive, restart): park the guard, disabled.
pub fn reset(g: &mut Game) {
    let gd = &mut g.guard;
    gd.x = 0.0;
    gd.z = 999.0;
    gd.y = HALF_HEIGHT as f32;
    gd.active = false;
    gd.enabled = false;
    gd.last_ground = 0.0;
    gd.dog_active = false;
    set_state(g, State::Disabled);
}

/// `im.run` / `rm.run` (run start, revive, restart).
pub fn run(g: &mut Game) {
    let gd = &mut g.guard;
    gd.active = true;
    gd.dog_active = true;
    gd.enabled = true;
    play(g, "Guard_run", "Dog_run", true, true, false);
}

/// Conditions of table 3.3, in `add` order; the first true one wins.
fn next_state(g: &Game) -> State {
    let p = &g.hero.player;
    let cur = g.guard.state;
    let running = g.state == GameState::Running;
    let tutorial = g.is_tutorial();
    if !p.dead && !p.running && cur != State::Intro {
        return State::Disabled;
    }
    if !p.running && cur == State::Disabled {
        return State::Intro;
    }
    if !tutorial && running && p.dizzy != 0.0 && cur != State::Disabled && cur != State::Catch {
        return State::Near;
    }
    if tutorial || (running && p.dizzy == 0.0 && cur == State::Near) {
        return State::Far;
    }
    if p.death_cause == "train" || (running && g.guard.distance > 60.0) {
        return State::GoAway;
    }
    if p.death_cause != "train" && p.dead {
        return State::Catch;
    }
    cur
}

/// `nm.update` (component phase, after the hero's components).
pub fn update(g: &mut Game) {
    if !g.guard.enabled {
        return;
    }
    // current state's update: near / far keep requesting run or jump clips
    if matches!(g.guard.state, State::Near | State::Far) {
        let near = g.guard.state == State::Near;
        if g.hero.landed() {
            play(g, "Guard_run", "Dog_run", true, true, false);
        } else {
            play(g, "Guard_jump", "Dog_jump", false, near, false);
        }
    }
    let s = next_state(g);
    set_state(g, s);
    let (delta, ds) = (g.delta, g.delta_secs);
    let gd = &mut g.guard;
    if gd.time <= gd.duration {
        gd.time += ds;
        if gd.time > gd.duration {
            gd.time = gd.duration;
        }
    }
    gd.x = lerp(gd.x as f64, g.stats_x, delta * 0.5) as f32;
    let h = &g.hero;
    if h.landed() {
        gd.last_ground = h.ground;
    }
    let bottom = gd.last_ground + (h.body.bottom() - gd.last_ground) * 0.5;
    gd.y = (bottom + HALF_HEIGHT) as f32;
    if gd.y as f64 - HALF_HEIGHT > h.body.bottom() {
        gd.y = (h.body.bottom() + HALF_HEIGHT) as f32;
    }
    let r = gd.time / gd.duration;
    let k = match gd.curve {
        Curve::Linear => r,
        Curve::SineIn => ease::sine_in(r),
        Curve::SineOut => ease::sine_out(r),
    };
    let d = lerp(gd.start, gd.end, k);
    set_distance(g, d);
    // guard anim (Zo.update) runs only while the mesh is shown (distance < 30)
    let ft = g.delta;
    if let Some(a) = g.guard_anim.as_mut() {
        a.active_flag = g.guard.model_active;
        a.update(ft);
    }
}

/// The dog's component update (its own `Zo.update`, every frame).
pub fn dog_update(g: &mut Game) {
    if !g.guard.dog_active {
        return;
    }
    let ft = g.delta;
    if let Some(a) = g.dog_anim.as_mut() {
        a.update(ft);
    }
}

/// End-of-frame timers (the dog's 100 ms scale-up).
pub fn after_frame(g: &mut Game) {
    if let Some(n) = g.guard.dog_scale_timer {
        if n <= 1 {
            g.guard.dog_scale = 1.0;
            g.guard.dog_scale_timer = None;
        } else {
            g.guard.dog_scale_timer = Some(n - 1);
        }
    }
}
