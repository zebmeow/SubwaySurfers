//! The game simulation core: systems that place and cull the level.
//!
//! Mirrors the JS classes closely (line refs into
//! decompiled/js_src/deobfuscated.js); see docs/js_notes/*.md:
//! * [`Game`]       — `Pg` (scene, systems, pool)
//! * level methods  — `xg` LevelSystem (48513-48776)
//! * route methods  — `Eg` RouteSystem + `Tg` builder (49104-49345)
//! * chunk mounting — `bg` Chunk, in [`crate::mount`]
//! * environment    — `Tm`, `K`, `ii`, `Fo`/`Po`, `$r`, in [`crate::environment`]
//! * theme config   — `Yr` engine, in [`crate::theme`]
//!
//! * hero           — `Gp` and its components, in [`crate::hero`]
//! * physics        — `Sg`, in [`crate::physics`]
//! * camera         — `sm` / `om` / intro `hg`, in [`crate::camera`]
//!
//! The only per-frame input is the keyboard ([`FrameInput`]).

use crate::data::{self, ChunkData, Node, Section};
use crate::camera::{Camera, Intro};
use crate::entities::{Body, Cls, Entity, EntityId};
use crate::hero::{Controller, Hero, Key};
use crate::library::Library;
use crate::rng::{Rng, Site};
use crate::scene::{ObjId, Scene};
use crate::theme::ThemeConfig;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub const BLOCK_SIZE: f64 = 90.0;
pub const LANE_WIDTH: f64 = 20.0;
pub const VISIBLE_MAX_DISTANCE: f64 = 1000.0;
pub const VISIBLE_MIN_DISTANCE: f64 = -500.0;

/// RNG call sites (oracle stream keys) used by the level/route code.
pub mod sites {
    use crate::rng::Site;
    pub const GET_SEQUENCE_MID: Site = Site("at R.item (assets/index-QNpTjs8S.js:1:56829) < at Eg.getSequence (assets/index-QNpTjs8S.js:1:1186553)");
    pub const SECTION_BY_LEVEL: Site = Site("at R.item (assets/index-QNpTjs8S.js:1:56985) < at Tg.getSectionByLevel (assets/index-QNpTjs8S.js:1:1182610)");
    pub const ENV_CAN_SPAWN: Site = Site("at R.item (assets/index-QNpTjs8S.js:1:56829) < at Tm.canSpawn (assets/index-QNpTjs8S.js:1:1024554)");
    pub const ENV_SETUP: Site = Site("at R.item (assets/index-QNpTjs8S.js:1:56829) < at Tm.setup (assets/index-QNpTjs8S.js:1:1025923)");
    pub const MOUNT_RANDOMIZER: Site = Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at bg.mount (assets/index-QNpTjs8S.js:1:1154296)");
    pub const MOUNT_OFFSET: Site = Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at bg.mount (assets/index-QNpTjs8S.js:1:1155031)");
    pub const MOUNT_MIRROR: Site = Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at bg.mount (assets/index-QNpTjs8S.js:1:1155148)");
    pub const TRAIN_TYPE: Site = Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at So.factory (assets/index-QNpTjs8S.js:1:344379)");
    pub const BLOCKER_TYPE: Site = Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at vr.factory (assets/index-QNpTjs8S.js:1:183520)");
    pub const OBSTACLE_PICK: Site = Site("at R.pick (assets/index-QNpTjs8S.js:1:56649) < at Ui.<computed> [as factory] (assets/index-QNpTjs8S.js:1:294618)");
    pub const PICKUP_TYPE: Site = Site("at R.item (assets/index-QNpTjs8S.js:1:56829) < at Ha.factory (assets/index-QNpTjs8S.js:1:316324)");
    pub const FILLER_KIND: Site = Site("at R.item (assets/index-QNpTjs8S.js:1:56829) < at e.mount (assets/index-QNpTjs8S.js:1:267910)");
    pub const FILLER_NUM: Site = Site("at R.item (assets/index-QNpTjs8S.js:1:56829) < at e.mount (assets/index-QNpTjs8S.js:1:268147)");
    pub const CFG_MESH_TOP: Site = Site("at e.addMeshFromDef (assets/index-QNpTjs8S.js:1:255085) < at assets/index-QNpTjs8S.js:1:253472");
    pub const CFG_CONTAINER_TOP: Site = Site("at e.addContainerFromDef (assets/index-QNpTjs8S.js:1:254357) < at assets/index-QNpTjs8S.js:1:253649");
    pub const CFG_CONTAINER_CHILD: Site = Site("at e.addContainerFromDef (assets/index-QNpTjs8S.js:1:254357) < at assets/index-QNpTjs8S.js:1:254049");
    pub const CFG_MESH_CHILD: Site = Site("at e.addMeshFromDef (assets/index-QNpTjs8S.js:1:255085) < at assets/index-QNpTjs8S.js:1:253969");
    pub const CFG_RCR_ACTIVATE: Site = Site("at e.randomChildRandomizer (assets/index-QNpTjs8S.js:1:257158) < at assets/index-QNpTjs8S.js:1:256709");
    pub const CFG_RCR_INDEX: Site = Site("at e.randomChildRandomizer (assets/index-QNpTjs8S.js:1:257256) < at assets/index-QNpTjs8S.js:1:256709");
}

/// `R` helpers (deobfuscated.js:2175-2214): one `Math.random()` each.
pub fn r_index(rng: &mut dyn Rng, site: Site, len: usize) -> usize {
    (rng.random(site) * len as f64).floor() as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameState {
    Idle,
    Running,
    Paused,
    Gameover,
}

/// Per-frame input: keys pressed (keydown) before this frame's update.
#[derive(Clone, Debug, Default)]
pub struct FrameInput {
    pub keys: Vec<Key>,
    /// UI navigation calls between the previous frame and this one
    /// (`nav.toGame()` = PLAY, `nav.toIdleScreen("title")` = Menu).
    pub nav: Vec<Nav>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    /// `nav.toGame()`: results / title PLAY, pause restart.
    ToGame,
    /// `nav.toIdleScreen("title")`: results / pause Menu.
    ToTitle,
}

/// Game clock `Mg` (49837) on the oracle's virtual `performance.now()`
/// (epoch 131072 ms, `+= 1000/60` per frame): smoothed delta in ms.
#[derive(Clone, Debug)]
pub struct Clock {
    pub now: f64,
    pub last: f64,
    pub delta_ms: f64,
}

pub const FRAME_MS: f64 = 1000.0 / 60.0;

impl Default for Clock {
    fn default() -> Self {
        Self { now: 131072.0, last: 131072.0, delta_ms: FRAME_MS }
    }
}

impl Clock {
    /// `Mg.nextUpdate` (smoothDelta 0.2, maxDeltaTime 100).
    pub fn next_update(&mut self) {
        self.now += FRAME_MS;
        self.update_delta();
    }
    /// The delta part of `nextUpdate` (skipped on PAUSED frames, while
    /// `performance.now()` keeps advancing: a 2-frame step afterwards).
    pub fn update_delta(&mut self) {
        let raw = (self.now - self.last).min(100.0);
        self.delta_ms -= (self.delta_ms - raw) * 0.2;
        self.last = self.now;
    }
    /// `game.delta` = `time.frameTime` (frames, ~1).
    pub fn frame_time(&self) -> f64 {
        self.delta_ms / FRAME_MS
    }
    /// `game.deltaSecs`.
    pub fn delta_secs(&self) -> f64 {
        self.delta_ms / 1000.0
    }
}

/// What happened, for parity checks and for the renderer.
#[derive(Clone, Debug)]
pub enum LogEvent {
    ChunkPlace { frame: i64, chunk: usize, name: String, start: f64, z: f64, length: f64, blocks: f64 },
    Spawn { frame: i64, entity: EntityId, chunk: Option<usize> },
    Despawn { frame: i64, entity: EntityId },
    /// Collected by the hero (hero body center at that sub-step).
    Pickup { frame: i64, entity: EntityId, hero: bevy::math::DVec3 },
    /// Solid hit (`rn` flags, intersection box before the push).
    Collision { frame: i64, entity: EntityId, flags: u32, hit: crate::entities::Aabb },
    Trigger { frame: i64, entity: EntityId, enter: bool },
    /// Run flow milestones (`game_runFromIntro`).
    RunFromIntro { frame: i64 },
}

/// A placed chunk (`bg`).
#[derive(Clone, Debug, Default)]
pub struct Chunk {
    pub id: usize,
    pub node: Option<Node>,
    pub name: String,
    pub blocks: f64,
    pub length: f64,
    pub z: f64,
    pub start: f64,
    pub end: f64,
    pub index: usize,
    pub env_tube: bool,
    pub env_station: bool,
    pub env_epic: bool,
    pub env_gates: bool,
    pub env_empty: bool,
    pub env_pillars: bool,
    pub has_ground: bool,
    pub filler_slots: HashMap<i64, bool>,
    pub floor_slots: HashMap<i64, bool>,
    /// The chunk's checkpoints (`Cr`, `chunk.checkpoints`).
    pub checkpoints: Vec<crate::entities::EntityId>,
}

/// Mount options (`opts` copied down the node tree).
#[derive(Clone, Copy, Debug, Default)]
pub struct MountOpts {
    pub flip: bool,
    /// `null` vs a lane offset (0 is a valid pick, and falsy for most factories).
    pub offset_x: Option<f64>,
}

pub struct Game {
    pub lib: Rc<Library>,
    pub chunks_data: Rc<ChunkData>,
    pub theme: Rc<ThemeConfig>,
    pub rng: Box<dyn Rng>,
    pub scene: Scene,
    pub ents: Vec<Entity>,
    /// `game.pool`: per-class LIFO + return queue.
    pub pool: HashMap<Cls, Vec<EntityId>>,
    pub return_queue: Vec<EntityId>,
    /// Root object entities are added to (`game` itself).
    pub root: ObjId,
    // JS module-level state
    /// `Ro.matchMap` (class match cache by node name).
    pub match_map: HashMap<String, Option<crate::mount::NodeClass>>,
    /// `Ha.initialSpawn`.
    pub pickup_initial_spawn: bool,
    /// `yg` intro train singleton.
    pub intro_train: Option<EntityId>,
    /// `Jr`: objects the theme config was applied to.
    pub configured: HashSet<ObjId>,
    pub frame: i64,
    pub state: GameState,
    pub input: FrameInput,
    /// Current hunt letter (`Na()`, date dependent), e.g. "S".
    pub hunt_letter: Option<String>,
    pub log: Vec<LogEvent>,
    /// Chunk being mounted (for log attribution).
    pub mounting_chunk: Option<usize>,

    // LevelSystem (xg)
    pub level_entities: Vec<EntityId>,
    pub chunks: Vec<Chunk>,
    pub next_position: f64,
    pub pre_update_count: i64,
    pub post_update_count: i64,
    pub sequence: Vec<String>,
    pub queued: Vec<String>,
    pub chunk_seq: usize,
    pub stats_chunk_index: usize,
    pub current_chunk: Option<usize>,

    // RouteSystem (Eg / Tg)
    pub first_passed: bool,
    pub builder_level: i64,
    /// `availableSections` in insertion order (short name, section).
    pub available_sections: Vec<(String, &'static Section)>,
    pub spawns: HashMap<String, f64>,
    pub tutorial_enabled: bool,
    /// Interactive play may run the tutorial (replays and tests never).
    pub tutorial_allowed: bool,
    /// The tutorial's arrow and message (`crate::tutorial`).
    pub tutorial: crate::tutorial::Tutorial,

    // Pickup system (wg): timed pickups (type, timer, target)
    pub timed_pickups: Vec<(&'static str, f64, f64)>,
    pub letters_in_scene: i64,

    // EnvironmentSystem (Tm)
    pub skyline: Option<EntityId>,

    // Stats (Og)
    pub stats_x: f64,
    pub stats_y: f64,
    pub stats_z: f64,
    /// Seconds of RUNNING time (sum of deltaSecs).
    pub stats_time: f64,
    pub coins: i64,
    /// `stats.data.score` (distance x multiplier, unscaled).
    pub score_raw: f64,
    /// `stats.multiplier` (2 with the 2x powerup) and `missionMultiplier`.
    pub multiplier: f64,
    pub mission_multiplier: f64,
    /// HUD and death-flow state.
    pub hud: crate::flow::Hud,
    pub flow: crate::flow::Flow,
    /// `hero.body.z` as level components see it.
    pub hero_z: f64,
    /// `ur`: global counter of the floating component (deobfuscated.js:5590).
    pub ur: i64,
    /// Hero render position x (lights blink when in the train's lane).
    pub hero_x: f64,
    /// `performance.now()` and the smoothed frame delta.
    pub clock: Clock,
    /// `game.delta` (frameTime) this frame.
    pub delta: f64,
    /// `game.deltaSecs` this frame.
    pub delta_secs: f64,
    /// Particle systems and FX rigs (`crate::fx`).
    pub fx: crate::fx::Fx,
    /// The hero's shadow, pops and revive halo (`crate::hero_fx`).
    pub hero_fx: crate::hero_fx::HeroFx,
    /// Today's Word Hunt (interactive play; None in replays and tests, which
    /// spawn the trace's letter).
    pub word_hunt: Option<crate::word_hunt::WordHunt>,
    /// The awards' run-time state (`crate::awards`).
    pub awards: crate::awards::Awards,
    /// Sound commands for the app (`crate::audio`).
    pub sound: crate::audio::Sound,
    /// The missions system `Bm` (`crate::missions`).
    pub missions: crate::missions::Missions,

    // hero, input, camera, physics
    pub hero: Hero,
    pub guard: crate::guard::Guard,
    /// Character models and clips; animators (`Zo`) of hero, guard, dog.
    pub actors: Option<Rc<crate::anim::ActorAssets>>,
    pub hero_anim: Option<crate::anim::Animator>,
    pub guard_anim: Option<crate::anim::Animator>,
    pub dog_anim: Option<crate::anim::Animator>,
    pub hero_face: crate::anim::Face,
    pub controller: Controller,
    pub camera: Camera,
    pub intro: Intro,
    /// `physics.entities` (add order).
    pub phys: Vec<EntityId>,
    /// `Sg._hasReset`: the first RUNNING frame after a cleanup does not move.
    pub phys_has_reset: bool,
    /// `config.god`: `Player.die` is a no-op.
    pub god: bool,
    /// `config.forcePickup` (`Xa` key): every spawned pickup is this type.
    pub force_pickup: Option<crate::entities::PickupKind>,
    /// `stats.data.speedIncrease` (min, max): the speed-up board power.
    pub speed_increase: (f64, f64),
    /// `stats.data.lerpTime`.
    pub speed_lerp_time: f64,
    /// `stats.data.prizes`: mystery boxes rolled in this run, opened after death.
    pub prizes: Vec<crate::prizes::Prize>,
    /// `config.chunk`: comma list replacing every route sequence.
    pub force_chunks: Option<Vec<String>>,
    /// `stats.distanceDelta` (set with `stats.z`).
    pub distance_delta: f64,
    /// `runWithIntro` waiting for its next-frame continuation.
    pub pending_intro: bool,
    /// `Game.reseted`: `reset()` runs once between run starts.
    pub reseted: bool,
    /// Runs started (`runWithIntro` calls; port bookkeeping for seed-locked
    /// restarts, nothing in the game reads it).
    pub run_starts: u32,
}

impl Game {
    pub fn new(lib: Rc<Library>, chunks_data: Rc<ChunkData>, theme: Rc<ThemeConfig>, rng: Box<dyn Rng>) -> Self {
        let mut scene = Scene::default();
        let root = scene.new_obj();
        let mut game = Self {
            lib,
            chunks_data,
            theme,
            rng,
            scene,
            ents: Vec::new(),
            pool: HashMap::new(),
            return_queue: Vec::new(),
            root,
            match_map: HashMap::new(),
            pickup_initial_spawn: false,
            intro_train: None,
            configured: HashSet::new(),
            frame: 0,
            state: GameState::Idle,
            input: FrameInput::default(),
            hunt_letter: None,
            log: Vec::new(),
            mounting_chunk: None,
            level_entities: Vec::new(),
            chunks: Vec::new(),
            next_position: 0.0,
            pre_update_count: 10,
            post_update_count: 0,
            sequence: Vec::new(),
            queued: Vec::new(),
            chunk_seq: 0,
            stats_chunk_index: 0,
            current_chunk: None,
            first_passed: false,
            builder_level: 0,
            available_sections: Vec::new(),
            spawns: HashMap::new(),
            tutorial_enabled: false,
            tutorial_allowed: false,
            tutorial: Default::default(),
            // wg ctor: letter first (word hunt not complete), then mysteryBox.
            timed_pickups: vec![("letter", 20.0, 20.0), ("mysteryBox", 0.0, 180.0)],
            letters_in_scene: 0,
            skyline: None,
            stats_x: 0.0,
            stats_y: 0.0,
            stats_z: 0.0,
            stats_time: 0.0,
            coins: 0,
            score_raw: 0.0,
            multiplier: 1.0,
            mission_multiplier: 0.0,
            hud: crate::flow::Hud::default(),
            flow: crate::flow::Flow::default(),
            hero_z: 0.0,
            ur: 1,
            hero_x: 0.0,
            clock: Clock::default(),
            delta: 1.0,
            delta_secs: FRAME_MS / 1000.0,
            fx: Default::default(),
            hero_fx: Default::default(),
            word_hunt: None,
            sound: Default::default(),
            awards: Default::default(),
            missions: crate::missions::Missions::prepare(&crate::flow::UserData::default()),
            hero: Hero::new(),
            guard: crate::guard::Guard::default(),
            actors: None,
            hero_anim: None,
            guard_anim: None,
            dog_anim: None,
            hero_face: crate::anim::Face::default(),
            controller: Controller::default(),
            camera: Camera::default(),
            intro: Intro::default(),
            phys: Vec::new(),
            phys_has_reset: true,
            god: true,
            force_pickup: None,
            distance_delta: 0.0,
            force_chunks: None,
            prizes: Vec::new(),
            reseted: false,
            run_starts: 0,
            speed_increase: (0.0, 0.0),
            speed_lerp_time: 0.0,
            pending_intro: false,
        };
        // Game.idle() at creation: camera system goes idle
        game.camera.idle();
        game.route_reset();
        game
    }

    /// Game creation up to the oracle's recording start (frame 5):
    /// `xg.reset()` places the intro with the boot-time library, then the
    /// full library and the seeded RNG take over (the oracle reseeds there),
    /// counters are pinned and frames 1..=5 run idle.
    pub fn boot(&mut self, full_lib: Rc<Library>, rng: Box<dyn Rng>) {
        self.frame = 0;
        self.queue_chunk(Some("intro"));
        self.place_chunks();
        self.lib = full_lib;
        self.rng = rng;
        self.pre_update_count = 10;
        self.post_update_count = 0;
        for f in 1..=5 {
            self.step(f, FrameInput::default());
        }
    }

    pub fn distance(&self) -> f64 {
        -self.stats_z
    }

    /// `performance.now()`.
    pub fn now_ms(&self) -> f64 {
        self.clock.now
    }

    /// `stats.speed` (49491): 110 -> 220 units/s over 180 s of running.
    pub fn stats_speed(&self) -> f64 {
        let (mut lo, mut hi) = (110.0, 220.0);
        if self.speed_increase.0 > 0.0 {
            let t = self.speed_lerp_time;
            lo = crate::camera::lerp(lo, lo + self.speed_increase.0, t);
            hi = crate::camera::lerp(hi, hi + self.speed_increase.1, t);
        }
        let t = self.stats_time;
        let s = if t < 180.0 { lo + (hi - lo) * (t / 180.0) } else { hi };
        s / 60.0
    }
    /// `stats.animationSpeed` = 0.75 + 0.25 * speedRatio.
    pub fn animation_speed(&self) -> f64 {
        0.75 + self.speed_ratio() * 0.25
    }
    /// `stats.speedRatio`.
    pub fn speed_ratio(&self) -> f64 {
        let (min, max) = ((110.0 + self.speed_increase.0) / 60.0, (220.0 + self.speed_increase.1) / 60.0);
        (self.stats_speed() - min) / (max - min)
    }

    /// Attach the characters (before [`Game::boot`]). As in the oracle, the
    /// hero's mixer clock starts at 0 when recording starts, with the menu
    /// clip `paintIdle` playing at speed 1/60.
    pub fn install_actors(&mut self, assets: Rc<crate::anim::ActorAssets>) {
        use crate::anim::{Action, Animator};
        let mut h = Animator::new(assets.jake_clips.clone(), &assets.jake, true, "jake-");
        if let Some(c) = h.clip_index("paintIdle") {
            h.clip_loop[c] = true;
            h.clip_speed[c] = crate::anim::BASE_ANIM_SPEED;
            let mut a = Action::new_public(c);
            a.looping = true;
            h.actions.insert(c, a);
            h.active.push(c);
            h.animation = Some(c);
            h.current_action = Some(c);
            h.name = "jake-paintIdle".into();
            h.clip_name = "paintIdle".into();
            h.loop_flag = true;
        }
        self.hero_anim = Some(h);
        self.guard_anim = Some(Animator::new(assets.guard_clips.clone(), &assets.guard, false, ""));
        self.dog_anim = Some(Animator::new(assets.dog_clips.clone(), &assets.dog, false, ""));
        self.actors = Some(assets);
    }

    /// The play button: `game.runWithIntro()`. Its continuation (hero and
    /// camera intro, first chunks) runs after the next frame's update.
    pub fn run_with_intro(&mut self) {
        self.run_starts += 1;
        self.reseted = false;
        self.idle();
        crate::audio::play_theme(self);
        crate::awards::gameplay_start(self); // nav.onGameplayStart
        self.pending_intro = true;
        // game.idle() clears the current clip name; the pose FSM re-enters
        // idle on the next frame (paintIdle replayed, sudden)
        if let Some(a) = self.hero_anim.as_mut() {
            a.name.clear();
        }
        self.hero.fsm.current = "empty";
    }

    /// The title's PLAY: menus closed, `runWithIntro`.
    pub fn start_run(&mut self) {
        self.flow.menu = crate::menu::Menu::None;
        self.flow.overlay = None;
        self.run_with_intro();
    }

    /// A UI call between frames.
    pub fn nav(&mut self, n: Nav) {
        match n {
            Nav::ToGame => self.to_game(),
            Nav::ToTitle => self.to_title(),
        }
    }

    /// `nav.toGame()` (67283): close every screen, `runWithIntro` (reset in place).
    pub fn to_game(&mut self) {
        crate::flow::reset_ui(self);
        self.flow.menu = crate::menu::Menu::None;
        self.run_with_intro();
    }

    /// `nav.toIdleScreen("title")`: close every screen, `idle()`, the title.
    pub fn to_title(&mut self) {
        crate::flow::reset_ui(self);
        self.idle();
        self.flow.menu = crate::menu::Menu::Title;
    }

    /// `Game.idle()` (50060): from a run (paused / game over) back to IDLE:
    /// `reset()`, the menu pose, the idle camera. A no-op when already idle.
    pub fn idle(&mut self) {
        if self.state == GameState::Idle {
            return;
        }
        // tutorial.enabled = !user.tutorial || config.tutorial
        self.tutorial_enabled = self.tutorial_allowed && !self.flow.user.tutorial;
        self.tutorial = Default::default();
        self.reset();
        self.state = GameState::Idle;
        crate::hero::player_reset(self, 0.0, 1.2);
        crate::guard::reset(self);
        // onIdle: camera sm.idle, HUD fg.idle
        self.camera.idle();
        crate::flow::hud_reset(self);
    }

    /// `Game.reset()` (50044) with every `onReset` listener in add order
    /// (docs/js_notes/pause_reset.md §3.2, §4). Not reset: RNG streams, pickup
    /// timers, the clock, pools, skyline, user data.
    pub fn reset(&mut self) {
        if self.reseted {
            return;
        }
        self.reseted = true;
        self.flow.free_revivals = 0;
        self.flow.paid_revivals = 0;
        crate::hero::hero_reset(self);
        crate::guard::reset(self);
        // a. physics: no reset (the level cleans it); b. level
        self.level_reset();
        // c. HUD
        crate::flow::hud_reset(self);
        // d. stats (Og.reset: Dg defaults; speedIncrease is shared, kept)
        self.stats_x = 0.0;
        self.stats_y = 0.0;
        self.stats_z = 0.0;
        self.distance_delta = 0.0;
        self.score_raw = 0.0;
        self.coins = 0;
        self.flow.keys = 0;
        self.stats_chunk_index = 0;
        self.multiplier = 1.0;
        self.stats_time = 0.0;
        self.speed_lerp_time = 0.0;
        self.mission_multiplier = self.missions.multiplier() as f64;
        self.prizes.clear();
        // e. missions
        crate::missions::reset(self);
        // f. controller hide
        self.controller = Controller::default();
        // g. route
        self.route_reset();
    }

    /// `xg.reset` (48535): skipped while the current chunk is the intro.
    fn level_reset(&mut self) {
        let in_intro = self.current_chunk.and_then(|id| self.chunks.iter().find(|c| c.id == id)).is_some_and(|c| c.name.contains("intro"));
        if in_intro {
            return;
        }
        // physics.cleanup
        self.phys.clear();
        self.phys_has_reset = true;
        self.chunks.clear();
        self.sequence.clear();
        self.queued.clear();
        self.next_position = 0.0;
        self.pre_update_count = 10;
        self.post_update_count = 0;
        // removeAllEntities (reverse order)
        let mut i = self.level_entities.len();
        while i > 0 {
            i -= 1;
            self.remove_level_entity(i);
        }
        self.queue_chunk(Some("intro"));
        self.place_chunks();
    }

    /// `game.runFromIntro()` (from the intro's completion).
    fn run_from_intro(&mut self) {
        crate::guard::run(self);
        crate::hero::run(self);
        self.camera.run();
        // onRun: controller.show, camera.run, environment run
        crate::hero::controller_show(self);
        crate::flow::hud_run(self);
        self.camera.run();
        self.environment_run();
        self.state = GameState::Running;
        self.reseted = false;
        crate::tutorial::run(self);
        self.log.push(LogEvent::RunFromIntro { frame: self.frame });
    }

    /// Keydown (applied before the frame).
    pub fn press(&mut self, key: Key) {
        // the Wm buttons listen on window: they work over any screen while showing
        match key {
            // the HUD pause button (key "Escape")
            Key::Pause => return crate::flow::pause_button(self),
            Key::Headstart => return crate::boosts::activate(self, crate::boosts::BoostKind::Headstart),
            Key::ScoreBooster => return crate::boosts::activate(self, crate::boosts::BoostKind::Multiplier),
            // a button key: only Save me listens to it
            Key::ReviveKeys => {
                crate::flow::key(self, key);
                return;
            }
            _ => {}
        }
        if crate::flow::key(self, key) {
            return;
        }
        crate::hero::press(self, key);
    }

    // ---- entity plumbing ------------------------------------------------------

    pub fn ent(&self, id: EntityId) -> &Entity {
        &self.ents[id.0]
    }
    pub fn ent_mut(&mut self, id: EntityId) -> &mut Entity {
        &mut self.ents[id.0]
    }
    pub fn body(&mut self, id: EntityId) -> &mut Body {
        self.ents[id.0].body.as_mut().expect("entity has no body")
    }

    /// `pool.get(Cls)`: LIFO reuse or construct (constructor side effects
    /// include theme-config RNG draws).
    pub fn pool_get(&mut self, cls: Cls) -> EntityId {
        if let Some(id) = self.pool.get_mut(&cls).and_then(Vec::pop) {
            return id;
        }
        crate::entities::construct(self, cls)
    }
    /// `pool.get(Cls, {})`: also calls `init()` (active = true).
    pub fn pool_get_init(&mut self, cls: Cls) -> EntityId {
        let id = self.pool_get(cls);
        self.set_active(id, true);
        id
    }

    pub fn alloc_entity(&mut self, e: Entity) -> EntityId {
        self.ents.push(e);
        EntityId(self.ents.len() - 1)
    }

    pub fn set_active(&mut self, id: EntityId, active: bool) {
        self.ents[id.0].active = active;
        let root = self.ents[id.0].root;
        self.scene.set_active(root, active);
    }

    /// `game.addChild(e)`: scene add + LevelSystem `_addEntity` (+ respawn).
    pub fn add_child(&mut self, id: EntityId) {
        let root = self.ents[id.0].root;
        self.scene.add_child(self.root, root);
        if self.ents[id.0].in_scene {
            return;
        }
        self.ents[id.0].in_scene = true;
        if self.ents[id.0].level_entity && !self.level_entities.contains(&id) {
            self.level_entities.push(id);
            crate::entities::respawn(self, id);
        }
        if self.ents[id.0].cls == Cls::Pickup(crate::entities::PickupKind::Letter) {
            self.letters_in_scene += 1;
        }
        crate::physics::entity_added(self, id);
        self.fx.entity_added(id);
        self.log.push(LogEvent::Spawn { frame: self.frame, entity: id, chunk: self.mounting_chunk });
    }

    /// `xg._removeEntity` (park, splice, removeChild, pool.return).
    pub fn remove_level_entity(&mut self, idx: usize) {
        let id = self.level_entities[idx];
        if let Some(b) = self.ents[id.0].body.as_mut() {
            b.set_cz(99999.0);
        }
        let root = self.ents[id.0].root;
        self.scene.get_mut(root).pos.z = 99999.0;
        self.level_entities.remove(idx);
        self.remove_child(id);
        self.return_queue.push(id);
    }

    pub fn remove_child(&mut self, id: EntityId) {
        if !self.ents[id.0].in_scene {
            return;
        }
        self.ents[id.0].in_scene = false;
        crate::physics::entity_removed(self, id);
        self.fx.entity_removed(id);
        let root = self.ents[id.0].root;
        self.scene.remove_from_parent(root);
        if self.ents[id.0].cls == Cls::Pickup(crate::entities::PickupKind::Letter) {
            self.letters_in_scene -= 1;
        }
        self.log.push(LogEvent::Despawn { frame: self.frame, entity: id });
    }

    // ---- frame loop -------------------------------------------------------------

    /// One frame, in the JS phase order (lifecycle.md §1, hero_core.md):
    /// keydown, clock, preupdate (level culling, stats, intro), system update
    /// (camera, stats, controller, intro tween, pickups, environment),
    /// component update (hero first, then level entities), render, postupdate
    /// (pool, physics, chunk placement), then the run-flow continuation.
    pub fn step(&mut self, frame: i64, input: FrameInput) {
        // the title's PLAY (a tap, Space) asked last frame: the run starts
        if std::mem::take(&mut self.flow.start_requested) {
            self.start_run();
        }
        // UI calls made between frames (before this frame's clock advance)
        for n in &input.nav {
            self.nav(*n);
        }
        self.frame = frame;
        // hooks.js stepFrame: performance.now() advances, then queued keydowns
        // fire, then the frame callback (Mg.nextUpdate reads the new now)
        self.clock.now += FRAME_MS;
        for k in input.keys {
            self.press(k);
        }
        // Pg.update returns while PAUSED (no nextUpdate): only the UI timers run
        if self.state == GameState::Paused {
            crate::flow::after_render(self);
            return;
        }
        self.clock.update_delta();
        self.delta = self.clock.frame_time();
        self.delta_secs = self.clock.delta_secs();
        if self.speed_increase.0 > 0.0 {
            self.speed_lerp_time += if self.state == GameState::Running { self.delta_secs } else { 0.0 };
        } else {
            self.speed_lerp_time = 0.0;
        }
        // preupdate: xg (last frame's stats.z), Og, hg
        self.level_preupdate();
        self.stats_x = self.hero.position.x;
        self.stats_y = self.hero.position.y - 5.5;
        crate::flow::set_stats_z(self, self.hero.position.z);
        if crate::camera::intro_preupdate(self) {
            self.run_from_intro();
        }
        // system update
        crate::camera::update(self);
        crate::flow::hud_update(self);
        if self.state == GameState::Running {
            self.stats_time += self.delta_secs;
        }
        crate::hero::controller_update(self);
        crate::camera::intro_update(self);
        if self.state == GameState::Running {
            let ds = self.delta_secs;
            for t in &mut self.timed_pickups {
                t.1 += ds;
            }
        }
        self.environment_update();
        // component update: hero (allEntities[0]), then level entities
        crate::hero::update(self);
        crate::guard::update(self);
        crate::guard::dog_update(self);
        self.hero_z = self.hero.body.cz();
        self.hero_x = self.hero.position.x;
        crate::entities::update_components(self);
        crate::entities::update_bobs(self);
        // the particle systems' components (allEntities order among them)
        crate::fx::update_systems(self);
        crate::tutorial::update(self);
        // component render
        crate::hero::render(self);
        crate::entities::render(self);
        // postupdate: pool, physics, level
        for id in std::mem::take(&mut self.return_queue) {
            let cls = self.ents[id.0].cls;
            self.pool.entry(cls).or_default().push(id);
        }
        let rigs = crate::fx::runner_snapshot(self);
        crate::physics::postupdate(self);
        self.level_postupdate();
        crate::fx::update_rigs(self, &rigs);
        crate::guard::after_frame(self);
        // after the update: microtasks (ad revive), then timers and tickers
        crate::flow::after_update(self);
        crate::flow::after_render(self);
        crate::powerups::after_render(self);
        // runWithIntro continuation (`await nextFrame()` resolves after this
        // frame's update): hero + camera intro, first chunks
        if self.pending_intro {
            self.pending_intro = false;
            crate::hero::play_intro(self);
            crate::guard::play_intro(self);
            crate::camera::intro_play(self);
            if self.tutorial_enabled {
                self.queue_tutorial();
            } else {
                self.queue_chunk(None);
                self.queue_chunk(None);
                self.queue_chunk(None);
            }
        }
    }

    // ---- LevelSystem (xg) ---------------------------------------------------------

    pub fn level_preupdate(&mut self) {
        let c = self.pre_update_count;
        self.pre_update_count -= 1;
        if c == 0 {
            self.pre_update_count = 20;
            self.remove_obsolete_entities();
        }
    }

    pub fn level_postupdate(&mut self) {
        let c = self.post_update_count;
        self.post_update_count -= 1;
        if c != 0 {
            return;
        }
        self.post_update_count = 20;
        let chunk = self.chunks.get(self.stats_chunk_index).map(|c| c.id);
        if chunk != self.current_chunk {
            self.current_chunk = chunk;
        }
        if let Some(i) = chunk {
            let end = self.chunks.iter().find(|c| c.id == i).map(|c| c.end).unwrap_or(0.0);
            if self.distance() > end {
                self.stats_chunk_index += 1;
            }
        }
        self.place_chunks();
    }

    pub fn is_tutorial(&self) -> bool {
        let Some(id) = self.current_chunk else { return false };
        let c = self.chunks.iter().find(|c| c.id == id).unwrap();
        c.name == "routeChunk_default_tutorial" && self.distance() < c.end - 300.0
    }

    pub fn remove_obsolete_entities(&mut self) {
        if self.is_tutorial() {
            return;
        }
        let limit = self.stats_z - VISIBLE_MIN_DISTANCE;
        let mut i = self.level_entities.len();
        while i > 0 {
            i -= 1;
            let id = self.level_entities[i];
            let e = &self.ents[id.0];
            let front = match &e.body {
                Some(b) => b.front(),
                None => self.scene.get(e.root).pos.z,
            };
            if !e.active || front + e.removal_offset > limit {
                self.remove_level_entity(i);
            }
        }
    }

    pub fn place_chunks(&mut self) {
        if !self.queued.is_empty() {
            let q = std::mem::take(&mut self.queued);
            for name in q.iter().rev() {
                self.place_next_chunk(Some(name));
            }
        }
        let mut guard = 16;
        let limit = self.distance() + VISIBLE_MAX_DISTANCE;
        while self.next_position < limit && self.state == GameState::Running {
            if guard == 0 {
                panic!("Too many chunks placed at same time");
            }
            guard -= 1;
            self.place_next_chunk(None);
        }
    }

    fn next_in_sequence(&mut self) -> (Node, String) {
        if self.sequence.is_empty() {
            self.sequence = self.get_sequence();
        }
        let name = self.sequence.remove(0);
        self.chunks_data.chunk(&name).unwrap_or_else(|| panic!("chunk not found: {name}"))
    }

    pub fn queue_chunk(&mut self, name: Option<&str>) {
        let (_, n) = match name {
            Some(n) => self.chunks_data.chunk(n).unwrap(),
            None => self.next_in_sequence(),
        };
        self.queued.push(n);
        self.post_update_count = 0;
    }

    pub fn queue_tutorial(&mut self) {
        self.sequence = self.get_sequence();
        let (_, n) = self.next_in_sequence();
        self.queued.push(n);
    }

    /// `level.setSafeLanding(target)` (48749): place chunks past `target`
    /// (no cap, no RUNNING check), then a `jetpack_landing` chunk; returns
    /// where the landing chunk starts.
    pub fn set_safe_landing(&mut self, target: f64) -> f64 {
        while self.next_position <= target {
            self.place_next_chunk(None);
        }
        let landing = self.next_position;
        self.place_next_chunk(Some("jetpack_landing"));
        landing
    }

    pub fn place_next_chunk(&mut self, name: Option<&str>) -> Option<usize> {
        let (node, _) = match name {
            Some(n) => self.chunks_data.chunk(n).unwrap(),
            None => self.next_in_sequence(),
        };
        if !self.env_can_spawn(&node) {
            return None;
        }
        self.chunk_seq += 1;
        let id = self.chunk_seq;
        let start = self.next_position;
        let index = self.chunks.len();
        let len = self.chunk_init(id, start, node, index);
        self.next_position += len;
        Some(id)
    }

    /// `bg.init` (48291): setup, mount, environment mount, intro.
    fn chunk_init(&mut self, id: usize, start: f64, node: Node, index: usize) -> f64 {
        let blocks = data::comp(&node, "RouteChunk").and_then(|rc| rc["_blockCount"].as_f64()).unwrap_or(0.0);
        let length = blocks * BLOCK_SIZE;
        let chunk = Chunk {
            id,
            node: Some(node.clone()),
            name: data::name(&node).to_string(),
            blocks,
            length,
            z: -start,
            start,
            end: start + length,
            index,
            ..Default::default()
        };
        self.chunks.push(chunk);
        let ci = self.chunks.len() - 1;
        let prev = self.mounting_chunk.replace(id);
        self.env_setup(ci);
        crate::mount::mount(self, ci, &node, MountOpts::default());
        self.env_mount(ci);
        if self.chunks[ci].name == "intro" {
            crate::mount::mount_intro(self, ci);
        }
        self.mounting_chunk = prev;
        let c = &self.chunks[ci];
        self.log.push(LogEvent::ChunkPlace {
            frame: self.frame,
            chunk: id,
            name: c.name.clone(),
            start: c.start,
            z: c.z,
            length: c.length,
            blocks: c.blocks,
        });
        c.length
    }

    pub fn chunk(&self, ci: usize) -> &Chunk {
        &self.chunks[ci]
    }

    /// `bg.setFillersByPosition` / `setFloorsByPosition` (48458-48479).
    pub fn set_slots_by_position(&mut self, ci: usize, a: f64, b: f64, fillers: bool, floors: bool) {
        let c = &mut self.chunks[ci];
        let s = BLOCK_SIZE * 2.0;
        let i0 = js_round((-a - c.start) / s);
        let i1 = js_round((-b - c.start) / s);
        for i in i0..i1 {
            if fillers {
                c.filler_slots.insert(i, true);
            }
            if floors {
                c.floor_slots.insert(i, true);
            }
        }
    }

    // ---- RouteSystem (Eg) -----------------------------------------------------------

    pub fn route_reset(&mut self) {
        self.builder_reset();
        self.first_passed = false;
        self.spawns = HashMap::from([("pickup".into(), -900.0), ("tube".into(), -90.0)]);
    }

    pub fn route_can_spawn(&self, kind: &str, z: f64) -> bool {
        match self.spawns.get(kind) {
            None => true,
            Some(&v) => z <= v,
        }
    }
    pub fn route_set_spawn(&mut self, kind: &str, z: f64) {
        self.spawns.insert(kind.into(), z);
    }

    fn builder_reset(&mut self) {
        self.builder_level = 0;
        self.available_sections.clear();
        self.add_available_sections(0);
    }

    fn add_available_sections(&mut self, level: usize) {
        for s in data::LEVEL_TABLES[level] {
            self.add_available_section(s);
        }
    }

    fn add_available_section(&mut self, s: &'static Section) {
        let short = data::short_name(s);
        if data::SECTIONS_MID.contains(&short.as_str()) {
            if let Some(slot) = self.available_sections.iter_mut().find(|(k, _)| *k == short) {
                slot.1 = s; // existing key keeps its position
            } else {
                self.available_sections.push((short, s));
            }
        }
    }

    /// `Tg.getSectionByLevel` (49167). The repeat filter is computed and
    /// discarded in the JS, so selection is uniform over all sections.
    fn get_section_by_level(&mut self, level: i64) -> &'static Section {
        if level > self.builder_level {
            self.builder_level = level;
            self.add_available_sections(level as usize);
            if level == 2 {
                for n in ["default_bonus_short", "default_bonus_long", "default_pogostick_start"] {
                    self.add_available_section(data::section(n).unwrap());
                }
            }
        }
        let i = r_index(self.rng.as_mut(), sites::SECTION_BY_LEVEL, self.available_sections.len());
        self.available_sections[i].1
    }

    /// `Eg.getSequence` (49264). Full resources are always loaded in the port.
    pub fn get_sequence(&mut self) -> Vec<String> {
        let level = ((self.stats_time / 20.0).floor() as i64).clamp(0, 3);
        let mut sections: Vec<&'static Section> = Vec::new();
        if self.first_passed {
            let s = self.get_section_by_level(level);
            sections.push(s);
        } else {
            let n = if self.tutorial_enabled { "tutorial" } else { "default_start" };
            sections.push(data::section(n).unwrap());
            self.first_passed = true;
        }
        let epic = sections.iter().position(|s| s.name == "route_section_default_epic");
        if let Some(i) = epic {
            sections.insert(i + 1, data::section("default_fallback").unwrap());
        }
        let mut chunks: Vec<String> = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for s in &sections {
            if !seen.contains(&s.name) || epic.is_some() {
                seen.push(s.name);
                chunks.extend(s.start.iter().map(|c| c.to_string()));
                let i = r_index(self.rng.as_mut(), sites::GET_SEQUENCE_MID, s.mid.len());
                if let Some(m) = s.mid.get(i) {
                    chunks.push(m.to_string());
                }
                chunks.extend(s.end.iter().map(|c| c.to_string()));
            }
        }
        // config.chunk (49300): a fixed sequence (the section draws above still happen)
        if let Some(forced) = &self.force_chunks {
            return forced.clone();
        }
        let mut i = 0;
        while i < chunks.len() {
            let (node, _) = self.chunks_data.chunk(&chunks[i]).unwrap();
            if has_moving_train(&node) {
                chunks.insert(i + 1, "routeChunk_default_fallback".into());
                i += 1;
            }
            i += 1;
        }
        chunks
    }

    // ---- EnvironmentSystem (Tm) glue ------------------------------------------------

    fn environment_run(&mut self) {
        crate::environment::run(self);
    }
    fn environment_update(&mut self) {
        crate::environment::update(self);
    }
    fn env_can_spawn(&mut self, node: &Node) -> bool {
        crate::environment::can_spawn(self, node)
    }
    fn env_setup(&mut self, ci: usize) {
        crate::environment::setup(self, ci)
    }
    fn env_mount(&mut self, ci: usize) {
        crate::environment::mount(self, ci)
    }
}

/// `getSequence`'s MovingTrainPlaceholder finder.
fn has_moving_train(n: &serde_json::Value) -> bool {
    data::children(n).iter().any(|c| {
        if data::comp(c, "MovingTrainPlaceholder").and_then(|m| m["_speed"].as_f64()).unwrap_or(0.0) > 0.0 {
            true
        } else if data::has_children(c) {
            has_moving_train(c)
        } else {
            false
        }
    })
}

/// JS `Math.round` (half toward +infinity).
pub fn js_round(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}
