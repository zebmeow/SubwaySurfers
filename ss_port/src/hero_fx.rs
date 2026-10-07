//! The hero's own little effects (deobfuscated.js 38286-38634), components
//! of the hero entity, drawn in its space by `crate::render_actors`:
//!
//! - `rp` "shadow": an 8x8 `character_shadow` plane (MULTIPLY, double
//!   sided), rx pi/2 at z 1, kept at `ground + 1` (`y = -entity.y + ground
//!   + 1`); on from `Player.run`, off at a (mortal) death.
//! - `Yf` "pop": `star7` (effects-tex, opacity 0.9, SCREEN) at (0, 0, -3),
//!   ry pi and a random rz, scale 0.5 -> 1.25 over 8 frames; every coin.
//! - `Zf` "popPickup": `pow` (effects-tex, opacity 0.95, additive) at the
//!   hero, ry pi, random rz, scale 0.5 then 1 -> 21 over 13 frames; every
//!   pickup (`Ha.onCollect`) and the hoverboard switching on.
//! - `$f` "reviveHalo": `powRevive` (effects-tex, opacity 0.7, SCREEN, no
//!   depth test or write), rx -0.3, ry pi, at y 2, 120 frames: scale
//!   `2 + 0.2 sin(0.1 t)`, opacity fading in to 0.6 then out over the last
//!   tenth, rz += 0.01 per frame; on revive, stopped by the hero's `init`.
//!
//! The random rotations draw `Math.random()` at the oracle's call sites
//! (keyed by the caller's class: V8 names a frame after its receiver).

use crate::entities::PickupKind;
use crate::game::Game;
use crate::rng::Site;
use std::f64::consts::PI;

pub mod sites {
    use super::Site;
    pub const COIN_POP: Site = Site("at Yf.play (assets/index-QNpTjs8S.js:1:882761) < at e.onCollect (assets/index-QNpTjs8S.js:1:177225)");
    pub const HOVERBOARD_POP: Site = Site("at Zf.play (assets/index-QNpTjs8S.js:1:884332) < at e.turnOn (assets/index-QNpTjs8S.js:1:734320)");
    /// `Ha.onCollect` (col 315126) on each pickup class.
    pub const fn pickup_pop(k: crate::entities::PickupKind) -> Site {
        use crate::entities::PickupKind as K;
        match k {
            K::Jetpack => Site("at Zf.play (assets/index-QNpTjs8S.js:1:884332) < at Ua.onCollect (assets/index-QNpTjs8S.js:1:315126)"),
            K::Pogo => Site("at Zf.play (assets/index-QNpTjs8S.js:1:884332) < at Wa.onCollect (assets/index-QNpTjs8S.js:1:315126)"),
            K::Magnet => Site("at Zf.play (assets/index-QNpTjs8S.js:1:884332) < at Ga.onCollect (assets/index-QNpTjs8S.js:1:315126)"),
            K::Sneakers => Site("at Zf.play (assets/index-QNpTjs8S.js:1:884332) < at Ka.onCollect (assets/index-QNpTjs8S.js:1:315126)"),
            K::Multiplier => Site("at Zf.play (assets/index-QNpTjs8S.js:1:884332) < at qa.onCollect (assets/index-QNpTjs8S.js:1:315126)"),
            K::Letter => Site("at Zf.play (assets/index-QNpTjs8S.js:1:884332) < at Ja.onCollect (assets/index-QNpTjs8S.js:1:315126)"),
            K::MysteryBox => Site("at Zf.play (assets/index-QNpTjs8S.js:1:884332) < at Ya.onCollect (assets/index-QNpTjs8S.js:1:315126)"),
            // `Xa.key` is an anonymous class: V8 names it after the property
            K::Key => Site("at Zf.play (assets/index-QNpTjs8S.js:1:884332) < at key.onCollect (assets/index-QNpTjs8S.js:1:315126)"),
        }
    }
}

/// A `Yf` / `Zf` pop: frames left, scale, z rotation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pop {
    pub count: f64,
    pub scale: f64,
    pub rz: f64,
}

impl Pop {
    pub fn active(&self) -> bool {
        self.count > 0.0
    }
}

/// `$f`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Halo {
    pub built: bool,
    pub active: bool,
    pub time: f64,
    pub scale: f64,
    pub opacity: f64,
    pub rz: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeroFx {
    /// `rp`: shown, and its y in the hero's space.
    pub shadow_on: bool,
    pub shadow_y: f64,
    pub pop: Pop,
    pub pop_pickup: Pop,
    pub halo: Halo,
}

const POP_FRAMES: f64 = 8.0;
const PICKUP_FRAMES: f64 = 13.0;
const HALO_FRAMES: f64 = 120.0;

/// `Yf.play` (coin collected).
pub fn coin_pop(g: &mut Game) {
    let rz = PI * 2.0 * g.rng.random(sites::COIN_POP);
    g.hero_fx.pop = Pop { count: POP_FRAMES, scale: 0.5, rz };
}

/// `Zf.play` from `Ha.onCollect`.
pub fn pickup_pop(g: &mut Game, k: PickupKind) {
    let rz = PI * 2.0 * g.rng.random(sites::pickup_pop(k));
    g.hero_fx.pop_pickup = Pop { count: PICKUP_FRAMES, scale: 0.5, rz };
}

/// `Zf.play` from `cf.turnOn`.
pub fn hoverboard_pop(g: &mut Game) {
    let rz = PI * 2.0 * g.rng.random(sites::HOVERBOARD_POP);
    g.hero_fx.pop_pickup = Pop { count: PICKUP_FRAMES, scale: 0.5, rz };
}

/// `$f.play` (`build` on first use: scale 2, opacity 0.7).
pub fn halo_play(g: &mut Game) {
    let h = &mut g.hero_fx.halo;
    if !h.built {
        *h = Halo { built: true, active: false, time: 0.0, scale: 2.0, opacity: 0.7, rz: 0.0 };
    }
    h.active = true;
    h.time = HALO_FRAMES;
}

/// `$f.stop` (the hero's `init`).
pub fn halo_stop(g: &mut Game) {
    if g.hero_fx.halo.built {
        g.hero_fx.halo.time = 0.0;
        g.hero_fx.halo.active = false;
    }
}

/// `rp.turnOn` / `turnOff`.
pub fn shadow(g: &mut Game, on: bool) {
    g.hero_fx.shadow_on = on;
}

fn pop_update(p: &mut Pop, ft: f64, duration: f64, base: f64, gain: f64) {
    if p.count == 0.0 {
        return;
    }
    p.count -= ft;
    p.scale = base + (1.0 - p.count / duration) * gain;
    if p.count <= 0.0 {
        *p = Pop::default();
    }
}

/// The components' updates (hero component phase, `frameTime`).
pub fn update(g: &mut Game) {
    let ft = g.delta;
    // rp: the entity's y is last frame's render position
    if g.hero_fx.shadow_on {
        g.hero_fx.shadow_y = -g.hero.position.y + g.hero.ground + 1.0;
    }
    let fx = &mut g.hero_fx;
    pop_update(&mut fx.pop, ft, POP_FRAMES, 0.5, 0.75);
    pop_update(&mut fx.pop_pickup, ft, PICKUP_FRAMES, 1.0, 20.0);
    let h = &mut fx.halo;
    if h.built && h.time != 0.0 {
        h.time -= ft;
        h.scale = (h.time * 0.1).sin() * 0.2 + 2.0;
        let k = HALO_FRAMES * 0.1;
        let a = if h.time > k { 1.0 - (h.time - k) / (HALO_FRAMES - k) } else { h.time / k };
        h.opacity = a * 0.6;
        h.rz += ft * 0.01;
        if h.time < 0.0 {
            h.time = 0.0;
            h.active = false;
        }
    }
}
