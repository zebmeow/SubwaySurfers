//! Debug: a skinned avatar's meshes (node, vertices, bounds, UV bounds).
//!   cargo run --release --example skindump -- <avatar.pk>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let m = ss_port::skin::SkinModel::load(std::path::Path::new(&a[1]), 0.01).unwrap();
    for (i, s) in m.meshes.iter().enumerate() {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &s.positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let (mut ulo, mut uhi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for u in &s.uvs {
            for k in 0..2 {
                ulo[k] = ulo[k].min(u[k]);
                uhi[k] = uhi[k].max(u[k]);
            }
        }
        let bones = m.bone_worlds(bevy::math::DMat4::IDENTITY, &m.rest);
        let sk = m.skin(i, &bones, &s.default_influences);
        let (mut slo, mut shi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &sk {
            for k in 0..3 {
                slo[k] = slo[k].min(p[k]);
                shi[k] = shi[k].max(p[k]);
            }
        }
        let wsum: Vec<f32> = s.weights.iter().map(|w| w.iter().sum()).collect();
        let maxj = s.joints.iter().flat_map(|j| j.iter()).max();
        println!("  skinned {slo:.3?}..{shi:.3?} wsum {:.2}..{:.2} maxj {maxj:?} joints {} weights {}", wsum.iter().cloned().fold(f32::MAX, f32::min), wsum.iter().cloned().fold(f32::MIN, f32::max), s.joints.len(), s.weights.len());
        println!("mesh {i} {:?} node {} ({}) verts {} tris {} morphs {} y {:.3}..{:.3} uv {:?}..{:?}", s.name, s.node, m.nodes[s.node].name, s.positions.len(), s.indices.len() / 3, s.morphs.len(), lo[1], hi[1], ulo, uhi);
    }
}
