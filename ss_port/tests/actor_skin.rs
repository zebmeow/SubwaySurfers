//! Character skinning against the running original.
//!
//! Fixtures (tests/data/actors, dumped from the oracle at f40 and f600, seed
//! 1, god mode, no inputs; docs/js_notes/actors_character_model.md §6): the
//! live bone locals of Jake, the guard and the dog, their world matrices,
//! and skinned vertex positions (three's `getVertexPosition` + matrixWorld).
//! The port rebuilds bone worlds and skinned vertices from the raw `.pk`
//! files plus those locals; animation sampling is tested separately.

use bevy::math::{DMat4, DQuat, DVec3};
use serde_json::Value;
use ss_port::skin::{SkinModel, Trs};
use std::path::Path;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}
fn fixture(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(root().join("ss_port/tests/data/actors").join(name)).unwrap()).unwrap()
}
fn v3(v: &Value) -> DVec3 {
    DVec3::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap())
}
fn trs(v: &Value) -> Trs {
    let q = &v["q"];
    Trs {
        t: v3(&v["p"]),
        r: DQuat::from_xyzw(q[0].as_f64().unwrap(), q[1].as_f64().unwrap(), q[2].as_f64().unwrap(), q[3].as_f64().unwrap()),
        s: v3(&v["s"]),
    }
}
fn mat(v: &Value) -> DMat4 {
    let a: Vec<f64> = v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    DMat4::from_cols_slice(&a)
}

/// Compare one actor: bone world translations and skinned vertices.
fn check(model: &SkinModel, root_m: DMat4, bones: &Value, verts: &[(&str, &Value)], what: &str) -> f64 {
    let mut pose = model.rest.clone();
    for b in bones.as_array().unwrap() {
        let k = model.joint_index(b["name"].as_str().unwrap()).unwrap_or_else(|| panic!("{what}: bone {}", b["name"]));
        pose[k] = trs(&b["local"]);
    }
    let worlds = model.bone_worlds(root_m, &pose);
    let mut worst: f64 = 0.0;
    for b in bones.as_array().unwrap() {
        let k = model.joint_index(b["name"].as_str().unwrap()).unwrap();
        let expect = mat(&b["world"]);
        let d = (worlds[k].w_axis.truncate() - expect.w_axis.truncate()).abs().max_element();
        worst = worst.max(d);
        assert!(d < 1e-4, "{what}: bone {} world {:?} vs {:?}", b["name"], worlds[k].w_axis, expect.w_axis);
    }
    for (mesh, data) in verts {
        let mi = model.meshes.iter().position(|m| m.name == *mesh).unwrap_or_else(|| panic!("{what}: mesh {mesh}"));
        let infl: Vec<f32> = match data["morph"].as_array() {
            Some(m) => m.iter().map(|x| x.as_f64().unwrap() as f32).collect(),
            None => model.meshes[mi].default_influences.clone(),
        };
        let skinned = model.skin(mi, &worlds, &infl);
        for v in data["list"].as_array().unwrap() {
            let i = v["i"].as_u64().unwrap() as usize;
            let p = skinned[i];
            let d = (DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64) - v3(&v["world"])).abs().max_element();
            worst = worst.max(d);
            assert!(d < 1e-4, "{what}: {mesh} vertex {i} {p:?} vs {}", v["world"]);
        }
    }
    worst
}

#[test]
fn skinning_matches_oracle() {
    let site = root().join("oracle/site/assets");
    let jake = SkinModel::load(&site.join("characters-idle/avatar_jake.pk"), 0.01).unwrap();
    let guard = SkinModel::load(&site.join("game-basic/model-guard.pk"), 1.0).unwrap();
    let dog = SkinModel::load(&site.join("game-basic/model-dog.pk"), 1.0).unwrap();
    assert_eq!(jake.joints.len(), 26);
    for f in [40, 600] {
        let val = fixture(&format!("val_{f}.json"));
        // hero entity -> model (y -4.5, ry PI) -> anim container (x100) -> scene root
        let h = &val["hero"];
        let hero_root = DMat4::from_translation(v3(&h["pos"])) * trs(&h["model"]).matrix() * DMat4::from_scale(DVec3::splat(100.0));
        let hv: Vec<(&str, &Value)> = ["jake_head", "eye_shader", "jake_Mesh"].iter().map(|m| (*m, &h["verts"][*m])).collect();
        let wj = check(&jake, hero_root, &h["bones"], &hv, &format!("jake f{f}"));
        // guard entity -> model = container (y -5.6, ry PI, x100)
        let g = &val["guard"];
        let model = DMat4::from_translation(DVec3::new(0.0, -5.6, 0.0)) * DMat4::from_rotation_y(std::f64::consts::PI) * DMat4::from_scale(DVec3::splat(100.0));
        let guard_root = DMat4::from_translation(v3(&g["pos"])) * model;
        let wg = check(&guard, guard_root, &g["bones"], &[("guard", &g["verts"]["guard"])], &format!("guard f{f}"));
        // dog: child of the guard at (5, 0, -6)
        let d = fixture(&format!("dog_{f}.json"));
        let dog_root = mat(&d["chain"][0]["matrixWorld"]) * trs(&d["model"]).matrix();
        let wd = check(&dog, dog_root, &d["bones"], &[("dog", &g["verts"]["dog"])], &format!("dog f{f}"));
        println!("f{f}: worst |error| jake {wj:.2e}, guard {wg:.2e}, dog {wd:.2e}");
    }
}

/// End to end: the port's own simulation (seed 1, god mode, no input, as
/// the fixtures were dumped) animates the bones; locals and skinned
/// vertices must match the oracle at f40 (intro) and f600 (running).
#[test]
fn animated_pose_matches_oracle() {
    use ss_port::sim_plugin::{DataPaths, KeyQueue, Sim};
    let mut sim = Sim::new(&DataPaths::from_repo(root()), 1, Some("S".into()), Box::new(KeyQueue::default())).unwrap();
    let assets = sim.game.actors.clone().unwrap();
    for f in [40, 600] {
        while sim.next_frame <= f {
            sim.step();
        }
        let g = &sim.game;
        let val = fixture(&format!("val_{f}.json"));
        let dogv = fixture(&format!("dog_{f}.json"));
        let mut worst: (f64, String) = (0.0, String::new());
        let mut cmp = |what: &str, model: &SkinModel, pose: &[Trs], bones: &Value| {
            for b in bones.as_array().unwrap() {
                let k = model.joint_index(b["name"].as_str().unwrap()).unwrap();
                let e = trs(&b["local"]);
                let p = pose[k];
                // per component (q and -q are the same rotation; animated
                // quaternions are not exactly unit, as in three)
                let comp = |a: DQuat, b: DQuat| (a.x - b.x).abs().max((a.y - b.y).abs()).max((a.z - b.z).abs()).max((a.w - b.w).abs());
                let dq = comp(p.r, e.r).min(comp(p.r, -e.r));
                let (dt, ds) = ((p.t - e.t).abs().max_element(), (p.s - e.s).abs().max_element());
                let d = dt.max(ds).max(dq);
                if d > worst.0 {
                    worst = (d, format!("{what} {} (pos {dt:.1e}, rot {dq:.1e}, scale {ds:.1e}; |pos| {:.2})", b["name"], e.t.length()));
                }
                assert!(d < 1e-6, "{what} f{f} bone {}: {p:?} vs {e:?}", b["name"]);
            }
        };
        let (hb, _) = g.hero_anim.as_ref().unwrap().pose(&assets.jake);
        cmp("jake", &assets.jake, &hb, &val["hero"]["bones"]);
        let (gb, _) = g.guard_anim.as_ref().unwrap().pose(&assets.guard);
        cmp("guard", &assets.guard, &gb, &val["guard"]["bones"]);
        let (db, _) = g.dog_anim.as_ref().unwrap().pose(&assets.dog);
        cmp("dog", &assets.dog, &db, &dogv["bones"]);
        println!("f{f}: worst bone-local |error| {:.2e} at {} (hero clip {})", worst.0, worst.1, g.hero_anim.as_ref().unwrap().name);
    }
}
