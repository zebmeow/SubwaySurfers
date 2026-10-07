//! Physics system `Sg` (deobfuscated.js:48777-49038): sub-stepped movement,
//! hero collisions, ground sensor, collectibles and triggers.
//!
//! See docs/js_notes/hero_physics.md. The frame is split into
//! `clamp(ceil(delta) + 1 + ceil(|hero.vz|), 1, 60)` sub-steps; in each the
//! hero moves first, then every physics entity is visited in reverse add
//! order (movables move, then broadphase, hit test, response).

use crate::entities::{Aabb, Cls, EntityId};
use crate::game::{Game, GameState, LogEvent};
use crate::hero::{self, flags};

const PHYSICS_EXTRA_STEPS: f64 = 1.0;
const PHYSICS_MAX_STEPS: i64 = 60;

/// `Sg.entityAddedToScene`: bodies that are not deco (the hero is separate).
pub fn entity_added(g: &mut Game, id: EntityId) {
    let ok = g.ent(id).body.as_ref().is_some_and(|b| !b.deco);
    if ok && !g.phys.contains(&id) {
        g.phys.push(id);
    }
}

/// `Sg.entityRemovedFromScene`.
pub fn entity_removed(g: &mut Game, id: EntityId) {
    if !g.ent(id).body.as_ref().is_some_and(|b| !b.deco) {
        return;
    }
    if let Some(i) = g.phys.iter().position(|&e| e == id) {
        g.phys.remove(i);
    }
}

/// `Sg.postupdate` (48815).
pub fn postupdate(g: &mut Game) {
    let delta = g.delta;
    if g.state != GameState::Running && !g.hero.player.dead {
        return;
    }
    if !g.hero.player.dead {
        g.hero.ground_before = g.hero.ground;
        g.hero.ground = 0.0;
    }
    let steps = ((delta.ceil() + PHYSICS_EXTRA_STEPS + g.hero.body.vz().abs().ceil()) as i64).clamp(1, PHYSICS_MAX_STEPS);
    let sd = delta / steps as f64;
    let mut n = steps;
    while n > 0 && !g.phys_has_reset {
        n -= 1;
        g.hero.move_body(sd);
        let mut i = g.phys.len();
        while i > 0 {
            i -= 1;
            // a hit can remove entities (a pickup collected, a crash clearing
            // obstacles), so `i` may be past the end: the original's `!e`
            let Some(&id) = g.phys.get(i) else { continue };
            if !g.ent(id).active {
                continue;
            }
            if g.ent(id).body.as_ref().unwrap().movable {
                g.body(id).integrate(sd, 0.0);
            }
            if g.phys_has_reset {
                break;
            }
            let ob = g.ent(id).body.as_ref().unwrap().clone();
            let hb = &g.hero.body;
            if ob.back() < hb.cz() - 6.0
                || ob.front() > hb.cz() + 3.0
                || ob.top() < hb.cy() - 10.0
                || ob.right() < hb.cx() - 5.0
                || ob.left() > hb.cx() + 5.0
            {
                continue;
            }
            if g.state != GameState::Running || g.hero.player.dead {
                break;
            }
            let hit = g.hero.body.bx.hit_test(&ob.bx);
            if let Some(h) = hit {
                if !ob.trigger {
                    resolve_hit(g, id, &h);
                }
            }
            if !ob.ghost && !ob.trigger {
                if g.hero.sensor.hit_test(&ob.bx).is_some() {
                    resolve_ground_sensor_hit(g, id);
                }
            }
            if ob.trigger {
                let idx = g.hero.colliding.iter().position(|&e| e == id);
                match (hit.is_some(), idx) {
                    (false, Some(k)) => {
                        g.hero.colliding.remove(k);
                        g.log.push(LogEvent::Trigger { frame: g.frame, entity: id, enter: false });
                        hero::trigger(g, id, false);
                    }
                    (true, None) => {
                        g.hero.colliding.push(id);
                        g.log.push(LogEvent::Trigger { frame: g.frame, entity: id, enter: true });
                        hero::trigger(g, id, true);
                    }
                    _ => {}
                }
            }
        }
    }
    if g.hero.ground < g.hero.ground_before {
        g.hero.ground_change_tolerance = 8.0;
    }
    g.phys_has_reset = false;
}

fn collectible(cls: Cls) -> bool {
    matches!(cls, Cls::Coin | Cls::Pickup(_))
}

/// `Sg.resolveHit` (48941).
fn resolve_hit(g: &mut Game, id: EntityId, hit: &Aabb) {
    if g.hero.player.dead {
        return;
    }
    let cls = g.ent(id).cls;
    if collectible(cls) {
        collect(g, id);
        return;
    }
    if g.hero.body.ghost {
        return;
    }
    let pas = g.ent(id).body.as_ref().unwrap().clone();
    let h = &g.hero.body;
    if cls == Cls::Ramp && h.right() >= pas.left() && h.left() <= pas.right() {
        return;
    }
    let o = if pas.movable { pas.origin } else { pas.bx };
    let gap = 0.2;
    let mut f = 0;
    let hero = &mut g.hero;
    let b = &mut hero.body;
    if b.cy() > pas.top() && hit.height() <= 6.0 && b.vy() > -1.0 {
        b.set_bottom(pas.top() + gap);
        f |= flags::BOTTOM;
        if hit.height() > 2.0 {
            f |= flags::SLOPE;
        }
    } else if b.origin.bottom() > o.top() {
        b.set_bottom(pas.top() + gap);
        f |= flags::BOTTOM;
    } else if b.origin.left() >= o.right() {
        b.bx.set_left(pas.bx.right() + gap);
        f |= flags::LEFT;
    } else if b.origin.right() <= o.left() {
        b.bx.set_right(pas.bx.left() - gap);
        f |= flags::RIGHT;
    } else if b.origin.top() < o.bottom() {
        b.bx.set_top(pas.bx.bottom() - gap);
        f |= flags::TOP;
    } else if b.origin.front() <= o.back() {
        b.bx.set_front(pas.bx.back() + gap);
        f |= flags::FRONT;
    }
    if f != 0 {
        hero.match_position();
        g.log.push(LogEvent::Collision { frame: g.frame, entity: id, flags: f, hit: *hit });
        let who = g.ent(id).cls.name().to_lowercase();
        hero::collision_enter(g, &who, pas.movable, f, hit);
    }
}

/// `Sg.resolveGroundSensorHit` (49004).
fn resolve_ground_sensor_hit(g: &mut Game, id: EntityId) {
    if g.hero.player.dead {
        return;
    }
    if g.hero.body.ghost {
        g.hero.ground = 0.0;
        return;
    }
    let ob = g.ent(id).body.as_ref().unwrap();
    let v = if g.ent(id).cls == Cls::Ramp {
        ob.sy() * ((ob.back() - g.hero.body.front()) / ob.sz()) + 0.11
    } else {
        ob.top() + 0.11
    };
    if v >= g.hero.ground {
        g.hero.ground = v;
    }
}

/// `Collectible.collect` + `onCollect`.
pub fn collect(g: &mut Game, id: EntityId) {
    if !g.ent(id).active {
        return;
    }
    let center = g.hero.body.center();
    crate::entities::collect(g, id);
    g.log.push(LogEvent::Pickup { frame: g.frame, entity: id, hero: center });
    match g.ent(id).cls {
        Cls::Coin => {
            g.coins += 1;
            crate::missions::add_stat(g, 1, "mission-pickup-coins");
            let arc = g.ent(id).arc;
            crate::audio::coin(g, arc);
            crate::hero_fx::coin_pop(g); // hero.pop.play()
        }
        // Ha.onCollect -> hero[type].turnOn()
        Cls::Pickup(crate::entities::PickupKind::Pogo) => {
            hero::pogo_turn_on(g);
            crate::missions::add_stat(g, 1, "mission-pickup-powerups");
            crate::awards::powerup(g); // onPickupPowerup
        }
        cls => crate::powerups::on_collect(g, cls),
    }
    // Ha.onCollect: every pickup but the jetpack sounds
    if let Cls::Pickup(k) = g.ent(id).cls {
        if k != crate::entities::PickupKind::Jetpack {
            crate::audio::play(g, "pickup-powerup");
        }
    }
    // ... then hero.popPickup.play()
    if let Cls::Pickup(k) = g.ent(id).cls {
        crate::hero_fx::pickup_pop(g, k);
        // ... and for a letter, Fa()
        if k == crate::entities::PickupKind::Letter {
            crate::word_hunt::collect(g);
        }
    }
}
