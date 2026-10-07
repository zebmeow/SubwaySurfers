//! Jumps are not in the seed1 oracle trace (the autopilot only rolls and
//! changes lanes), so this checks the ported `jf` against its own formulas:
//! ascent `y = lerp(startY, startY + 19, sineOut(t / 0.41 s))` driven through
//! the physics sub-steps, then gravity 0.055/frame² back to the ground.

use ss_port::hero::Key;
use ss_port::sim_plugin::{DataPaths, KeyQueue, Sim};
use std::path::Path;

#[test]
fn jump_rises_to_19_and_lands() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let keys = KeyQueue::default();
    let mut sim = Sim::new(&DataPaths::from_repo(root), 1, Some("S".into()), Box::new(keys.clone())).unwrap();
    // run to f120 (running since f80; the first obstacle is far ahead), dodge right first (the intro train stands in the left lane)
    while sim.next_frame < 100 {
        sim.step();
    }
    keys.0.borrow_mut().push(Key::Right);
    while sim.next_frame < 130 {
        sim.step();
    }
    let h = &sim.game.hero;
    assert_eq!(h.lane.lane, 1);
    assert!(h.landed() && h.body.bottom() == 0.0);
    keys.0.borrow_mut().push(Key::Up);
    sim.step();
    let start_y = sim.game.hero.jump.start_y;
    assert!(sim.game.hero.jump.is_jumping);
    assert_eq!(start_y, 6.5, "perform lifts the center by 1");
    assert_eq!(sim.game.hero.fsm.current, "running", "the FSM ran before the perform's physics moved the body");
    let mut peak: f64 = 0.0;
    let mut frames = 1;
    let mut states = vec![sim.game.hero.fsm.current];
    while sim.game.hero.jump.is_jumping && frames < 200 {
        sim.step();
        frames += 1;
        peak = peak.max(sim.game.hero.body.cy());
        let s = sim.game.hero.fsm.current;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    println!("jump: {frames} frames, peak center {peak}, states {states:?}");
    // the ascent stops just short of endY = startY + 19: on the frame the
    // ratio reaches 1, jf.update replaces the last velocity with 0
    let end_y = start_y + 19.0;
    assert!(peak < end_y && peak > end_y - 0.05, "peak {peak}");
    assert_eq!(sim.game.hero.body.bottom(), 0.0);
    // 0.41 s = 24.6 frames up, ~26.3 frames down under 0.055/frame²
    assert!((45..=60).contains(&frames), "{frames} frames airborne");
    assert_eq!(states, ["running", "ascending", "hangtime", "descending", "running"]);
}
