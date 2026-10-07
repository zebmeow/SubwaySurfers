//! Chunks the oracle traces never reached must not crash a live run: the
//! pillars environment (`io` + `to`/`no`/`ro`) is ported; station
//! environment, obstacles `Gi` and tutorial triggers are skipped (logged).
//! Each chunk is queued into a running seed-1 game and played on.

use ss_port::entities::Cls;
use ss_port::sim_plugin::{DataPaths, KeyQueue, Sim};
use std::path::Path;

fn run_with(chunk: &str) -> Sim {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let mut s = Sim::new(&DataPaths::from_repo(root), 1, Some("S".into()), Box::new(KeyQueue::default())).unwrap();
    s.game.god = true;
    while s.next_frame <= 300 {
        s.step();
    }
    s.game.queue_chunk(Some(chunk));
    while s.next_frame <= 1500 {
        s.step();
    }
    s
}

#[test]
fn pillars_environment_mounts() {
    for chunk in ["default_2_tracks_mid_var_1", "default_2_tracks_mid_var_2", "default_short_2_tracks_mid_var_1", "default_short_2_tracks_mid_var_2"] {
        let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let mut s = Sim::new(&DataPaths::from_repo(root), 1, Some("S".into()), Box::new(KeyQueue::default())).unwrap();
        while s.next_frame <= 300 {
            s.step();
        }
        s.game.queue_chunk(Some(chunk));
        // the queued chunk is placed on the next frame
        s.step();
        let count = |c: Cls| s.game.ents.iter().filter(|e| e.cls == c && e.active).count();
        // one io per pillar_environment node (2-block sections), each with a start and an end
        let io = count(Cls::PillarsEnv);
        assert!(io >= 1, "{chunk}: io");
        assert_eq!(count(Cls::PillarPiece(0)), io, "{chunk}: start pieces");
        assert_eq!(count(Cls::PillarPiece(2)), io, "{chunk}: end pieces");
        // the pieces carry the Bali_pillars_* meshes from the tunnel config
        let start = s.game.ents.iter().find(|e| e.cls == Cls::PillarPiece(0)).unwrap().root;
        assert!(!s.game.scene.meshes(start).is_empty(), "{chunk}: pillars_start has geometry");
        while s.next_frame <= 1500 {
            s.step();
        }
    }
}

#[test]
fn unported_scenery_is_skipped() {
    for chunk in ["default_1_track_mid_var_1", "default_1_track_mid_var_2", "default_short_1_track", "default_1_track_start", "default_tutorial"] {
        let s = run_with(chunk);
        assert!(s.game.frame >= 1500, "{chunk}: ran on");
    }
}
