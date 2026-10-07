//! Chunk node mounting: `bg.mount` (deobfuscated.js:48332), the class
//! registry `Ro` (12453) and the route-node factories (docs/js_notes/
//! chunk_mount_entities.md §3).

use crate::data::{self, Node};
use crate::entities::{BlockerKind, Cls, EntityId, GateKind, PickupKind, TrainKind};
use crate::game::{r_index, sites, Game, MountOpts, BLOCK_SIZE, LANE_WIDTH};
use crate::scene::{MatOpts, Visual};
use crate::theme::{self, ConfigSet};
use bevy::math::DVec3;
use serde_json::Value;
use std::f64::consts::PI;

/// Node -> class (`Ro.match`, cached by node name forever).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeClass {
    Blocker,
    Checkpoint,
    Coin,
    Gate,
    LightSignal,
    Obstacle,
    Pickup,
    Pillar,
    PillarsEnv,
    Ramp,
    Bag,
    StationEnv,
    Train,
    Trigger,
    TubeEnv,
}

fn env_type_has(n: &Value, t: &str) -> bool {
    data::comp(n, "Environment")
        .and_then(|e| e["_environmentKind"]["_type"].as_str())
        .is_some_and(|s| s.split(',').any(|x| x == t))
}

fn re_digit_after(name: &str, prefix: &str) -> bool {
    // /prefix(\d)_/
    let b = name.as_bytes();
    let p = prefix.as_bytes();
    (0..b.len()).any(|i| b[i..].starts_with(p) && b.len() > i + p.len() + 1 && b[i + p.len()].is_ascii_digit() && b[i + p.len() + 1] == b'_')
}

fn match_node(n: &Value) -> Option<NodeClass> {
    let name = data::name(n);
    // Lo = [vr, Cr, _r, Ii, Hi, Gi, Ha, eo, io, so, co, ho, So, No, Fo, $r]
    if name.contains("blocker") {
        return Some(NodeClass::Blocker);
    }
    if name.contains("checkpoint_") {
        return Some(NodeClass::Checkpoint);
    }
    if name.contains("Coin (") || name.contains("Coins (") {
        return Some(NodeClass::Coin);
    }
    if name.starts_with("gates_") && name.contains("_group_place") && name.find("gates_").unwrap() < name.find("_group_place").unwrap() {
        return Some(NodeClass::Gate);
    }
    if name.contains("lightSignal") {
        return Some(NodeClass::LightSignal);
    }
    if name.contains("obstacle_group") {
        return Some(NodeClass::Obstacle);
    }
    if name.contains("PickupSpawn") {
        return Some(NodeClass::Pickup);
    }
    if name.contains("pillar_group_place") {
        return Some(NodeClass::Pillar);
    }
    if env_type_has(n, "Pillars") {
        return Some(NodeClass::PillarsEnv);
    }
    if name.contains("train_ramp") {
        return Some(NodeClass::Ramp);
    }
    if name.contains("bag_place") {
        return Some(NodeClass::Bag);
    }
    if env_type_has(n, "Station") {
        return Some(NodeClass::StationEnv);
    }
    if re_digit_after(name, "trains_") || re_digit_after(name, "train_sub_") || name.contains("train_intro") {
        return Some(NodeClass::Train);
    }
    if name.contains("Trigger_") {
        return Some(NodeClass::Trigger);
    }
    if env_type_has(n, "Tube") {
        return Some(NodeClass::TubeEnv);
    }
    None
}

/// `Ro.getEntityClass(node)`: match + static resource check.
fn entity_class(g: &mut Game, n: &Value) -> Option<NodeClass> {
    let name = data::name(n).to_string();
    let m = match g.match_map.get(&name) {
        Some(m) => *m,
        None => {
            let m = match_node(n);
            g.match_map.insert(name, m);
            m
        }
    };
    let lib = &g.lib;
    let ok = match m? {
        NodeClass::Coin => lib.has_group("currency_coin"),
        NodeClass::Gate => lib.has_group("gates_base"),
        NodeClass::Pickup => lib.has_group("powerups_superSneakers"),
        NodeClass::PillarsEnv => lib.has_group("pillars_start"),
        NodeClass::StationEnv => lib.has_group("station_start"),
        NodeClass::TubeEnv => lib.has_group("epic_start"),
        _ => true,
    };
    ok.then_some(m?)
}

/// `bg.mount(node, opts)`.
pub fn mount(g: &mut Game, ci: usize, node: &Value, opts: MountOpts) {
    if g.chunks[ci].name == "intro" && data::name(node).contains("lightSignal") {
        return;
    }
    let mut opts = opts;
    if data::comp(node, "Randomizer").is_some() && data::has_children(node) {
        let ch = data::children(node);
        let i = r_index(g.rng.as_mut(), sites::MOUNT_RANDOMIZER, ch.len());
        let child = ch[i].clone();
        mount(g, ci, &child, opts);
        return;
    }
    if let Some(ro) = data::comp(node, "RandomizeOffset") {
        let o = &ro["randomOffsets"];
        let mut arr = Vec::new();
        if data::truthy(o.get("left")) {
            arr.push(-20.0);
        }
        if data::truthy(o.get("mid")) {
            arr.push(0.0);
        }
        if data::truthy(o.get("right")) {
            arr.push(20.0);
        }
        if !arr.is_empty() {
            let i = r_index(g.rng.as_mut(), sites::MOUNT_OFFSET, arr.len());
            opts.offset_x = Some(arr[i]);
        }
    }
    if data::comp(node, "Mirror").is_some() {
        opts.flip = r_index(g.rng.as_mut(), sites::MOUNT_MIRROR, 2) == 1;
    }
    let cls = entity_class(g, node);
    if let Some(c) = cls {
        let ent = run_factory(g, ci, c, node, opts);
        if let Some(e) = ent {
            g.set_active(e, true);
            if data::comp(node, "Environment").is_some() {
                let (back, front) = {
                    let b = g.ent(e).body.as_ref().unwrap();
                    (b.back(), b.front())
                };
                g.set_slots_by_position(ci, back, front, true, true);
            }
        }
    }
    if cls.is_none() && !data::name(node).is_empty() {
        mount_static_geometry(g, ci, node, opts);
    }
    let ch = data::children(node).to_vec();
    for c in ch.iter().rev() {
        mount(g, ci, c, opts);
    }
}

/// Returns the entity for factories that return one (and pool-path classes).
fn run_factory(g: &mut Game, ci: usize, c: NodeClass, n: &Value, o: MountOpts) -> Option<EntityId> {
    match c {
        NodeClass::Blocker => {
            blocker_factory(g, ci, n, o);
            None
        }
        NodeClass::Coin => {
            coin_factory(g, ci, n, o);
            None
        }
        NodeClass::Gate => {
            gate_factory(g, ci, n);
            None
        }
        NodeClass::LightSignal => {
            let e = g.pool_get(Cls::LightSignal);
            light_signal_awake(g, ci, e, n, o);
            g.add_child(e);
            Some(e)
        }
        NodeClass::Pickup => {
            pickup_factory(g, ci, n, o);
            None
        }
        NodeClass::Ramp => {
            let e = g.pool_get(Cls::Ramp);
            ramp_awake(g, ci, e, n, o);
            g.add_child(e);
            Some(e)
        }
        NodeClass::Bag => {
            let e = g.pool_get(Cls::Bag);
            let (x, _, z) = data::pos(n);
            let cz = g.chunks[ci].z;
            let b = g.body(e);
            b.set_cx(x);
            b.set_bottom(0.0);
            b.set_cz(cz - z);
            g.add_child(e);
            Some(e)
        }
        NodeClass::Train => {
            train_factory(g, ci, n, o);
            None
        }
        NodeClass::TubeEnv => Some(crate::environment::tube_factory(g, ci, n)),
        NodeClass::Checkpoint => {
            let e = g.pool_get(Cls::Checkpoint);
            let (x, _, z) = data::pos(n);
            let cz = g.chunks[ci].z;
            let b = g.body(e);
            b.set_cz(cz - z);
            b.set_cx(x);
            b.set_bottom(0.0);
            if let Some(ox) = o.offset_x.filter(|&v| v != 0.0) {
                b.set_cx(ox);
            }
            if o.flip {
                b.set_cx(b.cx() * -1.0);
            }
            g.set_active(e, true);
            g.add_child(e);
            g.chunks[ci].checkpoints.push(e);
            Some(e)
        }
        NodeClass::Pillar => {
            let e = g.pool_get(Cls::Pillar);
            let (x, _, z) = data::pos(n);
            let cz = g.chunks[ci].z;
            let b = g.body(e);
            b.set_cz(cz - z);
            b.set_cx(x);
            b.set_bottom(0.0);
            if let Some(ox) = o.offset_x.filter(|&v| v != 0.0) {
                b.set_cx(ox);
            }
            if o.flip {
                b.set_cx(b.cx() * -1.0);
            }
            g.set_active(e, true);
            if let Some(m) = g.ent(e).model {
                let root = g.ent(e).root;
                g.scene.add_child(root, m);
            }
            g.add_child(e);
            None
        }
        NodeClass::PillarsEnv => {
            let e = crate::environment::pillars_factory(g, ci, n);
            g.add_child(e);
            Some(e)
        }
        // not ported yet: skipped (logged once per class) instead of
        // crashing live play on a chunk the traces never reached
        // Ui.factory (10024): R.pick(Ki, qi, Ji), pool.get(cls, {}), placed on the ground
        NodeClass::Obstacle => {
            let k = crate::game::r_index(g.rng.as_mut(), sites::OBSTACLE_PICK, 3) as u8;
            let e = g.pool_get_init(Cls::Obstacle(k));
            crate::entities::obstacle_init(g, e, k);
            let (x, _, z) = data::pos(n);
            let cz = g.chunks[ci].z;
            {
                let b = g.body(e);
                b.set_cz(cz - z);
                b.set_cx(x);
                b.set_bottom(0.0);
                if let Some(ox) = o.offset_x.filter(|&v| v != 0.0) {
                    b.set_cx(ox);
                }
                if o.flip {
                    b.set_cx(b.cx() * -1.0);
                }
            }
            crate::entities::obstacle_awake(g, e);
            g.set_active(e, true);
            g.add_child(e);
            None
        }
        NodeClass::StationEnv => {
            let e = crate::environment::station_factory(g, ci, n);
            g.add_child(e);
            Some(e)
        }
        // No.factory: at the node's x / z, bottom 0; `type` = the name's
        // last "_" part
        NodeClass::Trigger => {
            let e = g.pool_get(Cls::No);
            let (x, _, z) = data::pos(n);
            let cz = g.chunks[ci].z;
            {
                let b = g.body(e);
                b.set_cz(cz - z);
                b.set_cx(x);
                b.set_bottom(0.0);
                if let Some(ox) = o.offset_x.filter(|&v| v != 0.0) {
                    b.set_cx(ox);
                }
                if o.flip {
                    b.set_cx(b.cx() * -1.0);
                }
            }
            let t = data::name(n).rsplit('_').next().unwrap_or("").to_string();
            g.ent_mut(e).tutorial_type = t;
            g.set_active(e, true);
            g.add_child(e);
            None
        }
    }
}

/// `mountStaticGeometry` (48410).
fn mount_static_geometry(g: &mut Game, ci: usize, n: &Value, o: MountOpts) {
    let name = data::name(n);
    if ["intro", "trains", "train_start_place"].contains(&name) {
        return;
    }
    let strip = |s: &str| {
        let s = s.strip_suffix("_place").unwrap_or(s);
        s.strip_suffix("_group").unwrap_or(s).to_string()
    };
    let cands = [name.to_string(), name.to_lowercase(), strip(name), strip(name).to_lowercase()];
    let Some(group) = cands.iter().find(|c| g.lib.has_group(c)).cloned() else { return };
    let lib = g.lib.clone();
    let obj = g.scene.get_entity(&lib, &group, &MatOpts::default());
    let t = data::comp(n, "Transform");
    let (x, y, z) = data::pos(n);
    let cz = g.chunks[ci].z;
    let ob = g.scene.get_mut(obj);
    if t.is_some_and(|t| t["position"].is_object()) {
        ob.pos = DVec3::new(x, y, cz - z);
    }
    if let Some(r) = t.map(|t| &t["rotation"]).filter(|r| r.is_object()) {
        let d = |k: &str| r[k].as_f64().unwrap_or(0.0) * (std::f64::consts::PI / 180.0);
        ob.rot = DVec3::new(d("x"), d("y"), d("z"));
    }
    if o.flip {
        ob.pos.x = -ob.pos.x;
    }
    // game.addChild(entity): in the scene for good (not a level entity)
    let e = g.alloc_entity(crate::entities::Entity::new(Cls::StaticGeometry, obj));
    g.ent_mut(e).level_entity = false;
    g.ent_mut(e).model = Some(obj);
    g.add_child(e);
}

/// Log an unported scenery class once per run of the program.
pub fn skip_unported(class: &str, node: &str) {
    static SEEN: std::sync::Mutex<Option<std::collections::HashSet<String>>> = std::sync::Mutex::new(None);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen.get_or_insert_with(Default::default).insert(class.to_string()) {
        bevy::log::warn!("Skipping unported scenery node: {class} ({node})");
    }
}

/// `bg.mountIntro` (48319).
pub fn mount_intro(g: &mut Game, ci: usize) {
    let t = match g.intro_train {
        Some(t) => t,
        None => {
            let t = crate::entities::construct(g, Cls::IntroTrain);
            g.intro_train = Some(t);
            t
        }
    };
    let root = g.ent(t).root;
    g.scene.get_mut(root).pos = DVec3::new(-19.8, 0.0, 30.0);
    g.chunks[ci].blocks = 2.0;
    g.chunks[ci].length = 90.0;
    g.add_child(t);
    crate::environment::track_mount(g, ci);
    crate::environment::filler_mount(g, ci);
}

// ---- blockers (vr, 5915-6044) ----------------------------------------------------

fn blocker_factory(g: &mut Game, ci: usize, n: &Value, o: MountOpts) {
    // R.pick(...Object.keys(Sr)) = jump, roll, standar, standard
    let mut kind = [BlockerKind::Jump, BlockerKind::Roll, BlockerKind::Standard, BlockerKind::Standard]
        [r_index(g.rng.as_mut(), sites::BLOCKER_TYPE, 4)];
    let name = data::name(n);
    if name.contains("jump") {
        kind = BlockerKind::Jump;
    }
    if name.contains("roll") {
        kind = BlockerKind::Roll;
    }
    let b = g.pool_get(Cls::Blocker(kind));
    let d = g.pool_get(Cls::Rr);
    let (x, _, z) = data::pos(n);
    let cz = g.chunks[ci].z;
    {
        let bb = g.body(b);
        bb.set_front(cz - z);
        bb.set_cx(x);
        if let Some(ox) = o.offset_x.filter(|&v| v != 0.0) {
            bb.set_cx(ox);
        }
        if o.flip {
            bb.set_cx(bb.cx() * -1.0);
        }
    }
    // awake: build model once, ry PI, z -5; subclass height/bottom/model.y
    if g.ent(b).model.is_none() {
        let group = match kind {
            BlockerKind::Jump => "blocker_jump",
            BlockerKind::Roll => "blocker_roll",
            BlockerKind::Standard => "blocker_standard",
        };
        if g.lib.has_group(group) {
            let lib = g.lib.clone();
            let m = g.scene.get_entity(&lib, group, &MatOpts::default());
            let root = g.ent(b).root;
            g.scene.add_child(root, m);
            g.ent_mut(b).model = Some(m);
        }
    }
    let m = g.ent(b).model;
    if let Some(m) = m {
        let mo = g.scene.get_mut(m);
        mo.rot.y = PI;
        mo.pos.z = -5.0;
    }
    match kind {
        BlockerKind::Jump => g.body(b).set_sy(26.0),
        BlockerKind::Roll | BlockerKind::Standard => {
            let h = if kind == BlockerKind::Roll { 19.0 } else { 4.0 };
            let bb = g.body(b);
            bb.set_sy(h);
            bb.set_bottom(10.0);
            if let Some(m) = m {
                g.scene.get_mut(m).pos.y = -h / 2.0 - 10.0;
            }
        }
    }
    g.add_child(b);
    g.add_child(d);
    let c = g.ent(b).body.as_ref().unwrap().center();
    g.body(d).set_center(c);
    if name.contains("w_coins") {
        if kind == BlockerKind::Jump {
            spawn_curve(g, c.x, 0.0, c.z);
        } else {
            spawn_line(g, c.x, 0.0, c.z, 5);
        }
    }
}

// ---- coins (_r, 5708-5914) -------------------------------------------------------

pub(crate) fn coin_awake(g: &mut Game, c: EntityId) {
    let e = g.ent_mut(c);
    e.arc = 0;
    if let Some(m) = e.movable.as_mut() {
        *m = Default::default();
    }
    if let Some(b) = e.body.as_mut() {
        b.movable = false;
        b.reset_velocity();
    }
    g.set_active(c, true);
}

/// `_r.spawn(chunk, node, opts)`.
fn coin_spawn(g: &mut Game, ci: usize, n: &Value, o: MountOpts) -> EntityId {
    let c = g.pool_get(Cls::Coin);
    let (x, y, z) = data::pos(n);
    let cz = g.chunks[ci].z;
    {
        let b = g.body(c);
        b.set_cx(x);
        b.set_bottom(y + 5.0);
        b.set_cz(cz - z);
        if let Some(ox) = o.offset_x.filter(|&v| v != 0.0) {
            b.set_cx(ox);
        }
        if o.flip {
            b.set_cx(b.cx() * -1.0);
        }
        if b.bottom() > 150.0 {
            b.set_bottom(34.0);
        }
    }
    g.ent_mut(c).removable_on_crash = y > 1.0;
    coin_awake(g, c);
    g.add_child(c);
    c
}

fn coin_factory(g: &mut Game, ci: usize, n: &Value, o: MountOpts) {
    let name = data::name(n);
    if name.contains("Line") {
        let (_, y, _) = data::pos(n);
        if y > 50.0 {
            return;
        }
        let cc = data::comp(n, "CoinCurve").unwrap();
        let off = cc["_curveOffset"].as_f64().unwrap();
        let l = cc["_curveParent"]["_cachedCurve"]["MaxCoords"]["z"].as_f64().unwrap();
        let shift = l * off;
        let step = l / 5.0;
        for i in 0..5 {
            let c = coin_spawn(g, ci, n, o);
            { let b = g.body(c); b.set_cz(b.cz() - (i as f64 * step - shift)); }
        }
    } else if name.contains("Jump Curve") {
        let s = g.stats_speed() * 50.0;
        let cnt = (s / 14.0).floor();
        let off = data::comp(n, "CoinCurve").and_then(|c| c["_curveOffset"].as_f64()).unwrap_or(0.5);
        let dz = s / cnt;
        let da = PI / cnt;
        let shift = s * off;
        for i in 0..cnt as i64 {
            let c = coin_spawn(g, ci, n, o);
            let i = i as f64;
            let b = g.body(c);
            let bottom = b.bottom() + (da * i).sin() * 22.0;
            b.set_bottom(bottom);
            b.set_cz(b.cz() - (i * dz - shift));
            g.ent_mut(c).arc = i as u32 + 1;
        }
    } else {
        coin_spawn(g, ci, n, o);
    }
}

/// `_r.spawnLine(chunk, x, y, z, count)` (blocker coins).
fn spawn_line(g: &mut Game, x: f64, y: f64, z: f64, count: usize) {
    let off = count as f64 * 29.0 * 0.5 - 15.0;
    for i in 0..count {
        let c = g.pool_get(Cls::Coin);
        {
            let b = g.body(c);
            b.set_cz(z - i as f64 * 30.0 + off);
            b.set_bottom(y + 5.0);
            b.set_cx(x);
        }
        coin_awake(g, c);
        g.ent_mut(c).removable_on_crash = y > 1.0;
        g.add_child(c);
    }
}

/// `_r.spawnCurve(chunk, x, y, z)` (jump-blocker coins).
fn spawn_curve(g: &mut Game, x: f64, y: f64, z: f64) {
    let s = g.stats_speed() * 50.0;
    let n = (s / 13.0).floor();
    let dz = s / n;
    let c0 = (dz - 1.0) * n * 0.5 - dz * 0.5;
    let da = PI / (n - 1.0);
    for i in 0..n as i64 {
        let c = g.pool_get(Cls::Coin);
        let i = i as f64;
        {
            let b = g.body(c);
            b.set_cz(z - i * dz + c0);
            b.set_bottom(y + (da * i).sin() * 22.0 + 5.0);
            b.set_cx(x);
        }
        coin_awake(g, c);
        g.ent_mut(c).arc = i as u32 + 1;
        g.ent_mut(c).removable_on_crash = true;
        g.add_child(c);
    }
}

// ---- trains (So, 11868-12082) ----------------------------------------------------

fn train_factory(g: &mut Game, ci: usize, n: &Value, o: MountOpts) {
    let name = data::name(n).to_string();
    let lib_has = |g: &Game, s: &str| g.lib.has_group(s);
    let kind = if name.contains("intro") {
        TrainKind::Cargo
    } else if name.contains("sub") {
        TrainKind::Sub
    } else if name.contains("cargo") {
        if lib_has(g, "train_sub") { TrainKind::Cargo } else { TrainKind::Sub }
    } else if name.contains("standard") {
        if lib_has(g, "train_standard") { TrainKind::Standard } else { TrainKind::Sub }
    } else if lib_has(g, "train_standard") {
        [TrainKind::Standard, TrainKind::Cargo, TrainKind::Sub][r_index(g.rng.as_mut(), sites::TRAIN_TYPE, 3)]
    } else {
        TrainKind::Sub
    };
    let chunk_intro = g.chunks[ci].name.contains("intro");
    let count = digit_between_underscores(&name).unwrap_or(0);
    let coins = name.contains("coins");
    let span = count as f64 * 60.0;
    let mut target = 0.0;
    let mut max_back = f64::NEG_INFINITY;
    let mut speed = 0.0;
    let mut last: Option<EntityId> = None;
    let mut k = count;
    while k > 0 {
        k -= 1;
        let t = g.pool_get(Cls::Train(kind));
        last = Some(t);
        // drop lights from a previous life
        for l in std::mem::take(&mut g.ent_mut(t).lights) {
            g.scene.destroy(l);
        }
        if name.contains("intro") {
            g.ent_mut(t).model_index = if name.contains("intro_a") { 1 } else { 2 };
            if name.contains("intro_a") {
                continue;
            }
        } else {
            g.ent_mut(t).model_index = k as i64;
        }
        train_build(g, t, kind);
        speed = data::comp(n, "MovingTrainPlaceholder").and_then(|m| m["_speed"].as_f64()).unwrap_or(0.0);
        let (px, _, pz) = data::pos(n);
        let cz = g.chunks[ci].z;
        let v = cz - pz - k as f64 * 60.0;
        {
            let b = g.body(t);
            b.set_cx(o.offset_x.unwrap_or(px));
            b.set_bottom(0.0);
            if chunk_intro {
                b.set_cz(v);
            } else {
                b.set_back(v);
            }
        }
        let prop = {
            let b = g.ent(t).body.as_ref().unwrap();
            if chunk_intro { b.cz() } else { b.back() }
        };
        if target == 0.0 {
            target = prop + span * 0.6 + 30.0;
        }
        if o.flip {
            { let b = g.body(t); b.set_cx(b.cx() * -1.0); }
        }
        movable_run(g, t, speed, target);
        // lights on the moving locomotive (bali: 1 light, (0,-14,0))
        let light = match kind {
            TrainKind::Standard => "train_light_standard",
            TrainKind::Sub | TrainKind::Cargo => "train_light_freight",
        };
        if speed != 0.0 && g.lib.has_group(light) && k == 0 && kind != TrainKind::Cargo {
            let lib = g.lib.clone();
            let l = g.scene.get_entity(
                &lib,
                light,
                &MatOpts {
                    map: Some("environment-tex".into()),
                    opacity: Some(0.6),
                    blend_mode: Some(crate::scene::BlendMode::Add),
                    ..Default::default()
                },
            );
            let root = g.ent(t).root;
            g.scene.add_child(root, l);
            g.ent_mut(t).lights.push(l);
            {
                let lo = g.scene.get_mut(l);
                lo.scale = DVec3::ONE;
                lo.pos.z += 0.0;
                lo.pos.y += -14.0;
                lo.pos.x += 0.0;
                lo.rot.y = PI;
            }
            // material color + depthMask false on every part
            let parts: Vec<_> = if g.scene.get(l).children.is_empty() { vec![l] } else { g.scene.get(l).children.clone() };
            for p in parts {
                if let Some(m) = g.scene.material_mut(p) {
                    m.color = Some(0xFFF2D7);
                    m.depth_mask = false;
                }
            }
        }
        g.add_child(t);
        g.set_active(t, true);
        let back = g.ent(t).body.as_ref().unwrap().back();
        if back > max_back {
            max_back = back;
        }
    }
    if coins {
        if let Some(lt) = last {
            let cnt = (count as f64 * 1.1).ceil() as i64;
            let z0 = max_back - 50.0;
            let lx = g.ent(lt).body.as_ref().unwrap().cx();
            for j in 0..cnt {
                let c = coin_spawn(g, ci, n, o);
                {
                    let b = g.body(c);
                    b.set_cx(lx);
                    b.set_bottom(34.0);
                    b.set_cz(z0 - j as f64 * 30.0);
                }
                movable_run(g, c, speed, target);
            }
        }
    }
}

/// `/_(\d)_/` -> digit.
fn digit_between_underscores(name: &str) -> Option<usize> {
    let b = name.as_bytes();
    (0..b.len().saturating_sub(2)).find_map(|i| (b[i] == b'_' && b[i + 1].is_ascii_digit() && b[i + 2] == b'_').then(|| (b[i + 1] - b'0') as usize))
}

/// `So.build()`: locomotive or numbered wagon model, rebuilt every time.
fn train_build(g: &mut Game, t: EntityId, kind: TrainKind) {
    let (loco, wagon) = kind.models();
    let mi = g.ent(t).model_index;
    let mut name = loco.to_string();
    if mi > 0 {
        // bali hasWagons() -> wagon base; Ao counts base01, base02, ...
        let base = wagon;
        let mut cnt = 0;
        while g.lib.has_group(&format!("{base}{:02}", cnt + 1)) {
            cnt += 1;
        }
        name = if cnt == 0 { wagon.to_string() } else { format!("{base}{:02}", (mi % cnt) + 1) };
    }
    if let Some(m) = g.ent_mut(t).model.take() {
        g.scene.remove_from_parent(m);
    }
    if !g.lib.has_group(&name) {
        name = wagon.to_string();
    }
    if g.lib.has_group(&name) {
        let lib = g.lib.clone();
        let m = g.scene.get_entity(&lib, &name, &MatOpts::default());
        {
            let mo = g.scene.get_mut(m);
            mo.pos.y = -14.5;
            mo.rot.y = PI;
        }
        g.body(t).deco = false;
        let root = g.ent(t).root;
        g.scene.add_child(root, m);
        g.ent_mut(t).model = Some(m);
    }
}

fn movable_run(g: &mut Game, id: EntityId, speed: f64, target: f64) {
    let back = g.ent(id).body.as_ref().unwrap().back();
    if let Some(m) = g.ent_mut(id).movable.as_mut() {
        m.speed = speed;
        m.origin = back;
        m.target = target;
        m.last_dest = Some(back);
    }
    g.body(id).movable = speed > 0.0;
}

// ---- ramp (so/oo, 11648-11722) -----------------------------------------------------

fn ramp_awake(g: &mut Game, ci: usize, e: EntityId, n: &Value, o: MountOpts) {
    let (x, _, z) = data::pos(n);
    let cz = g.chunks[ci].z;
    {
        let b = g.body(e);
        b.set_cx(x);
        b.set_bottom(0.0);
        b.set_cz(cz - z + 6.0);
        if let Some(ox) = o.offset_x.filter(|&v| v != 0.0) {
            b.set_cx(ox);
        }
        if o.flip {
            b.set_cx(b.cx() * -1.0);
        }
    }
    let rb = g.ent(e).body.clone().unwrap();
    for side in [rb.left(), rb.right()] {
        let w = g.pool_get(Cls::RampWall);
        {
            let b = g.body(w);
            *b = crate::entities::Body::default(); // reset(): size/center zeroed, deco false
            b.set_size(DVec3::new(0.2, rb.sy(), rb.sz() * 0.7));
            b.set_center(DVec3::new(side, rb.cy(), rb.cz()));
        }
        g.add_child(w);
    }
    g.set_active(e, true);
}

// ---- light signal (Hi, 9907-9999) ---------------------------------------------------

fn light_signal_awake(g: &mut Game, ci: usize, e: EntityId, n: &Value, o: MountOpts) {
    if g.ent(e).model.is_none() && g.lib.has_group("lightSignal") {
        let lib = g.lib.clone();
        let m = g.scene.get_entity(&lib, "lightSignal", &MatOpts::default());
        {
            let mo = g.scene.get_mut(m);
            mo.rot.y = PI;
            mo.pos.y = -21.0;
        }
        let root = g.ent(e).root;
        g.scene.add_child(root, m);
        g.ent_mut(e).model = Some(m);
        if g.lib.has_group("lightSignal_light_red") {
            // materialNameByHash.lightSignal_light_green = "lightSignal_light"
            let l = light_green(g);
            g.scene.add_child(m, l);
            g.ent_mut(e).flicker = Some((0.0, l));
        }
    }
    let (x, _, z) = data::pos(n);
    let cz = g.chunks[ci].z;
    let b = g.body(e);
    b.set_cx(x);
    if o.flip {
        b.set_cx(b.cx() * -1.0);
    }
    b.set_bottom(0.0);
    b.set_cz(cz - z);
}

fn light_green(g: &mut Game) -> crate::scene::ObjId {
    let lib = g.lib.clone();
    let l = g.scene.get_entity(&lib, "lightSignal_light_green", &MatOpts { map: Some("effects-tex".into()), ..Default::default() });
    // the forced material name makes it an additive "light" part
    if let Visual::Mesh { material, .. } = &mut g.scene.get_mut(l).visual {
        material.pk_material = Some("lightSignal_light".into());
        material.blend_mode = crate::scene::BlendMode::Add;
        material.blend = true;
        material.opacity = Some(1.0);
        material.depth_mask = false;
        material.fog = crate::scene::Fog::Off;
        material.color = Some(0xFF0000);
    }
    l
}

// ---- gates (Ii, 9656-9951) ---------------------------------------------------------

fn gate_factory(g: &mut Game, ci: usize, n: &Value) {
    let kind = GateKind::from_node(data::name(n)).unwrap();
    let gate = g.pool_get(Cls::Gate(kind));
    gate_awake(g, ci, gate, kind, n);
    g.add_child(gate);
    let li = g.pool_get(Cls::Li);
    let gc = g.ent(gate).body.as_ref().unwrap().center();
    g.body(li).set_center(DVec3::new(gc.x, gc.y, gc.z + 30.0));
    g.add_child(li);
}

fn gate_awake(g: &mut Game, ci: usize, gate: EntityId, kind: GateKind, n: &Value) {
    let (_, _, pz) = data::pos(n);
    let cz = g.chunks[ci].z;
    {
        let b = g.body(gate);
        b.set_cx(0.0);
        b.set_bottom(0.0);
        b.set_cz(cz - pz - BLOCK_SIZE * 2.0);
    }
    if let Some(m) = g.ent(gate).model {
        {
            let mo = g.scene.get_mut(m);
            mo.rot.y = PI;
            mo.pos.y = -25.0;
            mo.pos.z = 120.0 * 1.45 + 6.0;
        }
        let root = g.ent(gate).root;
        g.scene.add_child(root, m);
    }
    let gb = g.ent(gate).body.clone().unwrap();
    let mut cols = Vec::new();
    // ceiling, L, M, R
    for i in 0..4 {
        let c = g.pool_get(Cls::Zi);
        {
            let b = g.body(c);
            match i {
                0 => {
                    b.set_size(DVec3::new(gb.sx(), 16.0, gb.sz() * 0.9));
                    b.set_cx(0.0);
                    b.set_bottom(37.0);
                }
                1 => {
                    b.set_size(DVec3::new(20.0, 37.0, gb.sz() * 0.9));
                    b.set_bottom(0.0);
                    b.set_right(-LANE_WIDTH * 1.5);
                }
                2 => {
                    b.set_size(DVec3::new(20.0, 37.0, gb.sz() * 0.9));
                    b.set_bottom(0.0);
                    b.set_cx(0.0);
                }
                _ => {
                    b.set_size(DVec3::new(20.0, 37.0, gb.sz() * 0.9));
                    b.set_bottom(0.0);
                    b.set_left(LANE_WIDTH * 1.5);
                }
            }
            b.set_cz(gb.cz());
        }
        g.add_child(c);
        cols.push(c);
    }
    // gates base (new every time)
    let ri = crate::entities::construct(g, Cls::Ri);
    {
        let r = g.ent(ri).root;
        let ro = g.scene.get_mut(r);
        ro.rot.y = PI;
        ro.pos.z = cz;
    }
    g.add_child(ri);
    g.set_slots_by_position(ci, cz, cz - BLOCK_SIZE * 4.0, true, false);
    let rr = g.ent(ri).root;
    theme::handle_chunk_config(g, ConfigSet::Special, rr, &["gates_base"], false, &[0.0]);
    crate::environment::track_spawn_gates(g, ci);
    if g.chunks[ci].blocks > 4.0 {
        crate::environment::track_spawn(g, ci, crate::environment::TrackSpawn { z: Some(cz - BLOCK_SIZE * 4.0), ..Default::default() });
        crate::environment::filler_spawn(
            g,
            ci,
            Some(cz - BLOCK_SIZE * 4.0),
            crate::entities::FillerKind::High01L,
            crate::entities::FillerKind::High01R,
        );
    }
    // subclass column edits
    let lw = LANE_WIDTH;
    let (cl, cm, cr) = (cols[1], cols[2], cols[3]);
    match kind {
        GateKind::Mid | GateKind::Left | GateKind::Right => {
            {
                let b = g.body(cm);
                b.set_sx(0.0);
                b.set_cx(999.0);
            }
            let (lr, rl) = match kind {
                GateKind::Mid => (-lw * 0.5, lw * 0.5),
                GateKind::Left => (-lw * 1.5, -lw * 0.5),
                _ => (lw * 0.5, lw * 1.5),
            };
            let b = g.body(cl);
            b.set_sx(60.0);
            b.set_right(lr);
            let b = g.body(cr);
            b.set_sx(60.0);
            b.set_left(rl);
        }
        GateKind::Sides => {
            let b = g.body(cm);
            b.set_sx(20.0);
            b.set_cx(0.0);
            let b = g.body(cl);
            b.set_sx(20.0);
            b.set_right(-lw * 1.5);
            let b = g.body(cr);
            b.set_sx(20.0);
            b.set_left(lw * 1.5);
        }
    }
    g.ent_mut(gate).gate_cols = cols;
}

// ---- pickups (Ha, 10959-11104) ------------------------------------------------------

/// Weighted list `$a`: key order of `Qa` with each key repeated by weight.
fn pickup_weighted() -> Vec<PickupKind> {
    let mut v = Vec::new();
    for (k, w) in [
        (PickupKind::Magnet, 29),
        (PickupKind::Multiplier, 29),
        (PickupKind::Sneakers, 26),
        (PickupKind::MysteryBox, 18),
        (PickupKind::Jetpack, 14),
        (PickupKind::Pogo, 14),
        (PickupKind::Key, 2),
    ] {
        v.extend(std::iter::repeat_n(k, w));
    }
    v
}

fn pickup_factory(g: &mut Game, ci: usize, n: &Value, o: MountOpts) {
    if g.chunks[ci].env_tube {
        return;
    }
    let (px, py, pz) = data::pos(n);
    let z = g.chunks[ci].z - pz;
    let sp = data::comp(n, "PickupSpawnPoint");
    let mode = sp.and_then(|s| s["__spawnPointMode"].as_str()).unwrap_or("").to_string();
    let force = sp.and_then(|s| s["__forceSpawnPickupType"].as_str()).unwrap_or("").to_string();
    let kind = if let Some(k) = next_timed_pickup(g) {
        k
    } else if mode == "WillForcePickupType" {
        g.pickup_initial_spawn = true;
        match force.as_str() {
            "Jetpack" => PickupKind::Jetpack,
            "PogoStick" => PickupKind::Pogo,
            "CoinMagnet" => PickupKind::Magnet,
            "SuperSneakers" => PickupKind::Sneakers,
            "CoinMultiplier" => PickupKind::Multiplier,
            other => panic!("unknown forced pickup {other}"),
        }
    } else {
        if !g.pickup_initial_spawn || !g.route_can_spawn("pickup", z) {
            return;
        }
        let w = pickup_weighted();
        w[r_index(g.rng.as_mut(), sites::PICKUP_TYPE, w.len())]
    };
    // config.forcePickup (11044): every spawned pickup becomes that type
    let kind = g.force_pickup.unwrap_or(kind);
    if (kind == PickupKind::Jetpack && !g.route_can_spawn("jetpack", z)) || (kind == PickupKind::MysteryBox && !g.route_can_spawn("mysteryBox", z)) {
        return;
    }
    let p = g.pool_get(Cls::Pickup(kind));
    {
        let b = g.body(p);
        b.set_cz(z);
        b.set_cx(o.offset_x.unwrap_or(px));
        b.set_cy(py);
        if o.flip {
            b.set_cx(b.cx() * -1.0);
        }
    }
    g.ent_mut(p).removable_on_crash = py > 2.0;
    g.set_active(p, true);
    pickup_awake(g, p, kind);
    g.add_child(p);
    g.route_set_spawn("pickup", z - 1800.0);
    if kind == PickupKind::Jetpack {
        g.route_set_spawn("jetpack", z - 2700.0);
    }
    if kind == PickupKind::MysteryBox {
        g.route_set_spawn("mysteryBox", z - 9999.0);
    }
}

/// `wg.getNextPickup()` (49092): timers sorted descending.
fn next_timed_pickup(g: &mut Game) -> Option<PickupKind> {
    g.timed_pickups.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    for t in g.timed_pickups.iter_mut() {
        if t.1 >= t.2 && g.letters_in_scene == 0 {
            t.1 = 0.0;
            return Some(match t.0 {
                "letter" => PickupKind::Letter,
                _ => PickupKind::MysteryBox,
            });
        }
    }
    None
}

pub(crate) fn pickup_awake(g: &mut Game, p: EntityId, kind: PickupKind) {
    let root = g.ent(p).root;
    if kind == PickupKind::Letter {
        // Ja.awake: (re)build the letter model when the hunt letter changes
        let Some(letter) = g.hunt_letter.clone() else { return };
        if g.ent(p).letter != letter && g.lib.has_group(&letter) {
            let model = g.ent(p).model.unwrap();
            if let Some(old) = g.ent_mut(p).letter_model.take() {
                g.scene.remove_from_parent(old);
            }
            let lib = g.lib.clone();
            let lm = g.scene.get_entity(&lib, &letter, &MatOpts { map: Some("props-tex".into()), ..Default::default() });
            {
                let o = g.scene.get_mut(lm);
                o.rot.y = PI;
                o.scale = DVec3::splat(1.5);
            }
            g.ent_mut(p).letter = letter;
            g.ent_mut(p).letter_model = Some(lm);
            g.scene.add_child(model, lm);
            g.scene.add_child(root, model);
        }
        return;
    }
    if let Some(m) = g.ent(p).model {
        g.scene.add_child(root, m);
    }
}

// ---- pogo power-up (Kf, 38041-38250) --------------------------------------------------

const POGO_SPAWN_TYPE: crate::rng::Site = crate::rng::Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at Ha.spawnRandomType (assets/index-QNpTjs8S.js:1:317792)");
const POGO_POWERUP_LANE: crate::rng::Site = crate::rng::Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at Kf.spawnPowerup (assets/index-QNpTjs8S.js:1:880454)");

/// Pogo jump curve `Gf` (time, value, inTangent, outTangent).
pub const POGO_CURVE: [(f64, f64, f64, f64); 3] =
    [(0.0, 0.0, 1.1480881, 1.1480881), (0.3173979, 0.6406888, 1.8195611, 1.8195611), (1.0, 1.0, 0.0, 0.0)];

/// Hermite curve evaluation `vf` (36336).
pub fn curve_eval(c: &[(f64, f64, f64, f64)], t: f64) -> f64 {
    if t <= c[0].0 {
        return c[0].1;
    }
    let last = c[c.len() - 1];
    if t >= last.0 {
        return last.1;
    }
    let mut i = 1;
    while c[i].0 < t {
        i += 1;
    }
    let (a, b) = (c[i - 1], c[i]);
    let dt = b.0 - a.0;
    let u = (t - a.0) / dt;
    let (u2, u3) = (u * u, u * u * u);
    (u3 * 2.0 - u2 * 3.0 + 1.0) * a.1 + (u3 - u2 * 2.0 + u) * dt * a.3 + (u3 * -2.0 + u2 * 3.0) * b.1 + (u3 - u2) * dt * b.2
}

/// `z.lerp(a, b, t)` (deobfuscated.js:2287): t is clamped to [0, 1].
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// `Kf.turnOn` -> `spawnCoins` (rows of 3 coins along the jump, then a power-up).
pub fn pogo_turn_on(g: &mut Game, hero: DVec3) {
    let (jump_height, jump_distance, rows) = (160.0, 300.0, 14);
    let (y0, z0) = (hero.y, hero.z);
    let (y1, z1) = (y0 + jump_height, z0 - jump_distance);
    let n = rows + 1;
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let y = lerp(y0, y1, curve_eval(&POGO_CURVE, t));
        let z = lerp(z0, z1, t);
        if i < n {
            let mut k = 3;
            while k > 0 {
                k -= 1;
                let c = g.pool_get_init(Cls::Coin);
                {
                    let b = g.body(c);
                    b.set_cx((k as f64 - 1.0) * LANE_WIDTH);
                    b.set_bottom(y);
                    b.set_cz(z);
                }
                coin_awake(g, c);
                g.add_child(c);
            }
        } else {
            // Ha.spawnRandomType(game, [sneakers, jetpack, magnet, multiplier])
            let kinds = [PickupKind::Sneakers, PickupKind::Jetpack, PickupKind::Magnet, PickupKind::Multiplier];
            let kind = kinds[r_index(g.rng.as_mut(), POGO_SPAWN_TYPE, 4)];
            let p = g.pool_get_init(Cls::Pickup(kind));
            pickup_awake(g, p, kind);
            g.add_child(p);
            let lane = [-1.0, 0.0, 1.0][r_index(g.rng.as_mut(), POGO_POWERUP_LANE, 3)];
            let b = g.body(p);
            b.set_cx(lane * LANE_WIDTH);
            b.set_bottom(y);
            b.set_cz(z);
        }
    }
}

// keep the PickupKind import used in signatures
#[allow(dead_code)]
fn _unused(_: PickupKind, _: &Node) {}
