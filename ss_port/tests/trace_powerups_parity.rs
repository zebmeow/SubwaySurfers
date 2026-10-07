//! Powerup parity against oracle traces recorded with forced pickups
//! (`--query forcePickup=X`) and a hoverboard Space press: the whole hero
//! check of `common::run` plus the powerup timers, multipliers and the
//! attracted coins.

mod common;

fn check(trace: &str, frames: usize) {
    let r = common::run(trace);
    assert_eq!(r.frames, frames);
    let n = r.problems.len() + r.extra.len() + r.powerups.len();
    assert!(n == 0, "{trace}: {} hero, {} extra, {} powerup frames out of parity", r.problems.len(), r.extra.len(), r.powerups.len());
}

#[test]
fn magnet() {
    check("oracle/traces/seed1-god-magnet/trace.jsonl", 3600);
}

#[test]
fn multiplier() {
    check("oracle/traces/seed1-god-multiplier/trace.jsonl", 3600);
}

#[test]
fn sneakers() {
    check("oracle/traces/seed1-god-sneakers/trace.jsonl", 3600);
}

#[test]
fn jetpack() {
    check("oracle/traces/seed1-god-jetpack/trace.jsonl", 5400);
}

#[test]
fn hoverboard() {
    check("oracle/traces/seed1-god-hoverboard/trace.jsonl", 3600);
}

#[test]
fn hoverboard_shield() {
    check("oracle/traces/seed1-hoverboard-shield/trace.jsonl", 1200);
}

#[test]
fn sneakers_jump() {
    check("oracle/traces/seed1-god-sneakers-jump/trace.jsonl", 2400);
}

/// Headstart (v twice: the second mid-flight) and score booster (c x3),
/// owned through `gdHeadstart=3` / `gdScoreBooster=3`.
#[test]
fn boosts() {
    check("oracle/traces/seed1-god-boosts/trace.jsonl", 2400);
}
