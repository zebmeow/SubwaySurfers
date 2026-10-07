//! Word Hunt (`src/word_hunt.rs`): collecting the day's letters advances and
//! saves the progress, the last one pays the week's reward once, stops the
//! letter timer, and the HUD banner plays its drop / complete passes.

mod common;

use ss_port::sim_plugin::{DataPaths, KeyQueue, Sim};
use ss_port::word_hunt::{self, Day, Reward, REWARDS};

fn sim() -> Sim {
    Sim::new(&DataPaths::from_repo(common::root()), 1, Some("S".into()), Box::new(KeyQueue::default())).unwrap()
}

#[test]
fn collecting_the_word_pays_once() {
    let mut s = sim();
    // a Wednesday with two hunts done this week: the third reward (1050 coins)
    let day = Day(18806 + 7 * 100 + 2);
    assert_eq!(day.weekday(), 2);
    s.game.flow.user.completed_hunts = vec![Day(day.0 - 2).word().into(), Day(day.0 - 1).word().into()];
    s.game.flow.user.hunt_word = Day(day.0 - 1).word().into();
    let hunt = word_hunt::init(&mut s.game.flow.user, day);
    assert_eq!(hunt.reward(), Some(REWARDS[2]));
    assert_eq!(hunt.reward(), Some(Reward::Coins(1050)));
    word_hunt::attach(&mut s.game, hunt);
    let word = day.word();
    assert_eq!(s.game.hunt_letter.as_deref(), Some(&word[..1].to_ascii_uppercase()[..]));
    let coins = s.game.flow.user.coins;
    for i in 0..word.len() {
        word_hunt::collect(&mut s.game);
        assert_eq!(s.game.flow.user.hunt_index, i + 1, "saved progress");
        assert_eq!(s.game.flow.user.hunt_day, day.0);
        assert_eq!(s.game.hud.word_banner.map(|b| b.letter), Some(i), "banner shows letter {i}");
    }
    assert_eq!(s.game.flow.user.coins, coins + 1050, "paid once");
    assert!(s.game.flow.user.completed_hunts.iter().any(|w| w == word));
    assert!(s.game.word_hunt.as_ref().unwrap().done);
    assert_eq!(s.game.hunt_letter, None, "no letter left");
    assert!(!s.game.timed_pickups.iter().any(|t| t.0 == "letter"), "letter timer removed");
    // a collect after the end changes nothing
    word_hunt::collect(&mut s.game);
    assert_eq!(s.game.flow.user.coins, coins + 1050);
    // the same day again: done, no letter timer
    let again = word_hunt::init(&mut s.game.flow.user, day);
    assert!(again.done);
}

#[test]
fn a_box_reward_goes_to_the_run_prizes() {
    let mut s = sim();
    let day = Day(18806 + 7 * 100); // Monday: first hunt of the week -> mystery box
    let hunt = word_hunt::init(&mut s.game.flow.user, day);
    assert_eq!(hunt.reward(), Some(Reward::Box("mystery-box")));
    word_hunt::attach(&mut s.game, hunt);
    for _ in 0..day.word().len() {
        word_hunt::collect(&mut s.game);
    }
    assert_eq!(s.game.prizes.len(), 1);
    assert_eq!(s.game.prizes[0].box_type.as_deref(), Some("mystery-box"));
}

#[test]
fn banner_timeline_drops_and_shows_completion() {
    let mut s = sim();
    let day = Day(18806 + 7 * 100);
    let hunt = word_hunt::init(&mut s.game.flow.user, day);
    word_hunt::attach(&mut s.game, hunt);
    let n = day.word().len();
    for _ in 0..n {
        word_hunt::collect(&mut s.game);
    }
    // last letter: the drop (0.5 s delay), then the complete panel 1.5 s after it went up
    let start = s.game.frame;
    let mut saw_down = false;
    let mut saw_complete = false;
    while s.game.frame < start + 60 * 8 {
        s.step();
        if let Some(b) = s.game.hud.word_banner {
            saw_down |= b.drop() >= 1.0 && !b.complete_shown;
            saw_complete |= b.drop() >= 1.0 && b.complete_shown;
        }
    }
    assert!(saw_down && saw_complete);
    assert!(s.game.hud.word_banner.is_none(), "gone after the second pass");
}
