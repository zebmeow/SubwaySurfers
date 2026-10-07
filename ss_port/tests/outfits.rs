//! The avatars' outfit meshes (`jp` with `Ep[id].outfitMeshes`): each
//! outfit shows exactly its own meshes, including those saved hidden in the
//! `.pk` (Brody's `brody_outfit1/2` are `visible: false` in the file).
use ss_port::install::Install;
use ss_port::render_actors::outfit_node_visible;
use ss_port::skin::SkinModel;

fn visible(id: &str, outfit: usize) -> Vec<String> {
    let path = Install::locate().site.join(format!("assets/characters-idle/avatar_{id}.pk"));
    let m = SkinModel::load(&path, 0.01).unwrap();
    m.meshes.iter().filter(|s| outfit_node_visible(&m, id, outfit, s.node)).map(|s| s.name.clone()).collect()
}

#[test]
fn brody_shows_one_outfit_each() {
    // the original's live hero scene (oracle, `refreshScenes` per outfit)
    let base = ["brody_head", "eye_shader", "brody_Mesh"];
    for (k, outfit) in ["brody_outfit_default", "brody_outfit1", "brody_outfit2"].iter().enumerate() {
        let mut want: Vec<String> = base.iter().chain([outfit]).map(|s| s.to_string()).collect();
        let mut got = visible("brody", k);
        want.sort();
        got.sort();
        assert_eq!(got, want, "outfit {k}");
    }
}

#[test]
fn table_overrides_and_fallbacks() {
    // Yutani's outfit 1 replaces the body mesh; Ninja's outfit 2 is the default
    assert!(!visible("yutani", 1).contains(&"yutani_Mesh".to_string()));
    assert_eq!(visible("ninja", 2), visible("ninja", 0));
    // an outfit past the table falls back to outfits[0]
    assert_eq!(visible("brody", 7), visible("brody", 0));
}
