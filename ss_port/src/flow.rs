//! Run score, the in-game HUD state and the death flow (docs/js_notes/
//! ui_hud.md, ui_gameover.md). The renderer (`crate::ui`) only draws what
//! this module decides.
//!
//! * Score: `data.score += distanceDelta * (multiplier + missionMultiplier)`
//!   on every `stats.z` write, shown as `floor(data.score * 0.1)` (49448).
//! * HUD `fg`: opens 200 ms after the run starts (`go(0.2)`), refreshes its
//!   counters on every 4th RUNNING frame, hides at game over (47608).
//! * Death: `game.gameover()` -> after 0.75 s the "Save me!" popup with a
//!   6 s clock -> (decline) New High Score screen if beaten -> results
//!   notepad -> PLAY. Revive by Space (free, once per run) or keys.
//! * Timers (`setTimeout`) fire after the frame's render once the virtual
//!   clock reaches their due time; delays that are exact frame multiples
//!   fire one frame late, as in the oracle.

use crate::game::{Game, GameState};
use std::path::PathBuf;

/// `config.freeRevivals`.
pub const FREE_REVIVALS: u32 = 1;

/// Persisted user data, `GameSettings.json` (`Install::save_file`: the user's application data folder). It folds the original's
/// four localStorage stores into one file (docs/js_notes/ui_powerups_shop.md
/// §5): `GameSettings` (currencies, highscore), `ShopSettings.purchased`
/// (owned items, consumables, upgrade levels), `CharacterSettings`
/// (character, outfit) and `BoardSettings` (board, powerups). Missing fields
/// take the original defaults, so older saves still load.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UserData {
    #[serde(alias = "highscore")]
    pub high_score: i64,
    pub coins: i64,
    pub keys: i64,
    /// `CharacterSettings.name` / `.outfit`.
    pub selected_character: String,
    pub selected_outfit: usize,
    /// `BoardSettings.name` / `.powerups` (1-based indices, 0 = the board).
    pub selected_board: String,
    pub board_powers: Vec<usize>,
    /// `purchased.boosts.permanents`, 0..=6 each.
    pub upgrades: PowerUpUpgrades,
    /// `purchased.boosts.consumables`.
    pub hoverboards: i64,
    pub mystery_boxes: i64,
    pub score_boosters: i64,
    pub headstarts: i64,
    /// `purchased.characters / outfits / boards` (boards also hold "board~power").
    pub owned_characters: Vec<String>,
    pub owned_outfits: Vec<String>,
    pub owned_boards: Vec<String>,
    /// Word Hunt (`crate::word_hunt`): `huntWord`; `currentLetter` as the day
    /// it was saved and the letters collected that day; `completedHunts`
    /// (this week's finished words); `gameSettings.wordHunt.started`.
    pub hunt_word: String,
    pub hunt_day: i64,
    pub hunt_index: usize,
    pub completed_hunts: Vec<String>,
    pub word_hunt_started: bool,
    /// `gameSettings.missions`: per mission set, the missions' saved progress
    /// (`crate::missions`).
    pub missions: Vec<Vec<crate::missions::MissionSave>>,
    /// `gameSettings.name` (the leaderboard nickname, Settings) and
    /// `gameSettings.muted` (Settings / the title's sound button).
    pub name: String,
    pub muted: bool,
    /// `AwardsStore`: each award's progress (`crate::awards`).
    pub awards: std::collections::BTreeMap<String, crate::awards::Progress>,
    /// `gameSettings.tutorial`: the tutorial is done.
    pub tutorial: bool,
}

/// Upgrade tiers (`ShopSettings.purchased.boosts.permanents`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PowerUpUpgrades {
    pub magnet_tier: u32,
    pub jetpack_tier: u32,
    pub sneakers_tier: u32,
    pub multiplier_tier: u32,
}

impl Default for UserData {
    /// A fresh user (`rx` / `nx` / `Xb` / `Yb` defaults).
    fn default() -> Self {
        Self {
            high_score: 0,
            coins: 0,
            keys: 5,
            selected_character: "jake".into(),
            selected_outfit: 0,
            selected_board: "hoverboard".into(),
            board_powers: vec![0],
            upgrades: PowerUpUpgrades::default(),
            hoverboards: 3,
            mystery_boxes: 0,
            score_boosters: 0,
            headstarts: 0,
            owned_characters: vec!["jake".into()],
            owned_outfits: Vec::new(),
            owned_boards: vec!["hoverboard".into()],
            hunt_word: String::new(),
            hunt_day: 0,
            hunt_index: 0,
            completed_hunts: Vec::new(),
            word_hunt_started: false,
            missions: Vec::new(),
            // `ex.generateName()` draws a random name; the oracle's was this
            name: "HiddenBubble".into(),
            muted: false,
            awards: Default::default(),
            tutorial: false,
        }
    }
}

impl UserData {
    pub fn load(path: &std::path::Path) -> Self {
        let text = std::fs::read_to_string(path).ok();
        let mut u: Self = text.as_deref().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        // saves from before the tutorial existed: a player with a score has
        // played already (only a new player gets the tutorial)
        let has_field = text.as_deref().and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()).is_some_and(|v| v.get("tutorial").is_some());
        if text.is_some() && !has_field && u.high_score > 0 {
            u.tutorial = true;
        }
        // cleanSavedData: default entries are always present
        for (list, d) in [(&mut u.owned_characters, "jake"), (&mut u.owned_boards, "hoverboard")] {
            if !list.iter().any(|x| x == d) {
                list.insert(0, d.into());
            }
        }
        if u.board_powers.is_empty() {
            u.board_powers = vec![0];
        }
        u
    }
}

/// HUD `fg` state.
#[derive(Clone, Debug, Default)]
pub struct Hud {
    pub built: bool,
    pub visible: bool,
    /// `go(0.2)` before `run()` opens the view.
    open_due_ms: Option<f64>,
    pub update_count: u64,
    /// Displayed values (refreshed on ticks).
    pub score: i64,
    pub coins: i64,
    pub badge: String,
    /// Grey overlay alpha of the badge (2x powerup pulse).
    pub badge_overlay: f64,
    /// Powerup meters (`Km` rows), oldest first.
    pub items: Vec<crate::powerups::ItemTimer>,
    /// Boost buttons (`Wm`) and the gauge (`Dm`).
    pub boosts: Vec<crate::boosts::BoostButton>,
    pub gauge: crate::boosts::Gauge,
    /// The Word Hunt drop-down (`dg`), playing since a letter was collected.
    pub word_banner: Option<WordBanner>,
}

/// `dg`'s timeline (47450): after `delay` s it drops (0.3 s), highlights the
/// letter (`progress`: tint to yellow 0.25 s, scale 1.7 and back), goes up
/// again 2 s after its start (0.3 s); after the last letter it drops once
/// more (1.5 s later) with the "Word Hunt Complete" panel and the reward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WordBanner {
    /// The letter collected (`currentLetter`).
    pub letter: usize,
    /// Seconds since the timeline (re)started.
    pub t: f64,
    pub complete_shown: bool,
}

impl WordBanner {
    pub fn delay(&self) -> f64 {
        if self.complete_shown { 1.5 } else { 0.5 }
    }
    /// The panel's drop (0 up, 1 down), quad in-out.
    pub fn drop(&self) -> f64 {
        let ease = |x: f64| if x < 0.5 { 2.0 * x * x } else { 1.0 - (-2.0 * x + 2.0).powi(2) / 2.0 };
        let t = self.t - self.delay();
        if t <= 0.0 {
            0.0
        } else if t < 0.3 {
            ease(t / 0.3)
        } else if t < 2.0 {
            1.0
        } else if t < 2.3 {
            1.0 - ease((t - 2.0) / 0.3)
        } else {
            0.0
        }
    }
    /// The highlighted letter: (tint 0..1, scale).
    pub fn progress(&self) -> (f64, f64) {
        let t = self.t - self.delay() - 0.3;
        if t <= 0.0 {
            return (0.0, 1.0);
        }
        let ease = |x: f64| if x < 0.5 { 2.0 * x * x } else { 1.0 - (-2.0 * x + 2.0).powi(2) / 2.0 };
        let tint = ease((t / 0.25).min(1.0));
        let scale = if t < 0.25 { 1.0 + 0.7 * ease(t / 0.25) } else if t < 0.5 { 1.7 - 0.7 * ease((t - 0.25) / 0.25) } else { 1.0 };
        (tint, scale)
    }
}

/// The death flow's current screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Screen {
    None,
    /// `nav.gameover()`: `await go(0.75)` before Save me.
    Dying { due_ms: f64 },
    /// "Save me!" (`eb`): clock seconds left; tall when the free revive is offered.
    SaveMe { secs: f64, tall: bool },
    /// Not enough keys (`Ny`): keys still needed.
    NotEnough { needed: i64 },
    /// New High Score (`Gv`): message alpha (-5 -> 1), frames since open.
    HighScore { alpha: f64, frames: u32 },
    /// Results notepad (`Wv`).
    /// `doubled`: Double Up taken; `list`: the leaderboard's scroller.
    Results { score: i64, coins: i64, play_due_ms: f64, play_ready: bool, frames: u32, doubled: bool, list: Scroller },
    /// "Need hoverboards?" (`Fv`): mid-run (game paused) or from a menu.
    BuyBoards { in_run: bool, bump: u32 },
    /// The pause panel `ph` (Menu, Restart, RESUME).
    Paused,
    /// `hud.runCountdown(3)` before `game.resume()`: "Starting in\n{n}",
    /// one 600 ms `setTimeout` per step.
    Countdown { n: u32, due_ms: f64 },
    /// The prize screen `By` after a declined Save me (run mystery boxes).
    Prize(PrizeScreen),
}

/// Prize screen `By` state (64472): the remaining queue, the shown prize.
#[derive(Clone, Debug, PartialEq)]
pub struct PrizeScreen {
    pub queue: Vec<crate::prizes::Prize>,
    pub current: crate::prizes::Prize,
    pub state: PrizeState,
    /// Frames since `setup` (hint pulse) and since the open press.
    pub t: u32,
    pub opened: Option<u32>,
}

/// `zy`: OPENING (press opens), -1 (animating), CLOSING / REPEATING (press closes / next).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrizeState {
    Opening,
    Animating,
    Closing,
    Repeating,
}

impl PrizeScreen {
    /// `By.onOpen` + `setup()`: the first prize (None if the queue is empty).
    pub fn open(mut queue: Vec<crate::prizes::Prize>) -> Option<Self> {
        if queue.is_empty() {
            return None;
        }
        let current = queue.remove(0);
        Some(Self { queue, current, state: PrizeState::Opening, t: 0, opened: None })
    }
}

/// Open press, next prize, or close. Returns true when the screen closes.
pub fn prize_press(g: &mut Game, p: &mut PrizeScreen) -> bool {
    match p.state {
        PrizeState::Opening => {
            // openPrize: credit now; the reveal runs 5 s
            let cur = p.current.clone();
            crate::prizes::credit(g, &cur);
            crate::awards::prize_opened(g, cur.box_type.as_deref());
            crate::audio::stop_all_fx(g);
            crate::audio::music_fade_out(g);
            crate::audio::play(g, "open-prize");
            p.state = PrizeState::Animating;
            p.opened = Some(p.t);
            false
        }
        PrizeState::Animating => false,
        PrizeState::Repeating => {
            let next = p.queue.remove(0);
            *p = PrizeScreen { queue: std::mem::take(&mut p.queue), current: next, state: PrizeState::Opening, t: 0, opened: None };
            false
        }
        PrizeState::Closing => {
            // By.onClose
            crate::audio::music_fade_in(g);
            true
        }
    }
}

/// Per frame: the reveal ends 5 s after the open press.
pub fn prize_tick(p: &mut PrizeScreen) {
    p.t += 1;
    if let (PrizeState::Animating, Some(o)) = (p.state, p.opened) {
        if p.t - o >= 300 {
            p.state = if p.queue.is_empty() { PrizeState::Closing } else { PrizeState::Repeating };
        }
    }
}

/// Save me's exit callback (65409): the run's prizes first, then `toGameover`.
fn save_me_exit(g: &mut Game) {
    let prizes = std::mem::take(&mut g.prizes);
    match PrizeScreen::open(prizes) {
        Some(p) => g.flow.screen = Screen::Prize(p),
        None => to_gameover(g),
    }
}

/// What a UI click hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// Save me: the blue keys button.
    ReviveKeys,
    /// Save me's green "Free!" button (`btnReviveWatch`: `reviveWithAd`).
    ReviveFree,
    /// Results: PLAY.
    Play,
    /// Need hoverboards: the coin price button.
    BuyBoard,
    /// A boost button (`Wm`).
    Boost(crate::boosts::BoostKind),
    /// Title: tap area / PRESS TO PLAY.
    StartGame,
    /// Title: boards button (`Fv`), Me, Shop.
    TitleBoards,
    TitleMe,
    TitleShop,
    /// Notepad back button (Me panel, boost shop): to the title.
    Back,
    /// Me panel tabs, thumbs, feature buttons, the big select/buy button.
    MeTab(crate::menu::MeTab),
    MeThumb(usize),
    MeFeature(usize),
    MeSelect,
    /// Boost list: a card (toggle), its buy button.
    BoostCard(usize),
    BoostBuy(usize),
    /// Results: Menu / Boosts.
    ResultsMenu,
    ResultsBoosts,
    /// Close the top overlay.
    CloseOverlay,
    /// HUD pause button; pause panel Menu / Settings / RESUME.
    PauseButton,
    PauseMenu,
    PauseRestart,
    PauseResume,
    /// Title: settings (`Mm`), sound (`ub`), My Tour.
    TitleSettings,
    TitleSound,
    TitleMyTour,
    /// My Tour: the Word Hunt / Missions tab; a mission's skip button.
    TourSection(bool),
    MissionSkip(usize),
    /// Settings: the nickname row, the sound row; the nickname prompt's
    /// backdrop (closes it).
    SettingsNickname,
    SettingsSound,
    PromptClose,
    /// Pause panel: Settings.
    PauseSettings,
    /// Results: "Free Double Up!" (`Iv`).
    DoubleUp,
    /// Me panel: the Awards tab; an award's "Collect Award" (`awards::IDS`).
    MeAwards,
    AwardCollect(usize),
    /// Anywhere else on a screen.
    Elsewhere,
}

/// Run-scoped flow state.
#[derive(Clone, Debug)]
pub struct Flow {
    pub screen: Screen,
    pub user: UserData,
    /// Where `user` is saved (None: not persisted, e.g. in tests).
    pub save_path: Option<PathBuf>,
    pub free_revivals: u32,
    pub paid_revivals: u32,
    /// Run keys (`stats.keys`).
    pub keys: i64,
    /// An ad revive lands after the frame's update (the SDK promise).
    pending_ad_revive: bool,
    /// PLAY pressed: the app starts a new run.
    pub restart_requested: bool,
    /// Results "Menu": the app rebuilds the game on the title screen.
    pub menu_requested: bool,
    /// Title PLAY / Space: the app starts the run (`nav.toGame`).
    pub start_requested: bool,
    /// The menus (title, Me panel, boost shop) and modal overlays.
    pub menu: crate::menu::Menu,
    pub overlay: Option<crate::menu::Overlay>,
    /// The Me panel reopens on the last tab used.
    pub last_me_tab: crate::menu::MeTab,
    /// A click / scroll changed the menus: redraw before the next frame.
    pub ui_dirty: std::cell::Cell<bool>,
    /// The New High Score screen's stripes and character view.
    pub celebration: crate::celebration::Celebration,
    /// `user` changed (mission progress): saved within a second.
    pub user_dirty: bool,
    /// My Tour's last section (the panel keeps it): Missions, else Word Hunt.
    pub tour_missions: bool,
    /// The nickname prompt's input (`user.name` follows it while not empty).
    pub nickname_input: String,
    /// The Awards tab's list (`ty.trackpad`).
    pub awards_list: Scroller,
}

impl Default for Flow {
    fn default() -> Self {
        Self {
            screen: Screen::None,
            user: UserData::default(),
            save_path: None,
            free_revivals: 0,
            paid_revivals: 0,
            keys: 0,
            pending_ad_revive: false,
            restart_requested: false,
            menu_requested: false,
            start_requested: false,
            menu: crate::menu::Menu::None,
            overlay: None,
            last_me_tab: crate::menu::MeTab::Characters,
            ui_dirty: std::cell::Cell::new(true),
            celebration: Default::default(),
            user_dirty: false,
            tour_missions: false,
            nickname_input: String::new(),
            awards_list: Scroller::awards(crate::awards::IDS.len()),
        }
    }
}

impl Flow {
    /// Persist the user data (no-op without a save path).
    pub fn save_user(&self) {
        self.save();
    }
    fn save(&self) {
        if let Some(p) = &self.save_path {
            if let Ok(s) = serde_json::to_string_pretty(&self.user) {
                if let Err(e) = crate::install::write_atomic(p, s.as_bytes()) {
                    bevy::log::warn!("[Save] {}: {e}", p.display());
                }
            }
        }
    }
    /// `game.paidRevivalCost()`.
    pub fn paid_cost(&self) -> i64 {
        1 << self.paid_revivals
    }
}

// ---- score ------------------------------------------------------------------------------

/// The `stats.z` setter: distance and score accumulate.
pub fn set_stats_z(g: &mut Game, z: f64) {
    let delta = -z - g.distance();
    g.distance_delta = delta;
    g.stats_z = z;
    g.score_raw += delta * (g.multiplier + g.mission_multiplier);
    let s = score(g);
    crate::missions::set_stat(g, s, "mission-score", crate::missions::SetMode::Set);
}

/// `stats.score`.
pub fn score(g: &Game) -> i64 {
    (g.score_raw * 0.1).floor() as i64
}

// ---- HUD ----------------------------------------------------------------------------------

/// `nav.resetUi` (67210): every death-flow screen and popup closed.
pub fn reset_ui(g: &mut Game) {
    g.flow.screen = Screen::None;
    g.flow.overlay = None;
    g.flow.restart_requested = false;
    g.flow.menu_requested = false;
}

/// `fg.reset` / `fg.idle` (47697): panel closed, view hidden, boosts removed.
pub fn hud_reset(g: &mut Game) {
    g.hud.visible = false;
    g.hud.update_count = 1;
    g.hud.boosts.clear();
}

/// `hud.run()` (onRun): open 200 ms later.
pub fn hud_run(g: &mut Game) {
    g.hud.open_due_ms = Some(g.clock.now + 200.0);
}

fn hud_open(g: &mut Game) {
    g.hud.built = true;
    g.hud.visible = true;
}

/// `fg.update` (system update, after the camera).
pub fn hud_update(g: &mut Game) {
    if g.state != GameState::Running || !g.hud.built {
        return;
    }
    // multiplier.update(): the 2x overlay pulse (`time._lastTime` ms)
    g.hud.badge_overlay = if g.hero.multiplier.is_on() { 0.5 + (g.now_ms() * 0.01).sin() * 0.4 } else { 0.0 };
    g.hud.update_count += 1;
    crate::boosts::hud_update(g);
    if g.hud.update_count % 4 == 0 {
        let s = score(g);
        if g.hud.score <= s {
            g.hud.score = s;
        }
        g.hud.coins = g.coins;
        g.hud.badge = format!("x{}", g.multiplier + g.mission_multiplier);
    }
}

// ---- death flow ----------------------------------------------------------------------------

/// `game.gameover()` (from `Player.die`, inside physics).
pub fn gameover(g: &mut Game) {
    let f = &mut g.flow;
    f.user.keys += f.keys;
    f.keys = 0;
    f.save();
    g.hud.visible = false;
    g.controller.enabled = false;
    g.flow.screen = Screen::Dying { due_ms: g.clock.now + 750.0 };
}

/// `nav.toGameover()`: high-score screen first if the score beats it.
fn to_gameover(g: &mut Game) {
    crate::awards::gameplay_finish(g);
    if score(g) > g.flow.user.high_score {
        g.flow.screen = Screen::HighScore { alpha: -5.0, frames: 0 };
        crate::celebration::open(g);
        // Gv.onOpen
        crate::audio::stop_all_fx(g);
        crate::audio::music_fade_out(g);
        crate::audio::play(g, "unlock");
    } else {
        open_results(g);
    }
}

/// `gameover.open()`: bank the run, save, PLAY's Space binding in 2 s.
fn open_results(g: &mut Game) {
    // Gv.onClose (from the New High Score screen)
    if matches!(g.flow.screen, Screen::HighScore { .. }) {
        crate::audio::music_fade_in(g);
    }
    let s = score(g);
    let coins = g.coins;
    let u = &mut g.flow.user;
    u.coins += coins;
    let beat = s > u.high_score;
    if beat {
        u.high_score = s;
    }
    // the leaderboard's setUser: a better score than the entry's
    if beat {
        crate::missions::set_stat(g, 1, "mission-high-score", crate::missions::SetMode::Set);
    }
    g.flow.save();
    g.flow.screen = Screen::Results { score: s, coins, play_due_ms: g.clock.now + 2000.0, play_ready: false, frames: 0, doubled: false, list: Scroller::leaderboard(coins > 0) };
    g.coins = 0;
    g.flow.keys = 0;
}

/// The HUD pause button / Escape (`onBtnPausePress`, 47766): pause a run,
/// or resume a paused one with the countdown (ignored during it).
pub fn pause_button(g: &mut Game) {
    match g.state {
        GameState::Running if g.hud.visible => {
            pause(g);
            g.flow.screen = Screen::Paused;
        }
        GameState::Paused if g.flow.screen == Screen::Paused => start_countdown(g),
        _ => {}
    }
}

/// `Game.onBlur` (50038): a running game pauses (with the pause panel).
pub fn blur(g: &mut Game) {
    if g.state == GameState::Running && matches!(g.flow.screen, Screen::None) {
        pause(g);
        g.flow.screen = Screen::Paused;
        g.flow.ui_dirty.set(true);
    }
}

/// `game.resume(3)` -> `hud.runCountdown`: panel closed, HUD shown.
pub fn start_countdown(g: &mut Game) {
    g.hud.visible = true;
    g.flow.screen = Screen::Countdown { n: 3, due_ms: g.clock.now + 600.0 };
}

/// `Fv` board price (60074).
pub const BOARD_COST: i64 = 300;

/// `$.buyBoards.open()`; mid-run it pauses the game first (`hoverboard.turnOn` with none left).
pub fn open_buy_boards(g: &mut Game, in_run: bool) {
    if in_run {
        pause(g);
    }
    g.flow.screen = Screen::BuyBoards { in_run, bump: 0 };
}

/// `Fv.buyBoard` (60220): not enough coins does nothing.
pub fn buy_board(g: &mut Game) {
    if g.flow.user.coins >= BOARD_COST {
        g.flow.user.coins -= BOARD_COST;
        g.flow.user.hoverboards += 1;
        g.flow.save();
        if let Screen::BuyBoards { in_run, .. } = g.flow.screen {
            g.flow.screen = Screen::BuyBoards { in_run, bump: 15 };
        }
    }
}

/// Leave `Fv`: mid-run, `game.resume(3)` (a 3 s countdown).
pub fn close_buy_boards(g: &mut Game) {
    if let Screen::BuyBoards { in_run, .. } = g.flow.screen {
        if in_run {
            start_countdown(g);
        } else {
            g.flow.screen = Screen::None;
        }
    }
}

/// `game.pause()` (50162): only from RUNNING; HUD hidden, controller hidden.
pub fn pause(g: &mut Game) {
    if g.state == GameState::Running {
        g.state = GameState::Paused;
        g.controller = crate::hero::Controller::default();
        g.hud.visible = false;
    }
}

/// `game.resume()`: RUNNING, HUD open, controller shown.
pub fn resume(g: &mut Game) {
    if g.state == GameState::Paused {
        g.state = GameState::Running;
        g.hud.visible = true;
        crate::hero::controller_show(g);
    }
}

/// A key while a screen is up (keydown, before the frame). Returns true if
/// the UI consumed it.
pub fn key(g: &mut Game, key: crate::hero::Key) -> bool {
    use crate::hero::Key;
    use crate::menu::{self, Menu};
    if let Some(crate::menu::Overlay::Prize(mut p)) = g.flow.overlay.clone() {
        if key == Key::Action {
            g.flow.overlay = if prize_press(g, &mut p) { None } else { Some(crate::menu::Overlay::Prize(p)) };
        }
        return true;
    }
    // the nickname prompt takes the keyboard (`crate::ui` types into it)
    if let Some(crate::menu::Overlay::Settings { prompt: true }) = g.flow.overlay {
        return true;
    }
    if g.flow.overlay.is_some() {
        if key == Key::Action {
            g.flow.overlay = None;
        }
        return true;
    }
    match g.flow.menu.clone() {
        Menu::None => {}
        // title `onKeyDown`: Space starts the run (`startGame`: a tap sound)
        Menu::Title => {
            if key == Key::Action {
                crate::audio::play(g, "gui-tap");
                g.flow.start_requested = true;
            }
            return true;
        }
        Menu::Me(_) => {
            match key {
                Key::Left => menu::step_focus(g, -1),
                Key::Right => menu::step_focus(g, 1),
                Key::Action => menu::press_select(g),
                Key::Down => menu::back_to_title(g),
                _ => {}
            }
            return true;
        }
        Menu::MyTour { .. } => {
            match key {
                Key::Left => click(g, Click::TourSection(false)),
                Key::Right => click(g, Click::TourSection(true)),
                Key::Down => menu::back_to_title(g),
                _ => {}
            }
            return true;
        }
        Menu::Shop { open, .. } => {
            match key {
                Key::Down => menu::toggle_card(g, open.map_or(0, |i| (i + 1).min(menu::BOOST_CARDS - 1))),
                Key::Up => menu::toggle_card(g, open.map_or(0, |i| i.saturating_sub(1))),
                Key::Action => {
                    if let Some(i) = open {
                        menu::buy_card(g, i);
                    }
                }
                Key::Left => menu::back_to_title(g),
                _ => {}
            }
            return true;
        }
    }
    match g.flow.screen.clone() {
        Screen::BuyBoards { .. } => {
            if key == Key::Action {
                close_buy_boards(g);
            }
            true
        }
        Screen::Countdown { .. } | Screen::Paused => true,
        Screen::None | Screen::Dying { .. } => false,
        // Space: free revive by rewarded ad (lands after this frame's update)
        Screen::SaveMe { tall, .. } => {
            if key == Key::Action && tall {
                g.flow.pending_ad_revive = true;
            }
            if key == Key::ReviveKeys {
                revive_with_keys(g);
            }
            true
        }
        Screen::NotEnough { .. } => true,
        Screen::Prize(mut p) => {
            if key == Key::Action {
                if prize_press(g, &mut p) {
                    to_gameover(g);
                } else {
                    g.flow.screen = Screen::Prize(p);
                }
            }
            true
        }
        Screen::HighScore { alpha, .. } => {
            if key == Key::Action && alpha >= 0.2 {
                open_results(g);
            }
            true
        }
        Screen::Results { play_ready, .. } => {
            if key == Key::Action && play_ready {
                g.flow.restart_requested = true;
                g.to_game();
            }
            true
        }
    }
}

/// A click on a screen.
pub fn click(g: &mut Game, c: Click) {
    use crate::menu::{self, Menu, Overlay};
    // a button's tap (`Am.onPointerTap`, the title's start, the prompt's
    // close) sounds before its action
    if !matches!(c, Click::Elsewhere | Click::Boost(_)) && !matches!(g.flow.screen, Screen::Prize(_) | Screen::HighScore { .. }) {
        crate::audio::play(g, "gui-tap");
    }
    // modal overlays take every click
    if let Some(o) = g.flow.overlay.clone() {
        match (o, c) {
            (Overlay::BuyBoards { .. }, Click::BuyBoard) => {
                if g.flow.user.coins >= BOARD_COST {
                    g.flow.user.coins -= BOARD_COST;
                    g.flow.user.hoverboards += 1;
                    g.flow.save();
                    g.flow.overlay = Some(Overlay::BuyBoards { bump: 15 });
                }
            }
            (Overlay::Boosts { .. }, Click::BoostCard(i)) => menu::toggle_card(g, i),
            (Overlay::Boosts { .. }, Click::BoostBuy(i)) => menu::buy_card(g, i),
            (Overlay::Boosts { .. }, Click::Elsewhere) => {}
            (Overlay::Prize(mut p), _) => {
                g.flow.overlay = if prize_press(g, &mut p) { None } else { Some(Overlay::Prize(p)) };
            }
            (Overlay::Settings { prompt: true }, Click::PromptClose) => g.flow.overlay = Some(Overlay::Settings { prompt: false }),
            (Overlay::Settings { prompt: true }, _) => {}
            (Overlay::Settings { .. }, Click::SettingsNickname) => {
                g.flow.nickname_input = g.flow.user.name.clone();
                g.flow.overlay = Some(Overlay::Settings { prompt: true });
            }
            (Overlay::Settings { .. }, Click::SettingsSound) => toggle_muted(g),
            (Overlay::NotEnough { .. }, _) | (_, Click::CloseOverlay) => g.flow.overlay = None,
            _ => {}
        }
        return;
    }
    match (&g.flow.menu.clone(), c) {
        (Menu::Title, Click::StartGame) => g.flow.start_requested = true,
        (Menu::Title, Click::TitleBoards) => g.flow.overlay = Some(Overlay::BuyBoards { bump: 0 }),
        (Menu::Title, Click::TitleMe) => menu::open_me(g),
        (Menu::Title, Click::TitleShop) => menu::open_shop(g),
        (Menu::Title, Click::TitleSettings) => g.flow.overlay = Some(Overlay::Settings { prompt: false }),
        (Menu::Title, Click::TitleSound) => toggle_muted(g),
        (Menu::Title, Click::TitleMyTour) => g.flow.menu = Menu::MyTour { missions: g.flow.tour_missions },
        (Menu::MyTour { .. }, Click::TourSection(m)) => {
            g.flow.tour_missions = m;
            g.flow.menu = Menu::MyTour { missions: m };
        }
        (Menu::MyTour { .. }, Click::MissionSkip(i)) => {
            if !crate::missions::skip(g, i) {
                let needed = crate::missions::SKIP_COST - g.flow.user.coins;
                g.flow.overlay = Some(Overlay::NotEnough { currency: "coins".into(), needed });
            }
        }
        (Menu::Me(_) | Menu::Shop { .. } | Menu::MyTour { .. }, Click::Back) => menu::back_to_title(g),
        (Menu::Me(_), Click::MeTab(t)) => {
            if let Menu::Me(m) = &mut g.flow.menu {
                m.awards = false;
            }
            menu::set_tab(g, t)
        }
        (Menu::Me(_), Click::MeAwards) => {
            if let Menu::Me(m) = &mut g.flow.menu {
                m.awards = true;
            }
            g.flow.awards_list = Scroller::awards(crate::awards::IDS.len());
        }
        (Menu::Me(_), Click::AwardCollect(i)) => {
            if let Some(id) = crate::awards::IDS.get(i) {
                crate::awards::collect(g, id);
            }
        }
        (Menu::Me(_), Click::MeThumb(i)) => menu::focus(g, i),
        (Menu::Me(_), Click::MeFeature(k)) => menu::feature(g, k),
        (Menu::Me(_), Click::MeSelect) => menu::press_select(g),
        (Menu::Shop { .. }, Click::BoostCard(i)) => menu::toggle_card(g, i),
        (Menu::Shop { .. }, Click::BoostBuy(i)) => menu::buy_card(g, i),
        (Menu::None, _) => {}
        _ => {}
    }
    if g.flow.menu != Menu::None {
        return;
    }
    match (g.flow.screen.clone(), c) {
        (Screen::Results { .. }, Click::ResultsMenu) => g.to_title(),
        (Screen::Paused, Click::PauseMenu) => g.to_title(),
        (Screen::Paused, Click::PauseRestart) => g.to_game(),
        (Screen::Paused, Click::PauseSettings) => g.flow.overlay = Some(Overlay::Settings { prompt: false }),
        (Screen::Results { .. }, Click::DoubleUp) => double_up(g),
        (Screen::Paused, Click::PauseResume) => start_countdown(g),
        (Screen::Results { .. }, Click::ResultsBoosts) => g.flow.overlay = Some(Overlay::Boosts { open: None, scroll: 0.0 }),
        (Screen::SaveMe { .. }, Click::ReviveKeys) => revive_with_keys(g),
        // the rewarded ad succeeds; the revive lands after the frame's update
        (Screen::SaveMe { tall: true, .. }, Click::ReviveFree) => g.flow.pending_ad_revive = true,
        (Screen::SaveMe { .. }, _) | (Screen::NotEnough { .. }, _) => save_me_exit(g),
        (Screen::Prize(mut p), _) => {
            if prize_press(g, &mut p) {
                to_gameover(g);
            } else {
                g.flow.screen = Screen::Prize(p);
            }
        }
        (Screen::HighScore { alpha, .. }, _) => {
            if alpha >= 0.2 {
                open_results(g);
            }
        }
        (Screen::Results { .. }, Click::Play) => g.to_game(),
        (_, Click::PauseButton) => pause_button(g),
        (_, Click::Boost(k)) => crate::boosts::activate(g, k),
        (Screen::BuyBoards { .. }, Click::BuyBoard) => buy_board(g),
        (Screen::BuyBoards { .. }, _) => close_buy_boards(g),
        _ => {}
    }
}

/// The scroll container `vv` (58180) along y: drag (pointer px), wheel,
/// inertia (x0.95 per frame) and soft limits (x0.3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scroller {
    /// `targetY`: the content's position (units).
    pub target: f32,
    easing: f32,
    speed: f32,
    prev: f32,
    dragging: bool,
    drag_offset: f32,
    pub min: f32,
    pub max: f32,
}

impl Scroller {
    /// The results leaderboard (`updateLeaderboardArea`): from -198 to 60
    /// under the Double Up module, else to -40; at the top.
    pub fn leaderboard(double_up: bool) -> Self {
        let max = if double_up { 60.0 } else { -40.0 };
        let mut s = Self { min: -198.0, max, ..Default::default() };
        s.set_position(max);
        s
    }
    /// The Awards tab (`ty`, h 834): from -h/4 down to -n * 200 + h/2.
    pub fn awards(n: usize) -> Self {
        let max = -834.0 * 0.25;
        let mut s = Self { min: -(n as f32) * 200.0 + 834.0 * 0.5, max, ..Default::default() };
        s.set_position(max);
        s
    }
    pub fn set_position(&mut self, y: f32) {
        self.target = y;
        self.easing = y;
    }
    fn cap_soft(&mut self) {
        if self.easing > self.max {
            self.easing = self.max + (self.easing - self.max) * 0.3;
        } else if self.easing < self.min {
            self.easing = self.min + (self.easing - self.min) * 0.3;
        }
    }
    /// `onDown`.
    pub fn down(&mut self, y: f32) {
        self.speed = 0.0;
        self.target = self.easing;
        self.prev = self.easing;
        self.dragging = true;
        self.drag_offset = y - self.target;
    }
    /// `onMove`.
    pub fn moved(&mut self, y: f32) {
        if self.dragging {
            self.easing = y - self.drag_offset;
            self.cap_soft();
        }
    }
    /// `onUp`.
    pub fn up(&mut self) {
        self.dragging = false;
    }
    /// `onMouseWheel` (`deltaY` in browser px).
    pub fn wheel(&mut self, delta_y: f32) {
        self.speed = -delta_y * 0.15;
    }
    /// `update` (each frame).
    pub fn update(&mut self) {
        self.target += (self.easing - self.target) * 0.3;
        if self.dragging {
            let d = (self.easing - self.prev) * 0.7;
            self.speed += (d - self.speed) * 0.5;
            self.prev = self.easing;
        } else {
            self.speed *= 0.95;
            self.easing += self.speed;
            if self.easing > self.max {
                self.easing += (self.max - self.easing) * 0.3;
            } else if self.easing < self.min {
                self.easing += (self.min - self.easing) * 0.3;
            }
        }
    }
}

/// Pointer input for drag scrolling (`y`: window px).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    /// Pressed over the scroll area.
    Down(f32),
    Move(f32),
    Up,
    /// The wheel over the scroll area (`deltaY`, browser px).
    Wheel(f32),
}

/// The results leaderboard's scroller, if it shows.
pub fn results_list(g: &mut Game) -> Option<&mut Scroller> {
    if g.flow.overlay.is_some() {
        return None;
    }
    if matches!(&g.flow.menu, crate::menu::Menu::Me(m) if m.awards) {
        return Some(&mut g.flow.awards_list);
    }
    match &mut g.flow.screen {
        Screen::Results { list, .. } => Some(list),
        _ => None,
    }
}

pub fn pointer(g: &mut Game, p: Pointer) {
    let Some(list) = results_list(g) else { return };
    match p {
        Pointer::Down(y) => list.down(y),
        Pointer::Move(y) => list.moved(y),
        Pointer::Up => list.up(),
        Pointer::Wheel(d) => list.wheel(d),
    }
}

/// Is the nickname prompt taking the keyboard?
pub fn typing(g: &Game) -> bool {
    matches!(g.flow.overlay, Some(crate::menu::Overlay::Settings { prompt: true }))
}

/// The nickname prompt's keys (`rb`): a character (max 10), Backspace,
/// Enter closes it. `onNameChange`: a non-empty input becomes the name.
pub fn type_key(g: &mut Game, k: TypedKey) {
    if !typing(g) {
        return;
    }
    match k {
        TypedKey::Char(c) => {
            if g.flow.nickname_input.chars().count() < 10 && !c.is_control() {
                g.flow.nickname_input.push(c);
            }
        }
        TypedKey::Backspace => {
            g.flow.nickname_input.pop();
        }
        TypedKey::Enter => {
            crate::audio::play(g, "gui-tap"); // rb.hide
            g.flow.overlay = Some(crate::menu::Overlay::Settings { prompt: false });
            return;
        }
    }
    if !g.flow.nickname_input.is_empty() {
        g.flow.user.name = g.flow.nickname_input.clone();
        g.flow.save_user();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TypedKey {
    Char(char),
    Backspace,
    Enter,
}

/// `$.sound.toggleMuted()` (saved as `gameSettings.muted`; `crate::audio`
/// follows it).
fn toggle_muted(g: &mut Game) {
    g.flow.user.muted = !g.flow.user.muted;
    g.flow.save_user();
}

/// "Free Double Up!" (`Iv.onTap`, 60805): the rewarded ad (succeeds here),
/// then the run's coins once more to the wallet, the scoreboard shows them
/// doubled, the button hides and the leaderboard takes its room.
fn double_up(g: &mut Game) {
    let Screen::Results { coins, doubled, list, .. } = &mut g.flow.screen else { return };
    if *doubled || *coins == 0 {
        return;
    }
    *doubled = true;
    *list = Scroller::leaderboard(false);
    let c = *coins;
    g.flow.user.coins += c;
    g.flow.save_user();
}

/// After the frame's update (microtasks): the ad revive.
pub fn after_update(g: &mut Game) {
    if std::mem::take(&mut g.flow.pending_ad_revive) {
        revive(g, false);
    }
}

/// After the frame's render: timers and pixi tickers.
pub fn after_render(g: &mut Game) {
    // the mission notifications, spent coins, the throttled save
    crate::missions::after_render(g);
    crate::awards::after_render(g);
    crate::tutorial::after_render(g);
    // the Word Hunt banner's GSAP timeline (real time, also while paused)
    if let Some(b) = g.hud.word_banner.as_mut() {
        b.t += g.clock.delta_ms / 1000.0;
        if b.t >= b.delay() + 2.3 {
            let last = g.word_hunt.as_ref().is_some_and(|h| b.letter + 1 == h.word.len());
            if last && !b.complete_shown {
                b.complete_shown = true;
                b.t = 0.0;
            } else {
                g.hud.word_banner = None;
            }
        }
    }
    let now = g.clock.now;
    if let Some(crate::menu::Overlay::Prize(p)) = g.flow.overlay.as_mut() {
        prize_tick(p);
    }
    if let Some(due) = g.hud.open_due_ms {
        if now >= due {
            g.hud.open_due_ms = None;
            // fg.run(): updateCount = 1, score text, open
            g.hud.update_count = 1;
            g.hud.score = score(g);
            hud_open(g);
            crate::boosts::hud_run(g);
        }
    }
    match g.flow.screen.clone() {
        Screen::Dying { due_ms } if now >= due_ms => {
            let tall = g.flow.free_revivals < FREE_REVIVALS;
            g.flow.screen = Screen::SaveMe { secs: 6.0, tall };
        }
        // the clock ticks from the frame after it opened
        Screen::SaveMe { secs, tall } => {
            let secs = secs - 1.0 / 60.0;
            if secs < 0.0 {
                save_me_exit(g);
            } else {
                g.flow.screen = Screen::SaveMe { secs, tall };
            }
        }
        Screen::HighScore { alpha, frames } => {
            g.flow.screen = Screen::HighScore { alpha: alpha + (1.0 - alpha) * 0.01, frames: frames + 1 };
            crate::celebration::tick(g);
        }
        Screen::Prize(mut p) => {
            prize_tick(&mut p);
            g.flow.screen = Screen::Prize(p);
        }
        Screen::BuyBoards { in_run, bump } => {
            g.flow.screen = Screen::BuyBoards { in_run, bump: bump.saturating_sub(1) };
        }
        Screen::Countdown { n, due_ms } if now >= due_ms => {
            if n <= 1 {
                g.flow.screen = Screen::None;
                resume(g);
            } else {
                g.flow.screen = Screen::Countdown { n: n - 1, due_ms: now + 600.0 };
            }
        }
        Screen::Results { score, coins, play_due_ms, play_ready, frames, doubled, mut list } => {
            list.update();
            g.flow.screen = Screen::Results { score, coins, play_due_ms, play_ready: play_ready || now >= play_due_ms, frames: frames + 1, doubled, list };
        }
        _ => {}
    }
}

/// `game.revive(0, paid)` (50212).
pub fn revive(g: &mut Game, paid: bool) {
    if g.state == GameState::Running {
        return;
    }
    let (z, x) = (g.hero.body.cz(), g.hero.body.cx());
    crate::hero::player_reset(g, z, x);
    crate::hero_fx::halo_play(g); // Gp.revive: reviveHalo.play()
    crate::fx::revive_smoke(g, !paid);
    crate::audio::play(g, "hero-revive");
    crate::guard::reset(g);
    crate::powerups::unfreeze(g);
    if paid {
        g.flow.user.keys = (g.flow.user.keys - g.flow.paid_cost()).max(0);
        g.flow.save();
        g.flow.paid_revivals += 1;
    } else {
        g.flow.free_revivals += 1;
    }
    g.state = GameState::Running;
    remove_obstacles(g, 600.0);
    // onRevive: HUD open, controller hide; then player.run(0), controller.show, guard.run
    hud_open(g);
    crate::hero::player_run(g);
    crate::hero::controller_show(g);
    crate::guard::run(g);
    g.flow.screen = Screen::None;
}

/// Save me's keys button (click or `k`): revive for `paidRevivalCost` keys,
/// or the "not enough keys" popup.
fn revive_with_keys(g: &mut Game) {
    let cost = g.flow.paid_cost();
    if g.flow.user.keys >= cost {
        revive(g, true);
    } else {
        g.flow.screen = Screen::NotEnough { needed: cost - g.flow.user.keys };
    }
}

/// `LevelSystem.removeObstacles(range)` (48758): crash-removable entities
/// ahead of `stats.z - range` go back to the pool.
pub fn remove_obstacles(g: &mut Game, range: f64) {
    let limit = g.stats_z - range;
    let mut i = g.level_entities.len();
    while i > 0 {
        i -= 1;
        let id = g.level_entities[i];
        let e = g.ent(id);
        let back = e.body.as_ref().map(|b| b.back());
        if e.removable_on_crash && back.is_some_and(|b| b > limit) {
            if e.movable.is_some() {
                g.set_active(id, false);
            }
            g.remove_level_entity(i);
        }
    }
}
