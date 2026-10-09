//! The characters' own idles in the menus (`ss_port::char_idle`): the
//! original's `idle-<id>.pk` clips on the avatar's own skeleton, the
//! director's gesture / breathe alternation, and the props.

use bevy::math::{DMat4, DVec3};
use ss_port::char_idle::{props, IdlePlayer};
use ss_port::skin::SkinModel;
use std::path::Path;

fn site() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../oracle/site"))
}

fn avatar(id: &str) -> SkinModel {
    SkinModel::load(&site().join(format!("assets/characters-idle/avatar_{id}.pk")), 0.01).unwrap()
}

#[test]
fn every_characters_idle_loads() {
    for (id, c) in ss_port::char_idle::table() {
        let path = site().join(format!("assets/characters-idle/avatar_{id}.pk"));
        if !path.exists() {
            continue;
        }
        let model = avatar(id);
        let p = IdlePlayer::new(site(), &model, id, 1).unwrap_or_else(|e| panic!("{id}: {e}"));
        // it starts with a gesture when the character has any
        let flavors: Vec<&str> = c.idle.as_ref().unwrap().flavors.iter().map(|f| f.name.as_str()).collect();
        assert!(flavors.is_empty() || flavors.contains(&p.clip()), "{id}: {}", p.clip());
    }
}

#[test]
fn gestures_alternate_with_breathing() {
    let model = avatar("jake");
    let mut p = IdlePlayer::new(site(), &model, "jake", 3).unwrap();
    let mut seen = vec![p.clip().to_string()];
    for _ in 0..60 * 120 {
        p.advance(1.0 / 60.0);
        if seen.last().map(String::as_str) != Some(p.clip()) {
            seen.push(p.clip().to_string());
        }
    }
    assert!(seen.len() > 6, "{seen:?}");
    for w in seen.windows(2) {
        assert!((w[0] == "breathe") != (w[1] == "breathe"), "a gesture, then breathe, then a gesture: {seen:?}");
    }
    assert!(seen.iter().filter(|c| *c != "breathe").collect::<std::collections::HashSet<_>>().len() >= 2, "random gestures: {seen:?}");
}

#[test]
fn jakes_sandwich_is_in_his_hand() {
    // eating_sandwich: the sandwich (a prop on attachPoint3) stays by the
    // right hand throughout
    let model = avatar("jake");
    assert_eq!(props("jake")[0].attach, "attachPoint3");
    let sandwich = model.rigid.iter().find(|r| r.name == "Jake_sandwich").expect("the prop mesh");
    let mut p = IdlePlayer::new(site(), &model, "jake", 7).unwrap();
    let mut checked = 0;
    for _ in 0..20_000 {
        p.advance(0.02);
        if p.clip() != "eating_sandwich" || p.time < 0.2 {
            continue;
        }
        let (b, n) = p.pose(&model);
        let bones = model.bone_worlds_posed(DMat4::IDENTITY, &n, &b);
        let hand = bones[model.joint_index("R_Hand_jnt").unwrap()].w_axis.truncate();
        let at = model.node_world_posed(DMat4::IDENTITY, model.node_index("attachPoint3").unwrap(), &n);
        let c = sandwich.positions.iter().map(|v| at.transform_point3(DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64))).sum::<DVec3>() / sandwich.positions.len() as f64;
        assert!(c.distance(hand) < 0.012, "t {:.2}: sandwich {c:?}, hand {hand:?}", p.time);
        checked += 1;
        if checked > 100 {
            break;
        }
    }
    assert!(checked > 100);
}
