//! Debug helper: run the trace-driven sim to a frame and list entities of a class.
//!   cargo run --example inspect -- <frame> <cls>
use ss_port::describe::describe;
use ss_port::sim_plugin::{trace_hunt_letter, DataPaths, Sim, ReplayDriver};
use ss_port::trace::Trace;
use std::rc::Rc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let frame: i64 = a.get(1).and_then(|v| v.parse().ok()).unwrap_or(1200);
    let cls = a.get(2).cloned().unwrap_or("e".into());
    let root = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let trace = Rc::new(Trace::load(&root.join("oracle/traces/seed1-god/trace.jsonl")).unwrap());
    let mut sim = Sim::new(&DataPaths::from_repo(&root), 1, trace_hunt_letter(&trace), Box::new(ReplayDriver::new(trace.clone()))).unwrap();
    while sim.next_frame <= frame && sim.step() {}
    let cam = &trace.players[&frame]["camera"]["pos"];
    println!("frame {} camera {}", sim.game.frame, cam);
    for (i, e) in sim.game.ents.iter().enumerate() {
        if !e.in_scene || e.cls.name() != cls {
            continue;
        }
        let d = describe(&sim.game, ss_port::entities::EntityId(i));
        let vis: Vec<String> = d.meshes.iter().map(|m| format!("{}{}", m.mesh, if m.visible { "" } else { "(hidden)" })).collect();
        println!("  #{i} active={} pos={:.1?} scale={:.3?} {:?}", e.active, d.pos, d.scale, vis);
    }
}
