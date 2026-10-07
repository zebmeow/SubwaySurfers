//! Print when the particle systems and FX rigs change in a trace replay.
//!   cargo run --release --example fx_probe -- oracle/traces/<name>/trace.jsonl [last_frame]
use ss_port::sim_plugin::{trace_hunt_letter, DataPaths, ReplayDriver, Sim};
use ss_port::trace::Trace;
use std::rc::Rc;

fn main() {
    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let tp = std::env::args().nth(1).expect("trace");
    let last: i64 = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(3000);
    let trace = Rc::new(Trace::load(&root.join(tp)).unwrap());
    let seed = trace.meta["seed"].as_i64().unwrap() as i32;
    let mut sim = Sim::new(&DataPaths::from_repo(root), seed, trace_hunt_letter(&trace), Box::new(ReplayDriver::new(trace.clone()))).unwrap();
    sim.configure_from_trace(&trace);
    let mut prev = String::new();
    let mut seen = 0;
    while sim.next_frame <= last {
        sim.step();
        let g = &sim.game;
        let s = format!("systems {} rigs {:?}", g.fx.order.len(), g.fx.rigs.iter().map(|r| (r.kind, r.visible)).collect::<Vec<_>>());
        if std::env::var_os("JP").is_some() && (std::env::var("JP").unwrap().parse::<i64>().unwrap()..=std::env::var("JP").unwrap().parse::<i64>().unwrap() + 3).contains(&g.frame) {
            let v = ss_port::fx::prop_views(g).map(|(_, j)| j.w_axis.truncate());
            println!("f{} hero {:?} ry {} anim {} view {:?}", g.frame, g.hero.position, g.hero.ry, g.hero_anim.as_ref().map_or(String::new(), |a| a.name.clone()), v);
        }
        if std::env::var_os("LETTERS").is_some() {
            for e in &g.log[seen..] {
                if let ss_port::game::LogEvent::Pickup { frame, entity, .. } = e {
                    if g.ents[entity.0].cls == ss_port::entities::Cls::Pickup(ss_port::entities::PickupKind::Letter) {
                        println!("letter collected f{frame}");
                    }
                }
            }
            seen = g.log.len();
        }
        if std::env::var_os("SHADOW").is_some() && [10, 40, 80, 120].contains(&g.frame) {
            println!("f{} shadow_on {} y {}", g.frame, g.hero_fx.shadow_on, g.hero_fx.shadow_y);
        }
        if s != prev {
            println!("f{} {s}", g.frame);
            prev = s;
        }
    }
}
