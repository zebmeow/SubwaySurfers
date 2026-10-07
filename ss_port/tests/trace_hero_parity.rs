//! Hero parity against the seed 1 god-mode oracle trace (3600 frames): see
//! `common::run` for what is compared.

mod common;

#[test]
fn seed1_hero_matches_oracle() {
    let r = common::run("oracle/traces/seed1-god/trace.jsonl");
    assert_eq!(r.frames, 3600);
    assert!(r.problems.is_empty(), "{} frames out of parity (first 30 above)", r.problems.len());
    assert!(r.extra.is_empty(), "{} frames fail the extra checks (first 30 above)", r.extra.len());
    assert!(r.powerups.is_empty(), "{} frames fail the powerup checks", r.powerups.len());
}
