//! The New High Score screen's stripes and character view against the
//! oracle (oracle/hs_dump.mjs -> traces/seed1-highscore/celebration.json;
//! seed 1, no god mode, no input): the stripes are built at the open with
//! the oracle's own `R.range` streams, respawn and move frame for frame, and
//! the view slides in with the same lerps as the message alpha.

use ss_port::flow::Screen;
use ss_port::sim_plugin::{DataPaths, KeyQueue, Sim};
use std::path::Path;

#[test]
fn stripes_and_view_match_the_oracle() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let fx: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("oracle/traces/seed1-highscore/celebration.json")).unwrap()).unwrap();
    let frames = fx["frames"].as_array().unwrap();
    let mut s = Sim::new(&DataPaths::from_repo(root), 1, Some("S".into()), Box::new(KeyQueue::default())).unwrap();
    s.game.god = false;
    // run to the open
    while !matches!(s.game.flow.screen, Screen::HighScore { .. }) {
        assert!(s.next_frame < 1000, "the HS screen opens");
        s.step();
    }
    let open = s.game.frame;
    assert_eq!(frames[0]["f"].as_i64().unwrap(), open, "opens on the oracle's frame");
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6 * (1.0 + a.abs());
    let mut checked = 0;
    for r in frames {
        let f = r["f"].as_i64().unwrap();
        while s.game.frame < f {
            s.step();
        }
        let Screen::HighScore { alpha, .. } = s.game.flow.screen else { panic!("f{f}: screen {:?}", s.game.flow.screen) };
        let c = &s.game.flow.celebration;
        let v: Vec<f64> = r["view"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        assert!((0..3).all(|k| close(c.view[k], v[k])), "f{f}: view {:?} vs {v:?}", c.view);
        assert!(close(alpha, r["msg"].as_f64().unwrap()), "f{f}: alpha");
        let st = r["stripes"].as_array().unwrap();
        assert_eq!(st.len(), c.stripes.len());
        for (k, (p, o)) in c.stripes.iter().zip(st).enumerate() {
            let o: Vec<f64> = o.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
            let mine = [p.x, p.y, p.rotation, p.scale_y, p.speed];
            assert!((0..5).all(|i| close(mine[i], o[i])), "f{f} stripe {k}: {mine:?} vs {o:?}");
        }
        checked += 1;
    }
    assert!(checked > 300, "{checked} frames");
}
