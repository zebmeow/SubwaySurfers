//! Bevy rendering of the simulation's scene graph, matching the original
//! WebGL renderer (docs/rendering.md).
//!
//! Each frame, every effectively-visible mesh object of every in-scene
//! entity gets a Bevy entity with its world transform. Meshes come straight
//! from the `.pk` primitives (no axis conversion: three.js and Bevy share a
//! right-handed, y-up, -z-forward frame and CCW front faces).
//!
//! Materials are [`SubwayMaterial`] (src/shaders/subway.wgsl): the game's
//! `$n` (unlit map x color, clip-space world bend, smoothstep distance fog,
//! sRGB output without tone mapping) and the Bali water/foam shaders. As in
//! WebGL, encoded values are blended; the camera renders into a float
//! target and [`SrgbDecode`] decodes once at the end. Textures get CPU
//! generated mip chains (three's `LinearMipmapLinear`).
//!
//! Not reproduced yet: particle systems, procedural planes (halos), the
//! rails screen mask and glass reflections (both ignored by `$n` anyway).

use crate::scene::{BlendMode, Fog, Material as SimMaterial, ObjId, Shader, Visual};
use crate::sim_plugin::Sim;
use crate::theme::ThemeConfig;
use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::DMat4;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Face, PrimitiveTopology, RenderPipelineDescriptor, ShaderType,
    SpecializedMeshPipelineError, TextureFormat,
};
use bevy::shader::ShaderRef;
use std::collections::{HashMap, HashSet};

/// Theme-independent renderer constants (`Gn.group` / `Wn`, deobfuscated.js:
/// 4255, as measured in the running game). Sky and fog come from the theme
/// ([`crate::theme::ThemeConfig`]).
pub mod world {
    /// `uBend` at start-up: `config.bendX * aspectRatio, config.bendY` with
    /// the initial aspect 1 (the oracle resized once). A window resize
    /// changes x ([`super::bend`]).
    pub const BEND: [f32; 2] = [-5e-4, -3e-4];
    /// `uFogDistance` (raw shaders).
    pub const FOG_DISTANCE: f32 = 410.0;
    /// three PerspectiveCamera near/far (sm.setup).
    pub const NEAR: f32 = 3.0;
    pub const FAR: f32 = 1200.0;
}

/// Library map name -> file under oracle/site (the cache paths the game
/// used); theme maps come from the theme file.
pub fn map_path(map: &str, theme: &ThemeConfig) -> Option<String> {
    Some(match map {
        "environment-tex" => return Some(theme.env_texture.clone()),
        "train-start" => return Some(theme.train_start_texture.clone()),
        "trains-tex" => return Some(theme.train_texture.clone()),
        "props-tex" => return Some(theme.props_texture.clone()),
        "effects-tex" => "assets/game-basic/effects-tex.webp",
        "halo" => "assets/game-basic/halo.webp",
        "caustic" => "assets/game-basic/caustic.webp",
        "fountain" => "assets/game-basic/fountain.webp",
        "enemies" => "assets/game-basic/enemies.webp",
        "jake-tex" => "assets/characters-idle/jake-tex.webp",
        "reflection" => "assets/game-basic/reflection.webp",
        "wave_noise" => "assets/game-basic/wave_noise.webp",
        // particle / trail FX textures (crate::fx)
        "character_shadow" => "assets/game-basic/character_shadow.webp",
        "pogo-cloud" | "jetpack-trail" | "jetpack-flame" | "jetpack-puff" | "jetpack-puff2" | "spark" | "fog" | "grindSpark" | "spray-splash"
        | "vfx_spark" | "waterspray" => return Some(format!("assets/game-basic/{map}.webp")),
        // boards (`board-<id>-tex`) and the characters' outfit textures
        m if m.starts_with("board-") && m.ends_with("-tex") => return Some(format!("assets/boards/{m}.webp")),
        m if m.contains("-tex") && crate::shop::Catalog::get().characters.iter().any(|c| m.starts_with(&format!("{}-tex", c.id))) => {
            return Some(format!("assets/characters-idle/{m}.webp"))
        }
        _ => return None,
    }
    .to_string())
}

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct Params {
    pub color: Vec4,
    pub fog_color: Vec4,
    pub fog: Vec4,
    pub bend: Vec4,
    pub extra: Vec4,
    pub extra2: Vec4,
}

/// Pipeline variant: shader mode, blending, culling, depth write, map, fog.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct SubwayKey {
    pub shader: u8,
    /// 0 opaque, 1 normal alpha, 2 additive, 3 multiply
    pub blend: u8,
    /// 0 none, 1 back faces, 2 front faces
    pub cull: u8,
    pub depth_write: bool,
    pub has_map: bool,
    pub fog: bool,
    /// three `depthTest` (off: the revive halo).
    pub depth_test: bool,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(SubwayKey)]
pub struct SubwayMaterial {
    #[uniform(0)]
    pub params: Params,
    #[texture(1)]
    #[sampler(2)]
    pub map: Option<Handle<Image>>,
    #[texture(3)]
    #[sampler(4)]
    pub displacement: Option<Handle<Image>>,
    pub key: SubwayKey,
}

impl From<&SubwayMaterial> for SubwayKey {
    fn from(m: &SubwayMaterial) -> Self {
        m.key
    }
}

const SHADER: &str = "embedded://ss_port/shaders/subway.wgsl";

impl Material for SubwayMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.key.blend == 0 {
            AlphaMode::Opaque
        } else {
            AlphaMode::Blend
        }
    }
    /// three draws `renderOrder -1` (multiply) before other transparents.
    fn depth_bias(&self) -> f32 {
        if self.key.blend == 3 {
            -1e9
        } else {
            0.0
        }
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
        let attrs = vec![Mesh::ATTRIBUTE_POSITION.at_shader_location(0), Mesh::ATTRIBUTE_UV_0.at_shader_location(1)];
        let vl = layout.0.get_layout(&attrs)?;
        descriptor.vertex.buffers = vec![vl];
        let mut defs = vec![match k.shader {
            1 => "MODE_WATER",
            2 => "MODE_FOAM",
            _ => "MODE_BASIC",
        }
        .into()];
        if k.has_map {
            defs.push("HAS_MAP".into());
        }
        if k.fog {
            defs.push("FOG".into());
        }
        if k.blend != 0 {
            defs.push("BLEND".into());
        }
        descriptor.vertex.shader_defs.extend(defs.iter().cloned());
        let frag = descriptor.fragment.as_mut().unwrap();
        frag.shader_defs.extend(defs);
        let add = |src, dst| BlendComponent { src_factor: src, dst_factor: dst, operation: BlendOperation::Add };
        let blend = match k.blend {
            // NormalBlending (non-premultiplied)
            1 => Some(BlendState {
                color: add(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha),
                alpha: add(BlendFactor::One, BlendFactor::OneMinusSrcAlpha),
            }),
            // AdditiveBlending: blendFunc(SRC_ALPHA, ONE)
            2 => Some(BlendState { color: add(BlendFactor::SrcAlpha, BlendFactor::One), alpha: add(BlendFactor::SrcAlpha, BlendFactor::One) }),
            // CustomBlending DstColor x Zero (bridge state blendMode 2)
            3 => Some(BlendState { color: add(BlendFactor::Dst, BlendFactor::Zero), alpha: add(BlendFactor::Dst, BlendFactor::Zero) }),
            // CustomBlending One x OneMinusSrcColor (blendMode 3, SCREEN):
            // 1 - (1 - src)(1 - dst) for colour; opacity does not scale it
            4 => Some(BlendState { color: add(BlendFactor::One, BlendFactor::OneMinusSrc), alpha: add(BlendFactor::One, BlendFactor::OneMinusSrc) }),
            _ => None,
        };
        for t in frag.targets.iter_mut().flatten() {
            t.blend = blend;
        }
        descriptor.primitive.cull_mode = match k.cull {
            1 => Some(Face::Back),
            2 => Some(Face::Front),
            _ => None,
        };
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = Some(k.depth_write);
            if !k.depth_test {
                ds.depth_compare = Some(bevy::render::render_resource::CompareFunction::Always);
            }
        }
        Ok(())
    }
}

/// Final pass: decode the sRGB-encoded target (see the shader).
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
pub struct SrgbDecode {
    unused: f32,
}

impl FullscreenMaterial for SrgbDecode {
    fn fragment_shader() -> ShaderRef {
        "embedded://ss_port/shaders/srgb_decode.wgsl".into()
    }
    /// After the UI pass: the 2D overlay blends into the encoded target too,
    /// as pixi drew into the same canvas.
    fn schedule_configs(
        system: bevy::ecs::schedule::ScheduleConfigs<bevy::ecs::system::BoxedSystem>,
    ) -> bevy::ecs::schedule::ScheduleConfigs<bevy::ecs::system::BoxedSystem> {
        use bevy::ecs::schedule::IntoScheduleConfigs;
        system.after(bevy::ui_render::render_pass::ui_pass).before(bevy::core_pipeline::upscaling::upscaling)
    }
}

/// Camera components matching the original canvas: float target (blending
/// of encoded values), no tone mapping or dithering, no MSAA
/// (`antialias: false`), the sky clear color, the decode pass.
/// Anti-aliasing of the 3D view. The original draws without (the oracle
/// comparisons use `Off`); interactive play defaults to MSAA 4x.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AntiAlias {
    #[default]
    Off,
    /// MSAA 4x: geometry edges (rails, wires, roofs).
    Msaa,
    /// FXAA post-process (also texture edges, slightly softer).
    Fxaa,
    /// Both.
    Both,
}

impl AntiAlias {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "off" | "none" => Self::Off,
            "msaa" => Self::Msaa,
            "fxaa" => Self::Fxaa,
            "both" | "msaa+fxaa" => Self::Both,
            _ => return None,
        })
    }
    fn msaa(self) -> Msaa {
        if matches!(self, Self::Msaa | Self::Both) { Msaa::Sample4 } else { Msaa::Off }
    }
    fn fxaa(self) -> bevy::anti_alias::fxaa::Fxaa {
        use bevy::anti_alias::fxaa::{Fxaa, Sensitivity};
        Fxaa { enabled: matches!(self, Self::Fxaa | Self::Both), edge_threshold: Sensitivity::High, edge_threshold_min: Sensitivity::High }
    }
}

/// How far the 60 Hz accumulator is into the next simulation frame (0..1):
/// render frames between simulation frames show the state interpolated
/// between the last two (1 = exactly the latest frame, as in screenshots).
#[derive(Resource, Clone, Copy, Debug)]
pub struct RenderBlend {
    pub alpha: f32,
}

impl Default for RenderBlend {
    fn default() -> Self {
        Self { alpha: 1.0 }
    }
}

/// Interpolate two transforms (lerp / slerp).
pub fn blend_tf(a: &Transform, b: &Transform, t: f32) -> Transform {
    Transform { translation: a.translation.lerp(b.translation, t), rotation: a.rotation.slerp(b.rotation, t), scale: a.scale.lerp(b.scale, t) }
}

/// The gameplay camera (the window's 3D view). Other cameras (the New High
/// Score render, `crate::render_menu_3d::StageCamera`) must not match its
/// queries.
#[derive(Component)]
pub struct MainWorldCamera;

pub fn camera_bundle(theme: &ThemeConfig, aa: AntiAlias) -> impl Bundle {
    use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
    let [r, g, b] = theme.sky_color;
    (
        MainWorldCamera,
        Camera3d::default(),
        bevy::camera::Hdr,
        Tonemapping::None,
        DebandDither::Disabled,
        aa.msaa(),
        aa.fxaa(),
        // the target stores encoded values: clear with the raw sRGB bytes
        Camera { clear_color: ClearColorConfig::Custom(Color::LinearRgba(LinearRgba::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0))), ..default() },
        Projection::Perspective(PerspectiveProjection { fov: 68f32.to_radians(), near: world::NEAR, far: world::FAR, ..default() }),
        SrgbDecode::default(),
    )
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct MatKey {
    map: Option<String>,
    key: SubwayKey,
    opacity_bits: u64,
    color: Option<u32>,
    fog_mult_bits: u64,
}

#[derive(Resource, Default)]
pub struct RenderCache {
    meshes: HashMap<String, Handle<Mesh>>,
    materials: HashMap<MatKey, Handle<SubwayMaterial>>,
    images: HashMap<String, Handle<Image>>,
    /// images whose mip chain was generated
    mipped: HashSet<AssetId<Image>>,
    /// sim object -> Bevy entity
    spawned: HashMap<ObjId, Entity>,
    /// Theme (and simulation) the cache was built for.
    theme: String,
    sim_serial: u64,
    /// Per object: transform at the previous and the latest simulation frame.
    blend: HashMap<ObjId, (Transform, Transform)>,
    /// Simulation frame the transforms were taken at.
    last_frame: Option<i64>,
}

impl RenderCache {
    /// A library map as a Bevy image (loaded once, mipmapped).
    pub fn image(&mut self, assets: &AssetServer, theme: &ThemeConfig, map: &str) -> Option<Handle<Image>> {
        image(self, assets, theme, map)
    }
    /// True once every game texture requested so far has finished loading.
    pub fn textures_loaded(&self, assets: &AssetServer) -> bool {
        !self.images.is_empty() && self.images.values().all(|h| assets.is_loaded_with_dependencies(h.id()) && self.mipped.contains(&h.id()))
    }
}

/// Mirrors the sim scene into Bevy after every simulation step.
pub struct SceneRenderPlugin;

impl Plugin for SceneRenderPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/subway.wgsl");
        embedded_asset!(app, "shaders/srgb_decode.wgsl");
        app.add_plugins((MaterialPlugin::<SubwayMaterial>::default(), FullscreenMaterialPlugin::<SrgbDecode>::default()))
            .init_resource::<RenderCache>()
            .init_resource::<crate::render_actors::ActorRender>()
            .init_resource::<RenderBlend>()
            .add_systems(PostUpdate, (sync_scene, sync_sky, animate_theme_materials, crate::render_actors::sync_actors, generate_mips))
            .add_systems(PreUpdate, resize_bend);
    }
}

static BEND_X: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0xBA03126F); // -5e-4

/// The current `uBend`.
pub fn bend() -> [f32; 2] {
    [f32::from_bits(BEND_X.load(std::sync::atomic::Ordering::Relaxed)), world::BEND[1]]
}

/// `Game.resize` (49871): `updateWorldBend()` runs before `aspectRatio = h /
/// w` is set, so a resize bends x by `config.bendX` times the aspect of the
/// resize before it (1 at first). The window's first size counts as the
/// first resize; every later size change applies the rule, to every live
/// world and particle material.
fn resize_bend(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut last: Local<Option<(u32, u32, f32)>>,
    mut mats: ResMut<Assets<SubwayMaterial>>,
    mut fx: ResMut<Assets<crate::render_fx::FxMaterial>>,
) {
    let Ok(win) = windows.single() else { return };
    let (w, h) = (win.physical_width(), win.physical_height());
    if w == 0 || h == 0 {
        return;
    }
    match *last {
        None => *last = Some((w, h, h as f32 / w as f32)),
        Some((lw, lh, aspect)) if (lw, lh) != (w, h) => {
            let x = world::BEND[0] * aspect;
            BEND_X.store(x.to_bits(), std::sync::atomic::Ordering::Relaxed);
            *last = Some((w, h, h as f32 / w as f32));
            for (_, m) in mats.iter_mut() {
                m.params.bend.x = x;
            }
            for (_, m) in fx.iter_mut() {
                m.params.bend_scroll.x = x;
            }
        }
        _ => {}
    }
}

fn to_mat4(m: &DMat4) -> Mat4 {
    m.as_mat4()
}

pub(crate) fn build_mesh(sim: &Sim, hash: &str) -> Option<Mesh> {
    let p = sim.game.lib.primitive(hash)?;
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, p.positions.clone());
    // Textures come through pixi (flipY = false): v = 0 is the top row, as in Bevy.
    // (Vertex colors are ignored: the library's `$n` never sets vertexColors.)
    let uvs: Vec<[f32; 2]> = if p.uvs.len() == p.positions.len() { p.uvs.clone() } else { vec![[0.0, 0.0]; p.positions.len()] };
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    if !p.indices.is_empty() {
        mesh.insert_indices(Indices::U32(p.indices.clone()));
    }
    Some(mesh)
}

fn image(cache: &mut RenderCache, assets: &AssetServer, theme: &ThemeConfig, map: &str) -> Option<Handle<Image>> {
    if let Some(h) = cache.images.get(map) {
        return Some(h.clone());
    }
    let path = map_path(map, theme)?;
    // character textures are clamped (probe: wrapS 1001); the rest repeat
    let character = map.contains("-tex") && crate::shop::Catalog::get().characters.iter().any(|c| map.starts_with(&format!("{}-tex", c.id)));
    let wrap = if character || map == "enemies" { ImageAddressMode::ClampToEdge } else { ImageAddressMode::Repeat };
    let h = assets
        .load_builder()
        .with_settings(move |s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: wrap,
                address_mode_v: wrap,
                mipmap_filter: ImageFilterMode::Linear,
                ..ImageSamplerDescriptor::linear()
            });
        })
        .load(path);
    cache.images.insert(map.to_string(), h.clone());
    Some(h)
}

/// three `Color.setHex`: sRGB hex -> linear working space.
fn hex_linear(c: u32) -> Vec3 {
    let l = Color::srgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8).to_linear();
    Vec3::new(l.red, l.green, l.blue)
}
/// `sn(hex)`: raw 0..1 channels (the raw shaders' uColor).
fn hex_raw(c: u32) -> Vec3 {
    Vec3::new(((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0)
}

/// A material for an offscreen character scene (the New High Score `Xv`,
/// three's thumb renderer): no world bend, no fog, three's far plane `far`.
pub fn thumb_material(mut m: SubwayMaterial, far: f32) -> SubwayMaterial {
    m.params.bend = Vec4::ZERO;
    m.params.fog.z = far;
    m.key.fog = false;
    m
}

fn base_params(theme: &ThemeConfig) -> Params {
    let [fr, fg, fb] = theme.fog_color;
    Params {
        color: Vec4::ONE,
        fog_color: Vec4::new(fr, fg, fb, 1.0),
        fog: Vec4::new(theme.fog_near, theme.fog_far, world::FAR, world::FOG_DISTANCE),
        bend: Vec4::new(bend()[0], world::BEND[1], 0.0, 0.0),
        extra: Vec4::ZERO,
        extra2: Vec4::new(0.0, world::NEAR, 0.0, 0.0),
    }
}

/// The material a mesh part renders with: library `$n` state, or the theme
/// shader that replaced it (with that material's own three defaults).
pub fn subway_material(theme: &ThemeConfig, m: &SimMaterial, map: Option<Handle<Image>>, displacement: Option<Handle<Image>>) -> SubwayMaterial {
    let mut p = base_params(theme);
    let opacity = m.opacity.unwrap_or(1.0) as f32;
    let (key, displacement) = match m.shader {
        Shader::Basic => {
            let blend = match m.blend_mode {
                BlendMode::Add => 2,
                BlendMode::Multiply => 3,
                BlendMode::Screen => 4,
                BlendMode::Normal if m.blend => 1,
                BlendMode::Normal => 0,
            };
            let rgb = m.color.map(hex_linear).unwrap_or(Vec3::ONE);
            p.color = rgb.extend(opacity);
            if m.fog == Fog::White {
                p.fog_color = Vec4::ONE;
            }
            p.fog.y = theme.fog_far * m.fog_multiplier as f32;
            let cull = if !m.culling { 0 } else if m.cw { 2 } else { 1 };
            let key = SubwayKey { shader: 0, blend, cull, depth_write: m.depth_mask, has_map: map.is_some(), fog: m.fog != Fog::Off, depth_test: true };
            (key, None)
        }
        // Ir({color 0x313131, displacementStrength 0.3}) + applyBlendMode(2)
        Shader::Water => {
            p.color = hex_raw(0x313131).extend(1.0);
            p.extra.z = 0.3;
            (SubwayKey { shader: 1, blend: 2, cull: 1, depth_write: false, has_map: true, fog: true, depth_test: true }, displacement)
        }
        // Vr({color 0xCEFDF6, foamColor 0x7CFAE9}): ShaderMaterial defaults (opaque, front side)
        Shader::Foam => {
            p.color = hex_raw(0xCEFDF6).extend(1.0);
            p.extra = hex_raw(0x7CFAE9).extend(0.0);
            p.extra2.x = 0.2;
            (SubwayKey { shader: 2, blend: 0, cull: 1, depth_write: true, has_map: true, fog: true, depth_test: true }, None)
        }
    };
    SubwayMaterial { params: p, map, displacement, key }
}

fn material_handle(cache: &mut RenderCache, assets: &AssetServer, mats: &mut Assets<SubwayMaterial>, theme: &ThemeConfig, m: &SimMaterial) -> Handle<SubwayMaterial> {
    let tex = m.map.as_deref().and_then(|map| image(cache, assets, theme, map));
    let disp = if m.shader == Shader::Water { image(cache, assets, theme, "wave_noise") } else { None };
    let mat = subway_material(theme, m, tex, disp);
    let key = MatKey {
        map: m.map.clone(),
        key: mat.key,
        opacity_bits: m.opacity.unwrap_or(1.0).to_bits(),
        color: m.color,
        fog_mult_bits: m.fog_multiplier.to_bits(),
    };
    if let Some(h) = cache.materials.get(&key) {
        return h.clone();
    }
    let h = mats.add(mat);
    cache.materials.insert(key, h.clone());
    h
}

/// Per-frame uniforms of the theme shaders, from the game's virtual clock:
/// water `Lr` scrolls map / displacement offsets by `now_s * speed / size`
/// (deobfuscated.js:7609), foam `Rr` sets `uTime = lastTime_s / 20` and the
/// shader scrolls by `fract(uTime * (0, -20))`.
fn animate_theme_materials(sim: Option<NonSend<Sim>>, mut mats: ResMut<Assets<SubwayMaterial>>) {
    let Some(sim) = sim else { return };
    let t = sim.game.clock.now / 1000.0;
    let fract = |x: f64| x - x.floor();
    let ids: Vec<AssetId<SubwayMaterial>> = mats.iter().filter(|(_, m)| m.key.shader != 0).map(|(id, _)| id).collect();
    for id in ids {
        let Some(mut m) = mats.get_mut(id) else { continue };
        match m.key.shader {
            1 => {
                let (map_size, disp_size) = (64.0, 32.0);
                let o = t * 3.0 / map_size;
                m.params.bend.z = o as f32;
                m.params.bend.w = o as f32;
                m.params.extra.x = (-3.0 * t / disp_size / disp_size) as f32;
                m.params.extra.y = (-4.0 * t / disp_size / disp_size) as f32;
            }
            2 => {
                let u_time = t / 20.0;
                m.params.bend.z = fract(u_time * 0.0) as f32;
                m.params.bend.w = fract(u_time * -20.0) as f32;
            }
            _ => {}
        }
    }
}

/// three generates mipmaps (`LinearMipmapLinearFilter`); Bevy's loader does
/// not. Build the chain on the CPU, averaging in linear space.
fn generate_mips(mut events: MessageReader<AssetEvent<Image>>, mut images: ResMut<Assets<Image>>, mut cache: ResMut<RenderCache>) {
    let ours: HashSet<AssetId<Image>> = cache.images.values().map(|h| h.id()).collect();
    for ev in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = ev else { continue };
        if !ours.contains(id) || cache.mipped.contains(id) {
            continue;
        }
        if let Some(mut img) = images.get_mut(*id) {
            build_mip_chain(&mut img);
        }
        cache.mipped.insert(*id);
    }
}

pub fn build_mip_chain(img: &mut Image) {
    let fmt = img.texture_descriptor.format;
    let srgb = match fmt {
        TextureFormat::Rgba8UnormSrgb => true,
        TextureFormat::Rgba8Unorm => false,
        _ => return,
    };
    if img.texture_descriptor.mip_level_count > 1 {
        return;
    }
    let Some(data) = img.data.as_ref() else { return };
    let (mut w, mut h) = (img.width() as usize, img.height() as usize);
    let lut: Vec<f32> = (0..256).map(|i| {
        let c = i as f32 / 255.0;
        if !srgb { c } else if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }).collect();
    let encode = |v: f32| -> u8 {
        let c = if !srgb { v } else if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    };
    let mut level: Vec<f32> = data.chunks(4).flat_map(|p| [lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize], p[3] as f32 / 255.0]).collect();
    let mut out = data.clone();
    let mut levels = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0f32; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut s = 0.0;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (2 * x + dx).min(w - 1);
                        let sy = (2 * y + dy).min(h - 1);
                        s += level[(sy * w + sx) * 4 + c];
                    }
                    next[(y * nw + x) * 4 + c] = s * 0.25;
                }
            }
        }
        out.extend(next.chunks(4).flat_map(|p| [encode(p[0]), encode(p[1]), encode(p[2]), (p[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8]));
        level = next;
        w = nw;
        h = nh;
        levels += 1;
    }
    img.data = Some(out);
    img.texture_descriptor.mip_level_count = levels;
}

/// Clear color = the theme's sky (stored encoded, see [`camera_bundle`]).
fn sync_sky(sim: Option<NonSend<Sim>>, mut cams: Query<&mut Camera, With<MainWorldCamera>>) {
    let Some(sim) = sim else { return };
    let [r, g, b] = sim.game.theme.sky_color;
    for mut c in &mut cams {
        c.clear_color = ClearColorConfig::Custom(Color::LinearRgba(LinearRgba::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)));
    }
}

/// three `PlaneGeometry(w, h)`: XY plane facing +z. Its top row has
/// v = 1, which with pixi's flipY = false samples the image's last row.
pub(crate) fn plane_mesh(w: f32, h: f32) -> Mesh {
    let (x, y) = (w * 0.5, h * 0.5);
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-x, y, 0.0], [x, y, 0.0], [-x, -y, 0.0], [x, -y, 0.0]]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]]);
    mesh.insert_indices(Indices::U32(vec![0, 2, 1, 2, 3, 1]));
    mesh
}

/// `H.plane(w, h, opacity, map, ADD)` (deobfuscated.js:4799): `$n` without
/// fog, blend on, plus the coin shine's `depthMask = false`.
fn plane_material(map: &str, opacity: f32) -> SimMaterial {
    SimMaterial {
        map: Some(map.to_string()),
        opacity: Some(opacity as f64),
        color: None,
        blend: true,
        blend_mode: BlendMode::Add,
        depth_mask: false,
        culling: true,
        cw: false,
        pk_material: None,
        fog: Fog::Off,
        fog_multiplier: 1.0,
        shader: Shader::Basic,
    }
}

fn sync_scene(
    mut commands: Commands,
    sim: Option<NonSend<Sim>>,
    mut cache: ResMut<RenderCache>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<SubwayMaterial>>,
    mut q: Query<&mut Transform>,
    blend: Option<Res<RenderBlend>>,
) {
    let Some(sim) = sim else { return };
    let g = &sim.game;
    let alpha = blend.map_or(1.0, |b| b.alpha);
    // between simulation frames only the interpolated transforms change
    if cache.last_frame == Some(g.frame) && cache.sim_serial == sim.serial && cache.theme == g.theme.id {
        for (mid, ent) in &cache.spawned {
            if let (Some((a, b)), Ok(mut t)) = (cache.blend.get(mid), q.get_mut(*ent)) {
                *t = blend_tf(a, b, alpha);
            }
        }
        return;
    }
    cache.last_frame = Some(g.frame);
    // a new simulation (restart, theme switch) has new objects, library and maps
    if cache.theme != g.theme.id || cache.sim_serial != sim.serial {
        for (_, ent) in cache.spawned.drain() {
            commands.entity(ent).despawn();
        }
        cache.blend.clear();
        if cache.theme != g.theme.id {
            cache.meshes.clear();
            cache.materials.clear();
            cache.images.clear();
            cache.mipped.clear();
        }
        cache.theme = g.theme.id.clone();
        cache.sim_serial = sim.serial;
    }
    let mut live: HashSet<ObjId> = HashSet::new();
    for e in g.ents.iter().filter(|e| e.in_scene) {
        for mid in g.scene.meshes(e.root) {
            if !g.scene.effectively_visible(mid) {
                continue;
            }
            let (mesh_key, material) = match &g.scene.get(mid).visual {
                Visual::Mesh { hash, material, .. } => (hash.clone(), material.clone()),
                Visual::Procedural { map: Some(map), plane: Some([w, h, opacity]), .. } => (format!("<plane {w}x{h}>"), plane_material(map, *opacity)),
                _ => continue,
            };
            live.insert(mid);
            let m = to_mat4(&g.scene.world_matrix(mid));
            let cur = Transform::from_matrix(m);
            let prev = cache.blend.get(&mid).map_or(cur, |&(_, c)| c);
            cache.blend.insert(mid, (prev, cur));
            let tf = blend_tf(&prev, &cur, alpha);
            if let Some(&ent) = cache.spawned.get(&mid) {
                if let Ok(mut t) = q.get_mut(ent) {
                    *t = tf;
                }
                continue;
            }
            let mesh = match cache.meshes.get(&mesh_key) {
                Some(h) => h.clone(),
                None => {
                    let built = match &g.scene.get(mid).visual {
                        Visual::Procedural { plane: Some([w, h, _]), .. } => plane_mesh(*w, *h),
                        _ => {
                            let Some(b) = build_mesh(&sim, &mesh_key) else { continue };
                            b
                        }
                    };
                    let h = meshes.add(built);
                    cache.meshes.insert(mesh_key.clone(), h.clone());
                    h
                }
            };
            let mat = material_handle(&mut cache, &assets, &mut mats, &g.theme, &material);
            // the bend moves geometry after projection: Bevy's (unbent)
            // frustum test would drop parts the bend brings into view
            let ent = commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), tf, bevy::camera::visibility::NoFrustumCulling)).id();
            cache.spawned.insert(mid, ent);
        }
    }
    let stale: Vec<ObjId> = cache.spawned.keys().filter(|k| !live.contains(k)).copied().collect();
    for k in stale {
        if let Some(ent) = cache.spawned.remove(&k) {
            commands.entity(ent).despawn();
        }
        cache.blend.remove(&k);
    }
}
