//! Regression for the Windows panic `physics.rs:53: index out of bounds: the
//! len is 29 but the index is 29`: hits inside the physics loop can remove
//! entities from the physics list (a pickup collected, a crash clearing
//! obstacles), leaving the reverse index past the end. The original skips
//! such slots (`if (!e || ...) continue`). Mortal runs on many seeds with
//! busy input, restarting after each death, must never panic.

use ss_port::game::GameState;
use ss_port::hero::Key;
use ss_port::sim_plugin::{DataPaths, KeyQueue, Sim};
use std::path::Path;

#[test]
fn mortal_runs_with_pickups_and_crashes_never_panic() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let paths = DataPaths::from_repo(root);
    let keys_cycle = [Key::Left, Key::Up, Key::Right, Key::Down, Key::Action, Key::Right, Key::Up, Key::Left, Key::Down];
    let mut deaths = 0;
    for seed in 1..=12u32 {
        let keys = KeyQueue::default();
        let mut s = Sim::new(&paths, seed as _, Some("S".into()), Box::new(keys.clone())).unwrap();
        s.game.god = false;
        let mut over_since = None;
        let mut rng = seed.wrapping_mul(2654435761);
        for f in 0..4000i64 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            if rng % 13 == 0 {
                keys.0.borrow_mut().push(keys_cycle[(rng / 13) as usize % keys_cycle.len()]);
            }
            s.step();
            if s.game.state == GameState::Gameover {
                let since = *over_since.get_or_insert(f);
                if f - since == 90 {
                    deaths += 1;
                    s.game.to_game();
                    over_since = None;
                }
            } else {
                over_since = None;
            }
        }
    }
    assert!(deaths > 12, "the runs crash and restart ({deaths})");
}
