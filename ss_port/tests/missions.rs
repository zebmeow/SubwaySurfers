//! Missions (`crate::missions`, the original's `Bm`): the counters during a
//! replayed run, the one-run counters' reset, a set's completion (the
//! notification, the next set tracked, `missionMultiplier` at the next
//! reset), the save, the skip, and the x30 cap.

mod common;

use ss_port::flow::UserData;
use ss_port::game::Game;
use ss_port::missions::{self, MissionSave, Missions, SetMode};
use ss_port::sim_plugin::{DataPaths, KeyQueue, ReplayDriver, Sim};
use ss_port::trace::Trace;
use std::path::Path;
use std::rc::Rc;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

fn sim() -> Sim {
    let keys = KeyQueue::default();
    Sim::new(&DataPaths::from_repo(root()), 1, Some("S".into()), Box::new(keys)).unwrap()
}

fn replay(name: &str) -> Sim {
    let trace = Rc::new(Trace::load(&root().join(format!("oracle/traces/{name}/trace.jsonl"))).unwrap());
    let seed = trace.meta["seed"].as_i64().unwrap_or(1) as i32;
    let mut s = Sim::new(&DataPaths::from_repo(root()), seed, ss_port::sim_plugin::trace_hunt_letter(&trace), Box::new(ReplayDriver::new(trace.clone()))).unwrap();
    s.configure_from_trace(&trace);
    s
}

fn data(g: &Game, k: &str) -> i64 {
    g.missions.data.get(k).copied().unwrap_or(0)
}

#[test]
fn counters_follow_a_replayed_run() {
    // rolls (Down) from f128; the three jumps (f1155-1455) are Super
    // Sneakers jumps (`pp.jump`), which the original does not count
    let mut s = replay("seed1-god-sneakers-jump");
    let (mut jumps, mut rolls) = (0, 0);
    let (mut was_jumping, mut was_rolling) = (false, false);
    while s.game.frame < 2400 {
        s.step();
        let (j, r) = (s.game.hero.jump.is_jumping, s.game.hero.roll.is_rolling);
        jumps += i64::from(j && !was_jumping);
        rolls += i64::from(r && !was_rolling);
        was_jumping = j;
        was_rolling = r;
    }
    let g = &s.game;
    assert!(g.coins > 0);
    assert_eq!(data(g, "mission-pickup-coins"), g.coins as i64, "every coin counted");
    assert_eq!(data(g, "mission-pickup-coins-onerun"), g.coins as i64);
    assert_eq!(data(g, "mission-score-onerun"), ss_port::flow::score(g), "the run's score");
    assert!(rolls > 20);
    assert_eq!(data(g, "mission-jump"), jumps, "regular jumps only");
    assert_eq!(data(g, "mission-roll"), rolls);
    assert_eq!(data(g, "mission-pickup-sneakers"), 1, "the sneakers picked up");
    assert_eq!(data(g, "mission-roll-onerun"), data(g, "mission-roll"), "one run");
    // the current set's first mission ("Collect 500 Coins") shows the count
    let m = &g.missions.current()[0];
    assert_eq!(m.progress, (g.coins as i64).min(500));
    // set 1's "Score 1000 points in one run" done by then, saved
    let score_m = &g.missions.sets[0][1];
    assert_eq!(score_m.id, "mission-score-onerun");
    assert_eq!(score_m.completed, ss_port::flow::score(g) >= 1000);
    // the obstacles' class names are minified: the bump missions never count
    assert_eq!(data(g, "mission-bump-trains-onerun") + data(g, "mission-bump-barriers") + data(g, "mission-bump-lights"), 0);
}

#[test]
fn completing_set_one_raises_the_multiplier() {
    let mut s = sim();
    let g = &mut s.game;
    assert_eq!(g.missions.multiplier(), 0);
    missions::add_stat(g, 500, "mission-pickup-coins");
    assert!(g.missions.sets[0][0].completed);
    assert_eq!(g.missions.toasts.len(), 1);
    assert_eq!(g.missions.toasts[0].text, "Mission Complete\nCollect 500 Coins");
    missions::set_stat(g, 1000, "mission-score", SetMode::Set);
    assert!(g.missions.sets[0][1].completed, "score 1000 in one run");
    missions::add_stat(g, 2, "mission-pickup-powerups");
    assert!(g.missions.sets[0][2].completed);
    // the set is done: set 2 is tracked at once, the multiplier is read at
    // the next reset (x1 + 1 = x2)
    assert_eq!(g.missions.multiplier(), 1);
    assert_eq!(g.missions.tracked, 1);
    assert_eq!(g.missions.set_label(), 2);
    assert_eq!(g.mission_multiplier, 0.0, "not before the next run");
    g.reset();
    assert_eq!(g.mission_multiplier, 1.0);
    // the save holds set 1's progress
    let saved: Vec<(String, i64)> = g.flow.user.missions[0].iter().map(|m| (m.id.clone(), m.progress)).collect();
    assert!(saved.contains(&("mission-pickup-coins".into(), 500)));
    assert!(saved.contains(&("mission-score-onerun".into(), 1000)));
    // and a fresh start from that save sees x2
    let m = Missions::prepare(&g.flow.user);
    assert_eq!(m.multiplier(), 1);
}

#[test]
fn one_run_counters_reset_with_the_run() {
    let mut s = sim();
    let g = &mut s.game;
    missions::add_stat(g, 1, "mission-jump");
    missions::add_stat(g, 1, "mission-roll");
    assert_eq!((data(g, "mission-jump-onerun"), data(g, "mission-roll-onerun")), (1, 1));
    g.reset();
    assert_eq!((data(g, "mission-jump-onerun"), data(g, "mission-roll-onerun")), (0, 0));
    assert_eq!(data(g, "mission-jump"), 1, "the totals stay");
}

#[test]
fn skipping_costs_1700_coins_and_counts_as_spent() {
    let mut s = sim();
    let g = &mut s.game;
    missions::watch_coins(g);
    assert!(!missions::skip(g, 0), "no coins");
    g.flow.user.coins = 2000;
    missions::watch_coins(g);
    assert!(missions::skip(g, 0));
    assert_eq!(g.flow.user.coins, 300);
    assert!(g.missions.sets[0][0].completed);
    assert_eq!(g.missions.sets[0][0].progress, 500);
    assert_eq!(data(g, "mission-spend-coins"), 1700);
}

#[test]
fn the_multiplier_caps_at_x30() {
    let mut u = UserData::default();
    let all = Missions::prepare(&u);
    u.missions = all.sets.iter().map(|s| s.iter().map(|m| MissionSave { id: m.id.clone(), progress: m.amount, started: true }).collect()).collect();
    let m = Missions::prepare(&u);
    assert_eq!(m.multiplier(), 29, "29 sets");
    assert_eq!(m.mission_set(), 28, "the last set stays current");
    assert_eq!(m.set_label(), 30);
    let mut s = sim();
    s.game.missions = m;
    s.game.reset();
    assert_eq!(s.game.multiplier + s.game.mission_multiplier, 30.0, "the HUD's x30");
}


#[test]
fn double_up_and_leaderboard_scroll() {
    use ss_port::flow::{click, pointer, Click, Pointer, Screen, Scroller};
    let mut s = sim();
    let g = &mut s.game;
    g.flow.menu = ss_port::menu::Menu::None;
    g.flow.user.coins = 1000;
    g.flow.screen = Screen::Results { score: 10, coins: 120, play_due_ms: 0.0, play_ready: true, frames: 0, doubled: false, list: Scroller::leaderboard(true) };
    click(g, Click::DoubleUp);
    click(g, Click::DoubleUp);
    assert_eq!(g.flow.user.coins, 1120, "the run's coins once more, once");
    let Screen::Results { doubled, list, .. } = &g.flow.screen else { panic!() };
    assert!(*doubled);
    assert_eq!((list.target, list.max), (-40.0, -40.0), "the list moves up into the module's room");
    // drag up 300 px: soft limit below -198, then it settles back to -198
    pointer(g, Pointer::Down(500.0));
    pointer(g, Pointer::Move(200.0));
    pointer(g, Pointer::Up);
    for _ in 0..240 {
        if let Screen::Results { list, .. } = &mut g.flow.screen {
            list.update();
        }
    }
    let Screen::Results { list, .. } = &g.flow.screen else { panic!() };
    assert!((list.target + 198.0).abs() < 0.5, "settles at the end: {}", list.target);
}
