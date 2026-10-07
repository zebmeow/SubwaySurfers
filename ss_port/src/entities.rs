//! Level entity classes: construction (pool misses), respawn, per-frame
//! component updates. Factories (how a chunk node becomes entities) live in
//! [`crate::mount`] and [`crate::environment`].
//!
//! Class names returned by [`Cls::name`] are the runtime constructor names
//! the oracle records (`cls` in traces).

use crate::game::{Game, GameState, BLOCK_SIZE};
use crate::scene::{MatOpts, ObjId, Visual};
use crate::theme::{self, ConfigSet};
use bevy::math::DVec3;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub usize);

/// Track/floor pieces (`K` subclasses, deobfuscated.js:9560-9655).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrackKind {
    Si, Ci, Wi, Ti, Ei, Di, Oi, Ki, Ai, Ji, Mi, Ni, Pi, Fi,
}
impl TrackKind {
    pub fn info(self) -> (&'static str, &'static str, bool) {
        use TrackKind::*;
        match self {
            Si => ("Si", "track", true),
            Ci => ("Ci", "track_shadow_start", true),
            Wi => ("wi", "track_shadow_mid", true),
            Ti => ("Ti", "track_shadow_end", true),
            Ei => ("Ei", "track_shadow_short_start", true),
            Di => ("Di", "track_shadow_short_end", true),
            Oi => ("Oi", "ground", false),
            Ki => ("ki", "ground_shadow_start", false),
            Ai => ("Ai", "ground_shadow_mid", false),
            Ji => ("ji", "ground_shadow_end", false),
            Mi => ("Mi", "ground_shadow_short_start", false),
            Ni => ("Ni", "ground_shadow_short_end", false),
            Pi => ("Pi", "track_gates", false),
            Fi => ("Fi", "track_gates_shadows", false),
        }
    }
}

/// Building fillers (`ii` subclasses, 9389-9502).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FillerKind {
    Low01L, Low02L, Med01L, Med02L, Med03L, High01L, High02L, High03L,
    Low01R, Low02R, Med01R, Med02R, Med03R, High01R, High02R, High03R,
}
impl FillerKind {
    pub fn info(self) -> (&'static str, &'static str) {
        use FillerKind::*;
        match self {
            Low01L => ("ai", "low_01_left"),
            Low02L => ("oi", "low_02_left"),
            Med01L => ("si", "med_01_left"),
            Med02L => ("ci", "med_02_left"),
            Med03L => ("li", "med_03_left"),
            High01L => ("ui", "high_01_left"),
            High02L => ("di", "high_02_left"),
            High03L => ("fi", "high_03_left"),
            Low01R => ("pi", "low_01_right"),
            Low02R => ("mi", "low_02_right"),
            Med01R => ("hi", "med_01_right"),
            Med02R => ("gi", "med_02_right"),
            Med03R => ("_i", "med_03_right"),
            High01R => ("vi", "high_01_right"),
            High02R => ("yi", "high_02_right"),
            High03R => ("bi", "high_03_right"),
        }
    }
    /// `xi["Fillers" + kind + num + side]`.
    pub fn by_key(kind: &str, num: &str, left: bool) -> FillerKind {
        use FillerKind::*;
        match (kind, num, left) {
            ("Low", "01", true) => Low01L,
            ("Low", "02", true) => Low02L,
            ("Med", "01", true) => Med01L,
            ("Med", "02", true) => Med02L,
            ("Med", "03", true) => Med03L,
            ("High", "01", true) => High01L,
            ("High", "02", true) => High02L,
            ("High", "03", true) => High03L,
            ("Low", "01", false) => Low01R,
            ("Low", "02", false) => Low02R,
            ("Med", "01", false) => Med01R,
            ("Med", "02", false) => Med02R,
            ("Med", "03", false) => Med03R,
            ("High", "01", false) => High01R,
            ("High", "02", false) => High02R,
            ("High", "03", false) => High03R,
            _ => panic!("no filler class Fillers{kind}{num}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GateKind {
    Mid,
    Left,
    Right,
    Sides,
}
impl GateKind {
    pub fn from_node(name: &str) -> Option<Self> {
        Some(match name {
            "gates_mid_group_place" => GateKind::Mid,
            "gates_left_group_place" => GateKind::Left,
            "gates_right_group_place" => GateKind::Right,
            "gates_sides_group_place" => GateKind::Sides,
            _ => return None,
        })
    }
    pub fn model(self) -> &'static str {
        match self {
            GateKind::Mid => "gates_mid",
            GateKind::Left => "gates_left",
            GateKind::Right => "gates_right",
            GateKind::Sides => "gates_sides",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockerKind {
    Jump,
    Roll,
    Standard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrainKind {
    /// `Eo` standard
    Standard,
    /// `Do` freight + cargo containers
    Cargo,
    /// `Oo` freight + subway wagons
    Sub,
}
impl PickupKind {
    /// `Xa` keys (11232): the pickup types `config.forcePickup` accepts.
    pub fn from_xa(s: &str) -> Option<Self> {
        Some(match s {
            "jetpack" => Self::Jetpack,
            "pogo" => Self::Pogo,
            "magnet" => Self::Magnet,
            "sneakers" => Self::Sneakers,
            "multiplier" => Self::Multiplier,
            "key" => Self::Key,
            "letter" => Self::Letter,
            "mysteryBox" => Self::MysteryBox,
            _ => return None,
        })
    }
}

impl TrainKind {
    pub fn models(self) -> (&'static str, &'static str) {
        match self {
            TrainKind::Standard => ("train_standard", "train_standard_wagon"),
            TrainKind::Cargo => ("train_freight", "train_cargo"),
            TrainKind::Sub => ("train_freight", "train_sub_wagon"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PickupKind {
    Jetpack,
    Pogo,
    Magnet,
    Sneakers,
    Multiplier,
    Letter,
    MysteryBox,
    Key,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cls {
    Track(TrackKind),
    Filler(FillerKind),
    /// tube block
    Po,
    /// tube environment (`Fo`, runtime name "e")
    Fo,
    /// epic start/mid/end (`ei`/`ti`/`ni`)
    Epic(u8),
    /// pillars environment (`io`, runtime name "e")
    PillarsEnv,
    /// pillars start/mid/end pieces (`to`/`no`/`ro`)
    PillarPiece(u8),
    /// `Gi` obstacles: 0 dumpster `Ki`, 1 bush `qi`, 2 power box `Ji`
    Obstacle(u8),
    /// station pieces `fo` / `po` / `mo` (start / mid / end)
    Station(u8),
    /// station roof `ho` (runtime name "e")
    StationRoof,
    /// `mountStaticGeometry` (48410): a library entity, not a level entity
    StaticGeometry,
    /// station platform `uo` (runtime name "e") and its side colliders `lo`
    StationPlatform,
    StationSide,
    /// gates base
    Ri,
    /// gate collider
    Zi,
    /// gate trigger
    Li,
    Gate(GateKind),
    Blocker(BlockerKind),
    /// blocker dodge detector
    Rr,
    /// tutorial trigger `No` (`Trigger_<type>`: its `tutorial_type`)
    No,
    Coin,
    Train(TrainKind),
    /// ramp train `so`
    Ramp,
    /// ramp side wall `oo`
    RampWall,
    /// light signal `Hi`
    LightSignal,
    Pickup(PickupKind),
    Checkpoint,
    Pillar,
    /// start-screen bag `co`
    Bag,
    /// intro train with score sign `vg`
    IntroTrain,
    /// skyline `wm`
    Skyline,
}

impl Cls {
    /// Runtime constructor name (trace `cls`).
    pub fn name(self) -> &'static str {
        match self {
            Cls::Track(k) => k.info().0,
            Cls::Filler(k) => k.info().0,
            Cls::Po => "Po",
            Cls::Fo | Cls::Coin | Cls::Checkpoint | Cls::Pillar | Cls::PillarsEnv => "e",
            Cls::Station(0) => "fo",
            Cls::Station(1) => "po",
            Cls::Station(_) => "mo",
            Cls::StationRoof | Cls::StationPlatform | Cls::StaticGeometry => "e",
            Cls::StationSide => "lo",
            Cls::Obstacle(0) => "Ki",
            Cls::Obstacle(1) => "qi",
            Cls::Obstacle(_) => "Ji",
            Cls::PillarPiece(0) => "to",
            Cls::PillarPiece(1) => "no",
            Cls::PillarPiece(_) => "ro",
            Cls::Epic(0) => "ei",
            Cls::Epic(1) => "ti",
            Cls::Epic(_) => "ni",
            Cls::Ri => "Ri",
            Cls::Zi => "zi",
            Cls::Li => "Li",
            Cls::Gate(GateKind::Mid) => "gates_mid_group_place",
            Cls::Gate(GateKind::Left) => "gates_left_group_place",
            Cls::Gate(GateKind::Right) => "gates_right_group_place",
            Cls::Gate(GateKind::Sides) => "gates_sides_group_place",
            Cls::Blocker(BlockerKind::Jump) => "yr",
            Cls::Blocker(BlockerKind::Roll) => "br",
            Cls::Blocker(BlockerKind::Standard) => "xr",
            Cls::Rr => "rr",
            Cls::No => "No",
            Cls::Train(TrainKind::Standard) => "Eo",
            Cls::Train(TrainKind::Cargo) => "Do",
            Cls::Train(TrainKind::Sub) => "Oo",
            Cls::Ramp => "so",
            Cls::RampWall => "oo",
            Cls::LightSignal => "Hi",
            Cls::Pickup(PickupKind::Jetpack) => "Ua",
            Cls::Pickup(PickupKind::Pogo) => "Wa",
            Cls::Pickup(PickupKind::Magnet) => "Ga",
            Cls::Pickup(PickupKind::Sneakers) => "Ka",
            Cls::Pickup(PickupKind::Multiplier) => "qa",
            Cls::Pickup(PickupKind::Letter) => "Ja",
            Cls::Pickup(PickupKind::MysteryBox) => "Ya",
            Cls::Pickup(PickupKind::Key) => "key",
            Cls::Bag => "co",
            Cls::IntroTrain => "vg",
            Cls::Skyline => "wm",
        }
    }
}

/// Axis-aligned box `tn` (deobfuscated.js:2492-2633). Center and size live
/// in gl-matrix `Float32Array`s (`Jt`, 2225): every write rounds to f32,
/// reads widen back to f64. Edge getters compute in f64; edge setters derive
/// the center from the current size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aabb {
    pub c: [f32; 3],
    pub s: [f32; 3],
}

impl Aabb {
    pub fn x(&self) -> f64 { self.c[0] as f64 }
    pub fn y(&self) -> f64 { self.c[1] as f64 }
    pub fn z(&self) -> f64 { self.c[2] as f64 }
    pub fn width(&self) -> f64 { self.s[0] as f64 }
    pub fn height(&self) -> f64 { self.s[1] as f64 }
    pub fn depth(&self) -> f64 { self.s[2] as f64 }
    pub fn left(&self) -> f64 { self.x() - self.width() * 0.5 }
    pub fn right(&self) -> f64 { self.x() + self.width() * 0.5 }
    pub fn top(&self) -> f64 { self.y() + self.height() * 0.5 }
    pub fn bottom(&self) -> f64 { self.y() - self.height() * 0.5 }
    pub fn front(&self) -> f64 { self.z() - self.depth() * 0.5 }
    pub fn back(&self) -> f64 { self.depth() * 0.5 + self.z() }
    pub fn set_left(&mut self, v: f64) { self.c[0] = (v + self.width() * 0.5) as f32; }
    pub fn set_right(&mut self, v: f64) { self.c[0] = (v - self.width() * 0.5) as f32; }
    pub fn set_top(&mut self, v: f64) { self.c[1] = (v - self.height() * 0.5) as f32; }
    pub fn set_bottom(&mut self, v: f64) { self.c[1] = (v + self.height() * 0.5) as f32; }
    pub fn set_front(&mut self, v: f64) { self.c[2] = (v + self.depth() * 0.5) as f32; }
    pub fn set_back(&mut self, v: f64) { self.c[2] = (v - self.depth() * 0.5) as f32; }

    /// `tn.hitTest` (2607): inclusive overlap test; the intersection box is
    /// stored f32 (size first, then the center from the stored size; z is
    /// measured from the back).
    pub fn hit_test(&self, o: &Aabb) -> Option<Aabb> {
        let x = self.left() <= o.right() && self.right() >= o.left();
        let y = self.bottom() <= o.top() && self.top() >= o.bottom();
        let z = self.front() <= o.back() && self.back() >= o.front();
        if !x || !y || !z {
            return None;
        }
        let (x0, x1) = (self.left().max(o.left()), self.right().min(o.right()));
        let (y0, y1) = (self.bottom().max(o.bottom()), self.top().min(o.top()));
        let (z0, z1) = (self.front().max(o.front()), self.back().min(o.back()));
        let mut h = Aabb::default();
        h.s = [(x1 - x0) as f32, (y1 - y0) as f32, (z1 - z0) as f32];
        h.c[0] = (x0 + h.width() * 0.5) as f32;
        h.c[1] = (y0 + h.height() * 0.5) as f32;
        h.c[2] = (z1 - h.depth() * 0.5) as f32;
        Some(h)
    }
}

/// Physics body (`U`, deobfuscated.js:5022-5365): box, origin (the box at
/// the start of the last move) and velocity, all f32-stored.
#[derive(Clone, Debug, Default)]
pub struct Body {
    pub bx: Aabb,
    pub origin: Aabb,
    v: [f32; 3],
    pub trigger: bool,
    pub ghost: bool,
    pub deco: bool,
    pub soft: bool,
    pub movable: bool,
}

impl Body {
    pub fn new(w: f64, h: f64, d: f64) -> Self {
        let mut b = Self::default();
        b.set_size(DVec3::new(w, h, d));
        b
    }
    pub fn center(&self) -> DVec3 {
        DVec3::new(self.cx(), self.cy(), self.cz())
    }
    pub fn size(&self) -> DVec3 {
        DVec3::new(self.sx(), self.sy(), self.sz())
    }
    pub fn velocity(&self) -> DVec3 {
        DVec3::new(self.vx(), self.vy(), self.vz())
    }
    pub fn cx(&self) -> f64 { self.bx.x() }
    pub fn cy(&self) -> f64 { self.bx.y() }
    pub fn cz(&self) -> f64 { self.bx.z() }
    pub fn sx(&self) -> f64 { self.bx.width() }
    pub fn sy(&self) -> f64 { self.bx.height() }
    pub fn sz(&self) -> f64 { self.bx.depth() }
    pub fn vx(&self) -> f64 { self.v[0] as f64 }
    pub fn vy(&self) -> f64 { self.v[1] as f64 }
    pub fn vz(&self) -> f64 { self.v[2] as f64 }
    pub fn set_cx(&mut self, v: f64) { self.bx.c[0] = v as f32; }
    pub fn set_cy(&mut self, v: f64) { self.bx.c[1] = v as f32; }
    pub fn set_cz(&mut self, v: f64) { self.bx.c[2] = v as f32; }
    pub fn set_sx(&mut self, v: f64) { self.bx.s[0] = v as f32; }
    pub fn set_sy(&mut self, v: f64) { self.bx.s[1] = v as f32; }
    pub fn set_sz(&mut self, v: f64) { self.bx.s[2] = v as f32; }
    pub fn set_vx(&mut self, v: f64) { self.v[0] = v as f32; }
    pub fn set_vy(&mut self, v: f64) { self.v[1] = v as f32; }
    pub fn set_vz(&mut self, v: f64) { self.v[2] = v as f32; }
    /// `velocity.reset()`.
    pub fn reset_velocity(&mut self) {
        self.v = [0.0; 3];
    }
    pub fn set_center(&mut self, v: DVec3) {
        self.set_cx(v.x);
        self.set_cy(v.y);
        self.set_cz(v.z);
    }
    pub fn set_size(&mut self, v: DVec3) {
        self.set_sx(v.x);
        self.set_sy(v.y);
        self.set_sz(v.z);
    }
    pub fn front(&self) -> f64 { self.bx.front() }
    pub fn back(&self) -> f64 { self.bx.back() }
    pub fn bottom(&self) -> f64 { self.bx.bottom() }
    pub fn top(&self) -> f64 { self.bx.top() }
    pub fn left(&self) -> f64 { self.bx.left() }
    pub fn right(&self) -> f64 { self.bx.right() }
    pub fn set_front(&mut self, v: f64) { self.bx.set_front(v); }
    pub fn set_back(&mut self, v: f64) { self.bx.set_back(v); }
    pub fn set_bottom(&mut self, v: f64) { self.bx.set_bottom(v); }
    pub fn set_top(&mut self, v: f64) { self.bx.set_top(v); }
    pub fn set_left(&mut self, v: f64) { self.bx.set_left(v); }
    pub fn set_right(&mut self, v: f64) { self.bx.set_right(v); }

    /// `Body.move` (5103) without the sensor/tolerance part: copy the box to
    /// `origin`, integrate z, x, y (one f32 rounding each), clamp the bottom
    /// to `ground` unless ghost.
    pub fn integrate(&mut self, sd: f64, ground: f64) {
        self.origin = self.bx;
        self.bx.c[2] = (self.cz() + self.vz() * sd) as f32;
        self.bx.c[0] = (self.cx() + self.vx() * sd) as f32;
        self.bx.c[1] = (self.cy() + self.vy() * sd) as f32;
        if self.bottom() <= ground && !self.ghost {
            self.set_bottom(ground);
            self.v[1] = 0.0;
        }
    }
}

/// `Movable` component `mr` (5616-5706).
#[derive(Clone, Debug, Default)]
pub struct Movable {
    pub speed: f64,
    pub origin: f64,
    pub target: f64,
    pub last_dest: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub cls: Cls,
    /// A tutorial trigger's type (`No.type`: up, down, left, right,
    /// hoverboard, good, finished).
    pub tutorial_type: String,
    /// A coin's place on a jump curve (1..), 0 elsewhere (`Za.arc`: the
    /// pickup sound's pitch).
    pub arc: u32,
    pub root: ObjId,
    pub body: Option<Body>,
    pub level_entity: bool,
    pub removal_offset: f64,
    pub removable_on_crash: bool,
    pub active: bool,
    pub in_scene: bool,
    /// Model child (trains, blockers, ...); its local transform matters.
    pub model: Option<ObjId>,
    /// Secondary view (tube `Po.view`, pickup halo view).
    pub view: Option<ObjId>,
    pub halo: Option<ObjId>,
    pub movable: Option<Movable>,
    pub model_index: i64,
    pub lights: Vec<ObjId>,
    pub letter: String,
    pub letter_model: Option<ObjId>,
    /// Light-signal flicker component (`Vi`): accumulated frame time + light.
    pub flicker: Option<(f64, ObjId)>,
    /// Pickup halo component (`Ba`) present.
    pub has_halo: bool,
    pub has_initialized: bool,
    /// Gate colliders (ceiling, L, M, R).
    pub gate_cols: Vec<EntityId>,
    /// Floating component `dr`: rotation speed, starting rotation, rotation.
    pub spin: Option<f64>,
    pub spin_start: Option<f64>,
    pub spin_rot: f64,
    /// Coin shine `gr` halo plane.
    pub shine: Option<ObjId>,
    /// Letter bounce `La`.
    pub bounce: Option<f64>,
    /// Attractable `ar` (coins and pickups).
    pub attract: Option<crate::powerups::Attract>,
}

impl Entity {
    pub fn new(cls: Cls, root: ObjId) -> Self {
        Self {
            cls,
            tutorial_type: String::new(),
            arc: 0,
            root,
            body: None,
            level_entity: true,
            removal_offset: 0.0,
            removable_on_crash: false,
            active: true,
            in_scene: false,
            model: None,
            view: None,
            halo: None,
            movable: None,
            model_index: -1,
            lights: Vec::new(),
            letter: String::new(),
            letter_model: None,
            flicker: None,
            has_halo: false,
            has_initialized: false,
            gate_cols: Vec::new(),
            spin: None,
            spin_start: None,
            spin_rot: 0.0,
            shine: None,
            bounce: None,
            attract: None,
        }
    }
}

fn with_body(g: &mut Game, cls: Cls, body: Body) -> EntityId {
    let root = g.scene.new_obj();
    // `U` constructor parks its owner at z = 9999.
    g.scene.get_mut(root).pos.z = 9999.0;
    let mut e = Entity::new(cls, root);
    e.body = Some(body);
    g.alloc_entity(e)
}

fn plain(g: &mut Game, cls: Cls) -> EntityId {
    let root = g.scene.new_obj();
    g.alloc_entity(Entity::new(cls, root))
}

fn get_entity(g: &mut Game, group: &str, opts: &MatOpts) -> ObjId {
    let lib = g.lib.clone();
    g.scene.get_entity(&lib, group, opts)
}

fn add_obj(g: &mut Game, parent: ObjId, child: ObjId) {
    g.scene.add_child(parent, child);
}

/// Constructor of `cls` (a pool miss). Includes every constructor-time side
/// effect: models, theme config (RNG), theme hooks.
pub fn construct(g: &mut Game, cls: Cls) -> EntityId {
    match cls {
        Cls::Track(k) => {
            // K ctor: build() mounts the group's parts on the entity itself.
            let id = plain(g, cls);
            let (_, group, _rails) = k.info();
            if g.lib.has_group(group) {
                let lib = g.lib.clone();
                let root = g.ent(id).root;
                g.scene.mount_entity(&lib, root, group, &MatOpts::default());
            }
            id
        }
        Cls::Filler(k) => {
            let id = plain(g, cls);
            let (_, group) = k.info();
            let root = g.ent(id).root;
            g.scene.get_mut(root).name = Some(group.into());
            if g.lib.has_group(group) {
                let lib = g.lib.clone();
                g.scene.mount_entity(&lib, root, group, &MatOpts::default());
                theme::handle_chunk_config(g, ConfigSet::Filler, root, &[group], false, &[0.0]);
            }
            id
        }
        Cls::Po => {
            let mut body = Body::new(80.0, 0.0, BLOCK_SIZE * 2.0);
            body.deco = true;
            body.set_bottom(0.0);
            let id = with_body(g, cls, body);
            let view = get_entity(g, "tube", &MatOpts::default());
            {
                let v = g.scene.get_mut(view);
                v.rot.y = PI;
                v.pos.z = BLOCK_SIZE; // body.depth * 0.5
                v.pos.y = 0.0;
            }
            let root = g.ent(id).root;
            add_obj(g, root, view);
            g.ent_mut(id).view = Some(view);
            id
        }
        Cls::Fo => {
            let mut body = Body::new(80.0, 16.0, BLOCK_SIZE);
            body.set_top(88.0);
            with_body(g, cls, body)
        }
        // io ctor (11397): the roof the pillars carry
        Cls::PillarsEnv => {
            let mut body = Body::new(80.0, 20.0, BLOCK_SIZE);
            body.set_top(90.0);
            with_body(g, cls, body)
        }
        // to/no/ro ctors (11323-11393): deco body, then the tunnel config
        // dresses the entity with Bali_pillars_* (flip, half a block in)
        Cls::PillarPiece(k) => {
            let name = ["pillars_start", "pillars_mid", "pillars_end"][k as usize];
            let mut body = Body::new(80.0, 1.0, BLOCK_SIZE);
            body.deco = true;
            body.set_bottom(0.0);
            let id = with_body(g, cls, body);
            let root = g.ent(id).root;
            theme::handle_chunk_config(g, ConfigSet::Tunnel, root, &[name], true, &[BLOCK_SIZE * 0.5]);
            id
        }
        Cls::Epic(k) => {
            let name = ["epic_start", "epic_mid", "epic_end"][k as usize];
            let id = plain(g, cls);
            let root = g.ent(id).root;
            g.scene.get_mut(root).name = Some(name.into());
            // $r ctor: config before the model
            theme::handle_chunk_config(g, ConfigSet::Special, root, &[name], false, &[0.0]);
            let model = get_entity(g, name, &MatOpts::default());
            add_obj(g, root, model);
            match k {
                // bali handleEpicStart: removal offset -600 (+ swamp fog particles)
                0 => {
                    g.ent_mut(id).has_initialized = true;
                    g.ent_mut(id).removal_offset = -600.0;
                    for (x, z) in [(-91.6, 366.1), (93.7, 366.1)] {
                        let p = particles(g, "fog", id);
                        let o = g.scene.get_mut(p);
                        o.pos = DVec3::new(x, 0.0, z);
                        o.rot.x = -PI * 0.5;
                        add_obj(g, root, p);
                    }
                }
                // handleEpicMid/End remove the model again
                _ => g.scene.remove_from_parent(model),
            }
            id
        }
        Cls::Ri => {
            let id = plain(g, cls);
            let model = get_entity(g, "gates_base", &MatOpts::default());
            let root = g.ent(id).root;
            add_obj(g, root, model);
            g.ent_mut(id).model = Some(model);
            id
        }
        Cls::Zi => {
            let id = with_body(g, cls, Body::default());
            g.ent_mut(id).removable_on_crash = true;
            id
        }
        Cls::Li => {
            let mut b = Body::new(80.0, 50.0, 160.0);
            b.trigger = true;
            let id = with_body(g, cls, b);
            g.ent_mut(id).removable_on_crash = true;
            id
        }
        Cls::Gate(k) => {
            let mut b = Body::new(80.0, 50.0, 120.0);
            b.trigger = true;
            let id = with_body(g, cls, b);
            g.ent_mut(id).removable_on_crash = true;
            let root = g.ent(id).root;
            g.scene.get_mut(root).name = Some(k.model().into());
            let model = get_entity(g, k.model(), &MatOpts::default());
            {
                let m = g.scene.get_mut(model);
                m.rot.y = PI;
                m.pos.y = -25.0;
            }
            if k == GateKind::Sides {
                let m = g.scene.get_mut(model);
                m.scale.x = 0.9;
                m.pos.x = -1.0;
            } else {
                add_obj(g, root, model);
            }
            theme::handle_chunk_config(g, ConfigSet::Special, model, &[k.model()], false, &[0.0]);
            // bali handleGates*: spark particle systems on the gate entity
            let xs: &[f64] = match k {
                GateKind::Right => &[12.100000000000001, 29.5],
                GateKind::Left => &[-27.9, -10.5],
                GateKind::Mid => &[-7.899999999999999, 9.5],
                GateKind::Sides => &[-27.9, -10.5, 12.100000000000001, 29.5],
            };
            for &x in xs {
                let p = particles(g, "spark", id);
                let o = g.scene.get_mut(p);
                o.pos.x = x;
                o.rot.x = -PI * 0.5;
                add_obj(g, root, p);
            }
            g.ent_mut(id).model = Some(model);
            id
        }
        Cls::Blocker(k) => {
            let id = with_body(g, cls, Body::new(16.0, 26.0, 10.0));
            g.ent_mut(id).removable_on_crash = true;
            let _ = k;
            id
        }
        Cls::No => {
            let mut b = Body::new(60.0, 30.0, 1.0);
            b.trigger = true;
            with_body(g, cls, b)
        }
        Cls::Rr => {
            let mut b = Body::new(16.0, 100.0, 1.0);
            b.trigger = true;
            let id = with_body(g, cls, b);
            g.ent_mut(id).removable_on_crash = true;
            id
        }
        Cls::Coin => {
            let mut b = Body::new(5.0, 5.0, 5.0);
            b.ghost = true;
            let id = with_body(g, cls, b);
            g.ent_mut(id).movable = Some(Movable::default());
            // gr shine: halo plane 13x13 at z -1.2, scale 2
            let halo = g.scene.new_obj();
            g.scene.get_mut(halo).visual = Visual::Procedural { geometry: "BufferGeometry", map: Some("halo".into()), double_sided: true, plane: Some([13.0, 13.0, 0.2]) };
            {
                let h = g.scene.get_mut(halo);
                h.pos.z = -1.2;
                h.scale = DVec3::splat(2.0);
            }
            let root = g.ent(id).root;
            add_obj(g, root, halo);
            // build(): model
            let model = get_entity(g, "currency_coin", &MatOpts::default());
            add_obj(g, root, model);
            let e = g.ent_mut(id);
            e.model = Some(model);
            e.shine = Some(halo);
            e.spin = Some(0.06);
            id
        }
        Cls::Train(k) => {
            let id = with_body(g, cls, Body::new(18.0, 29.0, 58.0));
            g.body(id).deco = true;
            g.ent_mut(id).removable_on_crash = true;
            g.ent_mut(id).movable = Some(Movable::default());
            let _ = k;
            id
        }
        Cls::Ramp => {
            let id = with_body(g, cls, Body::new(18.0, 29.0, 70.0));
            g.ent_mut(id).removable_on_crash = true;
            let model = get_entity(g, "train_ramp", &MatOpts::default());
            {
                let m = g.scene.get_mut(model);
                m.pos.y = -14.5;
                m.rot.y = PI;
                m.pos.z = -8.0;
            }
            let root = g.ent(id).root;
            add_obj(g, root, model);
            g.ent_mut(id).model = Some(model);
            id
        }
        Cls::RampWall => {
            let id = with_body(g, cls, Body::default());
            g.ent_mut(id).removable_on_crash = true;
            id
        }
        Cls::LightSignal => {
            let mut b = Body::new(4.0, 42.0, 4.0);
            b.soft = true;
            let id = with_body(g, cls, b);
            g.ent_mut(id).removable_on_crash = true;
            id
        }
        Cls::Pickup(k) => construct_pickup(g, k),
        Cls::Checkpoint => {
            let mut b = Body::new(2.0, 20.0, 2.0);
            b.deco = true;
            with_body(g, cls, b)
        }
        Cls::Pillar => {
            let id = with_body(g, cls, Body::new(9.0, 80.0, 9.0));
            g.ent_mut(id).removable_on_crash = true;
            let model = get_entity(g, "pillar", &MatOpts::default());
            {
                let m = g.scene.get_mut(model);
                m.rot.y = PI;
                m.pos.y = -40.0;
            }
            let root = g.ent(id).root;
            add_obj(g, root, model);
            g.ent_mut(id).model = Some(model);
            id
        }
        // Gi (10000): ghost 18x14x1 plate, removable on crash; the model
        // (dumpster / bush_1 / powerBox) is attached by awake
        Cls::Obstacle(k) => {
            let mut b = Body::new(18.0, 14.0, 1.0);
            b.ghost = true;
            let id = with_body(g, cls, b);
            g.ent_mut(id).removable_on_crash = true;
            let model = get_entity(g, ["dumpster", "bush_1", "powerBox"][k as usize], &MatOpts::default());
            g.ent_mut(id).model = Some(model);
            id
        }
        // fo / po / mo (11666): deco floor plate, the station_* view (ry PI)
        // dressed once by the tunnel config; mo adds two swamp-fog systems
        Cls::Station(k) => {
            let name = ["station_start", "station_mid", "station_end"][k as usize];
            let mut b = Body::new(80.0, 0.0, [BLOCK_SIZE, BLOCK_SIZE * 2.0, BLOCK_SIZE][k as usize]);
            b.deco = true;
            b.set_bottom(0.0);
            let id = with_body(g, cls, b);
            let view = get_entity(g, name, &MatOpts::default());
            {
                let v = g.scene.get_mut(view);
                v.rot.y = PI;
                v.pos.y = 0.0;
            }
            let root = g.ent(id).root;
            add_obj(g, root, view);
            g.ent_mut(id).view = Some(view);
            theme::handle_chunk_config(g, ConfigSet::Tunnel, view, &[name], false, &[0.0]);
            if k == 2 {
                // bali handleStationEnd: createSwampFog x2 on the entity
                for x in [-102.5, 88.5] {
                    let p = particles(g, "fog", id);
                    let o = g.scene.get_mut(p);
                    o.pos = DVec3::new(x, 0.0, 45.0);
                    o.rot.x = -PI * 0.5;
                    add_obj(g, root, p);
                }
            }
            id
        }
        // ho (11732): the solid station roof 80x4x360 (center 0 until awake)
        Cls::StationRoof => with_body(g, cls, Body::new(80.0, 4.0, BLOCK_SIZE * 4.0)),
        // built by mount_static_geometry with the library object as its root
        Cls::StaticGeometry => plain(g, cls),
        // uo (11689): deco platform with the station_platforms view at local z +90
        Cls::StationPlatform => {
            let mut b = Body::new(80.0, 2.0, BLOCK_SIZE * 2.0);
            b.deco = true;
            let id = with_body(g, cls, b);
            g.ent_mut(id).removable_on_crash = true;
            let view = get_entity(g, "station_platforms", &MatOpts::default());
            {
                let v = g.scene.get_mut(view);
                v.rot.y = PI;
                v.pos.z = BLOCK_SIZE;
            }
            let root = g.ent(id).root;
            add_obj(g, root, view);
            g.ent_mut(id).view = Some(view);
            id
        }
        // lo (11601): the platform's invisible solid side collider
        Cls::StationSide => {
            let id = with_body(g, cls, Body::new(0.0, 0.0, 0.0));
            g.ent_mut(id).removable_on_crash = true;
            id
        }
        Cls::Bag => {
            let mut b = Body::new(4.0, 4.0, 4.0);
            b.deco = true;
            let id = with_body(g, cls, b);
            let model = get_entity(g, "startScreen_bag_base", &MatOpts { map: Some("props-tex".into()), ..Default::default() });
            {
                let m = g.scene.get_mut(model);
                m.rot.y = PI;
                m.pos.y = -1.3;
            }
            let root = g.ent(id).root;
            add_obj(g, root, model);
            g.ent_mut(id).model = Some(model);
            id
        }
        Cls::IntroTrain => {
            let id = plain(g, cls);
            g.ent_mut(id).level_entity = false;
            let model = get_entity(g, "train_start", &MatOpts::default());
            g.scene.get_mut(model).rot.y = PI;
            let root = g.ent(id).root;
            add_obj(g, root, model);
            // score sign: canvas texture plane (hidden until a score exists)
            let sign = g.scene.new_obj();
            {
                let s = g.scene.get_mut(sign);
                s.rot.y = PI * 0.5;
                s.pos = DVec3::new(9.0, 5.3, -7.0);
            }
            add_obj(g, root, sign);
            // score plane: rx 0.25, scale = canvas size * 0.01 (no highscore:
            // 25x24 px canvas), hidden while the score text is empty
            let plane = g.scene.new_obj();
            {
                let o = g.scene.get_mut(plane);
                o.visual = Visual::Procedural { geometry: "BufferGeometry", map: None, double_sided: true, plane: None };
                o.visible = false;
                o.rot.x = 0.25;
                o.scale = DVec3::new(0.25, 0.24, 1.0);
            }
            add_obj(g, sign, plane);
            id
        }
        Cls::Skyline => {
            let id = plain(g, cls);
            g.ent_mut(id).level_entity = false;
            let root = g.ent(id).root;
            g.scene.get_mut(root).rot.y = PI;
            let m = get_entity(g, "sl_monument_1", &MatOpts::default());
            add_obj(g, root, m);
            id
        }
    }
}

/// `H.unityParticle(...)`: a particle system object (procedural geometry).
/// `H.unityParticle(cfg, map, ...)` on a scenery entity: the scene object
/// (the particle texture lives in a shader uniform the recorder doesn't
/// see) and its `An` system (`crate::fx`), updated while `owner` is in the
/// scene.
fn particles(g: &mut Game, kind: &str, owner: EntityId) -> ObjId {
    let p = g.scene.new_obj();
    g.scene.get_mut(p).visual = Visual::Procedural { geometry: "InstancedBufferGeometry", map: None, double_sided: true, plane: None };
    g.fx.add_scenery(owner, p, kind);
    p
}

/// `Ha` + subclass constructors (10959-11276).
fn construct_pickup(g: &mut Game, k: PickupKind) -> EntityId {
    let mut b = Body::new(12.0, 12.0, 12.0);
    b.ghost = true;
    let id = with_body(g, Cls::Pickup(k), b);
    let root = g.ent(id).root;
    // Ba halo: view (z -3, ry PI) holding powBoost
    let view = g.scene.new_obj();
    let halo = get_entity(
        g,
        "powBoost",
        &MatOpts { map: Some("effects-tex".into()), opacity: Some(0.9), blend_mode: Some(crate::scene::BlendMode::Add), ..Default::default() },
    );
    add_obj(g, view, halo);
    {
        let v = g.scene.get_mut(view);
        v.pos.z = -3.0;
        v.rot.y = PI;
    }
    add_obj(g, root, view);
    {
        let e = g.ent_mut(id);
        e.view = Some(view);
        e.halo = Some(halo);
        e.has_halo = true;
    }
    let props = MatOpts { map: Some("props-tex".into()), ..Default::default() };
    let model = match k {
        PickupKind::Letter => Some(g.scene.new_obj()),
        PickupKind::MysteryBox => {
            let m = g.scene.new_obj();
            let base = get_entity(g, "mysteryBox_default", &props);
            add_obj(g, m, base);
            Some(m)
        }
        _ => {
            let (group, s) = match k {
                PickupKind::Jetpack => ("powerups_jetpack", 1.5),
                PickupKind::Pogo => ("powerups_rocketPogo", 1.75),
                PickupKind::Magnet => ("powerups_coinMagnet", 1.5),
                PickupKind::Sneakers => ("powerups_superSneakers", 1.5),
                PickupKind::Multiplier => ("powerups_2xMultiplier", 1.5),
                PickupKind::Key => ("currency_key", 1.5),
                _ => unreachable!(),
            };
            let m = get_entity(g, group, &props);
            g.scene.get_mut(m).scale = DVec3::splat(s);
            Some(m)
        }
    };
    g.ent_mut(id).model = model;
    if k == PickupKind::Letter {
        g.ent_mut(id).bounce = Some(0.0);
    } else {
        g.ent_mut(id).spin = Some(-0.03);
    }
    id
}

/// `Ki/qi/Ji.init` (every `pool.get(cls, {})`): height 14 / 12 / 8, model
/// y -H/2; the power box adds its two bushes each time (they stack on reuse,
/// as in the original).
pub fn obstacle_init(g: &mut Game, id: EntityId, k: u8) {
    let h = [14.0, 12.0, 8.0][k as usize];
    g.body(id).set_sy(h);
    if let Some(m) = g.ent(id).model {
        g.scene.get_mut(m).pos.y = -h * 0.5;
    }
    if k == 2 {
        let root = g.ent(id).root;
        for (p, rz, s) in [(DVec3::new(-4.97, -4.0, -6.4), -6.8f64, 0.67), (DVec3::new(4.63, -4.28, -6.4), 18.2, 0.54)] {
            let bush = get_entity(g, "bush_1", &MatOpts::default());
            let o = g.scene.get_mut(bush);
            o.pos = p;
            o.rot.y = PI;
            o.rot.z = rz.to_radians();
            o.scale = DVec3::splat(s);
            add_obj(g, root, bush);
        }
    }
}

/// `Gi.awake` (10016): model ry PI, z -4, attached.
pub fn obstacle_awake(g: &mut Game, id: EntityId) {
    if let Some(m) = g.ent(id).model {
        let o = g.scene.get_mut(m);
        o.rot.y = PI;
        o.pos.z = -4.0;
        let root = g.ent(id).root;
        if g.scene.get(m).parent != Some(root) {
            add_obj(g, root, m);
        }
    }
}

/// `sendMessage("respawn")` on add (component respawns that matter).
pub fn respawn(g: &mut Game, id: EntityId) {
    if g.ent(id).has_halo {
        // Ba.respawn: view and halo scale 1
        let (v, h) = (g.ent(id).view.unwrap(), g.ent(id).halo.unwrap());
        g.scene.get_mut(v).scale = DVec3::ONE;
        g.scene.get_mut(h).scale = DVec3::ONE;
    }
    if matches!(g.ent(id).cls, Cls::Coin | Cls::Pickup(_)) {
        // Collectible.respawn: scale 1; Attractable.respawn: not attracted, body.movable = false
        let root = g.ent(id).root;
        g.scene.get_mut(root).scale = DVec3::ONE;
        let sneakers_only = matches!(g.ent(id).cls, Cls::Pickup(_));
        let e = g.ent_mut(id);
        match e.attract.as_mut() {
            Some(a) => a.attracted = false,
            None => e.attract = Some(crate::powerups::Attract { sneakers_only, ..Default::default() }),
        }
        if let Some(b) = g.ent_mut(id).body.as_mut() {
            b.movable = false;
        }
    }
}

/// `Collectible.collect` (5552).
pub fn collect(g: &mut Game, id: EntityId) {
    let root = g.ent(id).root;
    g.scene.get_mut(root).scale = DVec3::splat(0.0001);
    g.set_active(id, false);
    if let Some(b) = g.ent_mut(id).body.as_mut() {
        b.movable = false;
        b.reset_velocity();
    }
    if let Some(a) = g.ent_mut(id).attract.as_mut() {
        a.attracted = false;
    }
    if g.ent(id).has_halo {
        // Ha.onCollect -> halo.hide()
        let (v, h) = (g.ent(id).view.unwrap(), g.ent(id).halo.unwrap());
        g.scene.get_mut(v).scale = DVec3::splat(0.00001);
        g.scene.get_mut(h).scale = DVec3::splat(0.00001);
    }
}

/// Per-frame component updates that change what the level looks like or
/// where it is (allEntities order = level entity order here).
pub fn update_components(g: &mut Game) {
    let running = g.state == GameState::Running;
    let ids: Vec<EntityId> = g.level_entities.clone();
    for id in ids {
        // Attractable (ar.update) runs before the coin's movable
        crate::powerups::attract_update(g, id);
        // Movable (mr.update): snap to the last destination, aim for the new
        // one (velocity); physics moves the body after the render copy.
        if let Some(m) = g.ent(id).movable.clone() {
            if !running {
                g.body(id).set_vz(0.0);
            } else if m.speed != 0.0 && g.ent(id).active {
                let mut m = m;
                if let Some(ld) = m.last_dest {
                    let b = g.body(id);
                    b.set_back(ld);
                    b.origin.set_back(ld);
                }
                let dest = m.origin + (m.target - g.stats_z) * m.speed;
                let back = g.ent(id).body.as_ref().unwrap().back();
                g.body(id).set_vz(dest - back);
                m.last_dest = Some(dest);
                g.ent_mut(id).movable = Some(m);
                // locomotive lights blink while the hero is in the train's lane
                let lights = g.ent(id).lights.clone();
                if !lights.is_empty() {
                    let root = g.ent(id).root;
                    let tx = g.scene.get(root).pos.x;
                    let on = if (g.hero_x - tx).abs() < 10.0 { g.now_ms() % 400.0 < 200.0 } else { true };
                    for l in lights {
                        g.scene.set_active(l, on);
                    }
                }
            }
        }
        // Light signal flicker (Vi.update)
        if let Some((t, light)) = g.ent(id).flicker {
            let t = t + 1.0;
            let idx = (((t / 100.0) * 5.0) % 2.0).floor() as usize;
            g.ent_mut(id).flicker = Some((t, light));
            g.scene.set_active(light, idx == 0);
        }
        // Floating (dr.update): global counter `ur` seeds the start rotation
        if let Some(speed) = g.ent(id).spin {
            if let Some(model) = g.ent(id).model {
                if g.ent(id).spin_start.is_none() {
                    let start = g.ur as f64 * 0.4;
                    g.ur += 1;
                    let e = g.ent_mut(id);
                    e.spin_start = Some(start);
                    e.spin_rot = start;
                }
                g.ent_mut(id).spin_rot -= speed;
                let bz = g.ent(id).body.as_ref().unwrap().cz();
                if g.hero_z - bz < 600.0 {
                    g.scene.get_mut(model).rot.y = g.ent(id).spin_rot;
                }
            }
        }
        // Coin shine (gr.update)
        if let Some(halo) = g.ent(id).shine {
            let bz = g.ent(id).body.as_ref().unwrap().cz();
            let v = 1.0 - (g.hero_z - bz) / 400.0;
            if v < 0.0 {
                g.scene.set_active(halo, false);
            } else {
                g.scene.get_mut(halo).scale = DVec3::splat(v);
                g.scene.set_active(halo, v > 0.5);
            }
        }
        // Letter bounce (La.update)
        if let Some(b) = g.ent(id).bounce {
            if let Some(model) = g.ent(id).model {
                let b = b + 0.1;
                g.ent_mut(id).bounce = Some(b);
                let m = g.scene.get_mut(model);
                m.pos.y = (b.sin() * 3.0).abs();
                m.rot.z = b.sin() * 0.2;
            }
        }
        // Pickup halo (Ba.update)
        if g.ent(id).has_halo && g.ent(id).active {
            let bz = g.ent(id).body.as_ref().unwrap().cz();
            let (v, h) = (g.ent(id).view.unwrap(), g.ent(id).halo.unwrap());
            if (g.scene.get(v).scale.x - 0.00001).abs() > 1e-12 {
                let d = g.stats_z + 20.0 - bz;
                g.scene.get_mut(h).scale = DVec3::splat(1.5 + (d * 0.03).sin() * 0.5);
                let d = g.stats_z - 10.0 - bz;
                let t = 1.0 - (d / 500.0).clamp(0.0, 1.0);
                g.scene.get_mut(v).scale = DVec3::splat(back_out(t));
                g.scene.get_mut(h).rot.z += -0.03;
            }
        }
    }
}

/// Bali `Wr.update` (deobfuscated.js:8307) on the mesh parts of in-scene
/// entities: `t = (time._lastTime + time.deltaTime) / 1000`.
pub fn update_bobs(g: &mut Game) {
    let t = (g.clock.now + g.clock.delta_ms) / 1000.0;
    let roots: Vec<ObjId> = g.level_entities.iter().map(|&id| g.ent(id)).filter(|e| e.in_scene && e.active).map(|e| e.root).collect();
    for root in roots {
        for mid in g.scene.meshes(root) {
            if let Some((origin, dir, freq)) = g.scene.get(mid).bob {
                g.scene.get_mut(mid).pos = origin + dir * (t * freq).sin();
            }
        }
    }
}

/// `Ra.backOut` (s = 1.70158).
pub fn back_out(t: f64) -> f64 {
    let s = 1.70158;
    let t = t - 1.0;
    t * t * ((s + 1.0) * t + s) + 1.0
}

/// Component render phase: `Body.render` copies `box.center` into the
/// entity position (before physics moves the body).
pub fn render(g: &mut Game) {
    for i in 0..g.ents.len() {
        let e = &g.ents[i];
        if !e.in_scene {
            continue;
        }
        if let Some(c) = e.body.as_ref().map(|b| b.center()) {
            let root = e.root;
            g.scene.get_mut(root).pos = c;
        }
    }
}
