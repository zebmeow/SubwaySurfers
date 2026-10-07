//! The tutorial route against the oracle (oracle with `game.tutorial.enabled`
//! and a save without the tutorial, seed 1, no input): the "up" trigger at
//! f246 (the arrow the frame after, the message at the centre), the death on
//! the route at f325 without a game over, the rewind from f386-389 back to the
//! chunk's checkpoint (z -132.80 at f461, the run again from -131.19 at
//! f462). And the awards' bookkeeping.

use ss_port::game::GameState;
use ss_port::sim_plugin::{DataPaths, KeyQueue, Sim};
use std::path::Path;

fn tutorial_run(god: bool) -> Sim {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let mut s = Sim::new_title(&DataPaths::from_repo(root), 1, Some("S".into()), Box::new(KeyQueue::default())).unwrap();
    s.game.tutorial_allowed = true;
    s.game.tutorial_enabled = true;
    s.game.god = god;
    s.start_run();
    s
}

fn step_to(s: &mut Sim, f: i64) {
    while s.game.frame < f {
        assert!(s.step());
    }
}

#[test]
fn up_prompt_like_the_oracle() {
    let mut s = tutorial_run(true);
    step_to(&mut s, 245);
    assert!(s.game.tutorial.msg.is_none());
    step_to(&mut s, 246);
    let m = s.game.tutorial.msg.clone().expect("message at f246");
    assert_eq!((m.text.as_str(), m.y), ("Press Arrow Key Up", 0.0));
    step_to(&mut s, 247);
    let a = s.game.tutorial.arrow.clone().expect("arrow");
    assert!(!a.pending && a.time == 0.0 && a.y() == 300.0, "shown at f247 from y 300");
    step_to(&mut s, 258);
    assert!((s.game.tutorial.arrow.as_ref().unwrap().y() - 135.0).abs() < 1e-6);
}

#[test]
fn a_death_rewinds_to_the_checkpoint() {
    let mut s = tutorial_run(false);
    step_to(&mut s, 324);
    assert!(!s.game.hero.player.dead);
    step_to(&mut s, 325);
    assert!(s.game.hero.player.dead, "dies at f325");
    assert_eq!(s.game.state, GameState::Running, "no game over on the tutorial route");
    // (the oracle was sampled every 4 frames: f385 dead, f389 rewinding)
    step_to(&mut s, 385);
    assert!(s.game.hero.player.rewind_to.is_none());
    step_to(&mut s, 389);
    assert!(s.game.hero.player.rewind_to.is_some(), "rewinding 1 s later");
    step_to(&mut s, 461);
    assert!((s.game.hero.body.cz() + 132.80).abs() < 0.01);
    step_to(&mut s, 462);
    assert!(!s.game.hero.player.dead && s.game.hero.player.running);
    assert!((s.game.hero.body.cz() + 131.19).abs() < 0.01);
}

#[test]
fn finishing_saves_the_tutorial() {
    let mut s = tutorial_run(true);
    ss_port::tutorial::exit_trigger(&mut s.game, "finished");
    assert!(s.game.flow.user.tutorial);
}

#[test]
fn awards_progress_collect_and_pay_twice() {
    use ss_port::awards;
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let mut s = Sim::new_title(&DataPaths::from_repo(root), 1, Some("S".into()), Box::new(KeyQueue::default())).unwrap();
    let g = &mut s.game;
    let id = "award-complete-missions";
    for _ in 0..7 {
        awards::mission_completed(g);
    }
    assert!(awards::ready(g, id), "bronze: 7 missions");
    assert_eq!(g.missions.toasts.back().map(|t| t.icon), Some("spraycan-big-bronze"));
    let keys = g.flow.user.keys;
    awards::collect(g, id);
    assert_eq!(awards::progress(g, id), awards::Progress { value: 0.0, tier: 1 });
    assert_eq!(g.flow.user.keys, keys + 3, "the prize screen credits 3 keys");
    g.clock.now += 2001.0;
    awards::after_render(g);
    assert_eq!(g.flow.user.keys, keys + 6, "and awardPlayer adds them again 2 s later");
    // owning a second character: "own 2 characters" at 1 of 2
    g.flow.user.owned_characters.push("tricky".into());
    awards::after_render(g);
    assert_eq!(awards::progress(g, "award-own-characters").value, 1.0);
}
