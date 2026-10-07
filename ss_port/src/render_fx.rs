//! Draws `crate::fx`: the particle systems (`Jn` instanced quads, here one
//! quad per slot with its centre in world space) and the rigs' ribbon
//! trails (`Sf.build`), with `shaders/fx.wgsl`.
//!
//! Like three, a system draws its first `instanceCount` slots in slot
//! order (dead slots have size 0); blending is non-premultiplied normal or
//! additive with depth test and no depth write; the rig materials'
//! `renderQueue` (three `renderOrder`) orders them after other transparents.
//! Meshes are rebuilt on each simulation frame; render frames in between
//! interpolate the particle centres (uncapped play).

use crate::fx::{Blend, Owner};
use crate::render::{RenderBlend, RenderCache};
use crate::sim_plugin::Sim;
use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::math::{DVec3, Vec3};
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexFormat};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use std::collections::HashMap;

pub const ATTR_CORNER: MeshVertexAttribute = MeshVertexAttribute::new("FxCorner", 912_301_001, VertexFormat::Float32x2);
pub const ATTR_DATA: MeshVertexAttribute = MeshVertexAttribute::new("FxData", 912_301_002, VertexFormat::Float32x4);
pub const ATTR_TILE: MeshVertexAttribute = MeshVertexAttribute::new("FxTile", 912_301_003, VertexFormat::Float32);
pub const ATTR_VELOCITY: MeshVertexAttribute = MeshVertexAttribute::new("FxVelocity", 912_301_004, VertexFormat::Float32x3);
pub const ATTR_TANGENT: MeshVertexAttribute = MeshVertexAttribute::new("FxTangent", 912_301_005, VertexFormat::Float32x3);
pub const ATTR_SIDE: MeshVertexAttribute = MeshVertexAttribute::new("FxSide", 912_301_006, VertexFormat::Float32);

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct FxParams {
    pub size_tiles: Vec4,
    pub misc: Vec4,
    pub bend_scroll: Vec4,
    pub uv_scale: Vec4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FxKey {
    /// 0 particle, 1 ribbon, 2 mesh
    pub kind: u8,
    pub stretch: bool,
    pub separate_axes: bool,
    pub additive: bool,
    pub render_order: i32,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
#[bind_group_data(FxKey)]
pub struct FxMaterial {
    #[uniform(0)]
    pub params: FxParams,
    #[texture(1)]
    #[sampler(2)]
    pub map: Option<Handle<Image>>,
    pub key: FxKey,
}

impl From<&FxMaterial> for FxKey {
    fn from(m: &FxMaterial) -> Self {
        m.key
    }
}

const SHADER: &str = "embedded://ss_port/shaders/fx.wgsl";

impl Material for FxMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    /// three sorts transparents by `renderOrder` first.
    fn depth_bias(&self) -> f32 {
        self.key.render_order as f32 * 1e6
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let k = key.bind_group_data;
        let (attrs, def): (Vec<_>, &str) = match k.kind {
            0 => (
                vec![
                    Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                    ATTR_CORNER.at_shader_location(1),
                    ATTR_DATA.at_shader_location(2),
                    Mesh::ATTRIBUTE_COLOR.at_shader_location(3),
                    ATTR_TILE.at_shader_location(4),
                    ATTR_VELOCITY.at_shader_location(5),
                ],
                "PARTICLE",
            ),
            1 => (
                vec![
                    Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                    Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
                    ATTR_TANGENT.at_shader_location(2),
                    ATTR_SIDE.at_shader_location(3),
                    Mesh::ATTRIBUTE_COLOR.at_shader_location(4),
                ],
                "RIBBON",
            ),
            _ => (vec![Mesh::ATTRIBUTE_POSITION.at_shader_location(0), Mesh::ATTRIBUTE_UV_0.at_shader_location(1)], "MESH"),
        };
        descriptor.vertex.buffers = vec![layout.0.get_layout(&attrs)?];
        let mut defs = vec![def.into()];
        if k.stretch {
            defs.push("STRETCH".into());
        }
        if k.separate_axes {
            defs.push("ROTATION_SEPARATE_AXES".into());
        }
        descriptor.vertex.shader_defs.extend(defs.iter().cloned());
        let frag = descriptor.fragment.as_mut().unwrap();
        frag.shader_defs.extend(defs);
        let add = |src, dst| BlendComponent { src_factor: src, dst_factor: dst, operation: BlendOperation::Add };
        let blend = if k.additive {
            BlendState { color: add(BlendFactor::SrcAlpha, BlendFactor::One), alpha: add(BlendFactor::SrcAlpha, BlendFactor::One) }
        } else {
            BlendState { color: add(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha), alpha: add(BlendFactor::One, BlendFactor::OneMinusSrcAlpha) }
        };
        for t in frag.targets.iter_mut().flatten() {
            t.blend = Some(blend);
        }
        descriptor.primitive.cull_mode = None;
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

pub struct FxRenderPlugin;

impl Plugin for FxRenderPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/fx.wgsl");
        app.add_plugins(MaterialPlugin::<FxMaterial>::default())
            .init_resource::<FxRender>()
            .add_systems(PostUpdate, sync_fx.after(crate::render_actors::sync_actors));
    }
}

/// One drawn system or trail.
struct Drawn {
    entity: Entity,
    mesh: Handle<Mesh>,
    /// Particle centres (world) and lifetimes at the previous / latest
    /// simulation frame, per drawn slot.
    prev: Vec<([f32; 3], f64)>,
    cur: Vec<([f32; 3], f64)>,
}

#[derive(Resource, Default)]
pub struct FxRender {
    serial: Option<u64>,
    last_frame: Option<i64>,
    systems: HashMap<usize, Drawn>,
    trails: HashMap<(usize, usize), Drawn>,
    /// `Df` flames: per (rig, side) the parts of `powerups_jetpack_FX`
    /// (entity, matrix in the flame entity).
    flames: HashMap<(usize, usize), Vec<(Entity, bevy::math::DMat4)>>,
}

const WORLD_FAR: f32 = crate::render::world::FAR;

fn spawn_mesh(commands: &mut Commands, meshes: &mut Assets<Mesh>, mat: Handle<FxMaterial>) -> (Entity, Handle<Mesh>) {
    let mesh = meshes.add(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()));
    let e = commands
        .spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::IDENTITY, Visibility::Hidden, bevy::camera::visibility::NoFrustumCulling))
        .id();
    (e, mesh)
}

fn base_params() -> FxParams {
    FxParams {
        size_tiles: Vec4::new(1.0, 1.0, 1.0, 1.0),
        misc: Vec4::new(1.0, 1.0, 0.0, WORLD_FAR),
        bend_scroll: Vec4::new(crate::render::bend()[0], crate::render::world::BEND[1], 0.0, 0.0),
        uv_scale: Vec4::new(1.0, 1.0, 0.0, 0.0),
    }
}

/// The particle quads of a system: four vertices per drawn slot.
fn particle_mesh(mesh: &mut Mesh, sys: &crate::particles::System, centres: &[([f32; 3], f64)], slots: &[usize], world: &bevy::math::DMat4) {
    let n = slots.len();
    let mut pos = Vec::with_capacity(n * 4);
    let mut corner = Vec::with_capacity(n * 4);
    let mut data = Vec::with_capacity(n * 4);
    let mut color = Vec::with_capacity(n * 4);
    let mut tile = Vec::with_capacity(n * 4);
    let mut vel = Vec::with_capacity(n * 4);
    let mut idx = Vec::with_capacity(n * 6);
    for (k, &i) in slots.iter().enumerate() {
        let v = sys.velocity_buf.as_ref().map_or([0.0; 3], |b| {
            let w = world.transform_vector3(DVec3::new(b[i][0] as f64, b[i][1] as f64, b[i][2] as f64));
            [w.x as f32, w.y as f32, w.z as f32]
        });
        for c in [[-0.5f32, 0.5], [0.5, 0.5], [-0.5, -0.5], [0.5, -0.5]] {
            pos.push(centres[k].0);
            corner.push(c);
            data.push(sys.data[i]);
            color.push(sys.color[i]);
            tile.push(sys.tile[i]);
            vel.push(v);
        }
        // PlaneGeometry(1, 1) indices
        let b = (k * 4) as u32;
        idx.extend_from_slice(&[b, b + 2, b + 1, b + 2, b + 3, b + 1]);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(ATTR_CORNER, corner);
    mesh.insert_attribute(ATTR_DATA, data);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, color);
    mesh.insert_attribute(ATTR_TILE, tile);
    mesh.insert_attribute(ATTR_VELOCITY, vel);
    mesh.insert_indices(Indices::U32(idx));
}

/// `Sf.build`: two vertices per chain point (sides -w / +w), widths from
/// the width curve and colours from the gradient along the length.
fn trail_mesh(mesh: &mut Mesh, t: &crate::fx::Trail) {
    let chain = &t.chain[..t.chain.len().min(48)];
    let n = chain.len();
    let total: f64 = (1..n).map(|i| chain[i].distance(chain[i - 1])).sum();
    let (mut pos, mut tan, mut side, mut col, mut uv) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut run = 0.0;
    for i in 0..n {
        if i > 0 {
            run += chain[i].distance(chain[i - 1]);
        }
        let s = if total > 0.0 { run / total } else { 0.0 };
        let w = t.data.width_multiplier * crate::fx::eval_keys(&t.data.width_curve, s) * 0.5;
        let g = chain[i.saturating_sub(1)] - chain[(i + 1).min(n - 1)];
        let c = t.data.color_at(s);
        for k in 0..2 {
            pos.push(chain[i].as_vec3().to_array());
            tan.push(g.as_vec3().to_array());
            side.push(if k == 0 { -w as f32 } else { w as f32 });
            col.push([c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32]);
            uv.push([s as f32, k as f32]);
        }
    }
    let mut idx = Vec::new();
    for i in 0..n.saturating_sub(1) as u32 {
        let a = i * 2;
        idx.extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(ATTR_TANGENT, tan);
    mesh.insert_attribute(ATTR_SIDE, side);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
}

#[allow(clippy::too_many_arguments)]
fn sync_fx(
    mut commands: Commands,
    sim: Option<NonSend<Sim>>,
    mut state: ResMut<FxRender>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<FxMaterial>>,
    mut cache: ResMut<RenderCache>,
    asset_server: Res<AssetServer>,
    blend: Option<Res<RenderBlend>>,
    mut vis: Query<&mut Visibility>,
) {
    let Some(sim) = sim else { return };
    let g = &sim.game;
    let alpha = blend.map_or(1.0, |b| b.alpha);
    if state.serial != Some(sim.serial) {
        let st = &mut *state;
        for d in st.systems.drain().map(|x| x.1).chain(st.trails.drain().map(|x| x.1)) {
            commands.entity(d.entity).despawn();
        }
        for (e, _) in st.flames.drain().flat_map(|x| x.1) {
            commands.entity(e).despawn();
        }
        state.serial = Some(sim.serial);
        state.last_frame = None;
    }
    let theme = g.theme.clone();
    let mut set = |e: Entity, on: bool| {
        if let Ok(mut v) = vis.get_mut(e) {
            let want = if on { Visibility::Visible } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
    };
    let new_frame = state.last_frame != Some(g.frame);
    // ---- particle systems
    for (i, slot) in g.fx.systems.iter().enumerate() {
        let sys = &slot.sys;
        let world = match slot.owner {
            Owner::Scene(obj) => g.scene.world_matrix(obj),
            Owner::Rig | Owner::Free => slot.world,
        };
        let on = slot.visible && sys.instance_count > 0;
        if !state.systems.contains_key(&i) {
            if !on {
                continue;
            }
            let rol = &sys.cfg.rotation_over_lifetime;
            let key = FxKey {
                kind: 0,
                stretch: sys.cfg.renderer.mode == crate::particles::MODE_STRETCH,
                separate_axes: rol.enabled && rol.separate_axes,
                additive: slot.look.blend == Blend::Add,
                render_order: slot.look.render_order,
            };
            let mut p = base_params();
            p.size_tiles = Vec4::new(slot.look.size_scale[0] as f32, slot.look.size_scale[1] as f32, 1.0, 1.0);
            p.misc = Vec4::new(slot.look.multiplier as f32, sys.cfg.renderer.length_scale.unwrap_or(1.0) as f32, sys.cfg.renderer.speed_scale.unwrap_or(0.0) as f32, WORLD_FAR);
            let map = cache.image(&asset_server, &theme, &slot.look.texture);
            let mat = mats.add(FxMaterial { params: p, map, key });
            let (entity, mesh) = spawn_mesh(&mut commands, &mut meshes, mat);
            state.systems.insert(i, Drawn { entity, mesh, prev: Vec::new(), cur: Vec::new() });
        }
        let d = state.systems.get_mut(&i).unwrap();
        set(d.entity, on);
        if !on {
            continue;
        }
        // the texture sheet's tile count is a material uniform set by spawn
        let slots: Vec<usize> = (0..sys.instance_count.min(sys.particles.len())).filter(|&k| sys.data[k][0] != 0.0).collect();
        if new_frame {
            d.prev = std::mem::take(&mut d.cur);
            d.cur = slots
                .iter()
                .map(|&k| {
                    let o = sys.offset[k];
                    let w = world.transform_point3(DVec3::new(o[0] as f64, o[1] as f64, o[2] as f64));
                    ([w.x as f32, w.y as f32, w.z as f32], sys.particles[k].lifetime)
                })
                .collect();
        }
        // interpolate centres between simulation frames
        let centres: Vec<([f32; 3], f64)> = if alpha < 1.0 && d.prev.len() == d.cur.len() {
            d.prev
                .iter()
                .zip(&d.cur)
                .map(|(a, b)| {
                    if b.1 <= a.1 {
                        (Vec3::from(a.0).lerp(Vec3::from(b.0), alpha).to_array(), b.1)
                    } else {
                        *b
                    }
                })
                .collect()
        } else {
            d.cur.clone()
        };
        if let Some(mut mesh) = meshes.get_mut(&d.mesh) {
            particle_mesh(&mut mesh, sys, &centres, &slots, &world);
        }
    }
    // ---- trails
    for (r, rig) in g.fx.rigs.iter().enumerate() {
        for (s, side) in rig.sides.iter().enumerate() {
            let t = &side.trail;
            let on = t.visible && t.drawn;
            if !state.trails.contains_key(&(r, s)) {
                if !on {
                    continue;
                }
                let key = FxKey { kind: 1, stretch: false, separate_axes: false, additive: t.look.additive, render_order: t.look.render_order };
                let mut p = base_params();
                p.misc.x = t.look.multiplier as f32;
                p.uv_scale = Vec4::new(t.look.uv_scale[0] as f32, t.look.uv_scale[1] as f32, 0.0, 0.0);
                let map = cache.image(&asset_server, &theme, &t.look.texture);
                let mat = mats.add(FxMaterial { params: p, map, key });
                let (entity, mesh) = spawn_mesh(&mut commands, &mut meshes, mat);
                state.trails.insert((r, s), Drawn { entity, mesh, prev: Vec::new(), cur: Vec::new() });
            }
            let d = &state.trails[&(r, s)];
            set(d.entity, on);
            if on && new_frame {
                if let Some(mut mesh) = meshes.get_mut(&d.mesh) {
                    trail_mesh(&mut mesh, t);
                }
            }
        }
    }
    // ---- Df flames: powerups_jetpack_FX under the boost node, flameMaterial
    for (r, rig) in g.fx.rigs.iter().enumerate() {
        let Some(look) = rig.flame_look.as_ref() else { continue };
        for (s, side) in rig.sides.iter().enumerate() {
            let Some(flame) = side.flame else { continue };
            if !state.flames.contains_key(&(r, s)) {
                if !rig.visible {
                    continue;
                }
                let key = FxKey { kind: 2, stretch: false, separate_axes: false, additive: look.additive, render_order: look.render_order };
                let mut p = base_params();
                p.misc.x = look.multiplier as f32;
                p.uv_scale = Vec4::new(look.uv_scale[0] as f32, look.uv_scale[1] as f32, 0.0, 0.0);
                let map = cache.image(&asset_server, &theme, &look.texture);
                let mat = mats.add(FxMaterial { params: p, map, key });
                let mut parts = Vec::new();
                let lib = g.lib.clone();
                if lib.has_group("powerups_jetpack_FX") {
                    let mut scene = crate::scene::Scene::default();
                    let root = scene.get_entity(&lib, "powerups_jetpack_FX", &Default::default());
                    for mid in scene.meshes(root) {
                        let crate::scene::Visual::Mesh { hash, .. } = &scene.get(mid).visual else { continue };
                        let Some(mesh) = crate::render::build_mesh(&sim, hash) else { continue };
                        let local = scene.world_matrix(root).inverse() * scene.world_matrix(mid);
                        let e = commands
                            .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), Transform::IDENTITY, Visibility::Hidden, bevy::camera::visibility::NoFrustumCulling))
                            .id();
                        parts.push((e, local));
                    }
                }
                state.flames.insert((r, s), parts);
            }
            for &(e, local) in &state.flames[&(r, s)] {
                set(e, rig.visible);
                if rig.visible {
                    let m = side.boost_world * flame * local;
                    commands.entity(e).insert(Transform::from_matrix(m.as_mat4()));
                }
            }
            if new_frame {
                let scroll = look.uv_scroll(rig.clock);
                for &(e, _) in &state.flames[&(r, s)] {
                    update_material(&mut commands, e, move |m| m.params.bend_scroll = Vec4::new(m.params.bend_scroll.x, m.params.bend_scroll.y, scroll[0] as f32, scroll[1] as f32));
                }
            }
        }
    }
    // uv scroll (pf.setTime(trail clock)) and texture-sheet tiles
    if new_frame {
        for (&(r, s), d) in &state.trails {
            let t = &g.fx.rigs[r].sides[s].trail;
            let scroll = t.look.uv_scroll(t.clock);
            update_material(&mut commands, d.entity, move |m| m.params.bend_scroll = Vec4::new(m.params.bend_scroll.x, m.params.bend_scroll.y, scroll[0] as f32, scroll[1] as f32));
        }
        for (&i, d) in &state.systems {
            let tiles = g.fx.systems[i].sys.uv_tiles;
            update_material(&mut commands, d.entity, move |m| {
                m.params.size_tiles.z = tiles[0] as f32;
                m.params.size_tiles.w = tiles[1] as f32;
            });
        }
    }
    state.last_frame = Some(g.frame);
}

/// Change an entity's FX material uniforms (after the commands apply).
fn update_material(commands: &mut Commands, e: Entity, f: impl FnOnce(&mut FxMaterial) + Send + 'static) {
    commands.queue(move |world: &mut World| {
        let Some(h) = world.get::<MeshMaterial3d<FxMaterial>>(e).map(|m| m.0.clone()) else { return };
        if let Some(mut mats) = world.get_resource_mut::<Assets<FxMaterial>>() {
            if let Some(mut m) = mats.get_mut(&h) {
                f(&mut m);
            }
        }
    });
}
