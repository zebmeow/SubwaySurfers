//! The menus' 3D views: one offscreen stage, three's thumb renderer
//! (`dv.renderThumb`, deobfuscated.js:57553), rendering whatever the
//! current screen shows into a 640 x 640 texture (`lv`) that the UI draws
//! as a sprite:
//!
//! | screen | subject | framing |
//! |---|---|---|
//! | New High Score (`Xv`, 61132) | the hero running `run_HighScore_main` at 0.35x | `Jv` |
//! | results notepad (`Wv` `_v`, 60778) | the hero idling (`popupIdle`) | `uv` |
//! | Me panel, characters (`hy` `_v`, 61724) | the focused character and outfit idling, turned by dragging | `uv` |
//! | Me panel, boards (`_v` board mode) | the selected character on the focused board: one trick, then `h_run` | `mv` |
//! | prize screen (`Dy`, 63632) | `mysteryBox_default` / `_super`: bobbing, then spinning away | `Ey` |
//!
//! The camera: fov 30, aspect 1, at `(cx + sin(yaw) cos(pitch) dist, cy +
//! sin(pitch) dist, cz + cos(yaw) cos(pitch) dist)` looking at (cx, cy, cz);
//! transparent clear, MSAA, no bend or fog. The characters stand on
//! `ensureFloorShadow` (`pv`). Characters are avatars posed through Jake's
//! skeleton and clips.
//!
//! Differences: the original's Me-panel idle is a "breathe" loop with
//! random gestures from per-character `idle-<id>.pk` files the captured build
//! does not contain (the offline original shows a broken pose); the port
//! loops `popupIdle` (the results' declared idle). Dragging the Me-panel
//! preview to turn it is a port addition. The prize's own model (`prizeThumb`)
//! stays the atlas icon.

use crate::anim::{Action, Animator};
use crate::render::{RenderBlend, RenderCache, SubwayMaterial};
use crate::render_actors::{build_board, build_hero_parts, build_prop_opts, ActiveCharacter, Part, Prop, Thumb};
use crate::sim_plugin::Sim;
use crate::skin::SkinModel;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::math::{DMat4, DQuat, DVec3, EulerRot};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::Arc;

/// `lv`: the thumb canvas size.
pub const SIZE: u32 = 640;
/// The render layer only the stage camera sees.
const LAYER: usize = 1;
/// three's far plane of every framing.
const FAR: f32 = 2.3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Framing {
    pub cx: f32,
    pub cy: f32,
    pub cz: f32,
    pub dist: f32,
    pub near: f32,
    pub yaw: f32,
    pub pitch: f32,
}

/// `Jv` (the high score run).
pub const JV: Framing = Framing { cx: 0.0, cy: 0.086, cz: 0.0, dist: 0.5, near: 0.028, yaw: 0.56, pitch: -0.05 };
/// `uv` (the default: idles).
pub const UV: Framing = Framing { cx: 0.0, cy: 0.086, cz: 0.0, dist: 0.426, near: 0.028, yaw: 0.0, pitch: 0.0 };
/// `mv` (board previews).
pub const MV: Framing = Framing { cx: 0.0, cy: 0.105, cz: 0.0, dist: 0.52, near: 0.028, yaw: 0.0, pitch: 0.26 };
/// `Ey` (the mystery box).
pub const EY: Framing = Framing { cx: 0.0, cy: 0.05, cz: 0.0, dist: 0.45, near: 0.028, yaw: 0.0, pitch: 0.74 };

/// `qv`: the high score run's speed.
const HS_SPEED: f64 = 0.35;
const HS_CLIP: &str = "run_HighScore_main";
const IDLE_CLIP: &str = "popupIdle";
const BOARD_RUN: &str = "h_run";
/// `gv`: the board preview's tricks.
const TRICKS: [&str; 5] = ["h_jump4_360_flip", "h_jump5_Impossible_flip", "h_jump11_fs_salto", "h_jump3_bs360grab", "h_jump10_heel360_flip"];

/// The texture: raw (encoded) values like the UI atlas, so the UI shows
/// it as drawn.
pub fn target_image() -> Image {
    let mut img = Image::new_target_texture(SIZE, SIZE, TextureFormat::Rgba8Unorm, None);
    img.sampler = bevy::image::ImageSampler::linear();
    img
}

/// GSAP 2's default ease (`Power1.easeOut`).
fn ease_out(p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    1.0 - (1.0 - p) * (1.0 - p)
}

/// A `.yoyo(true).repeat(-1)` tween from 0 to 1 over `dur`: forward, then
/// played backwards.
fn yoyo(t: f64, dur: f64) -> f64 {
    let n = (t / dur).floor();
    let p = t / dur - n;
    if n as i64 % 2 == 0 { ease_out(p) } else { ease_out(1.0 - p) }
}

/// The mystery box entity and its 2D shadow (`Q_.shadow`, `Z_`) at a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxPose {
    pub y: f64,
    pub rx: f64,
    pub ry: f64,
    pub rz: f64,
    pub scale: f64,
    /// The shadow sprite's scale relative to its `tween` start (448 x 224).
    pub shadow_k: f64,
    pub shadow_alpha: f64,
}

/// `Dy.tween(1)` from `setup` (ry 0.25, rx -0.56, rz -0.125, scale 0.008,
/// y 0.02 <-> 0.045 and the shadow 1 <-> 0.5 over 1.5 s), then `tween(2)`
/// `opened` seconds after the press (from where the bob was, y -> 0.04 in
/// 0.3 s; ry += 5 pi, scale -> 0, the shadow -> 0 and its alpha 0.3 -> 0 in
/// 0.5 s; the shadow restarts at full size). `since`: seconds since `setup`.
pub fn box_pose(since: f64, opened: Option<f64>) -> BoxPose {
    let bob = |t: f64| yoyo(t, 1.5);
    match opened {
        None => {
            let k = bob(since);
            BoxPose { y: 0.02 + 0.025 * k, rx: -0.56, ry: 0.25, rz: -0.125, scale: 0.008, shadow_k: 1.0 - 0.5 * k, shadow_alpha: 0.3 }
        }
        Some(t) => {
            let k0 = bob((since - t).max(0.0));
            // `tween()` resets the shadow's size first; the entity keeps its y
            let y0 = 0.02 + 0.025 * k0;
            let (a, b) = (ease_out(t / 0.3), ease_out(t / 0.5));
            BoxPose {
                y: y0 + (0.04 - y0) * a,
                rx: -0.56,
                ry: 0.25 + 5.0 * PI * b,
                rz: -0.125,
                scale: 0.008 * (1.0 - b),
                shadow_k: 1.0 - b,
                shadow_alpha: 0.3 * (1.0 - b),
            }
        }
    }
}

/// `Ly` / `Iy`: the prize kinds' models (and texture).
pub fn prize_model(kind: &str) -> (&'static str, Option<&'static str>) {
    match kind {
        "coins" => ("currency_coin", None),
        "keys" => ("currency_key", None),
        "hoverboard" => ("board_default_base", Some("board-hoverboard-tex")),
        "headstart" => ("headstartToken", None),
        _ => ("ScoreBooster", None),
    }
}

/// The library entity's own turn inside `Fy`'s group: the board's
/// rx = pi / 2.35, ry = pi / 2.
fn prize_inner(kind: &str) -> DMat4 {
    if kind == "hoverboard" {
        DMat4::from_quat(DQuat::from_euler(EulerRot::XYZ, PI / 2.35, PI / 2.0, 0.0))
    } else {
        DMat4::IDENTITY
    }
}

/// `Fy.measureBounds` (64300): the prize's first render's opaque pixels
/// (top, bottom, left, right in the 640 texture), by projecting the model
/// through the `uv` camera.
pub static PRIZE_BOUNDS: std::sync::Mutex<Option<(String, [f32; 4])>> = std::sync::Mutex::new(None);

/// The measured bounds of `kind`'s prize, if rendered.
pub fn prize_bounds(kind: &str) -> Option<[f32; 4]> {
    PRIZE_BOUNDS.lock().ok()?.as_ref().filter(|(k, _)| k == kind).map(|(_, b)| *b)
}

fn project_bounds(parts: &[Prop], at: DMat4, meshes: &Assets<Mesh>, mesh3d: &Query<&Mesh3d>) -> Option<[f32; 4]> {
    let view = camera_pose(&UV).to_matrix().inverse();
    let proj = Mat4::perspective_rh(30f32.to_radians(), 1.0, UV.near, FAR);
    let vp = proj * view;
    let (mut top, mut bottom, mut left, mut right) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    let mut any = false;
    for p in parts {
        let mesh = meshes.get(&mesh3d.get(p.entity).ok()?.0)?;
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { continue };
        let m = (at * p.local).as_mat4();
        for v in pos {
            let c = vp * (m * Vec3::from(*v).extend(1.0));
            if c.w <= 0.0 {
                continue;
            }
            let (x, y) = ((c.x / c.w * 0.5 + 0.5) * SIZE as f32, (0.5 - c.y / c.w * 0.5) * SIZE as f32);
            top = top.min(y);
            bottom = bottom.max(y);
            left = left.min(x);
            right = right.max(x);
            any = true;
        }
    }
    any.then_some([top, bottom, left, right])
}

/// What the stage shows this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Subject {
    /// A character (`id`, outfit) playing `motion`, optionally on a board.
    Character { id: String, outfit: usize, motion: Motion, framing: Framing, board: Option<(String, Vec<usize>)> },
    /// `Dy` with `mysteryBox_default` / `mysteryBox_super`; `opened`: seconds
    /// since the open press (`tween(2)`), else bobbing (`tween(1)`).
    MysteryBox { super_box: bool, since: f64, opened: Option<f64> },
    /// `Fy` (the prize, 64547): `kind`'s model `t` seconds into its tween
    /// (0.5 s after the open press, when the box has shrunk away).
    Prize { kind: String, t: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    /// The high score run: `ticks` screen updates.
    HighScore { ticks: f64 },
    Idle,
    /// The board preview: a trick, then `h_run`.
    Board,
}

/// The Me panel preview's yaw (dragging it turns the character; a port
/// addition) and whether the pointer is dragging.
#[derive(Resource, Default)]
pub struct PreviewSpin {
    pub yaw: f64,
    dragging: bool,
    last_x: f32,
    for_key: String,
}

/// Which subject the current screen shows (None: the stage is off).
pub fn subject(g: &crate::game::Game, hero: &ActiveCharacter) -> Option<Subject> {
    use crate::flow::Screen;
    use crate::menu::{Menu, MeTab, Overlay};
    let hero_char = |motion: Motion, framing: Framing| Subject::Character { id: hero.id.clone(), outfit: hero.outfit, motion, framing, board: None };
    // the prize overlay (shop) or screen (after a run)
    let prize = match (&g.flow.overlay, &g.flow.screen) {
        (Some(Overlay::Prize(p)), _) | (_, Screen::Prize(p)) => Some(p),
        _ => None,
    };
    if let Some(p) = prize {
        let opened = p.opened.map(|o| (p.t - o) as f64 / 60.0);
        // `af.delayedCall(0.5, () => prizeThumb.tween())`
        if let Some(t) = opened.filter(|t| *t >= 0.5) {
            return Some(Subject::Prize { kind: p.current.kind.clone(), t: t - 0.5 });
        }
        let bt = p.current.box_type.as_deref()?;
        let super_box = !(bt == "mystery-box" || bt == "mini-mystery-box");
        return Some(Subject::MysteryBox { super_box, since: p.t as f64 / 60.0, opened });
    }
    match &g.flow.screen {
        Screen::HighScore { .. } => return Some(hero_char(Motion::HighScore { ticks: g.flow.celebration.ticks as f64 }, JV)),
        Screen::Results { .. } => {
            // `$.user.character`: the saved selection
            let u = &g.flow.user;
            return Some(Subject::Character { id: u.selected_character.clone(), outfit: u.selected_outfit, motion: Motion::Idle, framing: UV, board: None });
        }
        _ => {}
    }
    if g.flow.overlay.is_some() {
        return None;
    }
    if let Menu::Me(m) = &g.flow.menu {
        // the Awards section has no preview
        if m.awards {
            return None;
        }
        let u = &g.flow.user;
        return Some(match m.tab {
            MeTab::Characters => {
                let id = crate::menu::characters().get(m.character).cloned().unwrap_or_else(|| "jake".into());
                Subject::Character { id, outfit: m.outfit, motion: Motion::Idle, framing: UV, board: None }
            }
            MeTab::Boards => {
                let board = crate::shop::Catalog::get().boards.get(m.board).map(|b| b.id.clone()).unwrap_or_else(|| "hoverboard".into());
                Subject::Character {
                    id: u.selected_character.clone(),
                    outfit: u.selected_outfit,
                    motion: Motion::Board,
                    framing: MV,
                    board: Some((board, m.board_preview.clone())),
                }
            }
        });
    }
    None
}

enum Model {
    Avatar { model: Arc<SkinModel>, parts: Vec<Part> },
    Box { parts: Vec<Prop> },
}

#[derive(Resource, Default)]
pub struct MenuStage {
    camera: Option<Entity>,
    shadow: Option<Entity>,
    /// The subject's identity the parts were built for (and the sim serial).
    key: Option<String>,
    model: Option<Model>,
    board: Vec<Prop>,
    /// Seconds since the subject started (idle and board clips).
    t: f64,
    /// The board preview's trick (chosen when it starts).
    trick: usize,
    /// The prize's bounds are measured (once per model).
    measured: bool,
    /// The board preview's first-frame hips (x, z), centred over the shadow.
    centre: Option<DVec3>,
    avatars: HashMap<String, Arc<SkinModel>>,
}

/// The stage camera (order -1, its image only, layer 1). Never a
/// `MainWorldCamera`.
#[derive(Component)]
pub struct StageCamera;

/// Jake's animator for the stage's clips (`Rc`s: main thread only).
#[derive(Default)]
pub struct StageAnimator(Option<Animator>);

pub struct MenuStagePlugin;

impl Plugin for MenuStagePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuStage>()
            .init_resource::<PreviewSpin>()
            .insert_non_send(StageAnimator::default())
            .add_systems(Update, drag_preview)
            .add_systems(PostUpdate, sync_stage);
    }
}

/// `Z_()`: black, alpha 0.85 -> 0.45 (at 0.5) -> 0, radius 64 of 128.
pub fn blob_image() -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension};
    let n = 128usize;
    let mut px = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - 64.0, y as f32 + 0.5 - 64.0);
            let r = (dx * dx + dy * dy).sqrt() / 64.0;
            let a = if r <= 0.5 { 0.85 - 0.4 * r / 0.5 } else if r < 1.0 { 0.45 * (1.0 - (r - 0.5) / 0.5) } else { 0.0 };
            px[(y * n + x) * 4 + 3] = (a * 255.0).round() as u8;
        }
    }
    let mut img = Image::new(Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8Unorm, RenderAssetUsages::default());
    img.sampler = bevy::image::ImageSampler::linear();
    img
}

/// `ensureFloorShadow`'s 128 x 128 canvas: black, alpha 1 -> 0.55 (at 0.6) -> 0.
fn shadow_image() -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension};
    let n = 128usize;
    let mut px = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - 64.0, y as f32 + 0.5 - 64.0);
            let r = (dx * dx + dy * dy).sqrt() / 64.0;
            let a = if r <= 0.6 { 1.0 - (1.0 - 0.55) * r / 0.6 } else if r < 1.0 { 0.55 * (1.0 - (r - 0.6) / 0.4) } else { 0.0 };
            px[(y * n + x) * 4 + 3] = (a * 255.0).round() as u8;
        }
    }
    let mut img = Image::new(Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8Unorm, RenderAssetUsages::default());
    img.sampler = bevy::image::ImageSampler::linear();
    img
}

fn camera_pose(f: &Framing) -> Transform {
    let d = f.pitch.cos() * f.dist;
    let pos = Vec3::new(f.cx + f.yaw.sin() * d, f.cy + f.pitch.sin() * f.dist, f.cz + f.yaw.cos() * d);
    Transform::from_translation(pos).looking_at(Vec3::new(f.cx, f.cy, f.cz), Vec3::Y)
}

fn spawn_camera(commands: &mut Commands, target: Handle<Image>) -> Entity {
    use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
    commands
        .spawn((
            StageCamera,
            Camera3d::default(),
            Camera { order: -1, is_active: false, clear_color: ClearColorConfig::Custom(Color::LinearRgba(LinearRgba::NONE)), ..default() },
            RenderTarget::Image(target.into()),
            Tonemapping::None,
            DebandDither::Disabled,
            Msaa::Sample4,
            Projection::Perspective(PerspectiveProjection { fov: 30f32.to_radians(), aspect_ratio: 1.0, near: JV.near, far: FAR, ..default() }),
            camera_pose(&JV),
            RenderLayers::layer(LAYER),
        ))
        .id()
}

fn spawn_shadow(commands: &mut Commands, sim: &Sim, meshes: &mut Assets<Mesh>, mats: &mut Assets<SubwayMaterial>, images: &mut Assets<Image>) -> Entity {
    use crate::scene::{BlendMode, Fog, Material as SimMaterial, Shader};
    let m = SimMaterial {
        map: Some("hs-shadow".into()),
        opacity: Some(0.4),
        color: None,
        blend: true,
        blend_mode: BlendMode::Normal,
        depth_mask: false,
        culling: false,
        cw: false,
        pk_material: None,
        fog: Fog::Off,
        fog_multiplier: 1.0,
        shader: Shader::Basic,
    };
    let tex = images.add(shadow_image());
    let mat = mats.add(crate::render::thumb_material(crate::render::subway_material(&sim.game.theme, &m, Some(tex), None), FAR));
    // PlaneGeometry(0.07, 0.05) turned flat (rotation.x = -pi/2)
    let (hx, hz) = (0.035f32, 0.025f32);
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-hx, 0.0, -hz], [hx, 0.0, -hz], [hx, 0.0, hz], [-hx, 0.0, hz]]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    mesh.insert_indices(Indices::U32(vec![0, 2, 1, 0, 3, 2]));
    commands
        .spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(mat),
            Transform::from_xyz(0.004, 0.002, 0.012),
            Visibility::Hidden,
            RenderLayers::layer(LAYER),
            bevy::camera::visibility::NoFrustumCulling,
        ))
        .id()
}

fn despawn(commands: &mut Commands, m: Model) {
    match m {
        Model::Avatar { parts, .. } => parts.into_iter().for_each(|p| commands.entity(p.entity).despawn()),
        Model::Box { parts } => parts.into_iter().for_each(|p| commands.entity(p.entity).despawn()),
    }
}

/// The high score clip time after `ticks` screen updates.
pub fn clip_time(ticks: f64) -> f64 {
    0.001 + ticks / 60.0 * HS_SPEED
}

fn subject_key(s: &Subject) -> String {
    match s {
        Subject::Character { id, outfit, motion, board, .. } => {
            let m = match motion {
                Motion::HighScore { .. } => "hs",
                Motion::Idle => "idle",
                Motion::Board => "board",
            };
            format!("char:{id}:{outfit}:{m}:{board:?}")
        }
        Subject::MysteryBox { super_box, .. } => format!("box:{super_box}"),
        Subject::Prize { kind, .. } => format!("prize:{kind}"),
    }
}

/// The Me panel preview's drag: horizontal pointer motion over the
/// preview's area turns the character (0.01 rad per px).
fn drag_preview(
    sim: Option<NonSend<Sim>>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    mut spin: ResMut<PreviewSpin>,
) {
    let (Some(sim), Ok(win)) = (sim, windows.single()) else { return };
    let crate::menu::Menu::Me(m) = &sim.game.flow.menu else {
        spin.dragging = false;
        return;
    };
    let key = format!("{:?}:{}:{}", m.tab, m.character, m.board);
    if spin.for_key != key {
        spin.for_key = key;
        spin.yaw = 0.0;
    }
    let Some(p) = win.cursor_position() else { return };
    // the preview: x -170 +- 150, y -310 .. 75 from the centre (units of
    // h / 1000 at s = h / 500 * 0.5)
    let s = win.height() / 1000.0;
    let (ux, uy) = ((p.x - win.width() / 2.0) / s, (p.y - win.height() / 2.0) / s);
    let inside = (-320.0..-20.0).contains(&ux) && (-310.0..75.0).contains(&uy);
    if buttons.just_pressed(MouseButton::Left) && inside {
        spin.dragging = true;
        spin.last_x = p.x;
    }
    if !buttons.pressed(MouseButton::Left) {
        spin.dragging = false;
    }
    if spin.dragging {
        spin.yaw += ((p.x - spin.last_x) * 0.01) as f64;
        spin.last_x = p.x;
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_stage(
    mut commands: Commands,
    sim: Option<NonSend<Sim>>,
    mut anim: NonSendMut<StageAnimator>,
    mut state: ResMut<MenuStage>,
    character: Option<Res<ActiveCharacter>>,
    spin: Res<PreviewSpin>,
    ui: Option<Res<crate::ui::UiAssets>>,
    (blend, time): (Option<Res<RenderBlend>>, Res<Time>),
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<SubwayMaterial>>,
    mut image_assets: ResMut<Assets<Image>>,
    mut cache: ResMut<RenderCache>,
    asset_server: Res<AssetServer>,
    mut cams: Query<(&mut Camera, &mut Transform), With<StageCamera>>,
    (mut vis, mesh3d): (Query<&mut Visibility>, Query<&Mesh3d>),
) {
    let (Some(sim), Some(ui)) = (sim, ui) else { return };
    let g = &sim.game;
    let hero = character.map(|c| c.clone()).unwrap_or_default();
    let subj = subject(g, &hero);
    let cam = *state.camera.get_or_insert_with(|| spawn_camera(&mut commands, ui.hs_view.clone()));
    let framing = match &subj {
        Some(Subject::Character { framing, .. }) => *framing,
        Some(Subject::MysteryBox { .. }) => EY,
        // `Fy` registers no framing: `uv`
        Some(Subject::Prize { .. }) => UV,
        None => JV,
    };
    if let Ok((mut c, mut tf)) = cams.get_mut(cam) {
        if c.is_active != subj.is_some() {
            c.is_active = subj.is_some();
        }
        let want = camera_pose(&framing);
        if *tf != want {
            *tf = want;
        }
    }
    let set = |vis: &mut Query<&mut Visibility>, e: Entity, show: bool| {
        if let Ok(mut v) = vis.get_mut(e) {
            let want = if show { Visibility::Visible } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
    };
    let Some(subj) = subj else { return };
    let Some(assets) = g.actors.clone() else { return };
    let is_char = matches!(subj, Subject::Character { .. });
    let shadow = *state.shadow.get_or_insert_with(|| spawn_shadow(&mut commands, &sim, &mut meshes, &mut mats, &mut image_assets));
    set(&mut vis, shadow, is_char);

    // (re)build for a new subject
    let key = format!("{}:{}", subject_key(&subj), sim.serial);
    if state.key.as_deref() != Some(key.as_str()) {
        if let Some(m) = state.model.take() {
            despawn(&mut commands, m);
        }
        for p in state.board.drain(..) {
            commands.entity(p.entity).despawn();
        }
        state.t = 0.0;
        state.trick = (time.elapsed_secs_f64() * 1000.0) as usize % TRICKS.len();
        state.centre = None;
        let thumb = Some(Thumb { far: FAR, layer: LAYER });
        let theme = g.theme.clone();
        match &subj {
            Subject::Character { id, outfit, board, .. } => {
                state.model = Some({
                    let (aid, outfit) = (id.clone(), *outfit);
                    let model = if aid == "jake" {
                        Arc::new(assets.jake.clone())
                    } else if let Some(m) = state.avatars.get(&aid) {
                        m.clone()
                    } else {
                        let path = crate::install::Install::locate().site.join(format!("assets/characters-idle/avatar_{aid}.pk"));
                        match SkinModel::load(&path, 0.01) {
                            Ok(m) => {
                                let m = Arc::new(m);
                                state.avatars.insert(aid.clone(), m.clone());
                                m
                            }
                            Err(e) => {
                                error!("[Menu3D] {aid}: {e} (drawing jake)");
                                Arc::new(assets.jake.clone())
                            }
                        }
                    };
                    let mut images = |map: &str| cache.image(&asset_server, &theme, map);
                    let parts = build_hero_parts(&mut commands, &sim, &model, &aid, outfit, &mut meshes, &mut mats, &mut images, thumb);
                    Model::Avatar { model, parts }
                });
                if let Some((bid, powers)) = board {
                    let mut images = |map: &str| cache.image(&asset_server, &theme, map);
                    state.board = build_board(&mut commands, &sim, bid, powers, &mut meshes, &mut mats, &mut images, thumb);
                }
            }
            Subject::MysteryBox { super_box, .. } => {
                let name = if *super_box { "mysteryBox_super" } else { "mysteryBox_default" };
                let mut images = |map: &str| cache.image(&asset_server, &theme, map);
                let (parts, _) = build_prop_opts(&mut commands, &sim, name, &crate::scene::MatOpts::default(), &mut meshes, &mut mats, &mut images, &mut |m| {
                    *m = crate::render::thumb_material(m.clone(), FAR)
                });
                for p in &parts {
                    crate::render_actors::thumb_layer(&mut commands, p.entity, thumb);
                }
                state.model = Some(Model::Box { parts });
            }
            Subject::Prize { kind, .. } => {
                let (group, map) = prize_model(kind);
                let mut images = |map: &str| cache.image(&asset_server, &theme, map);
                let opts = crate::scene::MatOpts { map: map.map(str::to_string), ..Default::default() };
                let (parts, _) = build_prop_opts(&mut commands, &sim, group, &opts, &mut meshes, &mut mats, &mut images, &mut |m| *m = crate::render::thumb_material(m.clone(), FAR));
                for p in &parts {
                    crate::render_actors::thumb_layer(&mut commands, p.entity, thumb);
                }
                state.model = Some(Model::Box { parts });
                state.measured = false;
            }
        }
        state.key = Some(key);
        // spawned this frame: shown and posed from the next
        return;
    }
    state.t += time.delta_secs_f64();

    match &subj {
        Subject::Prize { kind, t } => {
            let inner = prize_inner(kind);
            // `measureBounds` of the first render (x0.007, unturned)
            if !state.measured {
                let at = DMat4::from_scale(DVec3::splat(0.007)) * inner;
                let b = match &state.model {
                    Some(Model::Box { parts }) => project_bounds(parts, at, &meshes, &mesh3d),
                    _ => None,
                };
                if let Some(b) = b {
                    *PRIZE_BOUNDS.lock().unwrap() = Some((kind.clone(), b));
                    state.measured = true;
                }
            }
            let Some(Model::Box { parts }) = &state.model else { return };
            // `Fy.tween`: scale 0 -> 0.007 in 2 s, ry -= 4 pi in 4 s
            let k = 0.007 * ease_out(t / 2.0);
            let ry = -4.0 * PI * ease_out(t / 4.0);
            let m = DMat4::from_rotation_y(ry) * DMat4::from_scale(DVec3::splat(k)) * inner;
            for p in parts {
                set(&mut vis, p.entity, k > 0.0);
                commands.entity(p.entity).insert(Transform::from_matrix((m * p.local).as_mat4()));
            }
            return;
        }
        Subject::MysteryBox { since, opened, .. } => {
            let BoxPose { y, rx, ry, rz, scale, .. } = box_pose(*since, *opened);
            let rot = (ry, rx, rz);
            let m = DMat4::from_translation(DVec3::new(0.0, y, 0.0)) * DMat4::from_quat(DQuat::from_euler(EulerRot::XYZ, rot.1, rot.0, rot.2)) * DMat4::from_scale(DVec3::splat(scale));
            if let Some(Model::Box { parts }) = &state.model {
                for p in parts {
                    set(&mut vis, p.entity, scale > 0.0);
                    commands.entity(p.entity).insert(Transform::from_matrix((m * p.local).as_mat4()));
                }
            }
            return;
        }
        Subject::Character { motion, board, .. } => {
            // the clip and its time
            let animator = anim.0.get_or_insert_with(|| Animator::new(assets.jake_clips.clone(), &assets.jake, true, "jake-"));
            let (clip, t) = match motion {
                Motion::HighScore { ticks } => (HS_CLIP, clip_time((ticks + blend.as_ref().map_or(1.0, |b| b.alpha as f64) - 1.0).max(0.0))),
                Motion::Idle => (IDLE_CLIP, state.t),
                Motion::Board => {
                    let trick = TRICKS[state.trick];
                    let dur = animator.clip_index(trick).map_or(0.0, |c| animator.lib.clips[c].duration);
                    if state.t < dur { (trick, state.t) } else { (BOARD_RUN, state.t - dur) }
                }
            };
            let Some(c) = animator.clip_index(clip) else { return };
            let dur = animator.lib.clips[c].duration.max(1e-6);
            let mut a = Action::new_public(c);
            a.time = t.rem_euclid(dur);
            animator.actions.clear();
            animator.actions.insert(c, a);
            animator.active = vec![c];
            // The original's idle and board scenes face the camera and stand on
            // the shadow; the port poses the run skeleton (the idle frames
            // face -z and drift away from the origin): idles turned by pi and
            // kept with the hips over the shadow, the board preview centred on
            // its first frame. Plus the Me panel's turn (port addition).
            let root = if matches!(motion, Motion::HighScore { .. }) {
                DMat4::IDENTITY
            } else {
                let jake = &assets.jake;
                let hips = |animator: &mut Animator| {
                    let (bone_locals, jake_nodes) = animator.pose(jake);
                    let b = jake.bone_worlds_posed(DMat4::IDENTITY, &jake_nodes, &bone_locals);
                    b.first().map_or(DVec3::ZERO, |m| DVec3::new(m.w_axis.x, 0.0, m.w_axis.z))
                };
                let (c, turn) = if matches!(motion, Motion::Idle) {
                    (hips(animator), PI)
                } else {
                    (*state.centre.get_or_insert_with(|| hips(animator)), 0.0)
                };
                DMat4::from_rotation_y(turn + spin.yaw) * DMat4::from_translation(-c)
            };
            let mut attach: Option<DMat4> = None;
            match &state.model {
                Some(Model::Avatar { model, parts }) => {
                    let jake = &assets.jake;
                    let (bone_locals, jake_nodes) = animator.pose(jake);
                    let node_locals: Vec<crate::skin::Trs> =
                        model.nodes.iter().map(|n| jake.node_index(&n.name).map(|j| jake_nodes[j]).unwrap_or(n.local)).collect();
                    let bones = model.bone_worlds_posed(root, &node_locals, &bone_locals);
                    for p in parts {
                        set(&mut vis, p.entity, true);
                        let m = &model.meshes[p.mesh_index];
                        let pos = model.skin(p.mesh_index, &bones, &m.default_influences);
                        if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
                            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
                        }
                    }
                    attach = model.node_index("attachPoint1").map(|n| model.node_world_posed(root, n, &node_locals));
                }
                _ => {}
            }
            // the board on attachPoint1, rx -pi/2 (`ev`)
            if board.is_some() {
                if let Some(ap) = attach {
                    let view = ap * DMat4::from_rotation_x(-PI / 2.0);
                    for p in &state.board {
                        set(&mut vis, p.entity, true);
                        commands.entity(p.entity).insert(Transform::from_matrix((view * p.local).as_mat4()));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_bobs_then_spins_away() {
        // tween(1): y 0.02 -> 0.045 in 1.5 s, back by 3 s; the shadow halves
        let p = box_pose(0.0, None);
        assert_eq!((p.y, p.scale, p.shadow_k), (0.02, 0.008, 1.0));
        let p = box_pose(1.5, None);
        assert!((p.y - 0.045).abs() < 1e-9 && (p.shadow_k - 0.5).abs() < 1e-9);
        assert!((box_pose(3.0, None).y - 0.02).abs() < 1e-9);
        // tween(2) from mid-bob: continuous y, the shadow restarts at full size
        let since = 2.0;
        let (a, b) = (box_pose(since, None), box_pose(since, Some(0.0)));
        assert!((a.y - b.y).abs() < 1e-9 && b.shadow_k == 1.0);
        let end = box_pose(since + 0.6, Some(0.6));
        assert_eq!((end.y, end.scale, end.shadow_k, end.shadow_alpha), (0.04, 0.0, 0.0, 0.0));
        assert!((end.ry - (0.25 + 5.0 * PI)).abs() < 1e-9);
    }
}
