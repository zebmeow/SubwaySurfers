//! Asset library: port of class `bb` (deobfuscated.js:66794-67160).
//!
//! `refresh()` walks the resource cache in load order and builds
//! `geometryGroups` (group name -> geometry hashes, i.e. mesh parts),
//! `geometryByHash`, `geometrySceneNames` and `materialNameByHash`.
//! Name collisions resolve by load order, so the order matters; [`LOAD_ORDER`]
//! is the order the oracle's cache ended up in (oracle/dumps/library.json).

use crate::pk::{self, PkFile};
use std::collections::HashMap;

/// `.pk` load order observed in the shipped game (progressive loading).
pub const LOAD_ORDER: &[&str] = &[
    "assets/boards/board-hoverboard.pk",
    "assets/boards/board-superhero.pk",
    "bundles/bali/game-idle/environment-idle.pk",
    "assets/boards/board-hotrod.pk",
    "assets/game-basic/model-guard.pk",
    "assets/game-basic/model-dog.pk",
    "assets/boards/board-greatwhite.pk",
    "assets/boards/board-scoot.pk",
    "assets/boards/board-monster.pk",
    "assets/boards/board-daredevil.pk",
    "assets/boards/board-teleporter.pk",
    "assets/boards/board-windglider.pk",
    "assets/game-idle/props-start.pk",
    "assets/animations-basic/pogostick.pk",
    "assets/animations-basic/catch.pk",
    "bundles/bali/game-basic/trains-basic.pk",
    "assets/boards/board-sunset.pk",
    "assets/characters-idle/avatar_jake.pk",
    "bundles/bali/game-idle/trains-idle.pk",
    "assets/animations-basic/hoverboards.pk",
    "assets/animations-idle/idle.pk",
    "bundles/bali/game-basic/environment-basic.pk",
    "assets/animations-basic/enemy-guard-movement.pk",
    "assets/boards/board-bouncer.pk",
    "assets/boards/board-bigkahuna.pk",
    "assets/boards/board-starboard.pk",
    "assets/animations-basic/jetpack.pk",
    "assets/animations-basic/enemy-guard-catch.pk",
    "assets/game-basic/character-props.pk",
    "assets/animations-basic/movement.pk",
    "assets/game-idle/mystery-box.pk",
    "assets/boards/board-lumberjack.pk",
    "assets/boards/board-skullfire.pk",
    "assets/animations-basic/enemy-dog-movement.pk",
    "assets/animations-basic/enemy-dog-catch.pk",
    "assets/boards/board-lowrider.pk",
    "bundles/bali/game-full/environment-full.pk",
    "assets/game-full/jetpack-flame.pk",
    "assets/game-full/props-extra.pk",
    "assets/game-full/props.pk",
    "assets/game-full/sheep.pk",
    "bundles/bali/game-full/trains-full.pk",
    "assets/characters-idle/avatar_tricky.pk",
    "assets/characters-idle/avatar_yutani.pk",
    "assets/characters-idle/avatar_lucy.pk",
    "assets/characters-idle/avatar_ninja.pk",
    "assets/characters-idle/avatar_tasha.pk",
    "assets/characters-idle/avatar_king.pk",
    "assets/characters-idle/avatar_brody.pk",
    "assets/characters-idle/avatar_tagbot.pk",
];

/// First package of the game-full bundle (loaded after the game starts).
pub const FULL_BUNDLE_START: &str = "bundles/bali/game-full/environment-full.pk";

/// Texture per scene (`yb`, deobfuscated.js:66780).
pub fn scene_map(scene: &str) -> Option<&'static str> {
    Some(match scene {
        "environment-idle" | "environment-basic" | "environment-full" | "trains-idle" | "trains-basic"
        | "trains-full" => "environment-tex",
        "model-dog" | "model-guard" => "enemies",
        "props" | "props-start" => "props-tex",
        "model-tricky-idlexmas" => "tricky-tex",
        "board_default_base" => "board-hoverboard-tex",
        _ => return None,
    })
}

/// `xb`: "a/b/name.ext" -> "name"
pub fn scene_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.split('.').next().unwrap_or(file)
}

#[derive(Default)]
pub struct Library {
    /// scene name -> package
    pub scenes: HashMap<String, PkFile>,
    /// scene name -> source path (relative to oracle/site)
    pub scene_paths: HashMap<String, String>,
    /// group -> geometry hashes, in insertion order (mesh parts)
    pub geometry_groups: HashMap<String, Vec<String>>,
    /// geometry hash -> (scene, key in that scene's geometryHash)
    pub geometry_by_hash: HashMap<String, (String, String)>,
    /// group -> scene of its first geometry
    pub geometry_scene_names: HashMap<String, String>,
    /// geometry name (hash before " +") -> material name, lowercased
    pub material_name_by_hash: HashMap<String, String>,
}

impl Library {
    /// Load every package in [`LOAD_ORDER`] from `site_root` (oracle/site).
    pub fn load(site_root: &std::path::Path) -> Result<Self, String> {
        Self::load_until(site_root, None)
    }

    /// Load packages in [`LOAD_ORDER`] up to (not including) `stop`. The game
    /// mounts the intro chunk before the full bundle has loaded:
    /// `load_until(root, Some(FULL_BUNDLE_START))` reproduces that library.
    pub fn load_until(site_root: &std::path::Path, stop: Option<&str>) -> Result<Self, String> {
        let mut lib = Library::default();
        for path in LOAD_ORDER {
            if Some(*path) == stop {
                break;
            }
            let bytes = std::fs::read(site_root.join(path)).map_err(|e| format!("{path}: {e}"))?;
            let file = pk::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
            lib.add(path, file);
        }
        Ok(lib)
    }

    /// One iteration of `refresh()` for a newly cached .pk (deobfuscated.js:66834-66901).
    pub fn add(&mut self, path: &str, file: PkFile) {
        let scene = scene_name(path).to_string();
        if self.scenes.contains_key(&scene) {
            return;
        }
        for name in file.material_names() {
            if let Some((hash, mat)) = name.split_once("___") {
                // `[a, b] = name.split("___")`: b ends at the next "___" if any.
                let mat = mat.split("___").next().unwrap_or(mat);
                self.material_name_by_hash.insert(hash.to_string(), mat.to_lowercase());
            }
        }
        for key in file.geometry_keys() {
            let mut group = key.split(" +").next().unwrap_or("").to_string();
            group = group.split(".00").next().unwrap_or("").to_string();
            let mut hash = key.clone();
            match group.as_str() {
                "tube_start" => {
                    group = "tube".into();
                    hash = format!("{key} + ");
                }
                "tube_mid" => {
                    group = "tube".into();
                    hash = format!("{key} + 1");
                }
                "tube_end" => {
                    group = "tube".into();
                    hash = format!("{key} + 2");
                }
                _ => {}
            }
            group = group.replacen("_LOD0", "", 1).replacen("_LOD1", "_low", 1);
            if group.to_lowercase().contains("train_") {
                group = group.to_lowercase();
                if group == "train_standart_wagon1" {
                    group = "train_standart_wagon01".into();
                } else if group == "train_standart_wagon2" {
                    group = "train_standart_wagon02".into();
                }
            }
            if path.contains("environment") && ["train_sub", "train_cargo", "train_standard"].contains(&group.as_str()) {
                continue;
            }
            self.geometry_by_hash.insert(hash.clone(), (scene.clone(), key.clone()));
            let parts = self.geometry_groups.entry(group.clone()).or_default();
            if !parts.contains(&hash) {
                parts.push(hash);
            }
            self.geometry_scene_names.entry(group).or_insert_with(|| scene.clone());
        }
        self.scene_paths.insert(scene.clone(), path.to_string());
        self.scenes.insert(scene, file);
    }

    pub fn has_group(&self, group: &str) -> bool {
        self.geometry_groups.contains_key(group)
    }

    /// Geometry name used for material lookup: hash up to " +" (getEntityFromGeometry).
    pub fn material_name(&self, hash: &str) -> Option<&str> {
        self.material_name_by_hash.get(hash.split(" +").next().unwrap_or("")).map(String::as_str)
    }

    pub fn primitive(&self, hash: &str) -> Option<pk::Primitive> {
        let (scene, key) = self.geometry_by_hash.get(hash)?;
        self.scenes.get(scene)?.primitive(key)
    }
}
