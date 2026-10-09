//! Skinned character models from `.pk` files (avatar_jake, model-guard,
//! model-dog) and linear-blend skinning, as the original builds and draws
//! them (docs/js_notes/actors_character_model.md):
//!
//! * nodes -> objects with TRS from `decompose(transform)` (`lt`, RB:2676);
//! * one skeleton per skin, bones built from the joint nodes
//!   (`buildSkeleton`, RB:2500) with the root bones attached under the first
//!   skinned mesh (`ct`, RB:2655);
//! * three's `SkinnedMesh` in "attached" mode with an identity bind matrix:
//!   `world = Σ w · boneWorld · inverseBind · (scale · (pos + Σ infl · delta))`.
//!
//! Skinning runs on the CPU: positions come out in world space and are drawn
//! with the world's material, so the characters get the same clip-space
//! bend as everything else, and the face's morphs and UV rewrites stay
//! simple. Checked against the oracle's skinned vertices (tests/actor_skin.rs).

use crate::pk::{self, Buffer, PkFile};
use bevy::math::{DMat4, DQuat, DVec3};
use serde_json::Value;

/// A local transform (three `position`, `quaternion`, `scale`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trs {
    pub t: DVec3,
    pub r: DQuat,
    pub s: DVec3,
}

impl Default for Trs {
    fn default() -> Self {
        Self { t: DVec3::ZERO, r: DQuat::IDENTITY, s: DVec3::ONE }
    }
}

impl Trs {
    pub fn matrix(&self) -> DMat4 {
        DMat4::from_scale_rotation_translation(self.s, self.r, self.t)
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub local: Trs,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub visible: bool,
    pub geometry: Option<usize>,
    pub skin: Option<usize>,
}

/// One skinned primitive (a three `SkinnedMesh`).
#[derive(Clone, Debug)]
pub struct SkinMesh {
    /// Node name (primitive 0) or material name (`eye_shader`).
    pub name: String,
    /// Owning node.
    pub node: usize,
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// Joint slots (indices into the skin's joint list).
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// Relative morph position deltas per target.
    pub morphs: Vec<Vec<[f32; 3]>>,
    /// `geometry.weights`: initial morph influences.
    pub default_influences: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct SkinModel {
    pub nodes: Vec<Node>,
    /// Joint slot -> node.
    pub joints: Vec<usize>,
    /// Joint slot -> parent joint slot.
    pub joint_parent: Vec<Option<usize>>,
    pub inverse_bind: Vec<DMat4>,
    /// Bone rest pose (from the joint nodes).
    pub rest: Vec<Trs>,
    /// The node under whose (first) skinned mesh the root bones hang.
    pub attach_node: usize,
    pub meshes: Vec<SkinMesh>,
    /// Unskinned meshes (props, e.g. `Meshes/Props/Jake_sandwich`), in
    /// their node's space (no joints).
    pub rigid: Vec<SkinMesh>,
}

fn mat(b: Option<&Buffer>) -> Option<DMat4> {
    let v = b?.f32()?;
    if v.len() < 16 {
        return None;
    }
    let mut a = [0f64; 16];
    for (i, x) in v.iter().take(16).enumerate() {
        a[i] = *x as f64;
    }
    // `elements[15] === 0 && (elements[15] = 1)`
    if a[15] == 0.0 {
        a[15] = 1.0;
    }
    Some(DMat4::from_cols_array(&a))
}

/// three `Matrix4.decompose` (negative determinant flips the x scale).
fn decompose(m: &DMat4) -> Trs {
    let (s, r, t) = m.to_scale_rotation_translation();
    Trs { t, r, s }
}

fn f32s(file: &PkFile, v: &Value) -> Vec<f32> {
    v.as_u64().and_then(|i| file.buffers.get(i as usize)).and_then(Buffer::f32).map(<[f32]>::to_vec).unwrap_or_default()
}

fn u16x4(file: &PkFile, v: &Value) -> Vec<[u16; 4]> {
    let Some(b) = v.as_u64().and_then(|i| file.buffers.get(i as usize)) else { return Vec::new() };
    let raw: Vec<u16> = match b {
        Buffer::U16(x) => x.clone(),
        Buffer::U32(x) => x.iter().map(|&y| y as u16).collect(),
        Buffer::F32(x) => x.iter().map(|&y| y as u16).collect(),
    };
    raw.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
}

/// The node tree of any `.pk` scene (`$.library.getScene`), for rigid
/// models such as the boards: names, local transforms, geometry.
pub fn scene_nodes(path: &std::path::Path) -> Result<Vec<Node>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file = pk::parse(&bytes)?;
    Ok(parse_nodes(&file))
}

/// World matrix of node `n` relative to the scene root.
pub fn scene_node_world(nodes: &[Node], n: usize) -> DMat4 {
    let mut m = nodes[n].local.matrix();
    let mut p = nodes[n].parent;
    while let Some(i) = p {
        m = nodes[i].local.matrix() * m;
        p = nodes[i].parent;
    }
    m
}

fn parse_nodes(file: &PkFile) -> Vec<Node> {
    let m = &file.meta;
    let buf = |v: &Value| v.as_u64().and_then(|i| file.buffers.get(i as usize));
    let raw_nodes = m["nodes"].as_array().cloned().unwrap_or_default();
    let mut nodes: Vec<Node> = raw_nodes
        .iter()
        .map(|n| {
            let mut local = mat(buf(&n["transform"])).map(|x| decompose(&x)).unwrap_or_default();
            for c in [&mut local.s.x, &mut local.s.y, &mut local.s.z] {
                if *c == 0.0 {
                    *c = 1.0;
                }
            }
            Node {
                name: n["name"].as_str().unwrap_or("").to_string(),
                local,
                children: n["children"].as_array().into_iter().flatten().filter_map(Value::as_u64).map(|c| c as usize).collect(),
                parent: None,
                visible: n["visible"].as_bool() != Some(false),
                geometry: n["geometry"].as_u64().map(|g| g as usize),
                skin: n["skin"].as_u64().map(|s| s as usize),
            }
        })
        .collect();
    for i in 0..nodes.len() {
        for c in nodes[i].children.clone() {
            if c < nodes.len() && c != i {
                nodes[c].parent = Some(i);
            }
        }
    }
    nodes
}

impl SkinModel {
    /// Load a skinned `.pk`. `scale` multiplies positions and morph deltas
    /// (0.01 for the shared avatars, `Uo`; 1 for the guard and dog).
    pub fn load(path: &std::path::Path, scale: f32) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file = pk::parse(&bytes)?;
        let m = &file.meta;
        let buf = |v: &Value| v.as_u64().and_then(|i| file.buffers.get(i as usize));
        let raw_nodes = m["nodes"].as_array().ok_or("pk: no nodes")?;
        let mut nodes: Vec<Node> = raw_nodes
            .iter()
            .map(|n| {
                let mut local = mat(buf(&n["transform"])).map(|x| decompose(&x)).unwrap_or_default();
                // `lt`: a zero scale component becomes 1
                for c in [&mut local.s.x, &mut local.s.y, &mut local.s.z] {
                    if *c == 0.0 {
                        *c = 1.0;
                    }
                }
                Node {
                    name: n["name"].as_str().unwrap_or("").to_string(),
                    local,
                    children: n["children"].as_array().into_iter().flatten().filter_map(Value::as_u64).map(|c| c as usize).collect(),
                    parent: None,
                    visible: n["visible"].as_bool() != Some(false),
                    geometry: n["geometry"].as_u64().map(|g| g as usize),
                    skin: n["skin"].as_u64().map(|s| s as usize),
                }
            })
            .collect();
        for i in 0..nodes.len() {
            for c in nodes[i].children.clone() {
                if c < nodes.len() && c != i {
                    nodes[c].parent = Some(i);
                }
            }
        }
        // skeleton (skins[0]): bone rest TRS keeps the scale only if no component is 0
        let joints: Vec<usize> =
            m["skins"][0]["joints"].as_array().ok_or("pk: no skin")?.iter().filter_map(Value::as_u64).map(|j| j as usize).collect();
        let mut rest = Vec::new();
        let mut inverse_bind = Vec::new();
        for &j in &joints {
            let n = &raw_nodes[j];
            let mut trs = Trs::default();
            if let Some(x) = mat(buf(&n["transform"])) {
                let d = decompose(&x);
                trs.t = d.t;
                trs.r = d.r;
                if d.s.x != 0.0 && d.s.y != 0.0 && d.s.z != 0.0 {
                    trs.s = d.s;
                }
            }
            rest.push(trs);
            inverse_bind.push(mat(buf(&n["inverseBindMatrix"])).unwrap_or(DMat4::IDENTITY));
        }
        let joint_parent: Vec<Option<usize>> =
            joints.iter().map(|&j| joints.iter().position(|&p| raw_nodes[p]["children"].as_array().into_iter().flatten().any(|c| c.as_u64() == Some(j as u64)))).collect();
        // meshes, in node order (the first skinned one gets the root bones)
        let geoms = m["geometry"].as_array().ok_or("pk: no geometry")?;
        let materials = m["materials"].as_array();
        let mut meshes = Vec::new();
        let mut rigid = Vec::new();
        let mut attach_node = None;
        for (ni, n) in nodes.iter().enumerate() {
            let Some(g) = n.geometry else { continue };
            let skinned = n.skin.is_some();
            let geo = &geoms[g];
            let default_influences: Vec<f32> = geo["weights"].as_array().into_iter().flatten().filter_map(Value::as_f64).map(|x| x as f32).collect();
            for (pi, p) in geo["primitives"].as_array().into_iter().flatten().enumerate() {
                let a = &p["attributes"];
                let pos = f32s(&file, &a["positions"]);
                let uv = f32s(&file, &a["uvs"]);
                let w = f32s(&file, &a["weights"]);
                let nv = pos.len() / 3;
                let name = if pi == 0 {
                    n.name.clone()
                } else {
                    let mi = p["material"].as_u64().unwrap_or(pi as u64) as usize;
                    materials.and_then(|ms| ms.get(mi)).and_then(|mm| mm["name"].as_str()).unwrap_or("").to_string()
                };
                let list = if skinned { &mut meshes } else { &mut rigid };
                list.push(SkinMesh {
                    name,
                    node: ni,
                    positions: pos.chunks_exact(3).map(|c| [c[0] * scale, c[1] * scale, c[2] * scale]).collect(),
                    uvs: if uv.len() == nv * 2 { uv.chunks_exact(2).map(|c| [c[0], c[1]]).collect() } else { vec![[0.0; 2]; nv] },
                    indices: buf(&p["indices"]).map(Buffer::indices).unwrap_or_else(|| (0..nv as u32).collect()),
                    joints: u16x4(&file, &a["boneIndices"]),
                    weights: w.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect(),
                    morphs: p["targets"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|t| f32s(&file, &t["positions"]).chunks_exact(3).map(|c| [c[0] * scale, c[1] * scale, c[2] * scale]).collect())
                        .collect(),
                    default_influences: default_influences.clone(),
                });
                if skinned {
                    attach_node.get_or_insert(ni);
                }
            }
        }
        Ok(Self { nodes, joints, joint_parent, inverse_bind, rest, attach_node: attach_node.ok_or("pk: no skinned mesh")?, meshes, rigid })
    }

    /// `lp` (38650): re-bind `mesh` of `other` to this skeleton by bone name
    /// (bones this skeleton lacks map to slot 0 with their weight kept).
    pub fn rebind(&self, other: &SkinModel, mesh: &SkinMesh) -> SkinMesh {
        let map: Vec<u16> = other.joints.iter().map(|&j| self.joint_index(&other.nodes[j].name).unwrap_or(0) as u16).collect();
        let mut m = mesh.clone();
        for j in &mut m.joints {
            for k in j.iter_mut() {
                *k = map.get(*k as usize).copied().unwrap_or(0);
            }
        }
        m
    }

    pub fn joint_index(&self, name: &str) -> Option<usize> {
        self.joints.iter().position(|&j| self.nodes[j].name == name)
    }

    /// World matrix of node `n` under `root` (the scene root's world).
    pub fn node_world(&self, root: DMat4, n: usize) -> DMat4 {
        let locals: Vec<Trs> = self.nodes.iter().map(|n| n.local).collect();
        self.node_world_posed(root, n, &locals)
    }

    /// The same with animated node locals (clips also drive `Root` etc.).
    pub fn node_world_posed(&self, root: DMat4, n: usize, locals: &[Trs]) -> DMat4 {
        let mut m = locals[n].matrix();
        let mut p = self.nodes[n].parent;
        while let Some(i) = p {
            m = locals[i].matrix() * m;
            p = self.nodes[i].parent;
        }
        root * m
    }

    pub fn node_index(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.name == name)
    }

    /// Effective visibility of node `n` (itself and its ancestors), with
    /// extra hidden node names (outfits, props).
    pub fn node_visible(&self, n: usize, hidden: &[&str]) -> bool {
        let mut p = Some(n);
        while let Some(i) = p {
            if !self.nodes[i].visible || hidden.contains(&self.nodes[i].name.as_str()) {
                return false;
            }
            p = self.nodes[i].parent;
        }
        true
    }

    /// Bone world matrices for a pose (bone locals per joint slot).
    pub fn bone_worlds(&self, root: DMat4, pose: &[Trs]) -> Vec<DMat4> {
        let locals: Vec<Trs> = self.nodes.iter().map(|n| n.local).collect();
        self.bone_worlds_posed(root, &locals, pose)
    }

    /// Bone worlds with animated node locals.
    pub fn bone_worlds_posed(&self, root: DMat4, nodes: &[Trs], pose: &[Trs]) -> Vec<DMat4> {
        let attach = self.node_world_posed(root, self.attach_node, nodes);
        let mut out: Vec<Option<DMat4>> = vec![None; self.joints.len()];
        fn get(m: &SkinModel, attach: &DMat4, pose: &[Trs], out: &mut Vec<Option<DMat4>>, k: usize) -> DMat4 {
            if let Some(w) = out[k] {
                return w;
            }
            let parent = match m.joint_parent[k] {
                Some(p) => get(m, attach, pose, out, p),
                None => *attach,
            };
            let w = parent * pose[k].matrix();
            out[k] = Some(w);
            w
        }
        (0..self.joints.len()).map(|k| get(self, &attach, pose, &mut out, k)).collect()
    }

    /// Skinned world positions of mesh `mi` (`skinning_vertex` with
    /// relative morphs, base influence 1).
    pub fn skin(&self, mi: usize, bones: &[DMat4], influences: &[f32]) -> Vec<[f32; 3]> {
        self.skin_mesh(&self.meshes[mi], bones, influences)
    }

    /// Skin a mesh whose joint slots index this model's skeleton (e.g. a
    /// prop mesh re-bound with [`SkinModel::rebind`]).
    pub fn skin_mesh(&self, m: &SkinMesh, bones: &[DMat4], influences: &[f32]) -> Vec<[f32; 3]> {
        let skin_mats: Vec<DMat4> = bones.iter().zip(&self.inverse_bind).map(|(b, i)| *b * *i).collect();
        let mut out = Vec::with_capacity(m.positions.len());
        for (v, p) in m.positions.iter().enumerate() {
            let mut q = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
            for (t, d) in m.morphs.iter().enumerate() {
                let w = influences.get(t).copied().unwrap_or(0.0) as f64;
                if w != 0.0 {
                    q += DVec3::new(d[v][0] as f64, d[v][1] as f64, d[v][2] as f64) * w;
                }
            }
            let (j, w) = (m.joints[v], m.weights[v]);
            let mut r = DVec3::ZERO;
            for k in 0..4 {
                if w[k] != 0.0 {
                    r += skin_mats[j[k] as usize].transform_point3(q) * w[k] as f64;
                }
            }
            out.push([r.x as f32, r.y as f32, r.z as f32]);
        }
        out
    }
}
