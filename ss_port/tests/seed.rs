//! Layouts by seed: a run's chunks, obstacles, trains and coins depend on
//! the seed only (not on how long the title showed, not on an earlier
//! session), and the original's restart in place goes on with the RNG
//! streams (another layout: what `--lock-seed` avoids by starting every run
//! from a fresh boot).

use ss_port::sim_plugin::{DataPaths, KeyQueue, Sim};
use std::path::Path;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

/// The layout after `n` run frames: the chunks (name, z) and every level
/// entity's class and position.
fn layout(s: &mut Sim, n: i64) -> (Vec<String>, Vec<String>) {
    let start = s.game.frame;
    while s.game.frame < start + n {
        s.step();
    }
    let g = &s.game;
    let chunks = g.chunks.iter().map(|c| format!("{}@{:.1}", c.name, c.z)).collect();
    let mut ents: Vec<String> = g
        .level_entities
        .iter()
        .map(|&id| {
            let e = g.ent(id);
            let p = e.body.as_ref().map(|b| b.center()).unwrap_or_default();
            format!("{:?} {:.3} {:.3} {:.3}", e.cls, p.x, p.y, p.z)
        })
        .collect();
    ents.sort();
    (chunks, ents)
}

/// A boot of `seed`, `idle` frames on the title, then PLAY (god mode, no keys).
fn run(seed: i32, idle: i64) -> Sim {
    let mut s = Sim::new_title(&DataPaths::from_repo(root()), seed, Some("S".into()), Box::new(KeyQueue::default())).unwrap();
    s.game.god = true;
    for _ in 0..idle {
        s.step();
    }
    s.start_run();
    s
}

#[test]
fn a_seed_is_a_layout() {
    let a = layout(&mut run(42, 0), 900);
    assert!(a.0.len() >= 3 && a.1.len() > 50, "{} chunks, {} entities", a.0.len(), a.1.len());
    assert!(a.1.iter().any(|e| e.starts_with("Coin")), "coins");
    assert!(a.1.iter().any(|e| e.contains("Train")), "trains");
    // however long the title showed
    assert_eq!(layout(&mut run(42, 300), 900), a);
    // another session (sims share no state)
    let _other = layout(&mut run(7, 50), 400);
    assert_eq!(layout(&mut run(42, 17), 900), a);
    // another seed, another layout
    let b = layout(&mut run(7, 0), 900);
    assert_ne!(b.0, a.0);
}

#[test]
fn the_originals_restart_goes_on_with_the_streams() {
    let first = layout(&mut run(42, 0), 900);
    let mut s = run(42, 0);
    layout(&mut s, 700);
    s.game.to_game();
    assert_eq!(s.game.run_starts, 2);
    assert_ne!(layout(&mut s, 900), first, "R in place: another layout (the original)");
}
