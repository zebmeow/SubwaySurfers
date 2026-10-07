//! The hero's shadow, pops and revive halo (`src/hero_fx.rs`): when they
//! start and how they animate, on the oracle-checked runs (the screenshots
//! of these frames match the original; see README).

mod common;

use ss_port::flow::Screen;
use ss_port::game::GameState;
use ss_port::hero::Key;
use ss_port::sim_plugin::{DataPaths, KeyQueue, ReplayDriver, Sim};
use ss_port::trace::Trace;
use std::rc::Rc;

fn replay(name: &str) -> Sim {
    let root = common::root();
    let trace = Rc::new(Trace::load(&root.join(format!("oracle/traces/{name}/trace.jsonl"))).unwrap());
    let seed = trace.meta["seed"].as_i64().unwrap() as i32;
    let mut s = Sim::new(&DataPaths::from_repo(root), seed, ss_port::sim_plugin::trace_hunt_letter(&trace), Box::new(ReplayDriver::new(trace.clone()))).unwrap();
    s.configure_from_trace(&trace);
    s
}

fn step_to(s: &mut Sim, f: i64) {
    while s.next_frame <= f {
        s.step();
    }
}

#[test]
fn shadow_follows_the_ground_while_running() {
    let mut s = replay("seed1-god");
    step_to(&mut s, 300);
    let fx = &s.game.hero_fx;
    assert!(fx.shadow_on, "on since Player.run");
    // the oracle's shadow view at f300: y -4.5 (ground 0, entity y 5.5)
    assert!((fx.shadow_y - -4.5).abs() < 1e-9, "shadow y {}", fx.shadow_y);
}

#[test]
fn pickup_and_coin_pops() {
    let mut s = replay("seed1-god-hoverboard");
    // the hoverboard switching on plays popPickup: 0.5, then 1 -> 21 over 13 frames
    while !s.game.hero_fx.pop_pickup.active() {
        assert!(s.next_frame < 420, "the board switches on around f405");
        s.step();
    }
    // the double tap acts in the controller's system update, before the
    // components: the pop has already run one frame
    let start = s.game.frame;
    assert!((s.game.hero_fx.pop_pickup.count - 12.0).abs() < 1e-6, "{:?}", s.game.hero_fx.pop_pickup);
    step_to(&mut s, start + 3);
    let p = s.game.hero_fx.pop_pickup;
    assert!((p.scale - (1.0 + (1.0 - p.count / 13.0) * 20.0)).abs() < 1e-9 && (p.count - 9.0).abs() < 1e-6, "{p:?}");
    step_to(&mut s, start + 13);
    assert!(!s.game.hero_fx.pop_pickup.active(), "13 frame-units of frameTime");
    // coins: pop for 8 frames, 0.5 -> 1.25
    while !s.game.hero_fx.pop.active() {
        assert!(s.next_frame < 700, "the first coin around f659");
        s.step();
    }
    // collected in physics (after the components): 0.5 this frame
    let start = s.game.frame;
    assert_eq!(s.game.hero_fx.pop.scale, 0.5);
    step_to(&mut s, start + 3);
    let p = s.game.hero_fx.pop;
    assert!((p.scale - (0.5 + (1.0 - p.count / 8.0) * 0.75)).abs() < 1e-9 && (p.count - 5.0).abs() < 1e-6, "{p:?}");
}

#[test]
fn revive_halo_and_shadow_at_death() {
    let root = common::root();
    let keys = KeyQueue::default();
    let mut s = Sim::new(&DataPaths::from_repo(root), 1, Some("S".into()), Box::new(keys.clone())).unwrap();
    s.game.god = false;
    step_to(&mut s, 185);
    assert_eq!(s.game.state, GameState::Gameover);
    assert!(!s.game.hero_fx.shadow_on, "off at death");
    step_to(&mut s, 299);
    assert!(matches!(s.game.flow.screen, Screen::SaveMe { .. }));
    keys.0.borrow_mut().push(Key::Action);
    step_to(&mut s, 300);
    let h = s.game.hero_fx.halo;
    assert!(h.active && h.time == 120.0 && (h.opacity - 0.7).abs() < 1e-9, "{h:?}");
    assert!(s.game.hero_fx.shadow_on, "on again with the run");
    step_to(&mut s, 330);
    let h = s.game.hero_fx.halo;
    assert!(h.active && (h.time - 90.0).abs() < 1e-6 && (h.opacity - (1.0 - 78.0 / 108.0) * 0.6).abs() < 1e-6, "{h:?}");
    step_to(&mut s, 421);
    assert!(!s.game.hero_fx.halo.active, "120 frames");
}
