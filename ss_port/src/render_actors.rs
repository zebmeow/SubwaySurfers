//! Drawing the characters: the hero (any catalog avatar and outfit) with
//! its powerup props, the guard and the dog.
//!
//! Each frame the simulation's animators give the bone pose, the bones are
//! placed as in the original's entity tree, and every visible skinned mesh
//! is skinned on the CPU into world space (`crate::skin`). The meshes use the
//! world's [`SubwayMaterial`] with the characters' state: unlit, sRGB map,
//! clip-space bend, **no fog** (`unlit-high:bend:nofog`), opaque, Jake
//! double-sided (`Wo`), the guard and dog back-face culled
//! (docs/js_notes/actors_character_model.md §3, §4).
//!
//! Placement (§0.4, §4): hero entity at the body's render position, rotated
//! by the lane lean `ry` -> model (y -4.5, ry PI; not raised during rolls)
//! -> container x100 -> the file's nodes. Guard at its body center ->
//! container (y -5.6, ry PI, x100); the dog is a child of the guard at
//! (5, 0, -6) with its own scale.

use crate::anim::{Animator, Face, EYE_BLINK_UV, EYE_CLAMP, EYE_GAIN};
use crate::render::{subway_material, SubwayMaterial};
use crate::scene::{BlendMode, Fog, Material as SimMaterial, Shader};
use crate::sim_plugin::Sim;
use crate::skin::SkinModel;
use bevy::asset::RenderAssetUsages;
use bevy::math::{DMat4, DQuat, DVec3};
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Actor {
    Hero,
    Guard,
    Dog,
}

pub(crate) struct Part {
    actor: Actor,
    pub(crate) mesh_index: usize,
    pub(crate) mesh: Handle<Mesh>,
    pub(crate) entity: Entity,
    base_uvs: Vec<[f32; 2]>,
    /// Blink morph target (influence 0 driven by the face).
    blink: bool,
    /// Eye mesh (UVs rewritten by the face).
    eye: bool,
}

/// A prop mesh attached to the hero (`Kf.show`: the pogo stick on
/// `attachPoint1`): its local matrix under the view and its Bevy entity.
pub(crate) struct Prop {
    pub(crate) local: DMat4,
    pub(crate) entity: Entity,
}

/// The `superSneakers` skinned mesh re-bound to the hero skeleton (`sp`).
struct SneakersPart {
    mesh: crate::skin::SkinMesh,
    handle: Handle<Mesh>,
    entity: Entity,
}

#[derive(Resource, Default)]
pub struct ActorRender {
    sim_serial: u64,
    /// Guard and dog parts (built per simulation).
    parts: Vec<Part>,
    /// The hero avatar's parts and the (id, outfit) they were built for.
    hero_parts: Vec<Part>,
    hero_key: Option<(String, usize)>,
    /// Loaded avatars by character id (Jake comes from the simulation).
    avatars: HashMap<String, Arc<SkinModel>>,
    pogo: Vec<Prop>,
    jetpack: Vec<Prop>,
    magnet: Vec<Prop>,
    /// The dizzy stars `$o` (two `Dizzytrail`s, two `Dizzystar`s, with
    /// their place in the view).
    dizzy: Vec<(Prop, DMat4)>,
    /// The spray can (`sprayCan`) on `attachPoint2` in the paint idle.
    spray_can: Vec<Prop>,
    /// `crate::hero_fx`: the blob shadow, the coin pop (`star7`), the pickup
    /// pop (`pow`) and the revive halo (`powRevive`, with its materials for
    /// the animated opacity).
    shadow: Option<Entity>,
    pop: Vec<Prop>,
    pop_pickup: Vec<Prop>,
    halo: Vec<Prop>,
    halo_mats: Vec<Handle<SubwayMaterial>>,
    /// The selected board's visible parts and the (board, powers) key.
    board: Vec<Prop>,
    board_key: String,
    sneakers: Option<SneakersPart>,
    /// Skinned positions and prop transforms at the previous / latest
    /// simulation frame (render frames in between draw the blend).
    pos_blend: HashMap<AssetId<Mesh>, (Vec<[f32; 3]>, Vec<[f32; 3]>, Handle<Mesh>)>,
    tf_blend: HashMap<Entity, (Transform, Transform)>,
    last_frame: Option<i64>,
    /// Full passes still owed after spawning parts: their visibility and
    /// placement can only be set once the spawn commands have applied (a
    /// screenshot holds on one simulation frame, so the next sim frame may
    /// never come).
    full_passes: u8,
}

/// Every selectable character: the original Me-panel roster.
pub fn character_roster() -> Vec<String> {
    crate::shop::Catalog::get().roster.clone()
}

/// Which model draws the hero: a catalog character and outfit. Only the
/// renderer reads it: the simulation (and every parity trace) is the same
/// for all characters.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ActiveCharacter {
    pub id: String,
    pub outfit: usize,
}

impl Default for ActiveCharacter {
    fn default() -> Self {
        Self { id: "jake".into(), outfit: 0 }
    }
}

impl ActiveCharacter {
    pub fn new(id: &str, outfit: usize) -> Self {
        Self { id: id.to_string(), outfit }
    }
    pub fn name(&self) -> &str {
        &self.id
    }
    /// `--character` names: a roster id (any case), `<id>:<outfit>` for an outfit.
    pub fn from_name(s: &str) -> Option<Self> {
        let (name, outfit) = match s.split_once(':') {
            Some((n, k)) => (n, k.parse().ok()?),
            None => (s, 0),
        };
        character_roster().into_iter().find(|c| c.eq_ignore_ascii_case(name)).map(|c| Self::new(&c, outfit))
    }
}

/// Follow the saved selection (Me panel) when it changes; the startup
/// choice (`--character`) and KeyC stand until then.
pub fn sync_selected_character(sim: Option<NonSend<Sim>>, mut active: ResMut<ActiveCharacter>, mut last: Local<Option<(String, usize)>>) {
    let Some(sim) = sim else { return };
    let u = &sim.game.flow.user;
    let now = (u.selected_character.clone(), u.selected_outfit);
    match &*last {
        None => *last = Some(now),
        Some(prev) if *prev != now => {
            let mut c = ActiveCharacter::from_name(&now.0).unwrap_or_default();
            c.outfit = now.1;
            info!("[Character] Selected {} (outfit {})", c.name(), c.outfit);
            *active = c;
            *last = Some(now);
        }
        _ => {}
    }
}

/// Offscreen (thumb) parts: `thumb_material` with three's far plane, on
/// render layer `layer` only.
#[derive(Clone, Copy)]
pub(crate) struct Thumb {
    pub(crate) far: f32,
    pub(crate) layer: usize,
}

fn thumb_mat(mats: &mut Assets<SubwayMaterial>, m: SubwayMaterial, thumb: Option<Thumb>) -> Handle<SubwayMaterial> {
    mats.add(match thumb {
        Some(t) => crate::render::thumb_material(m, t.far),
        None => m,
    })
}

pub(crate) fn thumb_layer(commands: &mut Commands, e: Entity, thumb: Option<Thumb>) {
    if let Some(t) = thumb {
        commands.entity(e).insert(bevy::camera::visibility::RenderLayers::layer(t.layer));
    }
}

fn character_material(map: &str, double_sided: bool) -> SimMaterial {
    SimMaterial {
        map: Some(map.to_string()),
        opacity: None,
        color: None,
        blend: false,
        blend_mode: BlendMode::Normal,
        depth_mask: true,
        culling: !double_sided,
        cw: false,
        pk_material: None,
        fog: Fog::Off,
        fog_multiplier: 1.0,
        shader: Shader::Basic,
    }
}

fn spawn_skinned(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mat: &Handle<SubwayMaterial>,
    m: &crate::skin::SkinMesh,
) -> (Handle<Mesh>, Entity) {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone());
    mesh.insert_indices(Indices::U32(m.indices.clone()));
    let handle = meshes.add(mesh);
    let entity = commands
        .spawn((Mesh3d(handle.clone()), MeshMaterial3d(mat.clone()), Transform::IDENTITY, Visibility::Hidden, bevy::camera::visibility::NoFrustumCulling))
        .id();
    (handle, entity)
}

/// The guard and the dog.
fn build_parts(
    commands: &mut Commands,
    sim: &Sim,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<SubwayMaterial>,
    images: &mut impl FnMut(&str) -> Option<Handle<Image>>,
) -> Vec<Part> {
    let Some(assets) = sim.game.actors.clone() else { return Vec::new() };
    let theme = sim.game.theme.clone();
    let mut parts = Vec::new();
    for (actor, model) in [(Actor::Guard, &assets.guard), (Actor::Dog, &assets.dog)] {
        let mat = mats.add(subway_material(&theme, &character_material("enemies", false), images("enemies"), None));
        for (mi, m) in model.meshes.iter().enumerate() {
            if !model.node_visible(m.node, &[]) {
                continue;
            }
            let (mesh, entity) = spawn_skinned(commands, meshes, &mat, m);
            parts.push(Part { actor, mesh_index: mi, mesh, entity, base_uvs: m.uvs.clone(), blink: false, eye: false });
        }
    }
    parts
}

/// `Ep[id].outfitMeshes` (deobfuscated.js 39839, `data/outfit_meshes.json`,
/// from `extracted/extract_outfit_meshes.py`): per character, the meshes an
/// outfit switches (`body`) and the ones each outfit shows.
#[derive(serde::Deserialize)]
pub struct OutfitMeshes {
    pub body: Vec<String>,
    pub outfits: Vec<Vec<String>>,
}

pub fn outfit_meshes(id: &str) -> Option<&'static OutfitMeshes> {
    static T: std::sync::OnceLock<HashMap<String, OutfitMeshes>> = std::sync::OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("../data/outfit_meshes.json")).expect("outfit_meshes.json")).get(id)
}

/// Whether `node` of the avatar `id` shows in `outfit`: `jp` sets every
/// `body` node's visibility to "in `outfits[outfit]` (else `outfits[0]`)",
/// whatever the file says (the alternate outfits are saved hidden); other
/// nodes keep the file's flag; `Mp` hides `Props`. Up the parent chain.
pub fn outfit_node_visible(model: &SkinModel, id: &str, outfit: usize, node: usize) -> bool {
    let table = outfit_meshes(id);
    let shown = table.and_then(|t| t.outfits.get(outfit).or(t.outfits.first()));
    let mut p = Some(node);
    while let Some(i) = p {
        let n = &model.nodes[i];
        let visible = match table {
            Some(t) if t.body.contains(&n.name) => shown.is_some_and(|v| v.contains(&n.name)),
            _ => n.visible,
        };
        if !visible || n.name == "Props" {
            return false;
        }
        p = n.parent;
    }
    true
}

/// The hero avatar `id` in `outfit` (`jp`/`Mp`, [`outfit_node_visible`]).
/// Texture `<id>-tex` / `<id>-tex-<k>`; double-sided (`Wo`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_hero_parts(
    commands: &mut Commands,
    sim: &Sim,
    model: &SkinModel,
    id: &str,
    outfit: usize,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<SubwayMaterial>,
    images: &mut impl FnMut(&str) -> Option<Handle<Image>>,
    thumb: Option<Thumb>,
) -> Vec<Part> {
    let map = if outfit == 0 { format!("{id}-tex") } else { format!("{id}-tex-{outfit}") };
    let mat = thumb_mat(mats, subway_material(&sim.game.theme, &character_material(&map, true), images(&map), None), thumb);
    let mut parts = Vec::new();
    for (mi, m) in model.meshes.iter().enumerate() {
        if !outfit_node_visible(model, id, outfit, m.node) {
            continue;
        }
        let (mesh, entity) = spawn_skinned(commands, meshes, &mat, m);
        thumb_layer(commands, entity, thumb);
        parts.push(Part { actor: Actor::Hero, mesh_index: mi, mesh, entity, base_uvs: m.uvs.clone(), blink: !m.morphs.is_empty(), eye: m.name == "eye_shader" });
    }
    parts
}

/// A library group (`$.library.getEntity(name, {map})`) as static meshes
/// with the library's material rules. `drop_child`: remove that child of
/// the entity container first (the magnet's `removeChild(children[1])`).
#[allow(clippy::too_many_arguments)]
fn build_prop(
    commands: &mut Commands,
    sim: &Sim,
    group: &str,
    map: &str,
    drop_child: Option<usize>,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<SubwayMaterial>,
    images: &mut impl FnMut(&str) -> Option<Handle<Image>>,
) -> Vec<Prop> {
    use crate::scene::{MatOpts, Scene, Visual};
    let lib = sim.game.lib.clone();
    if !lib.has_group(group) {
        return Vec::new();
    }
    let mut scene = Scene::default();
    let root = scene.get_entity(&lib, group, &MatOpts { map: Some(map.to_string()), ..Default::default() });
    if let Some(i) = drop_child {
        if let Some(&c) = scene.get(root).children.get(i) {
            scene.remove_from_parent(c);
        }
    }
    let mut out = Vec::new();
    for mid in scene.meshes(root) {
        let Visual::Mesh { hash, material, .. } = &scene.get(mid).visual else { continue };
        let Some(mesh) = crate::render::build_mesh(sim, hash) else { continue };
        let tex = material.map.as_deref().and_then(&mut *images);
        let mat = mats.add(subway_material(&sim.game.theme, material, tex, None));
        // the part's matrix relative to the group root
        let local = scene.world_matrix(root).inverse() * scene.world_matrix(mid);
        let entity = commands
            .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat), Transform::IDENTITY, Visibility::Hidden, bevy::camera::visibility::NoFrustumCulling))
            .id();
        out.push(Prop { local, entity });
    }
    out
}

/// A library group with full material options, each part's material passed
/// through `tweak`; returns the parts and their materials.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_prop_opts(
    commands: &mut Commands,
    sim: &Sim,
    group: &str,
    opts: &crate::scene::MatOpts,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<SubwayMaterial>,
    images: &mut impl FnMut(&str) -> Option<Handle<Image>>,
    tweak: &mut dyn FnMut(&mut SubwayMaterial),
) -> (Vec<Prop>, Vec<Handle<SubwayMaterial>>) {
    use crate::scene::{Scene, Visual};
    let lib = sim.game.lib.clone();
    if !lib.has_group(group) {
        return (Vec::new(), Vec::new());
    }
    let mut scene = Scene::default();
    let root = scene.get_entity(&lib, group, opts);
    let (mut out, mut handles) = (Vec::new(), Vec::new());
    for mid in scene.meshes(root) {
        let Visual::Mesh { hash, material, .. } = &scene.get(mid).visual else { continue };
        let Some(mesh) = crate::render::build_mesh(sim, hash) else { continue };
        let tex = material.map.as_deref().and_then(&mut *images);
        let mut m = subway_material(&sim.game.theme, material, tex, None);
        tweak(&mut m);
        let mat = mats.add(m);
        let local = scene.world_matrix(root).inverse() * scene.world_matrix(mid);
        let entity = commands
            .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), Transform::IDENTITY, Visibility::Hidden, bevy::camera::visibility::NoFrustumCulling))
            .id();
        out.push(Prop { local, entity });
        handles.push(mat);
    }
    (out, handles)
}

/// `cf.setBoard` (32625): scene `board-<id>` with texture `board-<id>-tex`;
/// a node is shown iff its name is in the board's `features` plus those of
/// the selected powerups. Each shown node is its library group placed by
/// the node's transform in the board scene.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_board(
    commands: &mut Commands,
    sim: &Sim,
    id: &str,
    board_powers: &[usize],
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<SubwayMaterial>,
    images: &mut impl FnMut(&str) -> Option<Handle<Image>>,
    thumb: Option<Thumb>,
) -> Vec<Prop> {
    let c = crate::shop::Catalog::get();
    let Some(board) = c.board(id).or_else(|| c.board("hoverboard")) else { return Vec::new() };
    let mut visible: Vec<String> = board.features.clone();
    for &p in board_powers {
        if p != 0 {
            if let Some(po) = board.powerups.get(p - 1) {
                visible.extend(po.features.iter().cloned());
            }
        }
    }
    let path = crate::install::Install::locate().site.join(format!("assets/boards/board-{}.pk", board.id));
    let Ok(nodes) = crate::skin::scene_nodes(&path) else { return Vec::new() };
    let map = format!("board-{}-tex", board.id);
    let mut out = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        if !visible.contains(&n.name) || n.geometry.is_none() {
            continue;
        }
        let node = crate::skin::scene_node_world(&nodes, i);
        let parts = match thumb {
            None => build_prop(commands, sim, &n.name, &map, None, meshes, mats, images),
            Some(t) => {
                let opts = crate::scene::MatOpts { map: Some(map.clone()), ..Default::default() };
                let (parts, _) = build_prop_opts(commands, sim, &n.name, &opts, meshes, mats, images, &mut |m| *m = crate::render::thumb_material(m.clone(), t.far));
                for p in &parts {
                    thumb_layer(commands, p.entity, thumb);
                }
                parts
            }
        };
        for p in parts {
            out.push(Prop { local: node * p.local, entity: p.entity });
        }
    }
    out
}

fn set_visible(vis: &mut Query<&mut Visibility>, e: Entity, on: bool) {
    if let Ok(mut v) = vis.get_mut(e) {
        let want = if on { Visibility::Visible } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

fn place_props(out: &mut Vec<(Entity, Transform)>, props: &[Prop], view: DMat4) {
    for p in props {
        out.push((p.entity, Transform::from_matrix((view * p.local).as_mat4())));
    }
}

fn container(y: f64) -> DMat4 {
    DMat4::from_translation(DVec3::new(0.0, y, 0.0)) * DMat4::from_rotation_y(PI) * DMat4::from_scale(DVec3::splat(100.0))
}

/// `Go.getEyeLookUvOffset`: direction from the focus point to the eye
/// origin in head space -> clamped UV offset.
fn eye_offset(model: &SkinModel, root: DMat4, nodes: &[crate::skin::Trs], bones: &[DMat4]) -> [f64; 2] {
    let (Some(o), Some(t), Some(h)) = (model.node_index("eye_switch_meta_grp"), model.node_index("maya_eye_focus_point"), model.joint_index("Head_jnt")) else {
        return [0.0, 0.0];
    };
    let po = model.node_world_posed(root, o, nodes).w_axis.truncate();
    let pt = model.node_world_posed(root, t, nodes).w_axis.truncate();
    let (_, q, _) = bones[h].to_scale_rotation_translation();
    let d = q.inverse() * (po - pt).normalize();
    let a = [d.x.atan2(d.z), d.y.atan2(d.z)];
    [(a[0] * EYE_GAIN[0]).clamp(-EYE_CLAMP[0], EYE_CLAMP[0]), (a[1] * EYE_GAIN[1]).clamp(-EYE_CLAMP[1], EYE_CLAMP[1])]
}

#[allow(clippy::too_many_arguments)]
pub fn sync_actors(
    mut commands: Commands,
    sim: Option<NonSend<Sim>>,
    mut state: ResMut<ActorRender>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<SubwayMaterial>>,
    mut cache: ResMut<crate::render::RenderCache>,
    assets_server: Res<AssetServer>,
    character: Option<Res<ActiveCharacter>>,
    blend: Option<Res<crate::render::RenderBlend>>,
    mut vis: Query<&mut Visibility>,
) {
    let alpha = blend.map_or(1.0, |b| b.alpha);
    let Some(sim) = sim else { return };
    let character = character.map(|c| c.clone()).unwrap_or_default();
    let Some(assets) = sim.game.actors.clone() else { return };
    let rebuild = state.sim_serial != sim.serial;
    let theme = sim.game.theme.clone();
    let mut images = |map: &str| cache.image(&assets_server, &theme, map);
    if rebuild {
        state.pos_blend.clear();
        state.full_passes = 2;
    }
    let avatar_id = character.id.clone();
    let outfit = character.outfit;
    let avatar: Arc<SkinModel> = if avatar_id == "jake" {
        Arc::new(assets.jake.clone())
    } else if let Some(m) = state.avatars.get(&avatar_id) {
        m.clone()
    } else {
        let path = crate::install::Install::locate().site.join(format!("assets/characters-idle/avatar_{avatar_id}.pk"));
        match SkinModel::load(&path, 0.01) {
            Ok(m) => {
                let m = Arc::new(m);
                state.avatars.insert(avatar_id.clone(), m.clone());
                m
            }
            Err(e) => {
                error!("[Character] {avatar_id}: {e} (drawing jake)");
                Arc::new(assets.jake.clone())
            }
        }
    };
    let key = (avatar_id.clone(), outfit);
    if rebuild || state.hero_key.as_ref() != Some(&key) {
        state.pos_blend.clear();
        state.tf_blend.clear();
        for p in state.hero_parts.drain(..) {
            commands.entity(p.entity).despawn();
        }
        if let Some(s) = state.sneakers.take() {
            commands.entity(s.entity).despawn();
        }
        state.hero_parts = build_hero_parts(&mut commands, &sim, &avatar, &avatar_id, outfit, &mut meshes, &mut mats, &mut images, None);
        // pp.show -> sp(scene, "superSneakers"): re-bound to this skeleton
        let props_path = crate::install::Install::locate().site.join("assets/game-basic/character-props.pk");
        if let Ok(props) = SkinModel::load(&props_path, 0.01) {
            if let Some(m) = props.meshes.iter().find(|m| m.name == "superSneakers") {
                let mesh = avatar.rebind(&props, m);
                let mat = mats.add(subway_material(&theme, &character_material("props-tex", true), images("props-tex"), None));
                let (handle, entity) = spawn_skinned(&mut commands, &mut meshes, &mat, &mesh);
                state.sneakers = Some(SneakersPart { mesh, handle, entity });
            }
        }
        state.hero_key = Some(key);
        state.full_passes = 2;
    }
    let g = &sim.game;
    let board_key = format!("{}{:?}", g.hero.hoverboard.board_id, g.flow.user.board_powers);
    if rebuild || (state.board_key != board_key && !g.hero.hoverboard.board_id.is_empty()) {
        for p in state.board.drain(..) {
            commands.entity(p.entity).despawn();
        }
        state.board = build_board(&mut commands, &sim, &g.hero.hoverboard.board_id, &g.flow.user.board_powers, &mut meshes, &mut mats, &mut images, None);
        state.board_key = board_key;
        state.full_passes = 2;
    }
    if rebuild {
        for p in state.parts.drain(..).collect::<Vec<_>>() {
            commands.entity(p.entity).despawn();
        }
        let st = &mut *state;
        for (p, _) in st.dizzy.drain(..) {
            commands.entity(p.entity).despawn();
        }
        for list in [&mut st.pogo, &mut st.jetpack, &mut st.magnet, &mut st.spray_can] {
            for p in list.drain(..) {
                commands.entity(p.entity).despawn();
            }
        }
        state.parts = build_parts(&mut commands, &sim, &mut meshes, &mut mats, &mut images);
        state.pogo = build_prop(&mut commands, &sim, "powerups_rocketPogo", "props-tex", None, &mut meshes, &mut mats, &mut images);
        state.jetpack = build_prop(&mut commands, &sim, "powerups_jetpack", "props-tex", None, &mut meshes, &mut mats, &mut images);
        state.magnet = build_prop(&mut commands, &sim, "powerups_coinMagnet", "props-tex", Some(1), &mut meshes, &mut mats, &mut images);
        // Gp: sprayCan = getEntity("sprayCan") (props-tex, `applySprayCanMap`)
        state.spray_can = build_prop(&mut commands, &sim, "sprayCan", "props-tex", None, &mut meshes, &mut mats, &mut images);
        // $o.createView: trails at 0 and ry pi, stars at z -1.5 / 1.5;
        // effects-tex, opacity 0.5, additive
        {
            let opts = crate::scene::MatOpts { map: Some("effects-tex".into()), opacity: Some(0.5), blend_mode: Some(BlendMode::Add), depth_mask: None };
            let place = [
                ("Dizzytrail", DMat4::IDENTITY),
                ("Dizzytrail", DMat4::from_rotation_y(PI)),
                ("Dizzystar", DMat4::from_translation(DVec3::new(0.0, 0.0, -1.5))),
                ("Dizzystar", DMat4::from_translation(DVec3::new(0.0, 0.0, 1.5))),
            ];
            for (group, at) in place {
                for p in build_prop_opts(&mut commands, &sim, group, &opts, &mut meshes, &mut mats, &mut images, &mut |_| {}).0 {
                    state.dizzy.push((p, at));
                }
            }
        }
        // hero_fx: getEntity(name, {map: effects-tex, opacity, blendMode})
        let fx = |opacity: f64, blend: BlendMode| crate::scene::MatOpts { map: Some("effects-tex".into()), opacity: Some(opacity), blend_mode: Some(blend), depth_mask: None };
        let st = &mut *state;
        for p in st.pop.drain(..).chain(st.pop_pickup.drain(..)).chain(st.halo.drain(..)) {
            commands.entity(p.entity).despawn();
        }
        if let Some(e) = state.shadow.take() {
            commands.entity(e).despawn();
        }
        state.pop = build_prop_opts(&mut commands, &sim, "star7", &fx(0.9, BlendMode::Screen), &mut meshes, &mut mats, &mut images, &mut |_| {}).0;
        state.pop_pickup = build_prop_opts(&mut commands, &sim, "pow", &fx(0.95, BlendMode::Add), &mut meshes, &mut mats, &mut images, &mut |_| {}).0;
        // $f: depthTest and depthMask off
        let (halo, halo_mats) = build_prop_opts(&mut commands, &sim, "powRevive", &fx(0.7, BlendMode::Screen), &mut meshes, &mut mats, &mut images, &mut |m| {
            m.key.depth_test = false;
            m.key.depth_write = false;
        });
        state.halo = halo;
        state.halo_mats = halo_mats;
        // rp: H.plane(8, 8, 1, "character_shadow", MULTIPLY) + blend = true:
        // $n without fog, double sided (side 2 in the oracle), depth write on
        let shadow_mat = SimMaterial {
            map: Some("character_shadow".into()),
            opacity: Some(1.0),
            color: None,
            blend: true,
            blend_mode: BlendMode::Multiply,
            depth_mask: true,
            culling: false,
            cw: false,
            pk_material: None,
            fog: Fog::Off,
            fog_multiplier: 1.0,
            shader: Shader::Basic,
        };
        let mat = mats.add(subway_material(&theme, &shadow_mat, images("character_shadow"), None));
        let e = commands
            .spawn((Mesh3d(meshes.add(crate::render::plane_mesh(8.0, 8.0))), MeshMaterial3d(mat), Transform::IDENTITY, Visibility::Hidden, bevy::camera::visibility::NoFrustumCulling))
            .id();
        state.shadow = Some(e);
        state.sim_serial = sim.serial;
        state.full_passes = 2;
    }
    // between simulation frames only the blend changes
    if state.full_passes > 0 {
        state.full_passes -= 1;
    } else if state.last_frame == Some(g.frame) {
        apply_blend(&state, &mut meshes, &mut commands, alpha);
        return;
    }
    let mut new_pos: Vec<(Handle<Mesh>, Vec<[f32; 3]>)> = Vec::new();
    let mut new_tfs: Vec<(Entity, Transform)> = Vec::new();
    let gd = &g.guard;
    let guard_world = DMat4::from_translation(DVec3::new(gd.x as f64, gd.y as f64, gd.z as f64));
    let dog_world = guard_world * DMat4::from_translation(DVec3::new(5.0, 0.0, -6.0)) * DMat4::from_scale(DVec3::splat(gd.dog_scale));
    let hero_world = DMat4::from_translation(g.hero.position) * DMat4::from_rotation_y(g.hero.ry);
    let h = &g.hero;
    // hero_fx, in the hero entity's space
    {
        let fxs = &g.hero_fx;
        if let Some(e) = state.shadow {
            set_visible(&mut vis, e, fxs.shadow_on);
            if fxs.shadow_on {
                // rx pi/2 at z 1, y = ground + 1 - entity.y
                let m = hero_world * DMat4::from_translation(DVec3::new(0.0, fxs.shadow_y, 1.0)) * DMat4::from_rotation_x(PI * 0.5);
                new_tfs.push((e, Transform::from_matrix(m.as_mat4())));
            }
        }
        // three Euler XYZ: (0, pi, rz)
        let pop_m = |pos: DVec3, rx: f64, rz: f64, scale: f64| {
            hero_world * DMat4::from_translation(pos) * DMat4::from_rotation_x(rx) * DMat4::from_rotation_y(PI) * DMat4::from_rotation_z(rz) * DMat4::from_scale(DVec3::splat(scale))
        };
        for (list, p, pos) in [(&state.pop, fxs.pop, DVec3::new(0.0, 0.0, -3.0)), (&state.pop_pickup, fxs.pop_pickup, DVec3::ZERO)] {
            for part in list.iter() {
                set_visible(&mut vis, part.entity, p.active());
            }
            if p.active() {
                place_props(&mut new_tfs, list, pop_m(pos, 0.0, p.rz, p.scale));
            }
        }
        let hl = fxs.halo;
        for part in &state.halo {
            set_visible(&mut vis, part.entity, hl.active);
        }
        if hl.active {
            place_props(&mut new_tfs, &state.halo, pop_m(DVec3::new(0.0, 2.0, 0.0), -0.3, hl.rz, hl.scale));
            for h in &state.halo_mats {
                if let Some(mut m) = mats.get_mut(h) {
                    m.params.color.w = hl.opacity as f32;
                }
            }
        }
    }
    let props_on = [
        (0, h.pogo.is_on()),
        (1, h.jetpack.shown && h.jetpack.is_on()),
        (2, h.magnet.is_on() && !h.magnet.frozen),
        (3, h.hoverboard.shown && h.hoverboard.count != 0.0 && !h.hoverboard.paused),
    ];
    for (k, on) in props_on {
        let list = match k {
            0 => &state.pogo,
            1 => &state.jetpack,
            2 => &state.magnet,
            _ => &state.board,
        };
        for p in list {
            set_visible(&mut vis, p.entity, on);
        }
    }
    // the hero avatar: Jake's animator drives every avatar (same 26-joint
    // skeleton); named nodes (Root, attach points) map across by name
    let hero_root = hero_world * container(-crate::hero::REGULAR_HEIGHT * 0.5 + 1.0);
    if let Some(anim) = g.hero_anim.as_ref() {
        let jake = &assets.jake;
        let (bone_locals, jake_nodes) = anim.pose(jake);
        let node_locals: Vec<crate::skin::Trs> =
            avatar.nodes.iter().map(|n| jake.node_index(&n.name).map(|j| jake_nodes[j]).unwrap_or(n.local)).collect();
        let bones = avatar.bone_worlds_posed(hero_root, &node_locals, &bone_locals);
        for p in &state.hero_parts {
            set_visible(&mut vis, p.entity, true);
        }
        // the spray can on attachPoint2 (rx -pi/2) while the paint idle runs (Np)
        let can = g.hero.fsm.current == "idle";
        for p in &state.spray_can {
            set_visible(&mut vis, p.entity, can);
        }
        if let (true, Some(n)) = (can, avatar.node_index("attachPoint2")) {
            let ap = avatar.node_world_posed(hero_root, n, &node_locals);
            place_props(&mut new_tfs, &state.spray_can, ap * DMat4::from_rotation_x(-PI / 2.0));
        }
        // the dizzy stars on Head_jnt: 4 above the bone in world space, world
        // rotation (-0.5, spin, 0), x1.25 the bone's scale; spin 0.05 a frame unit
        let dizzy_on = h.player.dizzy_fx && h.player.dizzy > 0.0;
        for (p, _) in &state.dizzy {
            set_visible(&mut vis, p.entity, dizzy_on);
        }
        if let (true, Some(j)) = (dizzy_on, avatar.joint_index("Head_jnt")) {
            let (scale, _, pos) = bones[j].to_scale_rotation_translation();
            let view = DMat4::from_translation(pos + DVec3::new(0.0, 4.0, 0.0))
                * DMat4::from_quat(DQuat::from_euler(EulerRot::XYZ, -0.5, h.player.dizzy_spin, 0.0))
                * DMat4::from_scale(scale * 1.25);
            for (p, at) in &state.dizzy {
                new_tfs.push((p.entity, Transform::from_matrix((view * *at * p.local).as_mat4())));
            }
        }
        // pogo stick / board on attachPoint1, jetpack on LowerSpine_jnt, magnet on R_Hand_jnt
        if let Some(n) = avatar.node_index("attachPoint1") {
            let ap = avatar.node_world_posed(hero_root, n, &node_locals);
            if h.pogo.is_on() {
                place_props(&mut new_tfs, &state.pogo, ap * DMat4::from_translation(DVec3::new(0.0, -0.12, 0.0)) * DMat4::from_rotation_x(-PI / 2.0));
            }
            if h.hoverboard.shown {
                place_props(&mut new_tfs, &state.board, ap * DMat4::from_rotation_x(-PI / 2.0));
            }
        }
        if let (true, Some(j)) = (h.jetpack.shown, avatar.joint_index("LowerSpine_jnt")) {
            let view = bones[j] * DMat4::from_translation(DVec3::new(0.0, 0.53, 0.75)) * DMat4::from_rotation_y(PI) * DMat4::from_scale(DVec3::splat(0.6));
            place_props(&mut new_tfs, &state.jetpack, view);
        }
        if let (true, Some(j)) = (h.magnet.is_on(), avatar.joint_index("R_Hand_jnt")) {
            let view = bones[j] * DMat4::from_translation(DVec3::new(-2.0, 0.0, 0.0)) * DMat4::from_rotation_z(PI * 0.5) * DMat4::from_scale(DVec3::splat(0.4));
            place_props(&mut new_tfs, &state.magnet, view);
        }
        if let Some(s) = &state.sneakers {
            let on = h.sneakers.shown && h.sneakers.is_on();
            set_visible(&mut vis, s.entity, on);
            if on {
                new_pos.push((s.handle.clone(), avatar.skin_mesh(&s.mesh, &bones, &[])));
            }
        }
        {
            let face: &Face = &g.hero_face;
            let eye_off = eye_offset(&avatar, hero_root, &node_locals, &bones);
            for p in &state.hero_parts {
                let m = &avatar.meshes[p.mesh_index];
                let infl = if p.blink { face.influences(&m.default_influences) } else { m.default_influences.clone() };
                new_pos.push((p.mesh.clone(), avatar.skin(p.mesh_index, &bones, &infl)));
                let Some(mut mesh) = meshes.get_mut(&p.mesh) else { continue };
                if p.eye {
                    let blinking = face.amount > 0.5;
                    let off = if blinking { [0.0, 0.0] } else { eye_off };
                    let b = if blinking { 1.0 } else { 0.0 };
                    let uvs: Vec<[f32; 2]> = p
                        .base_uvs
                        .iter()
                        .map(|uv| [(uv[0] as f64 + off[0] + EYE_BLINK_UV[0] * b) as f32, (uv[1] as f64 + off[1] + EYE_BLINK_UV[1] * b) as f32])
                        .collect();
                    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
                }
            }
        }
    }
    // the guard and the dog
    let actors: [(Actor, &SkinModel, Option<&Animator>, DMat4, bool); 2] = [
        (Actor::Guard, &assets.guard, g.guard_anim.as_ref(), guard_world * container(-14.0 * 0.4), gd.active && gd.model_active),
        (Actor::Dog, &assets.dog, g.dog_anim.as_ref(), dog_world * container(-14.0 * 0.4), gd.active && gd.dog_active),
    ];
    for (actor, model, anim, root, visible) in actors {
        let parts: Vec<usize> = state.parts.iter().enumerate().filter(|(_, p)| p.actor == actor).map(|(i, _)| i).collect();
        for &i in &parts {
            set_visible(&mut vis, state.parts[i].entity, visible);
        }
        let (Some(anim), true) = (anim, visible) else { continue };
        let (bone_locals, node_locals) = anim.pose(model);
        let bones = model.bone_worlds_posed(root, &node_locals, &bone_locals);
        for &i in &parts {
            let p = &state.parts[i];
            let m = &model.meshes[p.mesh_index];
            new_pos.push((p.mesh.clone(), model.skin(p.mesh_index, &bones, &m.default_influences)));
        }
    }
    // record this frame's results (prev <- cur), then draw the blend
    for (h, pos) in new_pos {
        let prev = state.pos_blend.get(&h.id()).filter(|(_, c, _)| c.len() == pos.len()).map(|(_, c, _)| c.clone()).unwrap_or_else(|| pos.clone());
        state.pos_blend.insert(h.id(), (prev, pos, h));
    }
    for (e, tf) in new_tfs {
        let prev = state.tf_blend.get(&e).map_or(tf, |&(_, c)| c);
        state.tf_blend.insert(e, (prev, tf));
    }
    state.last_frame = Some(g.frame);
    apply_blend(&state, &mut meshes, &mut commands, alpha);
}

/// Write the interpolated vertex positions and prop transforms.
fn apply_blend(state: &ActorRender, meshes: &mut Assets<Mesh>, commands: &mut Commands, alpha: f32) {
    for (prev, cur, h) in state.pos_blend.values() {
        let Some(mut mesh) = meshes.get_mut(h) else { continue };
        let pos: Vec<[f32; 3]> = if alpha >= 1.0 {
            cur.clone()
        } else {
            prev.iter().zip(cur).map(|(a, b)| [a[0] + (b[0] - a[0]) * alpha, a[1] + (b[1] - a[1]) * alpha, a[2] + (b[2] - a[2]) * alpha]).collect()
        };
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    }
    for (&e, (a, b)) in &state.tf_blend {
        if let Ok(mut ec) = commands.get_entity(e) {
            ec.insert(crate::render::blend_tf(a, b, alpha));
        }
    }
}
