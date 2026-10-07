//! The New High Score screen's moving parts (`Gv`, deobfuscated.js:60939;
//! docs/js_notes/ui_gameover.md §2.8): the speed stripes `Kv` (61076) and
//! the character view's slide-in. The character itself is a 3D render
//! (`Xv`, `crate::render_menu_3d`); this is only its placement.
//!
//! `Kv` is built on the first open (10 `resetStripe`s) and lives on, so
//! later openings continue where the stripes were. Each update a stripe
//! first respawns at x = -256 if it crossed x > 0, then moves `speed` units
//! toward the bg origin along `atan2(y, x)`, which is also its rotation (0
//! until the first update). `R.range` is `Math.random`, one stream per call
//! site in the oracle, so the four sites below match it draw for draw.

use crate::game::Game;
use crate::rng::Site;

pub mod sites {
    use super::Site;
    pub const SCALE_Y: Site = Site("at R.range (assets/index-QNpTjs8S.js:1:56473) < at Kv.resetStripe (assets/index-QNpTjs8S.js:1:1575312)");
    pub const X: Site = Site("at R.range (assets/index-QNpTjs8S.js:1:56473) < at Kv.resetStripe (assets/index-QNpTjs8S.js:1:1575404)");
    pub const Y: Site = Site("at R.range (assets/index-QNpTjs8S.js:1:56473) < at Kv.resetStripe (assets/index-QNpTjs8S.js:1:1575459)");
    pub const SPEED: Site = Site("at R.range (assets/index-QNpTjs8S.js:1:56473) < at Kv.resetStripe (assets/index-QNpTjs8S.js:1:1575539)");
}

/// `Kv` space: 512 x 512 around the bg's centre.
const W: f64 = 512.0;
const H: f64 = 512.0;
pub const STRIPES: usize = 10;

/// One `celebration-stripe.png` sprite (512 x 32, anchor (0.05, 0.5),
/// scale (0.15, `scale_y`), alpha 0.75).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stripe {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub scale_y: f64,
    pub speed: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Celebration {
    /// Empty until the screen first opens.
    pub stripes: Vec<Stripe>,
    /// `character.view`: x, y (design units from the screen centre) and scale.
    pub view: [f64; 3],
    /// Updates since the screen opened (the character's clip time).
    pub ticks: u32,
}

fn range(g: &mut Game, site: Site, a: f64, b: f64) -> f64 {
    a + (b - a) * g.rng.random(site)
}

/// `resetStripe(s, x)`: scale, then x (when not given), y, speed.
fn reset(g: &mut Game, s: &mut Stripe, x: Option<f64>) {
    s.scale_y = range(g, sites::SCALE_Y, 0.07, 0.12);
    s.x = match x {
        Some(x) => x,
        None => range(g, sites::X, -W * 0.5, 0.0),
    };
    s.y = range(g, sites::Y, -H * 0.25, H * 0.5);
    s.speed = range(g, sites::SPEED, 3.0, 5.0);
}

/// `Gv.open`: build `Kv` the first time; the view starts small, right of centre.
pub fn open(g: &mut Game) {
    if g.flow.celebration.stripes.is_empty() {
        let mut v = Vec::with_capacity(STRIPES);
        for _ in 0..STRIPES {
            let mut s = Stripe::default();
            reset(g, &mut s, None);
            v.push(s);
        }
        g.flow.celebration.stripes = v;
    }
    g.flow.celebration.view = [400.0, 50.0, 0.5];
    g.flow.celebration.ticks = 0;
}

/// `Kv.updateTransform` + the view's lerps in `Gv.updateTransform`
/// (`Ticker.system.deltaTime` = 1 per frame).
pub fn tick(g: &mut Game) {
    let mut stripes = std::mem::take(&mut g.flow.celebration.stripes);
    for s in &mut stripes {
        if s.x > 0.0 {
            reset(g, s, Some(-W / 2.0));
        }
        let a = s.y.atan2(s.x);
        s.x -= a.cos() * s.speed;
        s.y -= a.sin() * s.speed;
        s.rotation = a;
    }
    let c = &mut g.flow.celebration;
    c.stripes = stripes;
    let k = 0.005;
    let [x, y, sc] = c.view;
    c.view = [x + (0.0 - x) * k, y + (0.0 - y) * k, sc + (1.0 - sc) * k];
    c.ticks += 1;
}
