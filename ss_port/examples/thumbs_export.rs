//! Export meshes for the UI thumbnails (rendered by tools/thumbs.py): the 16
//! boards (their base `features` placed by the board scene's node
//! transforms).
//!   cargo run --release --example thumbs_export -- OUT_DIR
use serde_json::json;

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("out dir"));
    std::fs::create_dir_all(&out).unwrap();
    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let site = root.join("oracle/site");
    let lib = ss_port::library::Library::load(&site).unwrap();
    for b in &ss_port::shop::Catalog::get().boards {
        let nodes = ss_port::skin::scene_nodes(&site.join(format!("assets/boards/board-{}.pk", b.id))).unwrap();
        let mut parts = Vec::new();
        for (i, n) in nodes.iter().enumerate() {
            if !b.features.contains(&n.name) {
                continue;
            }
            let m = ss_port::skin::scene_node_world(&nodes, i);
            for hash in lib.geometry_groups.get(&n.name).cloned().unwrap_or_default() {
                let Some(p) = lib.primitive(&hash) else { continue };
                let pos: Vec<[f64; 3]> = p.positions.iter().map(|v| m.transform_point3(bevy::math::DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)).to_array()).collect();
                parts.push(json!({"positions": pos, "uvs": p.uvs, "indices": p.indices}));
            }
        }
        let tex = site.join(format!("assets/boards/board-{}-tex.webp", b.id));
        std::fs::write(out.join(format!("board-{}.json", b.id)), json!({"texture": tex, "parts": parts}).to_string()).unwrap();
    }
    println!("exported to {}", out.display());
}
