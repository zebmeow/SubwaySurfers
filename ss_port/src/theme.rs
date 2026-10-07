//! City themes. A theme is one `data/theme_<id>.json`: its atmosphere and
//! texture paths (measured in the original, docs/rendering.md) and the config
//! trees for the theme config engine `Yr` (deobfuscated.js:8582-8835):
//! `jr` fillers, `Mr` special, `Nr` tunnel, extracted verbatim from the bundle.
//!
//! This build of the game ships exactly one theme, bali (the only bundle
//! the original downloads; see README). [`ThemeRegistry`] discovers whatever
//! theme files exist, so another city only needs its file and its bundle.
//!
//! Applied at most once per object (`Jr` map), on first construction, so
//! pool reuse decides which instances draw randomness.

use crate::game::{sites, Game};
use crate::math3;
use crate::rng::Site;
use crate::scene::{MatOpts, ObjId, Visual};
use bevy::math::DMat4;
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub enum ConfigSet {
    Filler,
    Special,
    Tunnel,
}

#[derive(Debug, Clone)]
pub enum Script {
    RandomChild { p: f64 },
    Other,
}

#[derive(Debug, Clone)]
pub struct Def {
    pub name: String,
    pub mesh_name: Option<String>,
    pub p: f64,
    pub matrix: Option<[f64; 12]>,
    pub scripts: Vec<Script>,
    pub children: Vec<Def>,
}

pub struct ThemeConfig {
    pub id: String,
    pub display_name: String,
    /// Clear color (`renderer.setClearColor`), sRGB bytes.
    pub sky_color: [u8; 3],
    /// `uFogColor` (linear).
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    /// Texture files under the site root.
    pub env_texture: String,
    pub train_start_texture: String,
    pub train_texture: String,
    pub props_texture: String,
    pub filler: Vec<Def>,
    pub special: Vec<Def>,
    pub tunnel: Vec<Def>,
}

/// Themes available on disk (`data/theme_<id>.json`) and the active one.
#[derive(bevy::prelude::Resource, Clone, Debug)]
pub struct ThemeRegistry {
    pub active: String,
    pub available: Vec<String>,
}

impl ThemeRegistry {
    /// Scan `dir` for `theme_<id>.json`; `bali` is the default.
    pub fn discover(dir: &std::path::Path) -> Self {
        let mut available: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n.strip_prefix("theme_").and_then(|r| r.strip_suffix(".json")).map(str::to_string)
            })
            .collect();
        available.sort();
        Self { active: "bali".into(), available }
    }
    /// Select a theme, or the error the CLI prints.
    pub fn select(&mut self, id: &str) -> Result<(), String> {
        if self.available.iter().any(|a| a == id) {
            self.active = id.to_string();
            Ok(())
        } else {
            Err(format!("Unknown theme '{id}'. Available: [{}]", self.available.join(", ")))
        }
    }
    /// The theme after the active one (wrapping).
    pub fn next(&self) -> String {
        let i = self.available.iter().position(|a| *a == self.active).unwrap_or(0);
        self.available[(i + 1) % self.available.len()].clone()
    }
}

fn parse_def(v: &Value) -> Def {
    let matrix = v["matrix"].as_array().filter(|m| m.len() >= 12).map(|m| {
        let mut a = [0.0; 12];
        for (i, x) in m.iter().take(12).enumerate() {
            a[i] = x.as_f64().unwrap();
        }
        a
    });
    Def {
        name: v["name"].as_str().unwrap_or("").to_string(),
        mesh_name: v["meshName"].as_str().filter(|s| !s.is_empty()).map(str::to_string),
        p: v["activationProbability"].as_f64().unwrap_or(0.0),
        matrix,
        scripts: v["scripts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| match s["name"].as_str() {
                Some("RandomChildRandomizer") => Script::RandomChild { p: s["activationProbability"].as_f64().unwrap_or(0.0) },
                _ => Script::Other,
            })
            .collect(),
        children: v["children"].as_array().into_iter().flatten().map(parse_def).collect(),
    }
}

impl ThemeConfig {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let objs = |k: &str| v[k]["objects"].as_array().into_iter().flatten().map(parse_def).collect();
        let a = &v["atmosphere"];
        let hex = a["skyColor"].as_str().ok_or("theme: atmosphere.skyColor")?;
        let rgb = u32::from_str_radix(hex.trim_start_matches('#'), 16).map_err(|e| e.to_string())?;
        let f = |k: &str| a[k].as_f64().ok_or(format!("theme: atmosphere.{k}")).map(|x| x as f32);
        let fc = &a["fogColorLinear"];
        let t = |k: &str| v["textures"][k].as_str().map(str::to_string).ok_or(format!("theme: textures.{k}"));
        Ok(Self {
            id: v["id"].as_str().ok_or("theme: id")?.to_string(),
            display_name: v["displayName"].as_str().unwrap_or("").to_string(),
            sky_color: [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8],
            fog_color: [0, 1, 2].map(|i| fc[i].as_f64().unwrap_or(0.0) as f32),
            fog_near: f("fogNear")?,
            fog_far: f("fogFar")?,
            env_texture: t("environment")?,
            train_start_texture: t("trainStart")?,
            train_texture: t("trains")?,
            props_texture: t("props")?,
            filler: objs("fillerConfig"),
            special: objs("specialConfig"),
            tunnel: objs("tunnelConfig"),
        })
    }
    fn set(&self, s: ConfigSet) -> &[Def] {
        match s {
            ConfigSet::Filler => &self.filler,
            ConfigSet::Special => &self.special,
            ConfigSet::Tunnel => &self.tunnel,
        }
    }
}

/// `meshName.replace("_LOD0","").replace("_LOD1","_low")` (addMeshFromDef).
fn norm_mesh(m: &str) -> String {
    m.replacen("_LOD0", "", 1).replacen("_LOD1", "_low", 1)
}

/// `Yr.handleChunkConfig(config, target, keys, flip, offsets)`.
pub fn handle_chunk_config(g: &mut Game, set: ConfigSet, target: ObjId, keys: &[&str], flip: bool, offsets: &[f64]) {
    if !g.configured.insert(target) {
        return;
    }
    let theme = g.theme.clone();
    for (i, key) in keys.iter().enumerate() {
        let off = *offsets.get(i).unwrap_or(&offsets[0]);
        let key = key.to_lowercase();
        let Some(def) = theme.set(set).iter().find(|d| d.name.to_lowercase().contains(&key)) else { continue };
        match &def.mesh_name {
            Some(mesh) => {
                let t = g.scene.get(target);
                let mesh = norm_mesh(mesh);
                if Some(&mesh) != t.name.as_ref() && Some(&mesh) != t.group_name.as_ref() {
                    add_mesh_from_def(g, def, target, flip, off, sites::CFG_MESH_TOP);
                } else {
                    add_children_from_def(g, def, target, flip, off);
                }
            }
            None => add_container_from_def(g, def, target, flip, off, sites::CFG_CONTAINER_TOP),
        }
    }
}

fn add_children_from_def(g: &mut Game, def: &Def, parent: ObjId, flip: bool, off: f64) {
    for c in &def.children {
        if c.mesh_name.is_some() {
            add_mesh_from_def(g, c, parent, flip, off, sites::CFG_MESH_CHILD);
        } else {
            add_container_from_def(g, c, parent, flip, off, sites::CFG_CONTAINER_CHILD);
        }
    }
    handle_scripts(g, def, parent);
}

fn add_container_from_def(g: &mut Game, def: &Def, parent: ObjId, flip: bool, off: f64, site: Site) {
    if g.rng.random(site) > def.p {
        return;
    }
    let c = g.scene.new_obj();
    g.scene.get_mut(c).unity_name = Some(def.name.clone());
    qr(g, def.matrix.as_ref(), c, flip, off);
    g.scene.add_child(parent, c);
    add_children_from_def(g, def, c, false, 0.0);
}

fn add_mesh_from_def(g: &mut Game, def: &Def, parent: ObjId, flip: bool, off: f64, site: Site) {
    if g.rng.random(site) > def.p {
        return;
    }
    let mesh = norm_mesh(def.mesh_name.as_deref().unwrap());
    if !g.lib.has_group(&mesh) {
        add_children_from_def(g, def, parent, true, off);
        return;
    }
    let lib = g.lib.clone();
    let m = g.scene.get_entity(&lib, &mesh, &MatOpts::default());
    g.scene.get_mut(m).unity_name = Some(def.name.clone());
    qr(g, def.matrix.as_ref(), m, flip, off);
    g.scene.add_child(parent, m);
    add_children_from_def(g, def, m, false, 0.0);
}

/// `RandomChildRandomizer`: keep one child (if activated), destroy the rest.
fn handle_scripts(g: &mut Game, def: &Def, obj: ObjId) {
    for s in &def.scripts {
        if let Script::RandomChild { p } = s {
            let act = g.rng.random(sites::CFG_RCR_ACTIVATE) <= *p;
            let n = g.scene.get(obj).children.len();
            let idx = (g.rng.random(sites::CFG_RCR_INDEX) * n as f64).floor() as usize;
            let children = g.scene.get(obj).children.clone();
            let mut dead = Vec::new();
            for (i, c) in children.iter().enumerate() {
                let keep = act && idx == i;
                g.scene.set_active(*c, keep);
                if !keep {
                    dead.push(*c);
                }
            }
            for c in dead {
                g.scene.destroy(c);
            }
        }
    }
}

/// `Qr(matrix, obj, flip, zOffset)` (8815): Unity 3x4 matrix -> three
/// transform, mirrored in x.
fn qr(g: &mut Game, m: Option<&[f64; 12]>, obj: ObjId, flip: bool, off: f64) {
    let mut x = match m {
        Some(m) => DMat4::from_cols_array(&[
            m[0], m[1], m[2], 0.0, m[3], m[4], m[5], 0.0, m[6], m[7], m[8], 0.0, m[9], m[10], m[11], 1.0,
        ]),
        None => DMat4::IDENTITY,
    };
    if flip {
        let zr = math3::compose(Default::default(), bevy::math::DVec3::new(0.0, std::f64::consts::PI, 0.0), bevy::math::DVec3::ONE);
        x = zr * x;
    }
    let (p, q, s) = math3::decompose(&x);
    let mut e = math3::euler_from_quat(q);
    let o = g.scene.get_mut(obj);
    o.pos = p;
    o.scale = s;
    o.pos.x = -o.pos.x;
    e.y = -e.y;
    e.z = -e.z;
    o.rot = e;
    o.pos.z += off;
    if o.scale.x < 0.0 && matches!(o.visual, Visual::Mesh { .. }) {
        if let Some(mat) = g.scene.material_mut(obj) {
            mat.cw = true;
        }
    }
}
