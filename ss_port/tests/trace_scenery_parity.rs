//! Differential parity: the port's chunk placement, scenery spawns and
//! despawns against the oracle trace of the original game, run as a headless
//! Bevy app (MinimalPlugins + SimPlugin, one update per game frame).
//!
//! The port runs the whole game (level/route/mount/environment, hero,
//! physics, pickups) with the oracle's per-call-site RNG streams; the only
//! input from the trace is the autopilot's keydowns. Hero, guard and camera
//! rig (`Gp`, `im`, `om`) are not scenery and are excluded.

use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use serde_json::Value;
use ss_port::describe::{describe, EntityDesc};
use ss_port::entities::EntityId;
use ss_port::game::LogEvent;
use ss_port::sim_plugin::{trace_hunt_letter, DataPaths, Sim, SimPlugin, ReplayDriver};
use ss_port::trace::Trace;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

const TOL: f64 = 1e-3;

thread_local! {
    /// Largest position/scale deviation seen (value, what).
    static WORST: std::cell::RefCell<(f64, String)> = std::cell::RefCell::new((0.0, String::new()));
}
fn track(a: DVec3, b: DVec3, what: &str) {
    let d = (a - b).abs().max_element();
    WORST.with(|w| {
        let mut w = w.borrow_mut();
        if d > w.0 {
            *w = (d, what.to_string());
        }
    });
}
use ss_port::sim_plugin::NOT_SCENERY;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

fn v3(v: &Value) -> DVec3 {
    DVec3::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap())
}
fn quat(v: &Value) -> DQuat {
    DQuat::from_xyzw(v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap(), v[3].as_f64().unwrap())
}
fn quat_close(a: DQuat, b: DQuat) -> bool {
    let d = a.dot(b).abs();
    // |dot| ~ 1 for the same rotation (q and -q)
    (1.0 - d) < TOL * TOL
        || ((a.x - b.x).abs() < TOL && (a.y - b.y).abs() < TOL && (a.z - b.z).abs() < TOL && (a.w - b.w).abs() < TOL)
        || ((a.x + b.x).abs() < TOL && (a.y + b.y).abs() < TOL && (a.z + b.z).abs() < TOL && (a.w + b.w).abs() < TOL)
}
fn close(a: DVec3, b: DVec3) -> bool {
    track(a, b, &format!("{a:?} vs {b:?}"));
    (a - b).abs().max_element() < TOL
}
/// "bundles/bali/game-idle/environment-tex@2x.webp" -> "environment-tex"
fn map_name(path: &str) -> String {
    let f = path.rsplit('/').next().unwrap();
    let f = f.split('.').next().unwrap();
    f.split('@').next().unwrap().to_string()
}

/// Field-by-field comparison; returns human-readable differences.
fn compare(port: &EntityDesc, tr: &Value) -> Vec<String> {
    let mut d = Vec::new();
    if port.cls != tr["cls"].as_str().unwrap() {
        d.push(format!("cls {} vs {}", port.cls, tr["cls"]));
        return d;
    }
    if !close(port.pos, v3(&tr["pos"])) {
        d.push(format!("pos {:?} vs {}", port.pos, tr["pos"]));
    }
    if !quat_close(port.rot, quat(&tr["rot"])) {
        d.push(format!("rot {:?} vs {}", port.rot, tr["rot"]));
    }
    if !close(port.scale, v3(&tr["scale"])) {
        d.push(format!("scale {:?} vs {}", port.scale, tr["scale"]));
    }
    match (&port.body, tr["body"].is_null()) {
        (Some((c, s)), false) => {
            if !close(*c, v3(&tr["body"]["center"])) || !close(*s, v3(&tr["body"]["size"])) {
                d.push(format!("body {c:?}/{s:?} vs {}/{}", tr["body"]["center"], tr["body"]["size"]));
            }
        }
        (None, true) => {}
        _ => d.push(format!("body presence {} vs {}", port.body.is_some(), !tr["body"].is_null())),
    }
    let tm = tr["meshes"].as_array().unwrap();
    if port.meshes.len() != tm.len() {
        d.push(format!(
            "mesh count {} vs {}: port {:?} trace {:?}",
            port.meshes.len(),
            tm.len(),
            port.meshes.iter().map(|m| m.mesh.as_str()).collect::<Vec<_>>(),
            tm.iter().map(|m| m["mesh"].as_str().unwrap_or("?")).collect::<Vec<_>>()
        ));
        return d;
    }
    for (i, (p, t)) in port.meshes.iter().zip(tm).enumerate() {
        let tag = format!("mesh[{i}] {}", p.mesh);
        if p.mesh != t["mesh"].as_str().unwrap_or("") {
            d.push(format!("{tag}: name vs {}", t["mesh"]));
            continue;
        }
        if p.pk.as_deref() != t["pk"].as_str() {
            d.push(format!("{tag}: pk {:?} vs {}", p.pk, t["pk"]));
        }
        if p.visible != t["visible"].as_bool().unwrap() {
            d.push(format!("{tag}: visible {} vs {}", p.visible, t["visible"]));
        }
        if !close(p.pos, v3(&t["pos"])) {
            d.push(format!("{tag}: pos {:?} vs {}", p.pos, t["pos"]));
        }
        if !quat_close(p.rot, quat(&t["rot"])) {
            d.push(format!("{tag}: rot {:?} vs {}", p.rot, t["rot"]));
        }
        if !close(p.scale, v3(&t["scale"])) {
            d.push(format!("{tag}: scale {:?} vs {}", p.scale, t["scale"]));
        }
        let ttex = t["material"]["texture"].as_str().map(map_name);
        if p.pk.is_some() || p.texture.is_some() {
            if p.texture != ttex {
                d.push(format!("{tag}: texture {:?} vs {:?}", p.texture, ttex));
            }
        }
        if p.pk.is_some() {
            if p.pk_material.as_deref() != t["material"]["pkMaterial"].as_str() {
                d.push(format!("{tag}: pkMaterial {:?} vs {}", p.pk_material, t["material"]["pkMaterial"]));
            }
            let st = &t["state"];
            if p.culling != st["culling"].as_bool() {
                d.push(format!("{tag}: culling {:?} vs {}", p.culling, st["culling"]));
            }
            let tb = match &st["blendMode"] {
                Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            if p.blend_mode.as_deref() != Some(tb.as_str()) {
                d.push(format!("{tag}: blendMode {:?} vs {}", p.blend_mode, tb));
            }
            if p.depth_mask != st["depthMask"].as_bool() {
                d.push(format!("{tag}: depthMask {:?} vs {}", p.depth_mask, st["depthMask"]));
            }
        }
    }
    d
}

#[test]
fn seed1_scenery_matches_oracle() {
    scenery("oracle/traces/seed1-god/trace.jsonl");
}

/// The obstacle / station chunks (forced with `config.chunk`).
#[test]
fn obstacle_and_station_chunks() {
    scenery("oracle/traces/seed1-god-default_1_track_start/trace.jsonl");
    scenery("oracle/traces/seed1-god-default_1_track_mid_var_1/trace.jsonl");
    scenery("oracle/traces/seed1-god-default_short_1_track/trace.jsonl");
}

/// Powerup runs: the jetpack's safe-landing chunks and sky coins, the
/// headstart's pickups, the magnet's collected coins.
#[test]
fn powerup_runs() {
    scenery("oracle/traces/seed1-god-jetpack/trace.jsonl");
    scenery("oracle/traces/seed1-god-boosts/trace.jsonl");
    scenery("oracle/traces/seed1-god-magnet/trace.jsonl");
}

/// A death, then `nav.toGame()`: the world reset in place and a second run.
#[test]
fn restart_in_place() {
    scenery("oracle/traces/seed1-restart/trace.jsonl");
}

fn scenery(trace_rel: &str) {
    println!("== {trace_rel}");
    let trace = Rc::new(Trace::load(&root().join(trace_rel)).unwrap());
    let seed = trace.meta["seed"].as_i64().unwrap() as i32;
    let paths = DataPaths::from_repo(root());
    let mut sim = Sim::new(&paths, seed, trace_hunt_letter(&trace), Box::new(ReplayDriver::new(trace.clone()))).unwrap();
    sim.configure_from_trace(&trace);

    // Headless Bevy app: one update = one game frame.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(SimPlugin);
    app.insert_non_send(sim);

    // trace records grouped by frame (ids are in add order)
    let mut spawns_by_frame: HashMap<i64, Vec<&Value>> = HashMap::new();
    for r in trace.of_type("entity_spawn") {
        if NOT_SCENERY.contains(&r["cls"].as_str().unwrap()) {
            continue;
        }
        spawns_by_frame.entry(r["f"].as_i64().unwrap()).or_default().push(r);
    }
    for v in spawns_by_frame.values_mut() {
        v.sort_by_key(|r| r["id"].as_i64().unwrap());
    }
    let trace_chunks: Vec<&Value> = trace.of_type("chunk_place").collect();
    let despawns: HashMap<i64, i64> =
        trace.of_type("entity_despawn").map(|r| (r["id"].as_i64().unwrap(), r["f"].as_i64().unwrap())).collect();

    let mut problems: Vec<String> = Vec::new();
    // current life of each (pooled, reused) port entity -> trace id
    let mut port_to_trace: HashMap<EntityId, i64> = HashMap::new();
    let mut matched_ids: Vec<i64> = Vec::new();
    let mut pending_capture: Vec<(i64, EntityId, &Value)> = Vec::new();
    let mut log_cursor = 0;
    let mut matched_spawns = 0;
    let mut port_chunks = Vec::new();
    let mut port_despawns: HashMap<i64, i64> = HashMap::new();
    let mut port_spawn_buckets: HashMap<i64, Vec<EntityId>> = HashMap::new();
    let mut deferred: Option<i64> = None;
    let mut frames = 0;

    let (_, last) = trace.frame_range();
    for f in 5..=last {
        if f > 5 {
            app.update();
            frames += 1;
        }
        let sim = app.world_mut().non_send_mut::<Sim>();
        assert_eq!(sim.game.frame, if f > 5 { f } else { 5 }, "sim frame out of step");
        // new log entries this frame, bucketed by the frame they were logged
        // in (UI calls between frames log under the previous frame)
        let new: Vec<LogEvent> = sim.game.log[log_cursor..].to_vec();
        log_cursor = sim.game.log.len();
        for ev in new {
            match ev {
                LogEvent::Spawn { frame, entity, .. } => port_spawn_buckets.entry(frame.max(5)).or_default().push(entity),
                LogEvent::Despawn { frame, entity } => {
                    if let Some(t) = port_to_trace.get(&entity) {
                        port_despawns.insert(*t, frame);
                    }
                }
                LogEvent::ChunkPlace { frame, name, start, z, length, blocks, .. } => port_chunks.push((frame, name, start, z, length, blocks)),
                _ => {}
            }
        }
        // compare this frame now; a frame where the port spawned nothing but the
        // trace did waits one update (UI calls between frames log late)
        let mut frames_done: Vec<i64> = Vec::new();
        if let Some(d) = deferred.take() {
            frames_done.push(d);
        }
        let empty_now = port_spawn_buckets.get(&f).is_none_or(Vec::is_empty);
        if empty_now && spawns_by_frame.get(&f).is_some_and(|v| !v.is_empty()) && f != last {
            deferred = Some(f);
        } else {
            frames_done.push(f);
        }
        for fd in frames_done {
            let port_spawns = port_spawn_buckets.remove(&fd).unwrap_or_default();
            let tr = spawns_by_frame.get(&fd).cloned().unwrap_or_default();
            if port_spawns.len() != tr.len() {
                problems.push(format!(
                    "f{fd}: {} port spawns vs {} trace spawns: port {:?} trace {:?}",
                    port_spawns.len(),
                    tr.len(),
                    port_spawns.iter().map(|&e| sim.game.ent(e).cls.name()).collect::<Vec<_>>(),
                    tr.iter().map(|r| r["cls"].as_str().unwrap()).collect::<Vec<_>>()
                ));
            }
            for (pe, r) in port_spawns.iter().zip(tr.iter()) {
                let tid = r["id"].as_i64().unwrap();
                assert_eq!(sim.ids.get(&tid), Some(pe), "sim id mapping for trace id {tid}");
                port_to_trace.insert(*pe, tid);
                matched_ids.push(tid);
                pending_capture.push((r["captured_f"].as_i64().unwrap_or(fd), *pe, r));
                matched_spawns += 1;
            }
        }
        // describe entities whose trace capture frame is now
        let mut keep = Vec::new();
        for (cf, pe, r) in pending_capture.drain(..) {
            if cf <= f {
                let d = compare(&describe(&sim.game, pe), r);
                if !d.is_empty() && problems.len() < 200 {
                    problems.push(format!("f{f} trace id {} ({}): {}", r["id"], r["cls"], d.join("; ")));
                }
            } else {
                keep.push((cf, pe, r));
            }
        }
        pending_capture = keep;
    }

    // chunk placements
    if port_chunks.len() != trace_chunks.len() {
        problems.push(format!("chunk count {} vs {}", port_chunks.len(), trace_chunks.len()));
    }
    for (p, t) in port_chunks.iter().zip(&trace_chunks) {
        let (frame, name, start, z, length, blocks) = p;
        // the intro is placed at boot; the trace snapshots it at recording start
        let tf = if t["preexisting"] == true { *frame } else { t["f"].as_i64().unwrap() };
        if *frame != tf
            || name != t["name"].as_str().unwrap()
            || (start - t["start"].as_f64().unwrap()).abs() > TOL
            || (z - t["anchor_z"].as_f64().unwrap()).abs() > TOL
            || (length - t["length"].as_f64().unwrap()).abs() > TOL
            || (blocks - t["blocks"].as_f64().unwrap()).abs() > TOL
        {
            problems.push(format!("chunk port f{frame} {name} start {start} z {z} len {length} vs trace f{tf} {} start {} z {}", t["name"], t["start"], t["anchor_z"]));
        }
    }
    // despawn frames for every matched entity
    let mut despawn_diffs = 0;
    for &tid in &matched_ids {
        let a = port_despawns.get(&tid);
        let b = despawns.get(&tid);
        if a != b {
            despawn_diffs += 1;
            if problems.len() < 220 {
                problems.push(format!("despawn of trace id {tid}: port {a:?} vs trace {b:?}"));
            }
        }
    }

    let trace_total: usize = spawns_by_frame.values().map(Vec::len).sum();
    let trace_despawns = matched_ids.iter().filter(|t| despawns.contains_key(t)).count();
    println!("bevy updates (game frames): {frames}");
    println!("chunks: port {} trace {}", port_chunks.len(), trace_chunks.len());
    println!("spawns matched: {matched_spawns} / trace {trace_total}");
    println!("despawn frames checked for {} entities ({trace_despawns} despawned in the trace), mismatches: {despawn_diffs}", matched_ids.len());
    WORST.with(|w| println!("largest position/scale deviation: {:.3e} ({})", w.borrow().0, w.borrow().1));
    // spawn-count and chunk problems first, then the first field diffs
    for p in problems.iter().filter(|p| p.contains("spawns vs") || p.starts_with("chunk")) {
        println!("  {p}");
    }
    for p in problems.iter().filter(|p| !(p.contains("spawns vs") || p.starts_with("chunk"))).take(60) {
        println!("  {p}");
    }
    assert!(problems.is_empty(), "{} parity problems (first 60 above)", problems.len());
    assert_eq!(matched_spawns, trace_total);
}
