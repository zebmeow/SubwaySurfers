//! Scene graph: the engine's `Entity` (three.js Object3D subclass) tree.
//!
//! Every engine object (entity roots, model containers, mesh parts, theme
//! config containers) is an [`Obj`] in an arena. Child order is kept exactly
//! as three.js would (`add` appends, re-adding moves to the end) because it
//! decides traversal order and which child `RandomChildRandomizer` keeps.

use crate::library::{self, Library};
use crate::math3;
use bevy::math::{DMat4, DVec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjId(pub usize);

/// Engine render-state blend mode (ResourceBridge `q`). `Normal` is the
/// state's default and serializes as "normal" in traces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Add,
    Multiply,
    /// `q.SCREEN` (3): CustomBlending, src * ONE + dst * (1 - src color).
    Screen,
}

/// Distance fog of a `$n` material (`customFog`, `whiteFog`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fog {
    Off,
    /// Toward the global fog color `Gn.group.uFogColor`.
    On,
    /// Toward white (multiply shadows fade out).
    White,
}

/// Material class: the library's `$n`, or a Bali theme shader
/// (`handleThemeMaterial`, deobfuscated.js:8366).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shader {
    Basic,
    /// `Ir` displaced water (tube_mid caustics).
    Water,
    /// `Vr` scrolling foam (tube_end foam, epic_start water).
    Foam,
}

/// Material + render state of one mesh part (getEntityFromGeometry, deobfuscated.js:67013).
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    /// Library map name (e.g. "environment-tex"), `None` for color materials.
    pub map: Option<String>,
    pub opacity: Option<f64>,
    pub color: Option<u32>,
    pub blend: bool,
    pub blend_mode: BlendMode,
    pub depth_mask: bool,
    pub culling: bool,
    /// `state.clockwiseFrontFace` (set by theme config for mirrored meshes).
    pub cw: bool,
    /// Material name from the .pk (`materialNameByHash`), lowercased.
    pub pk_material: Option<String>,
    pub fog: Fog,
    /// `fogMultiplier` (scales the fog far distance).
    pub fog_multiplier: f64,
    pub shader: Shader,
}

#[derive(Clone, Debug)]
pub enum Visual {
    None,
    /// A library geometry part.
    Mesh { hash: String, scene: String, key: String, material: Material },
    /// Geometry built in code (planes, particle systems): type name + map.
    /// `plane`: `H.plane(w, h, opacity, map, ADD)` (rendered); others are
    /// not drawn yet (particles).
    Procedural { geometry: &'static str, map: Option<String>, double_sided: bool, plane: Option<[f32; 3]> },
}

#[derive(Clone, Debug)]
pub struct Obj {
    pub pos: DVec3,
    /// Euler XYZ (three `rotation`).
    pub rot: DVec3,
    pub scale: DVec3,
    pub visible: bool,
    pub active: bool,
    pub parent: Option<ObjId>,
    pub children: Vec<ObjId>,
    pub visual: Visual,
    /// `geomHash` (set by mountEntity on each part).
    pub geom_hash: Option<String>,
    /// `groupName` (set by getEntity).
    pub group_name: Option<String>,
    /// `name` (entity names such as filler group names, "gates_right").
    pub name: Option<String>,
    /// `unityName` (theme config defs).
    pub unity_name: Option<String>,
    pub destroyed: bool,
    /// Bali `Wr` bob (handleThemeMaterial "Boat1"): origin, direction,
    /// frequency; `pos = origin + dir * sin(t * frequency)`.
    pub bob: Option<(DVec3, DVec3, f64)>,
}

impl Default for Obj {
    fn default() -> Self {
        Self {
            pos: DVec3::ZERO,
            rot: DVec3::ZERO,
            scale: DVec3::ONE,
            visible: true,
            active: true,
            parent: None,
            children: Vec::new(),
            visual: Visual::None,
            geom_hash: None,
            group_name: None,
            name: None,
            unity_name: None,
            destroyed: false,
            bob: None,
        }
    }
}

/// Material options passed to `getEntity(name, opts)`.
#[derive(Clone, Debug, Default)]
pub struct MatOpts {
    pub map: Option<String>,
    pub opacity: Option<f64>,
    pub blend_mode: Option<BlendMode>,
    pub depth_mask: Option<bool>,
}

#[derive(Default)]
pub struct Scene {
    pub objs: Vec<Obj>,
}

impl Scene {
    pub fn new_obj(&mut self) -> ObjId {
        self.objs.push(Obj::default());
        ObjId(self.objs.len() - 1)
    }
    pub fn get(&self, id: ObjId) -> &Obj {
        &self.objs[id.0]
    }
    pub fn get_mut(&mut self, id: ObjId) -> &mut Obj {
        &mut self.objs[id.0]
    }

    /// `Object3D.add`: detach from the old parent, append.
    pub fn add_child(&mut self, parent: ObjId, child: ObjId) {
        self.remove_from_parent(child);
        self.objs[child.0].parent = Some(parent);
        self.objs[parent.0].children.push(child);
    }
    pub fn remove_from_parent(&mut self, child: ObjId) {
        if let Some(p) = self.objs[child.0].parent.take() {
            self.objs[p.0].children.retain(|&c| c != child);
        }
    }
    /// Engine `destroy()`: removed from its parent (RB:2155).
    pub fn destroy(&mut self, id: ObjId) {
        self.objs[id.0].destroyed = true;
        self.remove_from_parent(id);
    }
    /// Engine `active` setter: also sets `visible` (RB:2110).
    pub fn set_active(&mut self, id: ObjId, active: bool) {
        let o = &mut self.objs[id.0];
        if o.active != active {
            o.active = active;
            o.visible = active;
        }
    }

    pub fn local_matrix(&self, id: ObjId) -> DMat4 {
        let o = &self.objs[id.0];
        math3::compose(o.pos, o.rot, o.scale)
    }
    pub fn world_matrix(&self, id: ObjId) -> DMat4 {
        let mut m = self.local_matrix(id);
        let mut p = self.objs[id.0].parent;
        while let Some(pid) = p {
            m = self.local_matrix(pid) * m;
            p = self.objs[pid.0].parent;
        }
        m
    }
    /// Visible the way the recorder judges it: every ancestor visible and no
    /// ancestor scaled to ~0 (the engine hides with `scale.set(0.000001)`).
    pub fn effectively_visible(&self, id: ObjId) -> bool {
        let mut p = Some(id);
        while let Some(pid) = p {
            let o = &self.objs[pid.0];
            if !o.visible || (o.scale.x * o.scale.y * o.scale.z).abs() < 1e-9 {
                return false;
            }
            p = o.parent;
        }
        true
    }
    /// Pre-order traversal (three `traverse`) collecting objects with a visual.
    pub fn meshes(&self, root: ObjId) -> Vec<ObjId> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !matches!(self.objs[id.0].visual, Visual::None) {
                out.push(id);
            }
            for &c in self.objs[id.0].children.iter().rev() {
                stack.push(c);
            }
        }
        out
    }

    // ---- library entity construction (class `bb`, deobfuscated.js:66938-67113) ----

    /// `$.library.getEntity(group, opts)`.
    pub fn get_entity(&mut self, lib: &Library, group: &str, opts: &MatOpts) -> ObjId {
        let mut group = group.to_string();
        if !lib.has_group(&group) && (group == "epic_mid" || group == "epic_end") {
            group = "epic_start".into();
        }
        let Some(parts) = lib.geometry_groups.get(&group) else {
            return self.new_obj(); // "returning empty entity"
        };
        let parts = parts.clone();
        let e = if parts.len() == 1 {
            self.entity_from_geometry(lib, &group, &parts[0], opts)
        } else {
            let c = self.new_obj();
            self.mount_entity(lib, c, &group, opts);
            c
        };
        self.objs[e.0].group_name = Some(group);
        e
    }

    /// `$.library.mountEntity(target, group, opts)`: one child per part.
    pub fn mount_entity(&mut self, lib: &Library, target: ObjId, group: &str, opts: &MatOpts) -> Vec<ObjId> {
        let parts = lib.geometry_groups.get(group).cloned().unwrap_or_default();
        let mut out = Vec::new();
        for hash in parts {
            let p = self.entity_from_geometry(lib, group, &hash, opts);
            self.objs[p.0].geom_hash = Some(hash);
            self.add_child(target, p);
            out.push(p);
        }
        out
    }

    /// `getEntityFromGeometry(group, hash, opts)`: material rules by .pk material name.
    pub fn entity_from_geometry(&mut self, lib: &Library, group: &str, hash: &str, opts: &MatOpts) -> ObjId {
        let mut o = opts.clone();
        let scene_name = lib.geometry_scene_names.get(group).cloned().unwrap_or_default();
        if o.map.is_none() {
            o.map = library::scene_map(&scene_name).map(str::to_string);
        }
        let geom_key = hash.split(" +").next().unwrap_or("").to_string();
        let mat = lib.material_name_by_hash.get(&geom_key).cloned();
        let has = |s: &str| mat.as_deref().is_some_and(|m| m.contains(s));
        let mut depth_mask_false = o.depth_mask == Some(false);
        // fog flags: customNoFog / whiteFog (textured parts fog unless customNoFog)
        let mut no_fog = false;
        let mut white_fog = false;
        let mut fog_multiplier = 1.0;
        if has("shadow") || has("baseshader") {
            o.blend_mode = Some(BlendMode::Multiply);
            o.map = Some("environment-tex".into());
            depth_mask_false = true;
            no_fog = false;
            white_fog = true;
        }
        if has("glass_transparent") || has("transparent_shader") {
            o.opacity = Some(0.5);
        } else if has("glass") || mat.as_deref() == Some("ice") {
            if geom_key.contains("epic_start") {
                fog_multiplier = 1.6;
            }
            if has("glass_opaque") {
                o.opacity = Some(0.5);
            }
        }
        if has("glow") || has("light") || geom_key.contains("glow") || geom_key.contains("Rainbow") {
            if !geom_key.contains("sl_monument_1") {
                o.blend_mode = Some(BlendMode::Add);
                no_fog = true;
                o.opacity = Some(1.0);
                depth_mask_false = true;
            }
        }
        if has("train_start") || has("trainstart") {
            o.map = Some("train-start".into());
            o.opacity = Some(0.999);
        }
        let opacity = o.opacity.filter(|&v| v != 0.0);
        let mut blend = opacity.is_some_and(|v| v < 1.0);
        let mut blend_mode = BlendMode::Normal;
        if let Some(b) = o.blend_mode {
            blend_mode = b;
            // K.blendMode setter enables blending (trace: blendMode set => blend true)
            if b != BlendMode::Normal {
                blend = true;
            }
        }
        // bali G.handleThemeMaterial (deobfuscated.js:8366): water/foam shaders
        let mut shader = Shader::Basic;
        if geom_key == "tube_mid" && mat.as_deref() == Some("environment_water") {
            o.map = Some("caustic".into());
            shader = Shader::Water;
        } else if (geom_key == "tube_end" && mat.as_deref() == Some("environment_foam"))
            || (geom_key == "epic_start" && mat.as_deref() == Some("water"))
        {
            o.map = Some("fountain".into());
            shader = Shader::Foam;
        }
        let (scene, key) = lib.geometry_by_hash.get(hash).cloned().unwrap_or_default();
        let id = self.new_obj();
        // handleThemeMaterial: boats bob (Wr, frequency 4) from the part's origin
        match geom_key.as_str() {
            "Boat1" => self.objs[id.0].bob = Some((DVec3::ZERO, DVec3::new(0.0, -2.0, 0.0), 4.0)),
            "Boat1_reflection" => self.objs[id.0].bob = Some((DVec3::ZERO, DVec3::new(0.0, 2.0, 0.0), 4.0)),
            _ => {}
        }
        self.objs[id.0].visual = Visual::Mesh {
            hash: hash.to_string(),
            scene,
            key,
            material: Material {
                map: o.map,
                opacity,
                color: None,
                blend,
                blend_mode,
                depth_mask: !depth_mask_false,
                culling: true,
                cw: false,
                pk_material: mat,
                fog: if no_fog {
                    Fog::Off
                } else if white_fog {
                    Fog::White
                } else {
                    Fog::On
                },
                fog_multiplier,
                shader,
            },
        };
        id
    }

    pub fn material_mut(&mut self, id: ObjId) -> Option<&mut Material> {
        match &mut self.objs[id.0].visual {
            Visual::Mesh { material, .. } => Some(material),
            _ => None,
        }
    }
}
