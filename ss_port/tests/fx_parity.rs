//! Particle systems against the oracle (oracle/fx_dump.mjs ->
//! traces/seed1-god/fx.json): every `An` in update order (the Bali gate
//! sparkles from f638, the pogo's paint clouds from f1087), each frame:
//! clock, emitted count, and every live particle's offset, size, rotation,
//! colour, lifetime and sprite tile, plus the rig particles' world matrix.
//! The systems share the oracle's random streams, so this also pins the
//! draw order. The hero's FX rigs: clock, visibility, trail points.

mod common;

use ss_port::sim_plugin::{ReplayDriver, Sim};
use ss_port::trace::Trace;
use std::rc::Rc;

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * (1.0 + a.abs().max(b.abs()))
}

#[test]
fn bali_sparkles_and_pogo_match_the_oracle() {
    check("seed1-god", 500, 1000);
}

/// The jetpack rig (startburst puffs, paint, trails) and the swamp fog.
#[test]
fn jetpack_and_fog_match_the_oracle() {
    check("seed1-god-jetpack", 1000, 1000);
}

/// Hoverboard grind sparks on the train roofs (f655-752), and the pogo.
#[test]
fn hoverboard_grind_matches_the_oracle() {
    check("seed1-god-hoverboard", 1000, 1000);
}

/// The crash smoke when the board absorbs a crash (f185).
#[test]
fn hoverboard_crash_smoke_matches_the_oracle() {
    check("seed1-hoverboard-shield", 600, 300);
}

/// The revive smoke (seed 1, no god mode, death at f185, Space at f300 on
/// "Save me!": the free revive; `oracle/fx_dump.mjs revive`).
#[test]
fn revive_smoke_matches_the_oracle() {
    let root = common::root();
    let keys = ss_port::sim_plugin::KeyQueue::default();
    let mut sim = Sim::new(&ss_port::sim_plugin::DataPaths::from_repo(root), 1, Some("S".into()), Box::new(keys.clone())).unwrap();
    sim.game.god = false;
    compare(&mut sim, "seed1-revive", 100, 300, &mut |f| {
        if f == 300 {
            keys.0.borrow_mut().push(ss_port::hero::Key::Action);
        }
    });
}

fn check(name: &str, min_frames: usize, min_particles: usize) {
    let root = common::root();
    let trace = Rc::new(Trace::load(&root.join(format!("oracle/traces/{name}/trace.jsonl"))).unwrap());
    let seed = trace.meta["seed"].as_i64().unwrap() as i32;
    let paths = ss_port::sim_plugin::DataPaths::from_repo(root);
    let mut sim = Sim::new(&paths, seed, ss_port::sim_plugin::trace_hunt_letter(&trace), Box::new(ReplayDriver::new(trace.clone()))).unwrap();
    sim.configure_from_trace(&trace);
    compare(&mut sim, name, min_frames, min_particles, &mut |_| {});
}

/// Step `sim` through the fixture's frames (`before(f)` runs before frame
/// f is stepped) and compare every system, particle and rig.
fn compare(sim: &mut Sim, name: &str, min_frames: usize, min_particles: usize, before: &mut dyn FnMut(i64)) {
    let root = common::root();
    let fx: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join(format!("oracle/traces/{name}/fx.json"))).unwrap()).unwrap();
    let mut compared = 0;
    let mut particles = 0;
    for fr in fx["frames"].as_array().unwrap() {
        let f = fr["f"].as_i64().unwrap();
        while sim.game.frame < f {
            before(sim.next_frame);
            sim.step();
        }
        let g = &sim.game;
        let oracle = fr["systems"].as_array().unwrap();
        assert_eq!(g.fx.order.len(), oracle.len(), "f{f}: number of systems");
        for (k, (&i, o)) in g.fx.order.iter().zip(oracle).enumerate() {
            let s = &g.fx.systems[i];
            let tex = o["tex"].as_str().unwrap_or("");
            assert!(tex.starts_with(&format!("{}.", s.look.texture)), "f{f} system {k}: texture {} vs {tex}", s.look.texture);
            let what = format!("f{f} system {k} ({})", s.look.texture);
            assert!(close(s.sys.time, o["time"].as_f64().unwrap(), 1e-9), "{what}: time {} vs {}", s.sys.time, o["time"]);
            assert!(close(s.sys.count, o["count"].as_f64().unwrap(), 1e-9), "{what}: count {} vs {}", s.sys.count, o["count"]);
            assert_eq!(s.sys.emitting(), o["emitting"].as_bool().unwrap(), "{what}: emitting");
            let alive: Vec<usize> = (0..s.sys.particles.len()).filter(|&j| s.sys.particles[j].alive).collect();
            let oa = o["alive"].as_array().unwrap();
            assert_eq!(alive.len(), oa.len(), "{what}: live particles");
            for (&j, op) in alive.iter().zip(oa) {
                let op: Vec<f64> = op.as_array().unwrap().iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect();
                assert_eq!(j, op[0] as usize, "{what}: slot");
                let mine: Vec<f64> = [s.sys.offset[j].map(|x| x as f64).to_vec(), s.sys.data[j].map(|x| x as f64).to_vec(), s.sys.color[j].map(|x| x as f64).to_vec()]
                    .concat()
                    .into_iter()
                    .chain([s.sys.particles[j].lifetime, s.sys.tile[j] as f64])
                    .collect();
                for (n, (a, b)) in mine.iter().zip(&op[1..]).enumerate() {
                    assert!(close(*a, *b, 2e-6), "{what} particle {j} field {n}: {a} vs {b}\n  port   {mine:?}\n  oracle {:?}", &op[1..]);
                }
                particles += 1;
            }
            if let (ss_port::fx::Owner::Rig | ss_port::fx::Owner::Free, Some(w)) = (&s.owner, o["world"].as_array()) {
                if s.visible {
                    let w: Vec<f64> = w.iter().map(|x| x.as_f64().unwrap()).collect();
                    let m = s.world.to_cols_array();
                    if std::env::var_os("FX_DEBUG").is_some() {
                        eprintln!("{what}: d=({:.3},{:.3},{:.3})", m[12] - w[12], m[13] - w[13], m[14] - w[14]);
                        continue;
                    }
                    for n in 0..16 {
                        assert!((m[n] - w[n]).abs() < 2e-3 * (1.0 + w[n].abs()), "{what}: world[{n}] {} vs {}\n {m:?}\n {w:?}", m[n], w[n]);
                    }
                }
            }
        }
        // the hero's FX rigs and their ribbon trails
        let mut trails = 0;
        for orig in fr["rigs"].as_array().unwrap() {
            let kind = match orig["name"].as_str().unwrap() {
                "pogo" => ss_port::fx::RigKind::Pogo,
                "jetpack" => ss_port::fx::RigKind::Jetpack,
                _ => ss_port::fx::RigKind::Headstart,
            };
            let rig = g.fx.rigs.iter().find(|r| r.kind == kind).unwrap_or_else(|| panic!("f{f}: rig {kind:?}"));
            assert!(close(rig.clock, orig["clock"].as_f64().unwrap(), 1e-9), "f{f} {kind:?}: clock");
            assert_eq!(rig.visible, orig["visible"].as_bool().unwrap(), "f{f} {kind:?}: visible");
            for (side, os) in rig.sides.iter().zip(orig["sides"].as_array().unwrap()) {
                let t = &side.trail;
                let ot = &os["trail"];
                assert!(close(t.clock, ot["clock"].as_f64().unwrap(), 1e-9), "f{f} {kind:?}: trail clock");
                assert_eq!(t.visible, ot["visible"].as_bool().unwrap(), "f{f} {kind:?}: trail visible");
                let pts = ot["points"].as_array().unwrap();
                assert_eq!(t.points.len(), pts.len(), "f{f} {kind:?}: trail points");
                for (p, o) in t.points.iter().zip(pts) {
                    let o: Vec<f64> = o.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
                    if std::env::var_os("FX_DEBUG").is_some() {
                        continue;
                    }
                    assert!((p.0 - bevy::math::DVec3::new(o[0], o[1], o[2])).length() < 1e-6 && close(p.1, o[3], 1e-9), "f{f} {kind:?}: point {p:?} vs {o:?}");
                }
                let chain = ot["chain"].as_array().unwrap();
                let drawn = ot["drawRange"].as_u64().unwrap() > 0;
                assert_eq!(t.drawn, drawn, "f{f} {kind:?}: trail drawn");
                if drawn {
                    assert_eq!(t.chain.len(), chain.len(), "f{f} {kind:?}: chain");
                }
                trails += 1;
            }
        }
        let _ = trails;
        compared += 1;
    }
    assert!(compared > min_frames && particles > min_particles, "{compared} frames, {particles} particles");
}
