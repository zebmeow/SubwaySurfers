//! Describe port entities in the oracle trace schema (`entity_spawn`), for
//! parity tests and debugging.

use crate::entities::EntityId;
use crate::game::Game;
use crate::math3;
use crate::scene::{BlendMode, Visual};
use bevy::math::{DQuat, DVec3};

#[derive(Clone, Debug)]
pub struct MeshDesc {
    /// `"<pk geometry key>"` with `#n` for primitive n, or `<procedural:Type>`.
    pub mesh: String,
    /// `"<scene>.pk"`.
    pub pk: Option<String>,
    pub visible: bool,
    pub pos: DVec3,
    pub rot: DQuat,
    pub scale: DVec3,
    /// Library map name (texture without folder/@2x/extension).
    pub texture: Option<String>,
    pub pk_material: Option<String>,
    pub culling: Option<bool>,
    /// "normal", 1 (add), 2 (multiply)
    pub blend_mode: Option<String>,
    pub depth_mask: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct EntityDesc {
    pub cls: &'static str,
    pub pos: DVec3,
    pub rot: DQuat,
    pub scale: DVec3,
    pub body: Option<(DVec3, DVec3)>,
    pub meshes: Vec<MeshDesc>,
}

/// Recorder's mesh name: `"S + 1"` -> `"S#1"`, `"track + "` -> `"track"`.
pub fn mesh_name(key: &str) -> String {
    let k = key.trim();
    if let Some(s) = k.strip_suffix(" +") {
        return s.to_string();
    }
    if let Some((a, b)) = k.rsplit_once(" + ") {
        if b.chars().all(|c| c.is_ascii_digit()) {
            return format!("{a}#{b}");
        }
    }
    k.to_string()
}

pub fn describe(g: &Game, id: EntityId) -> EntityDesc {
    let e = g.ent(id);
    // Rendered transform: body entities show the center copied in the last
    // render phase (Body.render), not where physics moved the body since.
    let root_m = g.scene.world_matrix(e.root);
    let (pos, rot, scale) = math3::decompose(&root_m);
    let mut meshes = Vec::new();
    for mid in g.scene.meshes(e.root) {
        // world = root (with body override) * path below root
        let mut local = bevy::math::DMat4::IDENTITY;
        let mut p = Some(mid);
        while let Some(pid) = p {
            if pid == e.root {
                break;
            }
            local = g.scene.local_matrix(pid) * local;
            p = g.scene.get(pid).parent;
        }
        let (mpos, mrot, mscale) = math3::decompose(&(root_m * local));
        let o = g.scene.get(mid);
        let d = match &o.visual {
            Visual::Mesh { key, scene, material, .. } => MeshDesc {
                mesh: mesh_name(key),
                pk: Some(format!("{scene}.pk")),
                visible: g.scene.effectively_visible(mid),
                pos: mpos,
                rot: mrot,
                scale: mscale,
                texture: material.map.clone(),
                pk_material: material.pk_material.clone(),
                culling: Some(material.culling),
                blend_mode: Some(match material.blend_mode {
                    BlendMode::Normal => "normal".into(),
                    BlendMode::Add => "1".into(),
                    BlendMode::Multiply => "2".into(),
                    BlendMode::Screen => "3".into(),
                }),
                depth_mask: Some(material.depth_mask),
            },
            Visual::Procedural { geometry, map, .. } => MeshDesc {
                mesh: format!("<procedural:{geometry}>"),
                pk: None,
                visible: g.scene.effectively_visible(mid),
                pos: mpos,
                rot: mrot,
                scale: mscale,
                texture: map.clone(),
                pk_material: None,
                culling: None,
                blend_mode: None,
                depth_mask: None,
            },
            Visual::None => unreachable!(),
        };
        meshes.push(d);
    }
    EntityDesc {
        cls: e.cls.name(),
        pos,
        rot,
        scale,
        body: e.body.as_ref().map(|b| (b.center(), b.size())),
        meshes,
    }
}
