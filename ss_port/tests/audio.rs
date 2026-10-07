//! Sound commands (`crate::audio`) from replayed runs: what plays and when,
//! the coins' pitch along a jump curve, the sneakers' steps, mute, the train
//! death's delayed hit. (The parity tests prove the sound code leaves the
//! simulation and its RNG streams alone.)

use ss_port::audio::Cmd;
use ss_port::sim_plugin::{DataPaths, KeyQueue, ReplayDriver, Sim};
use ss_port::trace::Trace;
use std::path::Path;
use std::rc::Rc;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

fn replay(name: &str) -> Sim {
    let trace = Rc::new(Trace::load(&root().join(format!("oracle/traces/{name}/trace.jsonl"))).unwrap());
    let seed = trace.meta["seed"].as_i64().unwrap_or(1) as i32;
    let mut s = Sim::new(&DataPaths::from_repo(root()), seed, ss_port::sim_plugin::trace_hunt_letter(&trace), Box::new(ReplayDriver::new(trace.clone()))).unwrap();
    s.configure_from_trace(&trace);
    s
}

/// Every command up to `frames`, with its frame.
fn run(s: &mut Sim, frames: i64) -> Vec<(i64, Cmd)> {
    let mut out = Vec::new();
    while s.game.frame < frames {
        if !s.step() {
            break;
        }
        let f = s.game.frame;
        out.extend(std::mem::take(&mut s.game.sound.cmds).into_iter().map(|c| (f, c)));
    }
    out
}

fn plays<'a>(cmds: &'a [(i64, Cmd)], id: &str) -> Vec<&'a (i64, Cmd)> {
    cmds.iter().filter(|(_, c)| matches!(c, Cmd::Play { id: i, .. } if *i == id)).collect()
}

#[test]
fn a_run_sounds_like_the_original() {
    let mut s = replay("seed1-god-sneakers-jump");
    let cmds = run(&mut s, 2400);
    let g = &s.game;
    // the theme once, with the run
    assert_eq!(cmds.iter().filter(|(_, c)| *c == Cmd::Theme).count(), 1);
    // a chime per coin, at volume 0.5
    let coins = plays(&cmds, "pickup-coin");
    assert_eq!(coins.len() as i64, g.coins as i64);
    assert!(coins.iter().all(|(_, c)| matches!(c, Cmd::Play { volume, rate, .. } if *volume == 0.5 && *rate >= 1.0)));
    // rolls, the sneakers (pickup, jump, alternating steps) and the guard
    assert_eq!(plays(&cmds, "hero-roll").len() as i64, s.game.missions.data["mission-roll"]);
    assert!(plays(&cmds, "pickup-powerup").len() >= 1);
    assert_eq!(plays(&cmds, "hero-sneakers-jump").len(), 3, "three super jumps");
    let (l, r) = (plays(&cmds, "hero-sneakers-foot-l").len(), plays(&cmds, "hero-sneakers-foot-r").len());
    assert!(l > 5 && l.abs_diff(r) <= 1, "steps {l} / {r}");
    assert_eq!(plays(&cmds, "guard-start").len(), 1, "the guard's intro");
    assert!(plays(&cmds, "hero-jump").is_empty(), "no regular jumps here");
}

#[test]
fn jump_curve_coins_rise_in_pitch() {
    // spawned curves number their coins 1, 2, 3 ... (`arc`); line coins 0
    let mut s = replay("seed1-god");
    let mut arcs = std::collections::BTreeSet::new();
    while s.game.frame < 1500 && s.step() {
        for &id in &s.game.level_entities {
            let e = s.game.ent(id);
            if e.cls == ss_port::entities::Cls::Coin && e.in_scene {
                arcs.insert(e.arc);
            }
        }
    }
    assert!(arcs.contains(&0) && arcs.contains(&1) && arcs.contains(&2) && arcs.contains(&3), "{arcs:?}");
    // a collected arc coin: 1 + arc * 0.05 (the one coin of this trace)
    let mut s = replay("seed1-god-default_1_track_mid_var_1");
    let cmds = run(&mut s, 3600);
    let rates: Vec<f32> = plays(&cmds, "pickup-coin").iter().map(|(_, c)| if let Cmd::Play { rate, .. } = c { *rate } else { 0.0 }).collect();
    assert_eq!(rates, vec![1.05], "its one coin is the first of a curve");
}

#[test]
fn muted_plays_nothing_but_the_theme() {
    let mut s = replay("seed1-god");
    s.game.flow.user.muted = true;
    let cmds = run(&mut s, 1500);
    assert!(cmds.iter().all(|(_, c)| !matches!(c, Cmd::Play { .. })), "{cmds:?}");
    assert!(cmds.iter().any(|(_, c)| *c == Cmd::Theme), "the music runs at volume 0");
}

#[test]
fn a_train_death_hits_600_ms_later() {
    // seed 1 without god: the run ends on the train at f185
    let keys = KeyQueue::default();
    let mut s = Sim::new(&DataPaths::from_repo(root()), 1, Some("S".into()), Box::new(keys)).unwrap();
    s.game.god = false;
    let cmds = run(&mut s, 200);
    let death = plays(&cmds, "hero-death");
    assert_eq!(death.len(), 1);
    assert_eq!(death[0].0, 185);
    if s.game.hero.player.death_cause == "train" {
        let hit = plays(&cmds, "hero-death-hitcam");
        assert!(matches!(hit[0].1, Cmd::Play { delay_ms, .. } if delay_ms == 600.0));
    }
}

