//! The obstacle / station chunks (`default_1_track*`, which seed 1 never
//! reaches) forced with `config.chunk`, against oracle traces: the whole hero
//! check of `common::run` (collisions with the `Gi` obstacles, stations).

mod common;

fn check(trace: &str) {
    let r = common::run(trace);
    assert_eq!(r.frames, 1800);
    let n = r.problems.len() + r.extra.len() + r.powerups.len();
    assert!(n == 0, "{trace}: {} hero, {} extra, {} powerup frames out of parity", r.problems.len(), r.extra.len(), r.powerups.len());
}

#[test]
fn default_1_track_start() {
    check("oracle/traces/seed1-god-default_1_track_start/trace.jsonl");
}

#[test]
fn default_1_track_mid_var_1() {
    check("oracle/traces/seed1-god-default_1_track_mid_var_1/trace.jsonl");
}

#[test]
fn default_short_1_track() {
    check("oracle/traces/seed1-god-default_short_1_track/trace.jsonl");
}
