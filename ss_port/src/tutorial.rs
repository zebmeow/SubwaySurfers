//! The tutorial (`jg`, deobfuscated.js 49736; the arrow `kg` 49625 and the
//! message `Ag` 49675; the player's checkpoint rewind 37759-37860).
//!
//! A player who has not finished it (`gameSettings.tutorial` false) runs
//! the tutorial route (`routeChunk_default_tutorial`, already in the level
//! code: `Game::tutorial_enabled`). Its `Trigger_<type>` entities (`No`)
//! show, on entering, the arrow for up / down / left / right and the message
//! (`tutorial_<type>_desktop`: "Press Arrow Key Up", ...); "hoverboard"
//! unlocks the board (locked for the tutorial's run); leaving "finished"
//! marks the tutorial done in the save. A death on the route does not end
//! the run: 1 s later the hero rewinds to the chunk's checkpoint ahead of
//! him and runs again.
//!
//! Replays and screenshots never enable it (the oracle traces have it off).

use crate::game::Game;

/// `kg`: the arrow sliding through the screen centre.
#[derive(Clone, Debug, PartialEq)]
pub struct Arrow {
    /// 0 up, 1 right, 2 down, 3 left (rotation = pi/2 x this).
    pub dir: u8,
    pub time: f64,
    pub duration: f64,
    /// `await nextFrame()`: visible after the next frame's update.
    pub pending: bool,
}

impl Arrow {
    /// `img.y`: 300 -> -300 over the duration.
    pub fn y(&self) -> f64 {
        300.0 - 600.0 * (self.time / self.duration)
    }
}

/// `Ag`: the message (Lilita 50, white, black stroke 5).
#[derive(Clone, Debug, PartialEq)]
pub struct Msg {
    pub text: String,
    pub time: f64,
    pub duration: f64,
    /// 300 under an arrow, else 0.
    pub y: f64,
    pub showing: bool,
    /// `scale.y`: 0 -> 1 in 0.01 s, back to 0 in 0.1 s when hidden.
    pub scale_y: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Tutorial {
    pub arrow: Option<Arrow>,
    pub msg: Option<Msg>,
    /// `view` is on screen (`show` at the run).
    pub shown: bool,
}

/// `tutorial_<type>_desktop` / `tutorial_<type>` (lang-en.json).
fn text(t: &str) -> Option<&'static str> {
    Some(match t {
        "good" => "Fresh Moves!",
        "hoverboard" => "Press Space for\nHoverboard",
        "finished" => "You rock! Now GO!",
        "up" => "Press Arrow Key Up",
        "down" => "Press Arrow Key Down",
        "left" => "Press Arrow Key Left",
        "right" => "Press Arrow Key Right",
        _ => return None,
    })
}

/// `game.onRun` -> `jg.run` -> `show`: the board locked, the view on.
pub fn run(g: &mut Game) {
    if g.tutorial_enabled {
        g.tutorial.shown = true;
        g.hero.hoverboard.locked = true;
    }
}

/// `jg.enterTrigger(type)`: not while moving backwards.
pub fn enter_trigger(g: &mut Game, t: &str) {
    if !g.tutorial_enabled || g.distance_delta < 0.0 {
        return;
    }
    let dir = match t {
        "up" => Some(0),
        "right" => Some(1),
        "down" => Some(2),
        "left" => Some(3),
        _ => None,
    };
    // the message reads `arrow.visible` before the new arrow shows (it
    // waits a frame): 300 below only under an arrow already showing
    let arrow_showing = g.tutorial.arrow.as_ref().is_some_and(|a| !a.pending);
    if let Some(dir) = dir {
        let keep = g.tutorial.arrow.as_ref().filter(|a| !a.pending).cloned();
        g.tutorial.arrow = Some(Arrow { dir, time: keep.as_ref().map_or(0.0, |a| a.time), duration: 40.0, pending: true });
    }
    if let Some(s) = text(t) {
        let y = if arrow_showing { 300.0 } else { 0.0 };
        g.tutorial.msg = Some(Msg { text: s.to_string(), time: 0.0, duration: 20.0 + s.chars().count() as f64 * 2.0, y, showing: true, scale_y: 0.0 });
    }
    if t == "hoverboard" {
        g.hero.hoverboard.locked = false;
    }
}

/// `jg.exitTrigger(type)`: "finished" saves the tutorial as done.
pub fn exit_trigger(g: &mut Game, t: &str) {
    if t == "finished" {
        g.flow.user.tutorial = true;
        g.flow.save_user();
    }
}

/// `jg.update(frameTime)` (with the scale tweens in real time).
pub fn update(g: &mut Game) {
    if !g.tutorial_enabled {
        return;
    }
    let ft = g.delta;
    let dt = g.clock.delta_ms / 1000.0;
    if let Some(a) = g.tutorial.arrow.as_mut() {
        if a.pending {
            // shown after this update, from the start
            a.pending = false;
            a.time = 0.0;
        } else {
            a.time += ft;
            if a.time / a.duration > 1.0 {
                g.tutorial.arrow = None;
            }
        }
    }
    if let Some(m) = g.tutorial.msg.as_mut() {
        if m.showing {
            m.scale_y = (m.scale_y + dt / 0.01).min(1.0);
            m.time += ft;
            if m.time > m.duration {
                m.showing = false;
            }
        } else {
            m.scale_y -= dt / 0.1;
            if m.scale_y <= 0.0 {
                g.tutorial.msg = None;
            }
        }
    }
}

/// `Player.goBackToLastCheckPoint`: `run3`, then rewind to the current
/// chunk's checkpoint (`getLastCheckpointByPosition`: the first listed one
/// behind the hero's z, else the first).
pub fn go_back(g: &mut Game) {
    crate::hero::hero_play(g, "run3", crate::anim::PlayOpts::looping());
    let z = g.hero.body.cz();
    let Some(id) = g.current_chunk else { return };
    let Some(c) = g.chunks.iter().find(|c| c.id == id) else { return };
    let mut pick = None;
    for &cp in c.checkpoints.iter().rev() {
        if g.ents[cp.0].body.as_ref().is_some_and(|b| b.cz() > z) {
            pick = Some(cp);
        }
    }
    let Some(cp) = pick.or_else(|| c.checkpoints.first().copied()) else { return };
    let target = g.ents[cp.0].body.as_ref().map(|b| b.center());
    g.hero.player.rewind_to = target;
}

/// `Player.render`'s rewind: the hero is pulled back (z += 4 a frame unit,
/// x and bottom eased to 0) until the checkpoint, then reset and run.
pub fn rewind(g: &mut Game) {
    let Some(end) = g.hero.player.rewind_to else { return };
    let ft = g.delta;
    let h = &mut g.hero;
    h.body.reset_velocity();
    h.body.ghost = true;
    let x = h.body.cx();
    h.body.set_cx(x + (0.0 - x) * (ft * 0.1));
    let b = h.body.bottom();
    h.body.set_bottom(b + (0.0 - b) * (ft * 0.3));
    let z = h.body.cz();
    h.body.set_cz(z + ft * 4.0);
    if h.body.cz() >= end.z - 0.1 {
        crate::hero::lane_reset_public(h);
        h.body.set_cz(end.z);
        h.body.set_vz(0.0);
        h.body.ghost = false;
        g.hero.player.rewind_to = None;
        crate::hero::player_reset(g, end.z, 0.0);
        crate::hero::player_run(g);
    }
}

/// The death on the tutorial route: rewind 1 s later instead of the
/// game over.
pub fn died(g: &mut Game) {
    g.hero.player.rewind_due = Some(g.clock.now + 1000.0);
}

/// The 1 s timeout.
pub fn after_render(g: &mut Game) {
    if g.hero.player.rewind_due.is_some_and(|t| g.clock.now >= t) {
        g.hero.player.rewind_due = None;
        go_back(g);
    }
}
