//! EnvironmentSystem `Tm` (deobfuscated.js:43836-44031) and the scenery it
//! spawns: tracks `K` (9304), fillers `ii` (9004), tube `Fo`/`Po` (12256),
//! epic `$r` (8836), skyline `wm`. See docs/js_notes/environment.md.

use crate::data::{self, Node};
use crate::entities::{Cls, EntityId, FillerKind, TrackKind};
use crate::game::{r_index, sites, Game, BLOCK_SIZE, LANE_WIDTH, VISIBLE_MAX_DISTANCE};
use crate::theme::{self, ConfigSet};
use bevy::math::DVec3;
use serde_json::Value;
use std::f64::consts::PI;

/// Environment kind list for a chunk node, drawing from `site` when the
/// RouteChunk's allowed-kinds list is used (canSpawn and setup draw
/// independently).
fn kinds(g: &mut Game, node: &Value, site: crate::rng::Site) -> Vec<String> {
    if let Some(env) = env_node(node) {
        if let Some(t) = data::comp(env, "Environment").and_then(|e| e["_environmentKind"]["_type"].as_str()) {
            return t.split(',').map(str::to_string).collect();
        }
    }
    let name = data::name(node);
    if name.contains("tunnel") {
        return vec!["Gates".into(), "All".into()];
    }
    if name.contains("epic") {
        return vec!["Epic".into(), "All".into()];
    }
    if let Some(rc) = data::comp(node, "RouteChunk") {
        let list = rc["_limitedAllowedEnvironmentKinds"].as_array().cloned().unwrap_or_default();
        if list.is_empty() {
            return vec!["Fillers".into(), "All".into()];
        }
        let i = r_index(g.rng.as_mut(), site, list.len());
        return list[i]["_type"].as_str().unwrap_or("").split(',').map(str::to_string).collect();
    }
    Vec::new()
}

/// `Cm.environment(node)`: first node with an Environment component,
/// searching children from the last.
pub fn env_node(n: &Value) -> Option<&Value> {
    if data::comp(n, "Environment").is_some() {
        return Some(n);
    }
    if !data::has_children(n) {
        return None;
    }
    data::children(n).iter().rev().find_map(env_node)
}

/// `Tm.canSpawn(node)`: only gates on resources (all loaded in the port),
/// but still draws.
pub fn can_spawn(g: &mut Game, node: &Node) -> bool {
    let k = kinds(g, node, sites::ENV_CAN_SPAWN);
    let has = |s: &str| k.iter().any(|x| x == s);
    let lib = &g.lib;
    !((has("Tube") && !lib.has_group("epic_start"))
        || (has("Station") && !lib.has_group("station_start"))
        || (has("Epic") && !lib.has_group("epic_start"))
        || (has("Gates") && !lib.has_group("gates_base"))
        || (has("Pillars") && !lib.has_group("pillars_start")))
}

/// `Tm.setup(chunk)`: env flags.
pub fn setup(g: &mut Game, ci: usize) {
    let node = g.chunks[ci].node.clone().unwrap();
    let k = kinds(g, &node, sites::ENV_SETUP);
    let has = |s: &str| k.iter().any(|x| x == s);
    let z = g.chunks[ci].z;
    let (pillars, tube, station, epic, gates, empty) =
        (has("Pillars"), has("Tube"), has("Station"), has("Epic"), has("Gates"), has("Empty"));
    let tube_ok = g.route_can_spawn("tube", z);
    let epic_ok = g.route_can_spawn("epic", z);
    let c = &mut g.chunks[ci];
    c.env_tube = false;
    c.env_station = false;
    c.env_epic = false;
    c.env_gates = false;
    c.env_empty = false;
    c.env_pillars = false;
    if pillars {
        c.env_pillars = true;
    } else if tube {
        c.env_tube = tube_ok;
    } else if station {
        c.env_station = true;
    } else if epic {
        c.env_epic = epic_ok;
    } else if gates {
        c.env_gates = true;
    } else if empty {
        c.env_empty = true;
    }
}

/// `Tm.mount(chunk)`.
pub fn mount(g: &mut Game, ci: usize) {
    let c = &g.chunks[ci];
    if c.env_tube {
        spawn_tube(g, ci);
    } else if c.env_station && g.lib.has_group("station_start") {
        spawn_station(g, ci);
    } else if c.env_epic {
        spawn_epic(g, ci);
    } else if c.env_gates {
        // spawnGates is empty
    } else {
        filler_mount(g, ci);
        track_mount(g, ci);
    }
}

/// `Tm.run` (on run): the skyline, created once.
pub fn run(g: &mut Game) {
    if g.skyline.is_none() {
        let s = crate::entities::construct(g, Cls::Skyline);
        g.skyline = Some(s);
        g.add_child(s);
    }
}

/// `Tm.update`: the skyline follows the hero.
pub fn update(g: &mut Game) {
    if let Some(s) = g.skyline {
        let root = g.ent(s).root;
        g.scene.get_mut(root).pos.z = g.stats_z - VISIBLE_MAX_DISTANCE * 0.999;
    }
}

// ---- tracks (K) --------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
pub struct TrackSpawn {
    pub z: Option<f64>,
    pub l: Option<TrackKind>,
    pub m: Option<TrackKind>,
    /// `m: "skip"`
    pub skip_m: bool,
    pub r: Option<TrackKind>,
    pub oy: f64,
}

fn track_get(g: &mut Game, k: TrackKind) -> EntityId {
    // pool.get(cls, {}) -> init -> active; build() is a no-op once mounted
    g.pool_get_init(Cls::Track(k))
}

/// `K.spawn(chunk, opts)`.
pub fn track_spawn(g: &mut Game, ci: usize, o: TrackSpawn) {
    let z = o.z.filter(|&z| z != 0.0).unwrap_or(g.chunks[ci].z);
    let mut l = o.l.unwrap_or(TrackKind::Si);
    let mut m = o.m.unwrap_or(l);
    let mut r = o.r.unwrap_or(m);
    if !g.lib.has_group("ground") {
        (l, m, r) = (TrackKind::Si, TrackKind::Si, TrackKind::Si);
    }
    let place = |g: &mut Game, k: TrackKind, x: f64| {
        let t = track_get(g, k);
        g.add_child(t);
        let root = g.ent(t).root;
        let ob = g.scene.get_mut(root);
        ob.pos = DVec3::new(x, o.oy, z);
        ob.rot.y = PI;
    };
    place(g, l, -LANE_WIDTH);
    if !o.skip_m {
        place(g, m, 0.0);
    }
    place(g, r, LANE_WIDTH);
}

/// `K.spawnGates(chunk)`.
pub fn track_spawn_gates(g: &mut Game, ci: usize) {
    let z = g.chunks[ci].z;
    let t = g.pool_get(Cls::Track(TrackKind::Pi));
    g.add_child(t);
    let root = g.ent(t).root;
    let ob = g.scene.get_mut(root);
    ob.pos = DVec3::new(0.0, 0.1, z);
    ob.rot.y = PI;
}

/// `K.mount(chunk)` (9396).
pub fn track_mount(g: &mut Game, ci: usize) {
    let name = g.chunks[ci].name.clone();
    let slots = (g.chunks[ci].blocks / 2.0) as i64;
    let z0 = g.chunks[ci].z;
    use TrackKind::{Oi, Si};
    let spawn_slot = |g: &mut Game, i: i64, o: TrackSpawn| {
        track_spawn(g, ci, o);
        g.chunks[ci].floor_slots.insert(i, true);
    };
    if name == "intro" {
        if !g.chunks[ci].floor_slots.contains_key(&0) {
            spawn_slot(g, 0, TrackSpawn { z: Some(BLOCK_SIZE * 2.0 - 90.0), l: Some(Si), m: Some(Si), r: Some(Si), ..Default::default() });
        }
    } else if name.contains("default_short_1_track") {
        for i in 0..slots {
            if g.chunks[ci].floor_slots.contains_key(&i) {
                continue;
            }
            let side = if i > 0 { Oi } else { Si };
            spawn_slot(g, i, TrackSpawn { z: Some(z0 - i as f64 * BLOCK_SIZE * 2.0), l: Some(side), m: Some(Si), r: Some(side), ..Default::default() });
        }
    } else if name.contains("default_1_track") {
        let mid = name.contains("_mid");
        for i in 0..slots {
            if g.chunks[ci].floor_slots.contains_key(&i) {
                continue;
            }
            let side = if mid { Oi } else { Si };
            spawn_slot(g, i, TrackSpawn { z: Some(z0 - i as f64 * BLOCK_SIZE * 2.0), l: Some(side), m: Some(Si), r: Some(side), ..Default::default() });
        }
    } else if name.contains("default_short_2_tracks") {
        let ground_mid = name.contains("_mid_") || name.contains("_end");
        for i in 0..slots {
            if g.chunks[ci].floor_slots.contains_key(&i) {
                continue;
            }
            let m = if ground_mid { Oi } else { Si };
            spawn_slot(g, i, TrackSpawn { z: Some(z0 - i as f64 * BLOCK_SIZE * 2.0), l: Some(Si), m: Some(m), r: Some(Si), ..Default::default() });
        }
    } else if name.contains("default_2_tracks") {
        let ground_mid = !name.contains("_end");
        for i in 0..slots {
            if g.chunks[ci].floor_slots.contains_key(&i) {
                continue;
            }
            let m = if ground_mid { Oi } else { Si };
            spawn_slot(g, i, TrackSpawn { z: Some(z0 - i as f64 * BLOCK_SIZE * 2.0), l: Some(Si), m: Some(m), r: Some(Si), ..Default::default() });
        }
    } else if !g.chunks[ci].has_ground {
        for i in 0..slots {
            if g.chunks[ci].floor_slots.contains_key(&i) {
                continue;
            }
            spawn_slot(g, i, TrackSpawn { z: Some(z0 - i as f64 * BLOCK_SIZE * 2.0), l: Some(Si), m: Some(Si), r: Some(Si), ..Default::default() });
        }
    }
}

// ---- fillers (ii) ------------------------------------------------------------------

fn fillers_available(g: &Game) -> bool {
    g.lib.has_group("low_01_left") || g.lib.has_group("high_02_left")
}

/// `ii.spawn(chunk, {z, l, r})`.
pub fn filler_spawn(g: &mut Game, ci: usize, z: Option<f64>, l: FillerKind, r: FillerKind) {
    let z = z.filter(|&z| z != 0.0).unwrap_or(g.chunks[ci].z);
    let (l, r) = if fillers_available(g) { (l, r) } else { (FillerKind::Med02L, FillerKind::Med02R) };
    let mut ids = Vec::new();
    for k in [l, r] {
        let f = g.pool_get(Cls::Filler(k));
        g.add_child(f);
        let root = g.ent(f).root;
        let ob = g.scene.get_mut(root);
        ob.pos = DVec3::new(0.0, 0.0, z);
        ob.rot.y = PI;
        ids.push(f);
    }
    if fillers_available(g) {
        fillers_event_items(g, ids[0], ids[1]);
    }
}

/// bali `fillersEventItems` (8425): hide the base mesh (container child 0)
/// of most right/left filler halves.
fn fillers_event_items(g: &mut Game, left: EntityId, right: EntityId) {
    for (e, names) in [
        (right, &["high_01_right", "high_02_right", "high_03_right", "low_01_right", "low_02_right"][..]),
        (left, &["high_02_left", "high_03_left", "low_01_left", "low_02_left"][..]),
    ] {
        let root = g.ent(e).root;
        let Some(&c0) = g.scene.get(root).children.first() else { continue };
        let hit = g.scene.get(c0).geom_hash.as_deref().is_some_and(|h| names.iter().any(|n| h.contains(n)));
        if hit {
            g.scene.set_active(c0, false);
        }
    }
}

/// `ii.mount(chunk)` (9070).
pub fn filler_mount(g: &mut Game, ci: usize) {
    let name = g.chunks[ci].name.clone();
    let slots = (g.chunks[ci].blocks / 2.0) as i64;
    let z0 = g.chunks[ci].z;
    use FillerKind::*;
    let fixed: Option<(FillerKind, FillerKind, f64)> = if name.contains("default_short_1_track") {
        Some((Med02L, Med02R, 0.0))
    } else if name.contains("default_short_2_tracks") {
        Some((Low02L, High02R, 0.0))
    } else if name.contains("default_2_tracks") {
        Some((Low01L, High01R, 0.0))
    } else if name.contains("intro") {
        Some((Med02L, Med02R, 90.0))
    } else {
        None
    };
    if let Some((l, r, dz)) = fixed {
        for i in 0..slots {
            if g.chunks[ci].filler_slots.contains_key(&i) {
                continue;
            }
            filler_spawn(g, ci, Some(z0 - i as f64 * BLOCK_SIZE * 2.0 + dz), l, r);
        }
        return;
    }
    let kinds = ["Low", "Med", "High"];
    let kind = kinds[r_index(g.rng.as_mut(), sites::FILLER_KIND, 3)];
    let nums: &[&str] = if kind == "Low" { &["01", "02"] } else { &["01", "02", "03"] };
    for i in 0..slots {
        if g.chunks[ci].filler_slots.contains_key(&i) {
            continue;
        }
        let num = nums[r_index(g.rng.as_mut(), sites::FILLER_NUM, nums.len())];
        let l = FillerKind::by_key(kind, num, true);
        let r = FillerKind::by_key(kind, num, false);
        filler_spawn(g, ci, Some(z0 - i as f64 * BLOCK_SIZE * 2.0), l, r);
    }
}

// ---- tube (Fo + Po) ----------------------------------------------------------------

/// `Tm.spawnTube` (43977).
fn spawn_tube(g: &mut Game, ci: usize) {
    let node = g.chunks[ci].node.clone().unwrap();
    let fo = g.pool_get(Cls::Fo);
    g.add_child(fo);
    fo_awake(g, ci, fo, &node);
    let c = &g.chunks[ci];
    let z = c.z - c.length - 360.0;
    g.route_set_spawn("tube", z);
}

/// `Fo.factory` (node with a Tube environment).
pub fn tube_factory(g: &mut Game, ci: usize, n: &Value) -> EntityId {
    let fo = g.pool_get(Cls::Fo);
    fo_awake(g, ci, fo, n);
    g.add_child(fo);
    fo
}

fn fo_awake(g: &mut Game, ci: usize, fo: EntityId, n: &Value) {
    let (_, _, pz) = data::pos(n);
    let mut blocks = g.chunks[ci].blocks;
    if let Some(e) = data::comp(n, "Environment") {
        blocks = e["_blockCount"].as_f64().unwrap_or(blocks);
    }
    let cz = g.chunks[ci].z;
    let back = {
        let b = g.body(fo);
        b.set_cx(0.0);
        b.set_sz(BLOCK_SIZE * blocks);
        b.set_back(cz - pz);
        b.back()
    };
    let segs = (blocks * 0.5) as i64;
    for i in 0..segs {
        let po = g.pool_get(Cls::Po);
        po_empty_theme(g, po);
        let root = g.ent(po).root;
        if i == 0 {
            if segs > 1 {
                po_set(g, po, |h| h.contains("end"), None);
            }
            theme::handle_chunk_config(g, ConfigSet::Tunnel, root, &["tube_start", "tube_mid"], true, &[BLOCK_SIZE]);
            if segs == 1 {
                po_set_end(g, po, false);
                theme::handle_chunk_config(g, ConfigSet::Tunnel, root, &["tube_end", "tube_mid"], true, &[BLOCK_SIZE]);
            }
        } else if i < segs - 1 {
            po_set(g, po, |h| !h.contains("mid"), None);
            theme::handle_chunk_config(g, ConfigSet::Tunnel, root, &["tube_mid"], true, &[BLOCK_SIZE]);
        } else {
            po_set_end(g, po, true);
            theme::handle_chunk_config(g, ConfigSet::Tunnel, root, &["tube_end", "tube_mid"], true, &[-BLOCK_SIZE, BLOCK_SIZE]);
        }
        g.body(po).set_back(back - BLOCK_SIZE * 2.0 * i as f64);
        g.add_child(po);
        // bali hideTubeFloors() -> no floors
    }
}

/// `Po.emptyTheme()`: destroys end-only parts with the forEach+destroy skip.
fn po_empty_theme(g: &mut Game, po: EntityId) {
    let view = g.ent(po).view.unwrap();
    let mut i = 0;
    loop {
        let children = &g.scene.get(view).children;
        let Some(&c) = children.get(i) else { break };
        let h = g.scene.get(c).geom_hash.clone().unwrap_or_default();
        if !h.contains("_start") && !h.contains("_mid") && h.contains("_end") {
            g.scene.destroy(c);
            // the array shrank: the next element shifted into slot i and is
            // skipped by the iteration, like the JS forEach.
            i += 1;
            g.scene.get_mut(c).visible = true;
            continue;
        }
        g.scene.get_mut(c).visible = true;
        i += 1;
    }
}

fn po_set(g: &mut Game, po: EntityId, hide: impl Fn(&str) -> bool, _z: Option<f64>) {
    let view = g.ent(po).view.unwrap();
    for c in g.scene.get(view).children.clone() {
        let h = g.scene.get(c).geom_hash.clone().unwrap_or_default();
        if hide(&h) {
            g.scene.get_mut(c).visible = false;
        }
    }
}

fn po_set_end(g: &mut Game, po: EntityId, hide_start: bool) {
    let view = g.ent(po).view.unwrap();
    for c in g.scene.get(view).children.clone() {
        let h = g.scene.get(c).geom_hash.clone().unwrap_or_default();
        if hide_start && h.contains("start") {
            g.scene.get_mut(c).visible = false;
        }
        if h.contains("end") {
            g.scene.get_mut(c).pos.z = BLOCK_SIZE * 2.0;
        }
    }
}

// ---- pillars (io + to/no/ro) ---------------------------------------------------------

/// `io.factory` -> `awake(chunk, node)` (11407): start piece, `blockCount - 2`
/// mids, end piece, each with its shadowed floor (bali has no
/// `hidePillarFloors` / `longPillarMids` / `handlePillar*` hooks).
pub fn pillars_factory(g: &mut Game, ci: usize, n: &Value) -> EntityId {
    use TrackKind::{Ai, Ci, Ji, Ki, Ti, Wi};
    let io = g.pool_get(Cls::PillarsEnv);
    let (_, _, pz) = data::pos(n);
    let blocks = data::comp(n, "Environment").and_then(|e| e["_blockCount"].as_f64()).unwrap_or(g.chunks[ci].blocks);
    let cz = g.chunks[ci].z;
    let back = {
        let b = g.body(io);
        b.set_cx(0.0);
        b.set_sz(BLOCK_SIZE * blocks);
        b.set_back(cz - pz);
        b.back()
    };
    let start = g.pool_get(Cls::PillarPiece(0));
    g.body(start).set_back(back);
    let start_back = g.body(start).back();
    g.add_child(start);
    track_spawn(g, ci, TrackSpawn { z: Some(start_back), l: Some(Ci), m: Some(Ki), r: Some(Ci), ..Default::default() });
    for i in 0..(blocks as i64 - 2).max(0) {
        let mid = g.pool_get(Cls::PillarPiece(1));
        g.body(mid).set_back(start_back - BLOCK_SIZE * (i + 1) as f64);
        let z = g.body(mid).back();
        g.add_child(mid);
        track_spawn(g, ci, TrackSpawn { z: Some(z), l: Some(Wi), m: Some(Ai), r: Some(Wi), ..Default::default() });
    }
    let end = g.pool_get(Cls::PillarPiece(2));
    g.body(end).set_back(start_back - BLOCK_SIZE * (blocks - 1.0));
    let z = g.body(end).back() + BLOCK_SIZE;
    g.add_child(end);
    track_spawn(g, ci, TrackSpawn { z: Some(z), l: Some(Ti), m: Some(Ji), r: Some(Ti), ..Default::default() });
    io
}

// ---- station (ho + fo/po/mo, uo + lo) -------------------------------------------------

/// `ho.awake(chunk, node)` (11743), node path: roof, pieces, platforms, floors.
pub fn station_factory(g: &mut Game, ci: usize, n: &Value) -> EntityId {
    use TrackKind::{Ai, Ci, Ji, Ki, Ti, Wi};
    let ho = g.pool_get(Cls::StationRoof);
    let blocks = data::comp(n, "Environment").and_then(|e| e["_blockCount"].as_f64()).unwrap_or(4.0);
    let (_, _, pz) = data::pos(n);
    let mut z0 = g.chunks[ci].z - pz;
    if !g.chunks[ci].name.contains("short") {
        z0 += BLOCK_SIZE * 4.0;
    }
    let (back, front) = {
        let b = g.body(ho);
        b.set_sz(BLOCK_SIZE * blocks);
        b.set_cx(0.0);
        b.set_top(86.0);
        b.set_back(z0);
        (b.back(), b.front())
    };
    let piece = |g: &mut Game, k: u8, cz: f64| {
        let e = g.pool_get(Cls::Station(k));
        g.body(e).set_cz(cz);
        let root = g.ent(e).root;
        g.scene.get_mut(root).scale.z = 1.01;
        g.add_child(e);
    };
    piece(g, 0, z0);
    let mut k = 0.0;
    while k < (blocks - 2.0) * 0.5 {
        piece(g, 1, z0 - BLOCK_SIZE - BLOCK_SIZE * 2.0 * k);
        k += 1.0;
    }
    piece(g, 2, z0 - (blocks - 1.0) * BLOCK_SIZE);
    g.set_slots_by_position(ci, back, front, true, false);
    let half = blocks * 0.5;
    let mut k = 0.0;
    while k < half {
        let p = g.pool_get(Cls::StationPlatform);
        platform_awake(g, p, z0 - BLOCK_SIZE * 2.0 * k);
        g.add_child(p);
        k += 1.0;
    }
    let mut k = 0.0;
    while k < half {
        let z = z0 - k * BLOCK_SIZE * 2.0;
        let (l, m) = if k == 0.0 { (Ki, Ci) } else if k < half - 1.0 { (Ai, Wi) } else { (Ji, Ti) };
        track_spawn(g, ci, TrackSpawn { z: Some(z), l: Some(l), m: Some(m), r: Some(l), ..Default::default() });
        k += 1.0;
    }
    g.set_slots_by_position(ci, back, front, true, false);
    g.set_slots_by_position(ci, back, front, false, true);
    ho
}

/// `uo.awake(chunk, node, back)` (11713): the platform, then its two `lo` sides.
fn platform_awake(g: &mut Game, uo: EntityId, back: f64) {
    let (cz, depth) = {
        let b = g.body(uo);
        b.set_back(back);
        b.set_cx(0.0);
        (b.cz(), b.sz())
    };
    for x in [-LANE_WIDTH, LANE_WIDTH] {
        let lo = g.pool_get(Cls::StationSide);
        let b = g.body(lo);
        // lo.reset(): size and center zeroed
        b.bx.c = [0.0; 3];
        b.bx.s = [0.0; 3];
        b.set_sx(20.0);
        b.set_sy(9.0);
        b.set_sz(depth);
        b.set_cx(x);
        b.set_bottom(0.0);
        b.set_cz(cz);
        g.add_child(lo);
    }
}

/// `Tm.spawnStation` (43994): a roof shell straight from the pool (no
/// awake), then floors before fillers.
fn spawn_station(g: &mut Game, ci: usize) {
    let shell = g.pool_get(Cls::StationRoof);
    g.add_child(shell);
    track_mount(g, ci);
    filler_mount(g, ci);
}

// ---- epic ($r) -----------------------------------------------------------------------

/// `Tm.spawnEpic` (43969): pieces, tracks (bali addEpicFloor), spawn gate.
fn spawn_epic(g: &mut Game, ci: usize) {
    let blocks = g.chunks[ci].blocks;
    let count = (blocks * 0.25) as i64;
    let mut dz = 0.0;
    for i in 0..count {
        let part = if i == 0 { 0 } else if i < count - 1 { 1 } else { 2 };
        let e = g.pool_get(Cls::Epic(part));
        let cz = g.chunks[ci].z;
        let root = g.ent(e).root;
        {
            let o = g.scene.get_mut(root);
            o.pos = DVec3::new(0.0, 0.0, cz - dz);
            o.rot.y = PI;
        }
        g.add_child(e);
        dz += BLOCK_SIZE * 4.0;
    }
    track_mount(g, ci);
    let c = &g.chunks[ci];
    let z = c.z - c.length - 1800.0;
    g.route_set_spawn("epic", z);
}
