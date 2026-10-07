#![allow(dead_code)]
//! Differential parity of the hero simulation against an oracle trace of
//! the original game (shared by the trace parity tests).
//!
//! The port runs headless (MinimalPlugins + SimPlugin, one update per game
//! frame). The only thing taken from the trace is the keyboard: the
//! autopilot's keydowns, applied before their recorded frames. Every frame
//! the hero's end-of-frame state is compared with the trace's `player`
//! record: position, velocity, ground, pose state and lane (asserted), and
//! as extra checks body size, stats (time, speed, z, coins), the camera, the
//! guard (state, distance, x, z, visibility) and the pickup / collision /
//! trigger events.

use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use serde_json::Value;
use ss_port::entities::EntityId;
use ss_port::game::LogEvent;
use ss_port::hero::flags;
use ss_port::sim_plugin::{trace_hunt_letter, DataPaths, ReplayDriver, Sim, SimPlugin};
use ss_port::trace::Trace;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::rc::Rc;

/// Required tolerance (the trace stores 6 decimals).
const TOL: f64 = 1e-3;
const TARGET: f64 = 1e-4;

pub fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}
fn v3(v: &Value) -> DVec3 {
    DVec3::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap())
}

/// Largest deviation per quantity: (value, frame).
#[derive(Default)]
pub struct Worst(pub BTreeMap<&'static str, (f64, i64)>);
impl Worst {
    fn track(&mut self, what: &'static str, d: f64, f: i64) {
        let e = self.0.entry(what).or_insert((0.0, 0));
        if d > e.0 || d.is_nan() {
            *e = (d, f);
        }
    }
}

fn flag_names(f: u32) -> Vec<&'static str> {
    let mut v = Vec::new();
    for (bit, name) in [
        (flags::LEFT, "left"),
        (flags::TOP, "top"),
        (flags::RIGHT, "right"),
        (flags::BOTTOM, "bottom"),
        (flags::FRONT, "front"),
        (flags::BACK, "back"),
        (flags::SLOPE, "slope"),
    ] {
        if f & bit != 0 {
            v.push(name);
        }
    }
    v
}

/// What a parity run found.
pub struct Report {
    pub frames: usize,
    pub problems: Vec<String>,
    pub extra: Vec<String>,
    pub powerups: Vec<String>,
    pub worst: Worst,
}

/// Replay `trace_rel` (relative to the repo root) through the port and
/// compare every frame. `quiet`: no summary print.
pub fn run(trace_rel: &str) -> Report {
    let trace = Rc::new(Trace::load(&root().join(trace_rel)).unwrap());
    let seed = trace.meta["seed"].as_i64().unwrap() as i32;
    let paths = DataPaths::from_repo(root());
    let mut sim = Sim::new(&paths, seed, trace_hunt_letter(&trace), Box::new(ReplayDriver::new(trace.clone()))).unwrap();
    sim.configure_from_trace(&trace);

    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(SimPlugin);
    app.insert_non_send(sim);

    // trace events by frame
    let mut events: HashMap<i64, Vec<&Value>> = HashMap::new();
    for r in trace.of_type("event") {
        events.entry(r["f"].as_i64().unwrap()).or_default().push(r);
    }

    let mut worst = Worst::default();
    let mut problems: Vec<String> = Vec::new();
    let mut extra: Vec<String> = Vec::new();
    let mut pw: Vec<String> = Vec::new();
    let mut log_cursor = 0;
    let mut frames = 0;
    let (first, last) = trace.frame_range();
    assert_eq!(first, 6);
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();

    for f in 6..=last {
        app.update();
        frames += 1;
        let sim = app.world_mut().non_send_mut::<Sim>();
        let g = &sim.game;
        assert_eq!(g.frame, f, "sim frame out of step");
        let p = trace.players.get(&f).expect("player record");
        let h = &g.hero;

        // asserted per frame
        let pos = h.body.center();
        let dpos = (pos - DVec3::new(p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap(), p["z"].as_f64().unwrap())).abs().max_element();
        let dvel = (h.body.velocity() - v3(&p["vel"])).abs().max_element();
        let dground = (h.ground - p["ground_y"].as_f64().unwrap()).abs();
        worst.track("position", dpos, f);
        worst.track("velocity", dvel, f);
        worst.track("ground_y", dground, f);
        let mut bad = Vec::new();
        if !(dpos < TOL) {
            bad.push(format!("pos {pos:?} vs [{}, {}, {}] (d {dpos:.2e})", p["x"], p["y"], p["z"]));
        }
        if !(dvel < TOL) {
            bad.push(format!("vel {:?} vs {} (d {dvel:.2e})", h.body.velocity(), p["vel"]));
        }
        if !(dground < TOL) {
            bad.push(format!("ground {} vs {}", h.ground, p["ground_y"]));
        }
        if h.fsm.current != p["state"].as_str().unwrap() {
            bad.push(format!("state {} vs {}", h.fsm.current, p["state"]));
        }
        if h.lane.lane as i64 != p["lane"].as_i64().unwrap() {
            bad.push(format!("lane {} vs {}", h.lane.lane, p["lane"]));
        }
        if !bad.is_empty() {
            problems.push(format!("f{f}: {}", bad.join("; ")));
        }

        // extra checks
        let mut xbad = Vec::new();
        let dsize = (h.body.size() - v3(&p["size"])).abs().max_element();
        worst.track("size", dsize, f);
        if !(dsize < TOL) {
            xbad.push(format!("size {:?} vs {}", h.body.size(), p["size"]));
        }
        if h.lane.changing != p["lane_changing"].as_bool().unwrap() {
            xbad.push(format!("lane_changing {} vs {}", h.lane.changing, p["lane_changing"]));
        }
        if h.landed() != p["landed"].as_bool().unwrap() {
            xbad.push(format!("landed {} vs {}", h.landed(), p["landed"]));
        }
        let st = &p["stats"];
        let dt = (g.stats_time - st["time"].as_f64().unwrap()).abs();
        let ds = (g.stats_speed() - st["speed"].as_f64().unwrap()).abs();
        let dz = (g.stats_z - st["z"].as_f64().unwrap()).abs();
        worst.track("stats.time", dt, f);
        worst.track("stats.speed", ds, f);
        worst.track("stats.z", dz, f);
        if !(dt < TOL && ds < TOL && dz < TOL) {
            xbad.push(format!("stats time/speed/z {}/{}/{} vs {}/{}/{}", g.stats_time, g.stats_speed(), g.stats_z, st["time"], st["speed"], st["z"]));
        }
        if ss_port::flow::score(g) != st["score"].as_f64().unwrap() as i64 {
            xbad.push(format!("score {} vs {}", ss_port::flow::score(g), st["score"]));
        }
        if g.coins != st["coins"].as_i64().unwrap() {
            xbad.push(format!("coins {} vs {}", g.coins, st["coins"]));
        }
        // hero animation (Zo / X / the current action)
        let an = &p["anim"];
        if let Some(a) = g.hero_anim.as_ref() {
            let cur = a.current();
            let num = |v: &Value| v.as_f64();
            let mut abad = Vec::new();
            let close6 = |x: Option<f64>, y: Option<f64>, what: &str, bad: &mut Vec<String>, worst: &mut Worst, key: &'static str| match (x, y) {
                (Some(x), Some(y)) => {
                    let d = (x - y).abs();
                    worst.track(key, d, f);
                    if !(d < 2e-6) {
                        bad.push(format!("{what} {x} vs {y}"));
                    }
                }
                (None, None) => {}
                _ => bad.push(format!("{what} {x:?} vs {y:?}")),
            };
            if a.name != an["name"].as_str().unwrap_or("") {
                abad.push(format!("name {} vs {}", a.name, an["name"]));
            }
            if a.clip_name != an["clip"].as_str().unwrap_or("") {
                abad.push(format!("clip {} vs {}", a.clip_name, an["clip"]));
            }
            close6(cur.map(|c| c.time), num(&an["time"]), "time", &mut abad, &mut worst, "anim.time");
            close6(cur.map(|c| c.eff_weight), num(&an["weight"]), "weight", &mut abad, &mut worst, "anim.weight");
            let clip = a.animation;
            close6(clip.map(|c| a.lib.clips[c].duration), num(&an["duration"]), "duration", &mut abad, &mut worst, "anim.duration");
            close6(clip.map(|c| a.clip_speed[c]), num(&an["speed"]), "speed", &mut abad, &mut worst, "anim.speed");
            if clip.map(|c| a.clip_loop[c]) != an["loop"].as_bool() {
                abad.push(format!("loop {:?} vs {}", clip.map(|c| a.clip_loop[c]), an["loop"]));
            }
            if a.mixing != an["mixing"].as_bool().unwrap_or(false) {
                abad.push(format!("mixing {} vs {}", a.mixing, an["mixing"]));
            }
            close6(Some(a.mix_ratio), num(&an["mix_ratio"]), "mix_ratio", &mut abad, &mut worst, "anim.mix_ratio");
            let prev = a.last_animation.map(|c| a.lib.clips[c].name.clone());
            if prev.as_deref() != an["prev"].as_str() {
                abad.push(format!("prev {prev:?} vs {}", an["prev"]));
            }
            if !abad.is_empty() {
                xbad.push(format!("anim [{}]", abad.join(", ")));
            }
        }
        // the pursuers (guard `im`)
        let gr = &p["guard"];
        let gd = &g.guard;
        let dgd = (gd.distance - gr["distance"].as_f64().unwrap()).abs();
        let dgz = (gd.z as f64 - gr["z"].as_f64().unwrap()).abs();
        let dgx = (gd.x as f64 - gr["x"].as_f64().unwrap()).abs();
        worst.track("guard.distance", dgd, f);
        worst.track("guard.z", dgz, f);
        worst.track("guard.x", dgx, f);
        if gd.state.name() != gr["state"].as_str().unwrap()
            || gd.model_active != gr["active"].as_bool().unwrap()
            || !(dgd < TOL && dgz < TOL && dgx < TOL)
        {
            xbad.push(format!(
                "guard {} d {} x {} z {} active {} vs {}",
                gd.state.name(),
                gd.distance,
                gd.x,
                gd.z,
                gd.model_active,
                gr
            ));
        }
        let cam = &p["camera"];
        if !cam.is_null() {
            let (cp, cr) = g.camera.rig.pose();
            let dcp = (cp - v3(&cam["pos"])).abs().max_element();
            let tq = DQuat::from_xyzw(cam["rot"][0].as_f64().unwrap(), cam["rot"][1].as_f64().unwrap(), cam["rot"][2].as_f64().unwrap(), cam["rot"][3].as_f64().unwrap());
            // per component, q and -q being the same rotation
            let comp = |a: DQuat, b: DQuat| (a.x - b.x).abs().max((a.y - b.y).abs()).max((a.z - b.z).abs()).max((a.w - b.w).abs());
            let dcr = comp(cr, tq).min(comp(cr, -tq));
            worst.track("camera.pos", dcp, f);
            worst.track("camera.rot (quat)", dcr, f);
            let dfov = (g.camera.rig.fov - cam["fov"].as_f64().unwrap()).abs();
            worst.track("camera.fov", dfov, f);
            if !(dcp < TOL && dcr < TOL && dfov < TOL) {
                xbad.push(format!("camera pos {cp:?} rot {cr:?} fov {} vs {} {} {}", g.camera.rig.fov, cam["pos"], cam["rot"], cam["fov"]));
            }
        }

        // events this frame: port log vs trace
        let new: Vec<LogEvent> = g.log[log_cursor..].to_vec();
        log_cursor = g.log.len();
        let ev = events.get(&f).cloned().unwrap_or_default();
        // pooled entities are reused: the current life is the latest trace id
        // only live entities have a trace id (the recorder drops removed ones)
        let id_of = |e: EntityId| if g.ent(e).in_scene { sim.ids.iter().filter(|&(_, &v)| v == e).map(|(k, _)| *k).max() } else { None };
        let mut port_ev: Vec<String> = Vec::new();
        for e in &new {
            match e {
                LogEvent::Pickup { entity, .. } => port_ev.push(format!("pickup {:?}", id_of(*entity))),
                LogEvent::Collision { entity, flags, hit, .. } => port_ev.push(format!(
                    "collision {:?} {:?} {:.3} {:.3} {:.3} {:.3} {:.3} {:.3}",
                    id_of(*entity),
                    flag_names(*flags),
                    hit.x(),
                    hit.y(),
                    hit.z(),
                    hit.width(),
                    hit.height(),
                    hit.depth()
                )),
                LogEvent::Trigger { entity, enter: true, .. } => port_ev.push(format!("trigger {:?}", id_of(*entity))),
                LogEvent::RunFromIntro { .. } => port_ev.push("runFromIntro".into()),
                _ => {}
            }
        }
        let mut trace_ev: Vec<String> = Vec::new();
        for e in &ev {
            match e["kind"].as_str().unwrap() {
                "pickup" => trace_ev.push(format!("pickup {:?}", e["id"].as_i64())),
                "collision" => {
                    let hb = &e["hit"];
                    let names: Vec<&str> = e["flags"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
                    trace_ev.push(format!(
                        "collision {:?} {:?} {:.3} {:.3} {:.3} {:.3} {:.3} {:.3}",
                        e["other_id"].as_i64(),
                        names,
                        hb["x"].as_f64().unwrap(),
                        hb["y"].as_f64().unwrap(),
                        hb["z"].as_f64().unwrap(),
                        hb["w"].as_f64().unwrap(),
                        hb["h"].as_f64().unwrap(),
                        hb["d"].as_f64().unwrap()
                    ))
                }
                "trigger" => trace_ev.push(format!("trigger {:?}", e["other_id"].as_i64())),
                "game_runFromIntro" => trace_ev.push("runFromIntro".into()),
                _ => {}
            }
        }
        for k in ["pickup", "collision", "trigger", "runFromIntro"] {
            let c = counts.entry(k).or_default();
            c.0 += port_ev.iter().filter(|e| e.starts_with(k)).count();
            c.1 += trace_ev.iter().filter(|e| e.starts_with(k)).count();
        }
        port_ev.sort();
        trace_ev.sort();
        if port_ev != trace_ev {
            xbad.push(format!("events {port_ev:?} vs {trace_ev:?}"));
        }
        if !xbad.is_empty() {
            extra.push(format!("f{f}: {}", xbad.join("; ")));
        }
        // powerup state (absent in the trace = off)
        let pu = &p["powerups"];
        let mut pbad = Vec::new();
        let mut cmp = |what: &'static str, port: f64, tr: f64, worst: &mut Worst| {
            let d = (port - tr).abs();
            worst.track(what, d, f);
            if !(d < TOL) {
                pbad.push(format!("{what} {port} vs {tr}"));
            }
        };
        let num = |v: &Value| v.as_f64().unwrap_or(0.0);
        cmp("magnet.count", h.magnet.count, num(&pu["magnet"]), &mut worst);
        cmp("multiplier.count", h.multiplier.count, num(&pu["multiplier"]), &mut worst);
        cmp("sneakers.time", h.sneakers.time, num(&pu["sneakers"]), &mut worst);
        cmp("jetpack.distance", h.jetpack.distance, num(&pu["jetpack"]["distance"]), &mut worst);
        if pu["jetpack"].is_object() {
            cmp("jetpack.total", h.jetpack.distance_total, num(&pu["jetpack"]["total"]), &mut worst);
            cmp("jetpack.speed", h.jetpack.speed, num(&pu["jetpack"]["speed"]), &mut worst);
            cmp("jetpack.take_off", h.jetpack.take_off_time, num(&pu["jetpack"]["take_off"]), &mut worst);
        }
        cmp("hoverboard.count", h.hoverboard.count, num(&pu["hoverboard"]["count"]), &mut worst);
        let (m, mm) = if pu["mult"].is_array() { (num(&pu["mult"][0]), num(&pu["mult"][1])) } else { (1.0, g.mission_multiplier) };
        cmp("stats.multiplier", g.multiplier, m, &mut worst);
        cmp("stats.missionMultiplier", g.mission_multiplier, mm, &mut worst);
        if h.hoverboard.paused != pu["hoverboard"]["paused"].as_bool().unwrap_or(false) && h.hoverboard.count != 0.0 {
            pbad.push(format!("hoverboard.paused {} vs {}", h.hoverboard.paused, pu["hoverboard"]["paused"]));
        }
        // attracted coins/pickups: trace id -> position, time, duration
        let id_of = |e: EntityId| sim.ids.iter().filter(|&(_, &v)| v == e).map(|(k, _)| *k).max();
        let mut port_att: BTreeMap<i64, [f64; 5]> = BTreeMap::new();
        for (i, e) in g.ents.iter().enumerate() {
            if let (Some(a), Some(b), true) = (e.attract.as_ref(), e.body.as_ref(), e.in_scene) {
                if a.attracted {
                    if let Some(tid) = id_of(EntityId(i)) {
                        port_att.insert(tid, [b.cx(), b.cy(), b.cz(), a.time, a.duration]);
                    }
                }
            }
        }
        let mut tr_att: BTreeMap<i64, [f64; 5]> = BTreeMap::new();
        for r in pu["attracted"].as_array().into_iter().flatten() {
            tr_att.insert(r[0].as_i64().unwrap(), [num(&r[1]), num(&r[2]), num(&r[3]), num(&r[4]), num(&r[5])]);
        }
        if port_att.keys().ne(tr_att.keys()) {
            pbad.push(format!("attracted ids {:?} vs {:?}", port_att.keys().collect::<Vec<_>>(), tr_att.keys().collect::<Vec<_>>()));
        } else {
            for (k, a) in &port_att {
                let b = tr_att[k];
                let d = (0..5).map(|i| (a[i] - b[i]).abs()).fold(0.0, f64::max);
                worst.track("attracted", d, f);
                if !(d < TOL) {
                    pbad.push(format!("attracted {k} {a:?} vs {b:?}"));
                }
            }
        }
        if !pbad.is_empty() {
            pw.push(format!("f{f}: {}", pbad.join("; ")));
        }
    }

    let r = Report { frames, problems, extra, powerups: pw, worst };
    println!("{trace_rel}: frames simulated: {frames} (f6..=f{last})");
    println!("largest deviations (value @ frame):");
    for (k, (d, f)) in &r.worst.0 {
        println!("  {k:<24} {d:.3e} @ f{f}");
    }
    for (k, (p, t)) in &counts {
        println!("  events {k:<13} port {p} trace {t}");
    }
    for (what, list) in [("hero checks", &r.problems), ("extra checks", &r.extra), ("powerup checks", &r.powerups)] {
        println!("frames failing {what}: {} / {frames}", list.len());
        for p in list.iter().take(30) {
            println!("  {p}");
        }
    }
    r
}
