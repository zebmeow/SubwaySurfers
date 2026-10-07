//! The death flow's timeline against the oracle (docs/js_notes/
//! ui_gameover.md §1, seed 1, no input, not god mode): death at f185, the
//! "Save me!" popup at f231 (0.75 s timer, one frame late), its 6 s clock out
//! at f592 (361 ticks), the New High Score screen (score 17 > 0) accepting
//! Space from f793 (alpha >= 0.2), the results banking the run, PLAY's Space
//! binding 121 frames after the results open. Also the free revive by Space.

use ss_port::flow::Screen;
use ss_port::game::GameState;
use ss_port::hero::Key;
use ss_port::sim_plugin::{DataPaths, KeyQueue, Sim};
use std::path::Path;

fn sim() -> (Sim, KeyQueue) {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let keys = KeyQueue::default();
    let mut s = Sim::new(&DataPaths::from_repo(root), 1, Some("S".into()), Box::new(keys.clone())).unwrap();
    s.game.god = false;
    (s, keys)
}
fn step_to(s: &mut Sim, f: i64) {
    while s.next_frame <= f {
        s.step();
    }
}

#[test]
fn death_flow_timeline() {
    let (mut s, keys) = sim();
    step_to(&mut s, 184);
    assert_eq!(s.game.state, GameState::Running);
    assert!(s.game.hud.visible);
    step_to(&mut s, 185);
    assert_eq!(s.game.state, GameState::Gameover, "death at f185");
    assert!(!s.game.hud.visible, "HUD hidden in the death frame");
    assert_eq!(ss_port::flow::score(&s.game), 17);
    step_to(&mut s, 230);
    assert!(matches!(s.game.flow.screen, Screen::Dying { .. }));
    step_to(&mut s, 231);
    assert!(matches!(s.game.flow.screen, Screen::SaveMe { tall: true, .. }), "Save me opens at f231");
    step_to(&mut s, 411);
    let Screen::SaveMe { secs, .. } = s.game.flow.screen else { panic!() };
    assert!((secs - 3.0).abs() < 1e-9, "clock {secs} at f411");
    step_to(&mut s, 591);
    assert!(matches!(s.game.flow.screen, Screen::SaveMe { .. }));
    step_to(&mut s, 592);
    assert!(matches!(s.game.flow.screen, Screen::HighScore { .. }), "clock out at f592 -> high score");
    // Space before the message has faded in is ignored
    keys.0.borrow_mut().push(Key::Action);
    step_to(&mut s, 700);
    assert!(matches!(s.game.flow.screen, Screen::HighScore { .. }));
    step_to(&mut s, 792);
    let Screen::HighScore { alpha, .. } = s.game.flow.screen else { panic!() };
    assert!(alpha < 0.2);
    step_to(&mut s, 793);
    let Screen::HighScore { alpha, .. } = s.game.flow.screen else { panic!() };
    assert!(alpha >= 0.2, "accepting Space from f793 ({alpha})");
    // Space at the start of f794 (keydown after the clock advance)
    keys.0.borrow_mut().push(Key::Action);
    step_to(&mut s, 794);
    let open = s.game.frame;
    let Screen::Results { score, play_ready, .. } = s.game.flow.screen.clone() else { panic!("results after Space") };
    assert_eq!(score, 17);
    assert!(!play_ready);
    assert_eq!(s.game.flow.user.high_score, 17, "high score banked at results");
    // PLAY's Space binding: setTimeout 2000 -> open + 121 frames
    step_to(&mut s, open + 120);
    assert!(matches!(s.game.flow.screen, Screen::Results { play_ready: false, .. }));
    step_to(&mut s, open + 121);
    assert!(matches!(s.game.flow.screen, Screen::Results { play_ready: true, .. }));
    keys.0.borrow_mut().push(Key::Action);
    step_to(&mut s, open + 122);
    // PLAY resets in place and runs with the intro
    assert_eq!(s.game.state, GameState::Idle);
    assert_eq!(s.game.flow.screen, Screen::None);
    assert_eq!(s.game.queued.len(), 3, "the intro continuation queued the first chunks");
    assert_eq!(s.game.hero.body.cz(), 0.0, "hero back at the start");
}

#[test]
fn free_revive_by_space() {
    let (mut s, keys) = sim();
    step_to(&mut s, 239);
    assert!(matches!(s.game.flow.screen, Screen::SaveMe { tall: true, .. }));
    keys.0.borrow_mut().push(Key::Action);
    step_to(&mut s, 240);
    assert_eq!(s.game.state, GameState::Running, "revived at the end of f240");
    assert!(s.game.hud.visible);
    assert_eq!(s.game.flow.free_revivals, 1);
    assert!(!s.game.hero.player.dead);
    // keep running after the revive (the blocker was removed)
    step_to(&mut s, 400);
    assert_eq!(s.game.state, GameState::Running);
    assert!(s.game.hero.body.cz() < -250.0, "moving again: z {}", s.game.hero.body.cz());
}

/// A mystery box collected in the run (`Ya.onCollect` rolls it at once) is
/// opened after the declined Save me: prize screen -> credit -> results.
#[test]
fn mystery_box_after_death() {
    let (mut s, keys) = sim();
    step_to(&mut s, 100);
    let prize = ss_port::prizes::roll(&mut s.game, "mystery-box");
    s.game.prizes.push(prize.clone());
    let before = s.game.flow.user.clone();
    step_to(&mut s, 592);
    let Screen::Prize(p) = s.game.flow.screen.clone() else { panic!("prize screen after Save me, got {:?}", s.game.flow.screen) };
    assert_eq!(p.current, prize);
    keys.0.borrow_mut().push(Key::Action);
    step_to(&mut s, 594);
    let u = &s.game.flow.user;
    let credited = match prize.kind.as_str() {
        "coins" => u.coins - before.coins,
        "keys" => u.keys - before.keys,
        "hoverboard" => u.hoverboards - before.hoverboards,
        "headstart" => u.headstarts - before.headstarts,
        _ => u.score_boosters - before.score_boosters,
    };
    assert_eq!(credited, prize.amount, "{} credited at the open press", prize.label());
    // the reveal runs 5 s, then Space continues to the high score
    step_to(&mut s, 594 + 300);
    keys.0.borrow_mut().push(Key::Action);
    step_to(&mut s, 594 + 302);
    assert!(matches!(s.game.flow.screen, Screen::HighScore { .. }), "{:?}", s.game.flow.screen);
}

/// Clicking Save me's green "Free!" button revives (it used to fall through
/// to the panel's catch-all and decline).
#[test]
fn free_revive_by_click() {
    let (mut s, _keys) = sim();
    step_to(&mut s, 239);
    assert!(matches!(s.game.flow.screen, Screen::SaveMe { tall: true, .. }));
    ss_port::flow::click(&mut s.game, ss_port::flow::Click::ReviveFree);
    assert!(matches!(s.game.flow.screen, Screen::SaveMe { .. }), "not declined");
    step_to(&mut s, 240);
    assert_eq!(s.game.state, GameState::Running, "revived after the frame's update");
    assert_eq!(s.game.flow.free_revivals, 1);
}

/// `k` is the keys button: revive for `paidRevivalCost` keys, or the
/// "not enough keys" popup.
#[test]
fn keys_revive_by_k() {
    let (mut s, keys) = sim();
    s.game.flow.user.keys = 3;
    step_to(&mut s, 239);
    keys.0.borrow_mut().push(Key::ReviveKeys);
    step_to(&mut s, 240);
    assert_eq!(s.game.state, GameState::Running, "revived for keys");
    assert_eq!(s.game.flow.user.keys, 2, "one key spent");
    assert_eq!(s.game.flow.paid_revivals, 1);

    let (mut s, keys) = sim();
    s.game.flow.user.keys = 0;
    step_to(&mut s, 239);
    keys.0.borrow_mut().push(Key::ReviveKeys);
    step_to(&mut s, 240);
    assert_eq!(s.game.flow.screen, Screen::NotEnough { needed: 1 });
    assert_ne!(s.game.state, GameState::Running);
    // during the run `k` does nothing
    let (mut s, keys) = sim();
    keys.0.borrow_mut().push(Key::ReviveKeys);
    step_to(&mut s, 100);
    assert_eq!(s.game.state, GameState::Running);
}

/// A collected key adds to the run's keys (`stats.keys += 1`), which the
/// game over banks into the saved keys.
#[test]
fn collected_keys_are_banked() {
    let (mut s, _keys) = sim();
    let before = s.game.flow.user.keys;
    ss_port::powerups::on_collect(&mut s.game, ss_port::entities::Cls::Pickup(ss_port::entities::PickupKind::Key));
    ss_port::powerups::on_collect(&mut s.game, ss_port::entities::Cls::Pickup(ss_port::entities::PickupKind::Key));
    assert_eq!(s.game.flow.keys, 2);
    step_to(&mut s, 186); // death at f185: gameover() banks the run keys
    assert_eq!(s.game.flow.keys, 0);
    assert_eq!(s.game.flow.user.keys, before + 2);
}
