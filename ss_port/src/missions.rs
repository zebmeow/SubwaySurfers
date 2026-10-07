//! Missions (`Bm`, deobfuscated.js 44897; the data side `w_.prepareMissions`
//! 56566 and `$.user.progressMission` 72539).
//!
//! The 87 missions of `y_` (`data/missions.json`, from
//! `extracted/extract_missions.py`) form 29 sets of three (`C_`). Each set
//! completed adds one to `missionMultiplier`, the second score multiplier
//! (`score += distanceDelta * (multiplier + missionMultiplier)`, the HUD's
//! "x"), read at every run's reset: x1 for a new player, up to x30.
//!
//! `Bm` keeps 41 counters (`Lm`). The sim reports events with
//! [`add_stat`] / [`set_stat`]; the counters with "onerun" in their id reset
//! with each run. Only the current set's missions are tracked: a mission's
//! progress is `min(counter, amount)`; on completion the "Mission Complete"
//! notification (`Py`) queues and, once all three are done, the next set
//! starts tracking (its counters restart from the saved progress). Progress
//! is saved per set (`gameSettings.missions[set] = [{id, progress,
//! started}]`). The My Tour panel can skip a mission for 1700 coins.
//!
//! As in the original, the bump missions ("Bump into ... trains / light
//! signals", "Stumble into ... barriers") look for "train" / "light" /
//! "blocker" in the obstacle's class name, which the minified build names
//! "xr", "Ki", ...: they never progress (sets 8, 15, 18, 21, 25, 27 and 29
//! need a skip).
//!
//! Nothing here changes the simulation's state or RNG streams except
//! `missionMultiplier` at the next reset (and the coins of a skip).

use crate::game::Game;
use std::collections::{BTreeMap, VecDeque};

/// The JSON table and strings.
#[derive(serde::Deserialize)]
struct Table {
    missions: Vec<Def>,
    text: BTreeMap<String, String>,
}

#[derive(serde::Deserialize, Clone)]
struct Def {
    id: String,
    amount: i64,
}

fn table() -> &'static Table {
    static T: std::sync::OnceLock<Table> = std::sync::OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("../data/missions.json")).expect("missions.json"))
}

/// `lang-en.json` text of a key, `{{amount}}` filled in.
pub fn text(key: &str, amount: i64) -> String {
    table().text.get(key).map(|s| s.replace("{{amount}}", &amount.to_string())).unwrap_or_else(|| key.to_string())
}

/// `Lm`: every counter, 0 at start.
pub const STATS: [&str; 41] = [
    "mission-bump-barriers",
    "mission-bump-lights",
    "mission-bump-trains-onerun",
    "mission-buy-mystery",
    "mission-character-tokens",
    "mission-daily-challenges",
    "mission-dodge",
    "mission-get-caught",
    "mission-headstart",
    "mission-high-score",
    "mission-hoverboard",
    "mission-hoverboard-nocrash",
    "mission-jump",
    "mission-jump-onerun",
    "mission-jump-trains",
    "mission-pickup-coins",
    "mission-pickup-coins-air",
    "mission-pickup-coins-jetpack",
    "mission-pickup-coins-pogo",
    "mission-pickup-coins-magnet",
    "mission-pickup-coins-onerun",
    "mission-pickup-jetpacks",
    "mission-pickup-jetpacks-onerun",
    "mission-pickup-pogos",
    "mission-pickup-pogos-onerun",
    "mission-pickup-keys",
    "mission-pickup-magnets",
    "mission-pickup-magnets-onerun",
    "mission-pickup-mystery",
    "mission-pickup-powerups",
    "mission-pickup-sneakers",
    "mission-pickup-sneakers-onerun",
    "mission-roll",
    "mission-roll-lane",
    "mission-roll-onerun",
    "mission-score",
    "mission-score-nocoins",
    "mission-score-onerun",
    "mission-scoreBooster",
    "mission-spend-coins",
    "mission-coins-mystery",
];

/// The skip price on the My Tour panel (`Sy`, 63393).
pub const SKIP_COST: i64 = 1700;

/// One saved mission (`gameSettings.missions[set][i]`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MissionSave {
    pub id: String,
    pub progress: i64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub started: bool,
}

/// A mission of `C_` with its live progress.
#[derive(Clone, Debug, PartialEq)]
pub struct Mission {
    pub id: String,
    pub amount: i64,
    pub progress: i64,
    pub completed: bool,
    pub set: usize,
}

impl Mission {
    /// `translate(id, {amount})`.
    pub fn description(&self) -> String {
        text(&self.id, self.amount)
    }
}

/// `$.notification` (`Py`, 64217): queued "Mission Complete" notes.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub text: String,
    /// The icon (atlas frame) and the base's height (`None`: the icon's + 20).
    pub icon: &'static str,
    pub height: Option<f32>,
    /// Seconds since this note started (`go(0.5)`, slide in 0.3, hold 3,
    /// slide out 0.3).
    pub t: f64,
}

impl Toast {
    pub const DURATION: f64 = 0.5 + 0.3 + 3.0 + 0.3;
    /// The content's y offset (0 = in place at y 100, -300 = hidden), or
    /// None before it opens.
    pub fn offset(&self) -> Option<f64> {
        let t = self.t - 0.5;
        if t < 0.0 {
            return None;
        }
        let sine_out = |p: f64| (p.clamp(0.0, 1.0) * std::f64::consts::FRAC_PI_2).sin();
        let sine_in = |p: f64| 1.0 - (p.clamp(0.0, 1.0) * std::f64::consts::FRAC_PI_2).cos();
        Some(if t < 0.3 {
            -300.0 * (1.0 - sine_out(t / 0.3))
        } else if t < 3.3 {
            0.0
        } else {
            -300.0 * sine_in((t - 3.3) / 0.3)
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Missions {
    /// `Bm.data`.
    pub data: BTreeMap<&'static str, i64>,
    /// `C_`: the sets with their progress.
    pub sets: Vec<Vec<Mission>>,
    /// The set whose missions are tracked (`missionsToTrack`).
    pub tracked: usize,
    /// `completedMissions` of the tracked set.
    pub completed: usize,
    /// The notifications: the showing one first.
    pub toasts: VecDeque<Toast>,
    /// The saved coins last seen (`onCoinsSpent` fires on any decrease).
    last_coins: Option<i64>,
}

impl Missions {
    /// `w_.prepareMissions`: the table with the saved progress.
    pub fn prepare(user: &crate::flow::UserData) -> Self {
        let defs = &table().missions;
        let sets = defs
            .chunks(3)
            .enumerate()
            .map(|(set, c)| {
                c.iter()
                    .map(|d| {
                        let saved = user.missions.get(set).and_then(|s| s.iter().find(|m| m.id == d.id));
                        let progress = saved.map_or(0, |m| m.progress).min(d.amount);
                        Mission { id: d.id.clone(), amount: d.amount, progress, completed: progress == d.amount, set }
                    })
                    .collect()
            })
            .collect();
        let mut m = Self { data: STATS.iter().map(|&k| (k, 0)).collect(), sets, ..Default::default() };
        m.tracked = m.mission_set();
        m
    }

    /// `getMissionMultiplier`: the sets completed in a row from the first.
    pub fn multiplier(&self) -> usize {
        self.sets.iter().take_while(|s| s.iter().all(|m| m.completed)).count()
    }

    /// `getMissionSet`: the set being played.
    pub fn mission_set(&self) -> usize {
        self.multiplier().min(self.sets.len().saturating_sub(1))
    }

    /// `getCurrentMissions`.
    pub fn current(&self) -> &[Mission] {
        &self.sets[self.mission_set()]
    }

    /// The number shown as "MISSION SET n" (and the multiplier label): the
    /// set + 1, one more when its three are done (the last set).
    pub fn set_label(&self) -> usize {
        let done = self.current().iter().all(|m| m.completed);
        self.mission_set() + 1 + usize::from(done)
    }
}

/// `$.user.progressMission(id, progress, set)`.
fn save_progress(g: &mut Game, id: &str, progress: i64, set: usize) {
    let u = &mut g.flow.user;
    while u.missions.len() <= set {
        u.missions.push(Vec::new());
    }
    match u.missions[set].iter_mut().find(|m| m.id == id) {
        Some(m) => m.progress = progress,
        None => u.missions[set].push(MissionSave { id: id.to_string(), progress, started: false }),
    }
    g.flow.user_dirty = true;
}

/// `initMissionTracking`: track the current set; its counters restart from
/// the saved progress; `markMissionStarted` for the new ones.
pub fn init_tracking(g: &mut Game) {
    let m = &mut g.missions;
    let set = m.mission_set();
    m.tracked = set;
    m.completed = 0;
    let current = m.sets[set].clone();
    for ms in &current {
        if let Some(k) = STATS.iter().find(|&&k| k == ms.id) {
            g.missions.data.insert(k, ms.progress);
        }
        g.missions.completed += usize::from(ms.completed);
        if !ms.completed {
            let u = &mut g.flow.user;
            while u.missions.len() <= set {
                u.missions.push(Vec::new());
            }
            match u.missions[set].iter_mut().find(|s| s.id == ms.id) {
                Some(s) if s.started => {}
                Some(s) => {
                    s.started = true;
                    g.flow.user_dirty = true;
                }
                None => {
                    u.missions[set].push(MissionSave { id: ms.id.clone(), progress: 0, started: true });
                    g.flow.user_dirty = true;
                }
            }
        }
    }
}

/// `Bm.reset` (a run's `onReset`): the one-run counters to 0, then track.
pub fn reset(g: &mut Game) {
    for (k, v) in g.missions.data.iter_mut() {
        if k.contains("onerun") {
            *v = 0;
        }
    }
    init_tracking(g);
}

fn data<'a>(g: &'a mut Game, k: &'static str) -> &'a mut i64 {
    g.missions.data.entry(k).or_insert(0)
}

/// `Bm.addStat(amount, id)`.
pub fn add_stat(g: &mut Game, amount: i64, id: &'static str) {
    let mut amount = amount;
    match id {
        // processCoinsPickup
        "mission-pickup-coins" => {
            let h = &g.hero;
            let (jet, air, mag, pogo) = (h.jetpack.is_on(), !h.landed(), h.magnet.is_on(), h.pogo.is_on());
            *data(g, "mission-pickup-coins-jetpack") += i64::from(jet);
            *data(g, "mission-pickup-coins-air") += i64::from(air);
            *data(g, "mission-pickup-coins-magnet") += i64::from(mag);
            *data(g, "mission-pickup-coins-pogo") += i64::from(pogo);
            *data(g, "mission-pickup-coins-onerun") += 1;
        }
        // processPowerups
        "mission-pickup-powerups" => {
            let h = &g.hero;
            let on = [
                (h.jetpack.is_on(), "mission-pickup-jetpacks", "mission-pickup-jetpacks-onerun"),
                (h.magnet.is_on(), "mission-pickup-magnets", "mission-pickup-magnets-onerun"),
                (h.pogo.is_on(), "mission-pickup-pogos", "mission-pickup-pogos-onerun"),
                (h.sneakers.is_on(), "mission-pickup-sneakers", "mission-pickup-sneakers-onerun"),
            ];
            for (is_on, a, b) in on {
                if is_on {
                    *data(g, a) += 1;
                    *data(g, b) += 1;
                }
            }
        }
        // processRoll
        "mission-roll" => {
            if g.hero.lane.lane == 0 {
                *data(g, "mission-roll-lane") += 1;
            }
            *data(g, "mission-roll-onerun") += 1;
        }
        // processJump
        "mission-jump" => {
            if g.hero.ground > 7.0 {
                *data(g, "mission-jump-trains") += 1;
            }
            *data(g, "mission-jump-onerun") += 1;
        }
        "mission-get-caught" if g.stats_time > 10.0 => amount = 0,
        _ => {}
    }
    *data(g, id) += amount;
    progress_all(g);
}

/// `Player.processBumpMission(name)`: the obstacle's class name.
pub fn process_bump(g: &mut Game, who: &str) {
    if who.contains("light") {
        add_stat(g, 1, "mission-bump-lights");
    } else if who.contains("train") {
        add_stat(g, 1, "mission-bump-trains-onerun");
    } else if who.contains("blocker") {
        add_stat(g, 1, "mission-bump-barriers");
    }
}

/// `zm`: how `setStat` applies the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetMode {
    Set,
    Increment,
}

/// `Bm.setStat(value, id, mode)`.
pub fn set_stat(g: &mut Game, value: i64, id: &'static str, mode: SetMode) {
    if id == "mission-score" {
        // processScore
        if g.coins == 0 {
            *data(g, "mission-score-nocoins") = value;
        }
        *data(g, "mission-score-onerun") = value;
    }
    match mode {
        SetMode::Set => *data(g, id) = value,
        SetMode::Increment => *data(g, id) += value,
    }
    progress_all(g);
}

/// `missionsToTrack.forEach(m => !m.completed && progressMission(m))` (the
/// list as it was when the loop started).
fn progress_all(g: &mut Game) {
    let set = g.missions.tracked;
    for i in 0..g.missions.sets[set].len() {
        if !g.missions.sets[set][i].completed {
            progress_mission(g, set, i);
        }
    }
}

/// `Bm.progressMission`.
fn progress_mission(g: &mut Game, set: usize, i: usize) {
    let (id, amount) = {
        let m = &g.missions.sets[set][i];
        (m.id.clone(), m.amount)
    };
    let have = STATS.iter().find(|&&k| k == id).map_or(0, |k| g.missions.data[k]);
    let p = have.min(amount);
    let m = &mut g.missions.sets[set][i];
    if m.progress == p {
        return;
    }
    m.progress = p;
    m.completed = p == amount;
    if m.completed {
        g.missions.completed += 1;
        crate::awards::mission_completed(g); // onCompleteMission
        // showNotification
        let text = format!("{}\n{}", text("mission-complete", 0), text(&id, amount));
        g.missions.toasts.push_back(Toast { text, icon: "mission-completed-checkmark", height: Some(130.0), t: 0.0 });
        if g.missions.completed == g.missions.sets[g.missions.tracked].len() {
            init_tracking(g);
        }
    }
    save_progress(g, &id, p, set);
}

/// `onCoinsSpent`: the saved coins went down (any purchase, a skip).
pub fn watch_coins(g: &mut Game) {
    let now = g.flow.user.coins;
    if let Some(last) = g.missions.last_coins {
        if now < last {
            add_stat(g, last - now, "mission-spend-coins");
        }
    }
    g.missions.last_coins = Some(now);
}

/// `Sy`'s skip button: pay 1700 coins, the mission is done (it does not
/// count toward `completedMissions` until the next tracking; the original
/// shows no notification). Returns false without the coins.
pub fn skip(g: &mut Game, i: usize) -> bool {
    if g.flow.user.coins < SKIP_COST {
        return false;
    }
    g.flow.user.coins -= SKIP_COST;
    watch_coins(g);
    let set = g.missions.mission_set();
    let Some(m) = g.missions.sets[set].get_mut(i) else { return false };
    m.completed = true;
    m.progress = m.amount;
    let (id, amount) = (m.id.clone(), m.amount);
    save_progress(g, &id, amount, set);
    g.flow.save_user();
    g.flow.user_dirty = false;
    true
}

/// The notifications' timers (real time, also while paused), and a
/// throttled save of the progress (`$.user.save()` on every change).
pub fn after_render(g: &mut Game) {
    watch_coins(g);
    let dt = g.clock.delta_ms / 1000.0;
    if let Some(t) = g.missions.toasts.front_mut() {
        let was = t.t;
        t.t += dt;
        // `Py.show`: after `go(0.5)` the sound, then it opens
        if was < 0.5 && t.t >= 0.5 {
            crate::audio::play(g, "mission-notification");
        }
        let Some(t) = g.missions.toasts.front() else { return };
        if t.t >= Toast::DURATION {
            g.missions.toasts.pop_front();
        }
    }
    if g.flow.user_dirty && (g.frame % 60 == 0 || g.state != crate::game::GameState::Running) {
        g.flow.save_user();
        g.flow.user_dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_29_sets_of_three() {
        let m = Missions::prepare(&crate::flow::UserData::default());
        assert_eq!(m.sets.len(), 29);
        assert!(m.sets.iter().all(|s| s.len() == 3));
        assert_eq!(m.sets[0][0].id, "mission-pickup-coins");
        assert_eq!(m.sets[0][0].amount, 500);
        assert_eq!(m.multiplier(), 0);
        assert_eq!(text("mission-jump", 20), "Jump 20 times");
        // every tracked id is a counter
        assert!(m.sets.iter().flatten().all(|ms| STATS.contains(&ms.id.as_str())));
    }

    #[test]
    fn toast_slides_in_holds_and_out() {
        let t = |t| Toast { text: String::new(), icon: "", height: None, t }.offset();
        assert_eq!(t(0.2), None);
        assert_eq!(t(0.5), Some(-300.0));
        assert_eq!(t(0.8), Some(0.0));
        assert_eq!(t(3.0), Some(0.0));
        assert!((t(4.1).unwrap() + 300.0).abs() < 1e-9);
    }
}
