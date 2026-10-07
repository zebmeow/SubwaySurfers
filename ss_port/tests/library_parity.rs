//! The port's library tables must equal the game's (oracle/dumps/library.json,
//! written by `node oracle/dump_library.mjs`).

use ss_port::library::Library;
use std::collections::HashMap;

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn library_tables_match_game() {
    let lib = Library::load(&root().join("oracle/site")).unwrap();
    let dump: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root().join("oracle/dumps/library.json")).unwrap()).unwrap();

    let groups: HashMap<String, Vec<String>> = serde_json::from_value(dump["geometryGroups"].clone()).unwrap();
    assert_eq!(lib.geometry_groups.len(), groups.len(), "group count");
    for (g, parts) in &groups {
        assert_eq!(lib.geometry_groups.get(g), Some(parts), "group {g}");
    }
    let scenes: HashMap<String, String> = serde_json::from_value(dump["geometrySceneNames"].clone()).unwrap();
    assert_eq!(lib.geometry_scene_names, scenes);
    let mats: HashMap<String, String> = serde_json::from_value(dump["materialNameByHash"].clone()).unwrap();
    assert_eq!(lib.material_name_by_hash, mats);

    // Every part decodes to a non-empty primitive.
    for parts in lib.geometry_groups.values() {
        for h in parts {
            let p = lib.primitive(h).unwrap_or_else(|| panic!("primitive {h}"));
            assert!(!p.positions.is_empty(), "{h} has no positions");
        }
    }
}
