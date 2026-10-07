//! In-run boost buttons `Wm` (45082-45334): headstart and score booster,
//! offered for the first 8 s of a run when the player owns some, pressed
//! with the button, `v` or `c`. And the gauge `Dm` they light up.
//! See docs/js_notes/boosts_mysterybox.md §1.

use crate::entities::{Cls, PickupKind};
use crate::game::Game;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoostKind {
    Headstart,
    Multiplier,
}

/// One `Wm` button.
#[derive(Clone, Debug, PartialEq)]
pub struct BoostButton {
    pub kind: BoostKind,
    /// Presses this run (max 3).
    pub level: u32,
    /// Slide in (0 -> 1) / out (1 -> 0) progress for drawing.
    pub slide: f64,
    pub leaving: bool,
    /// Seconds since shown (idle flashes) and since the last press (flash).
    pub age: f64,
    pub pressed_at: Option<f64>,
}

/// The gauge `Dm`: visible until (virtual ms), lit slices ("1".."3", "+5".."+7").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gauge {
    pub visible_until: f64,
    pub lit: Vec<String>,
}

pub const SHOW_FRAMES: u64 = 480;

/// `fg.run()` after its 0.2 s delay: headstart first, then the booster.
pub fn hud_run(g: &mut Game) {
    g.hud.boosts.clear();
    g.hud.gauge.lit.clear();
    let u = &g.flow.user;
    let mut kinds = Vec::new();
    if u.headstarts > 0 {
        kinds.push(BoostKind::Headstart);
    }
    if u.score_boosters > 0 {
        kinds.push(BoostKind::Multiplier);
    }
    for kind in kinds {
        g.hud.boosts.push(BoostButton { kind, level: 0, slide: 0.0, leaving: false, age: 0.0, pressed_at: None });
    }
}

/// Per HUD update: animations, and the auto-hide at update 480.
pub fn hud_update(g: &mut Game) {
    let ds = g.delta_secs;
    for b in &mut g.hud.boosts {
        b.age += ds;
        b.slide = if b.leaving { (b.slide - ds / 0.3).max(0.0) } else { (b.slide + ds / 0.3).min(1.0) };
    }
    g.hud.boosts.retain(|b| !(b.leaving && b.slide <= 0.0));
    if g.hud.update_count == SHOW_FRAMES {
        for b in &mut g.hud.boosts {
            b.leaving = true;
        }
    }
}

fn showing(g: &Game, kind: BoostKind) -> Option<usize> {
    g.hud.boosts.iter().position(|b| b.kind == kind && !b.leaving)
}

/// `Wm.activateBoost` (key or click): works while the button shows.
pub fn activate(g: &mut Game, kind: BoostKind) {
    let Some(i) = showing(g, kind) else { return };
    match kind {
        BoostKind::Headstart => {
            if g.hud.boosts[i].level == 3 || g.flow.user.headstarts < 1 {
                return;
            }
            g.hud.boosts[i].level += 1;
            let level = g.hud.boosts[i].level;
            g.stats_time += level as f64 * 20.0;
            crate::powerups::jetpack_turn_on(g, level);
            g.flow.user.headstarts -= 1;
            g.flow.save_user();
            gauge_light(g, &level.to_string(), true);
            crate::missions::set_stat(g, 1, "mission-headstart", crate::missions::SetMode::Increment);
            if level >= 3 || g.flow.user.headstarts <= 0 {
                g.hud.boosts[i].leaving = true;
            }
        }
        BoostKind::Multiplier => {
            if g.hud.boosts[i].level == 3 || g.flow.user.score_boosters < 1 {
                return;
            }
            g.hud.boosts[i].level += 1;
            let add = 4 + g.hud.boosts[i].level;
            g.multiplier = 1.0 + add as f64; // overwrites (6 / 7 / 8)
            g.flow.user.score_boosters -= 1;
            g.flow.save_user();
            gauge_light(g, &format!("+{add}"), false);
            crate::missions::set_stat(g, 1, "mission-scoreBooster", crate::missions::SetMode::Increment);
            if g.hud.boosts[i].level >= 3 || g.flow.user.score_boosters <= 0 {
                g.hud.boosts[i].leaving = true;
            }
        }
    }
    g.hud.boosts[i].pressed_at = Some(g.hud.boosts[i].age);
}

/// `Dm.show(5)` + lowlight that side + highlight the slice.
fn gauge_light(g: &mut Game, slice: &str, headstart: bool) {
    g.hud.gauge.visible_until = g.now_ms() + 5000.0;
    g.hud.gauge.lit.retain(|s| s.starts_with('+') == headstart);
    g.hud.gauge.lit.push(slice.to_string());
}

/// `jetpack.turnOff` -> `lowlightHeadstart`.
pub fn lowlight_headstart(g: &mut Game) {
    g.hud.gauge.lit.retain(|s| s.starts_with('+'));
}

/// `kf.spawnPickups` (37043): magnet, 2x, sneakers in the sky at the landing
/// (spawned on the first press, moved on later ones), lanes shuffled.
pub fn spawn_pickups(g: &mut Game, y: f64, z: f64) {
    const SHUFFLE: crate::rng::Site = crate::rng::Site("at R.shuffle (assets/index-QNpTjs8S.js:1:57171) < at kf.spawnPickups (assets/index-QNpTjs8S.js:1:838275)");
    if g.hero.jetpack.pickups.is_empty() {
        for kind in [PickupKind::Magnet, PickupKind::Multiplier, PickupKind::Sneakers] {
            let p = g.pool_get_init(Cls::Pickup(kind));
            crate::mount::pickup_awake(g, p, kind);
            g.add_child(p);
            g.hero.jetpack.pickups.push(p);
        }
    }
    // R.shuffle: Fisher-Yates from the end
    let mut xs = [-20.0, 0.0, 20.0];
    let mut n = xs.len();
    while n > 0 {
        let j = (g.rng.random(SHUFFLE) * n as f64).floor() as usize;
        n -= 1;
        xs.swap(n, j);
    }
    for (k, &p) in g.hero.jetpack.pickups.clone().iter().enumerate() {
        let b = g.body(p);
        b.set_cx(xs[k]);
        b.set_cy(y);
        b.set_cz(z);
    }
}
