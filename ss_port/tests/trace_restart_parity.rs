//! In-place restart against the oracle: seed 1 mortal, death at f185, then
//! `nav.toGame()` (what results PLAY calls) before f605. The original resets
//! the world in place (`Game.reset` / `idle` / `runWithIntro`); the RNG streams
//! continue, so the second run's route differs (s-s-s-s, tunnel_notrain,
//! pogostick_start), and it ends in its own death at f817.

mod common;

#[test]
fn restart_in_place() {
    let r = common::run("oracle/traces/seed1-restart/trace.jsonl");
    assert_eq!(r.frames, 1400);
    let n = r.problems.len() + r.extra.len() + r.powerups.len();
    assert!(n == 0, "{} hero, {} extra, {} powerup frames out of parity", r.problems.len(), r.extra.len(), r.powerups.len());
}

/// Escape at run frame 200 (pause), Escape again at 260 (the 3 x 600 ms
/// countdown), resume, then the 2-frame clock step after the pause.
#[test]
fn pause_and_resume() {
    let r = common::run("oracle/traces/seed1-god-pause/trace.jsonl");
    assert_eq!(r.frames, 1200);
    let n = r.problems.len() + r.extra.len() + r.powerups.len();
    assert!(n == 0, "{} hero, {} extra, {} powerup frames out of parity", r.problems.len(), r.extra.len(), r.powerups.len());
}
