//! Debug: Jake's joint hierarchy and rest world positions (scene space).
use bevy::math::DMat4;
fn main() {
    let site = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../oracle/site/assets"));
    let m = ss_port::skin::SkinModel::load(&site.join("characters-idle/avatar_jake.pk"), 0.01).unwrap();
    let w = m.bone_worlds(DMat4::IDENTITY, &m.rest);
    for (k, &j) in m.joints.iter().enumerate() {
        let p = m.joint_parent[k].map(|p| m.nodes[m.joints[p]].name.clone()).unwrap_or("-".into());
        let t = w[k].w_axis;
        println!("{:<16} parent {:<16} world ({:.3}, {:.3}, {:.3})", m.nodes[j].name, p, t.x, t.y, t.z);
    }
}
