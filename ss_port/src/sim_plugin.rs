//! Bevy integration: the simulation as a (non-send) resource advanced one
//! game frame per tick.
//!
//! The only outside input is the keyboard, supplied per frame by a
//! [`Driver`]: [`ReplayDriver`] replays the keydowns logged in an oracle
//! trace at their recorded frames, [`KeyQueue`] forwards live key presses.

use crate::data::ChunkData;
use crate::entities::EntityId;
use crate::game::{FrameInput, Game};
use crate::hero::Key;
use crate::library::{Library, FULL_BUNDLE_START};
use crate::rng::{OracleRng, SingleStreamRng};
use crate::theme::ThemeConfig;
use crate::trace::Trace;
use bevy::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Supplies per-frame keyboard input.
pub trait Driver {
    /// Last frame this driver can provide (None: unbounded).
    fn last_frame(&self) -> Option<i64>;
    fn input(&mut self, frame: i64) -> FrameInput;
    /// Driver ids of the scenery entities spawned in `frame`, in add order
    /// (for matching port entities to trace ids in parity tests).
    fn spawn_ids(&self, _frame: i64) -> Vec<i64> {
        Vec::new()
    }
}

/// Character state the port doesn't treat as scenery (hero, guard, camera
/// rig, pogo effects `Tf`, jetpack effects `Df` and their trails `Sf`).
pub const NOT_SCENERY: &[&str] = &["Gp", "im", "om", "Tf", "Sf", "Df"];

/// Replays the keydowns of an oracle trace (`input` events, applied before
/// frame `at`). Nothing else from the trace reaches the simulation.
pub struct ReplayDriver {
    pub trace: Rc<Trace>,
    keys: HashMap<i64, Vec<Key>>,
    nav: HashMap<i64, Vec<crate::game::Nav>>,
    spawns: HashMap<i64, Vec<i64>>,
}

impl ReplayDriver {
    pub fn new(trace: Rc<Trace>) -> Self {
        let mut keys: HashMap<i64, Vec<Key>> = HashMap::new();
        for r in trace.of_type("event").filter(|r| r["kind"] == "input") {
            let at = r["at"].as_i64().or_else(|| r["f"].as_i64()).unwrap();
            if let Some(k) = r["key"].as_str().and_then(Key::from_dom) {
                keys.entry(at).or_default().push(k);
            }
        }
        // scripted UI calls (record.mjs --eval): before frame `at`
        let mut nav: HashMap<i64, Vec<crate::game::Nav>> = HashMap::new();
        for r in trace.of_type("event").filter(|r| r["kind"] == "nav") {
            let code = r["code"].as_str().unwrap_or("");
            let n = if code.contains("toGame") {
                crate::game::Nav::ToGame
            } else if code.contains("toIdleScreen") {
                crate::game::Nav::ToTitle
            } else {
                continue;
            };
            nav.entry(r["at"].as_i64().unwrap()).or_default().push(n);
        }
        let mut spawns: HashMap<i64, Vec<i64>> = HashMap::new();
        for r in trace.of_type("entity_spawn") {
            if NOT_SCENERY.contains(&r["cls"].as_str().unwrap_or("")) {
                continue;
            }
            spawns.entry(r["f"].as_i64().unwrap()).or_default().push(r["id"].as_i64().unwrap());
        }
        for v in spawns.values_mut() {
            v.sort();
        }
        Self { trace, keys, nav, spawns }
    }
}

impl Driver for ReplayDriver {
    fn spawn_ids(&self, frame: i64) -> Vec<i64> {
        self.spawns.get(&frame).cloned().unwrap_or_default()
    }
    fn last_frame(&self) -> Option<i64> {
        Some(self.trace.frame_range().1)
    }
    fn input(&mut self, f: i64) -> FrameInput {
        FrameInput { keys: self.keys.get(&f).cloned().unwrap_or_default(), nav: self.nav.remove(&f).unwrap_or_default() }
    }
}

/// Live keyboard: keys pushed by the window are delivered before the next
/// simulated frame.
#[derive(Clone, Default)]
pub struct KeyQueue(pub Rc<RefCell<Vec<Key>>>);

impl Driver for KeyQueue {
    fn last_frame(&self) -> Option<i64> {
        None
    }
    fn input(&mut self, _frame: i64) -> FrameInput {
        FrameInput { keys: std::mem::take(&mut *self.0.borrow_mut()), nav: Vec::new() }
    }
}

/// Paths to the game data (the HAR mirror and the port's data folder).
#[derive(Clone)]
pub struct DataPaths {
    pub site: PathBuf,
    pub theme: PathBuf,
    /// Character clip tables (`Cp`, data/anim_clips.json).
    pub clips: PathBuf,
}

impl DataPaths {
    /// Defaults relative to the repository root.
    pub fn from_repo(root: &Path) -> Self {
        Self::with_theme(root, "bali")
    }
    /// Data paths for theme `id` (`ss_port/data/theme_<id>.json`).
    pub fn with_theme(root: &Path, id: &str) -> Self {
        Self::installed(&crate::install::Install::repo(root), id)
    }
    /// Data paths for theme `id` in an installation (repository or portable folder).
    pub fn installed(inst: &crate::install::Install, id: &str) -> Self {
        Self {
            site: inst.site.clone(),
            theme: inst.port.join(format!("data/theme_{id}.json")),
            clips: inst.port.join("data/anim_clips.json"),
        }
    }
}

/// The running simulation (non-send: it holds `Rc`s).
pub struct Sim {
    pub game: Game,
    pub driver: Box<dyn Driver>,
    /// Driver entity id -> port entity (from the driver's spawn ids).
    pub ids: HashMap<i64, EntityId>,
    /// Next frame to simulate.
    pub next_frame: i64,
    pub finished: bool,
    /// Distinguishes simulations (a restart replaces the resource).
    pub serial: u64,
    log_cursor: usize,
}

static SIM_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Sim {
    /// Build the game, boot it (intro with the boot-time library, then the
    /// full library and the seeded RNG) up to the oracle's recording start
    /// (frame 5), then press play (`runWithIntro`).
    pub fn new(paths: &DataPaths, seed: i32, hunt_letter: Option<String>, driver: Box<dyn Driver>) -> Result<Self, String> {
        let mut s = Self::new_title(paths, seed, hunt_letter, driver)?;
        s.start_run();
        s.game.flow.menu = crate::menu::Menu::None;
        Ok(s)
    }

    /// The game booted to the title screen (IDLE: the intro chunk, the hero
    /// painting); [`Sim::start_run`] presses play.
    pub fn new_title(paths: &DataPaths, seed: i32, hunt_letter: Option<String>, driver: Box<dyn Driver>) -> Result<Self, String> {
        let full = Rc::new(Library::load(&paths.site)?);
        let boot_lib = Rc::new(Library::load_until(&paths.site, Some(FULL_BUNDLE_START))?);
        let chunks = Rc::new(ChunkData::load(&paths.site)?);
        let theme = Rc::new(ThemeConfig::load(&paths.theme)?);
        let mut game = Game::new(boot_lib, chunks, theme, Box::new(SingleStreamRng::new(0)));
        game.hunt_letter = hunt_letter;
        let actors = crate::anim::ActorAssets::load(&paths.site, &paths.clips)?;
        game.install_actors(Rc::new(actors));
        game.boot(full, Box::new(OracleRng::new(seed)));
        game.flow.menu = crate::menu::Menu::Title;
        let serial = SIM_SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut sim = Self { game, driver, ids: HashMap::new(), next_frame: 6, finished: false, serial, log_cursor: 0 };
        sim.map_spawns(5);
        Ok(sim)
    }

    /// Pair this frame's port spawns with the driver's spawn ids.
    fn map_spawns(&mut self, frame: i64) {
        let new: Vec<EntityId> = self.game.log[self.log_cursor..]
            .iter()
            .filter_map(|e| match e {
                crate::game::LogEvent::Spawn { entity, .. } => Some(*entity),
                _ => None,
            })
            .collect();
        self.log_cursor = self.game.log.len();
        for (id, pe) in self.driver.spawn_ids(frame).into_iter().zip(new) {
            self.ids.insert(id, pe);
        }
    }

    /// `nav.toGame()`: close the menus and run with the intro.
    pub fn start_run(&mut self) {
        self.game.start_run();
    }

    /// Advance one game frame. Returns false once the driver has no more input.
    pub fn step(&mut self) -> bool {
        if self.driver.last_frame().is_some_and(|l| self.next_frame > l) {
            self.finished = true;
            return false;
        }
        let f = self.next_frame;
        let mut input = self.driver.input(f);
        // UI calls made between the previous frame and this one: their
        // spawns are logged (and paired) under the previous frame, as in the oracle
        let nav = std::mem::take(&mut input.nav);
        if !nav.is_empty() {
            for n in nav {
                self.game.nav(n);
            }
            self.map_spawns(f - 1);
        }
        self.game.step(f, input);
        self.map_spawns(f);
        self.next_frame += 1;
        true
    }
}

impl Sim {
    /// Apply the trace's GAME_CONFIG overrides (`meta.query`): `god`
    /// (present = on) and `forcePickup`.
    pub fn configure_from_trace(&mut self, trace: &Trace) {
        let q = &trace.meta["query"];
        self.game.god = q.get("god").is_some();
        self.game.force_pickup = q["forcePickup"].as_str().and_then(crate::entities::PickupKind::from_xa);
        // gdHoverboards / gdHeadstart / gdScoreBooster (72363): added to the inventory at boot
        let gd = |k: &str| q[k].as_str().and_then(|v| v.parse::<f64>().ok()).map(|v| v.round() as i64).unwrap_or(0);
        let u = &mut self.game.flow.user;
        u.hoverboards = (u.hoverboards + gd("gdHoverboards")).max(0);
        u.headstarts = (u.headstarts + gd("gdHeadstart")).max(0);
        u.score_boosters = (u.score_boosters + gd("gdScoreBooster")).max(0);
        self.game.force_chunks = q["chunk"].as_str().filter(|s| !s.is_empty()).map(|s| s.split(',').map(str::to_string).collect());
    }
}

/// Hunt letter used by the trace (the letter pickup's model name).
pub fn trace_hunt_letter(trace: &Trace) -> Option<String> {
    trace
        .of_type("entity_spawn")
        .find(|r| r["cls"] == "Ja")
        .and_then(|r| r["meshes"].as_array()?.iter().find_map(|m| m["mesh"].as_str().filter(|s| s.len() == 1).map(str::to_string)))
}

/// Advances [`Sim`] by one frame every update.
pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, advance_sim);
    }
}

fn advance_sim(sim: Option<NonSendMut<Sim>>) {
    if let Some(mut sim) = sim {
        if !sim.finished {
            sim.step();
        }
    }
}
