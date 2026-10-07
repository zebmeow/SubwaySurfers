//! Awards (`Xg` with its 12 handlers `Lg`, deobfuscated.js 50290-50690;
//! `data/awards.json`: the site's `awards.json` tiers and the English
//! texts).
//!
//! Each award has four tiers (bronze, silver, gold, diamond) with a goal and
//! a key reward. Its progress (`value`, `tier`) is saved; when the value
//! reaches the tier's goal the "... complete" notification shows and the Me
//! panel's Awards tab offers "Collect Award": the keys (`awardPlayer`), the
//! next tier, the value back to 0.
//!
//! The handlers, as in the original:
//!
//! | award | progress |
//! |---|---|
//! | complete-missions | +1 a completed mission |
//! | collect-coins-single-run | a run ending with exactly the goal's coins |
//! | own-characters / own-boards / own-outfits | owned count - 1 |
//! | pickup-powerups | +1 a powerup pickup |
//! | score-no-coins | each second of a run: no coins yet and score >= goal |
//! | missions-in-one-run | missions completed in a run, at its end |
//! | super-sneakers-minutes | each second with sneakers on: +1/60 minute |
//! | open-mystery-boxes | +1 a (mini) mystery box opened |
//! | score-no-change-lanes | each second until a lane change: score >= goal |
//! | score-no-jump | each second until a jump or roll: score >= goal |
//!
//! The three "score without" checks run only after the tutorial is done
//! (`$.user.tutorial`), as in the original. `awardPlayer` both opens the
//! prize screen with the keys (crediting them) and adds them again 2 s
//! later: the original pays twice.

use crate::game::Game;
use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
struct Table {
    tiers: BTreeMap<String, BTreeMap<String, TierData>>,
    text: BTreeMap<String, String>,
}

#[derive(serde::Deserialize, Clone, Copy)]
struct TierData {
    goal: f64,
    reward: i64,
}

fn table() -> &'static Table {
    static T: std::sync::OnceLock<Table> = std::sync::OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("../data/awards.json")).expect("awards.json"))
}

/// The handlers in `Xg.initialise` order.
pub const IDS: [&str; 12] = [
    "award-complete-missions",
    "award-collect-coins-single-run",
    "award-own-characters",
    "award-own-boards",
    "award-pickup-powerups",
    "award-own-outfits",
    "award-score-no-coins",
    "award-missions-in-one-run",
    "award-super-sneakers-minutes",
    "award-open-mystery-boxes",
    "award-score-no-change-lanes",
    "award-score-no-jump",
];

/// `Ig`.
pub const TIERS: [&str; 4] = ["bronze", "silver", "gold", "diamond"];
/// `Fg.COMPLETED`.
pub const COMPLETED: u8 = 4;

/// An English text (`{{x}}` filled in).
pub fn text(key: &str, x: f64) -> String {
    table().text.get(key).map(|s| s.replace("{{x}}", &fmt_num(x))).unwrap_or_else(|| key.to_string())
}

fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 { format!("{}", x as i64) } else { format!("{x}") }
}

/// A saved award (`store.getProgress(id)`).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Progress {
    pub value: f64,
    pub tier: u8,
}

/// The tier's goal and reward (`tiers[tier]`; past diamond: infinite, 0).
pub fn tier(id: &str, tier: u8) -> (f64, i64) {
    TIERS
        .get(tier as usize)
        .and_then(|t| table().tiers.get(id).and_then(|m| m.get(*t)))
        .map_or((f64::INFINITY, 0), |d| (d.goal, d.reward))
}

pub fn progress(g: &Game, id: &str) -> Progress {
    g.flow.user.awards.get(id).copied().unwrap_or_default()
}

pub fn all_tiers_completed(g: &Game, id: &str) -> bool {
    progress(g, id).tier >= COMPLETED
}

/// `getProgressRatio`.
pub fn ratio(g: &Game, id: &str) -> f64 {
    if all_tiers_completed(g, id) {
        return 1.0;
    }
    let p = progress(g, id);
    let (goal, _) = tier(id, p.tier);
    p.value.min(goal) / goal
}

/// `isReadyToCollect`.
pub fn ready(g: &Game, id: &str) -> bool {
    !all_tiers_completed(g, id) && ratio(g, id) >= 1.0
}

/// `hasPrizeToCollect`.
pub fn has_prize(g: &Game) -> bool {
    IDS.iter().any(|id| ready(g, id))
}

/// `setProgresValue`.
fn set(g: &mut Game, id: &'static str, v: f64) {
    if ready(g, id) || all_tiers_completed(g, id) {
        return;
    }
    let p = progress(g, id);
    let (goal, _) = tier(id, p.tier);
    let v = v.min(goal);
    if p.value != v {
        g.flow.user.awards.insert(id.to_string(), Progress { value: v, tier: p.tier });
        g.flow.user_dirty = true;
        if v != 0.0 && ready(g, id) {
            // showNotification: "<title>\ncomplete", the tier's spray can
            let tier_name = TIERS.get(p.tier as usize).copied().unwrap_or("diamond");
            let text = format!("{}\n{}", text(id, 0.0), text("award-complete", 0.0));
            let icon = match tier_name {
                "bronze" => "spraycan-big-bronze",
                "silver" => "spraycan-big-silver",
                "gold" => "spraycan-big-gold",
                _ => "spraycan-big-diamond",
            };
            g.missions.toasts.push_back(crate::missions::Toast { text, icon, height: None, t: 0.0 });
        }
    }
}

fn add(g: &mut Game, id: &'static str, v: f64) {
    let cur = progress(g, id).value;
    set(g, id, cur + v);
}

/// The run-time state of the handlers.
#[derive(Clone, Debug, Default)]
pub struct Awards {
    /// `Bg.accumulated`: missions completed this run.
    missions_this_run: u32,
    /// `Yg.accumulated`: minutes with sneakers on.
    sneakers: f64,
    /// The `setInterval(…, 1000)` checks running this run: no coins, no
    /// lane change, no jump, sneakers; and the next tick (clock ms).
    no_coins: bool,
    no_lanes: bool,
    no_jump: bool,
    sneakers_on: bool,
    next_tick: f64,
    /// `awardPlayer`'s second payment: (due ms, keys).
    pending_keys: Vec<(f64, i64)>,
}

/// `missions.onCompleteMission`.
pub fn mission_completed(g: &mut Game) {
    add(g, "award-complete-missions", 1.0);
    g.awards.missions_this_run += 1;
}

/// `game.onPickupPowerup`.
pub fn powerup(g: &mut Game) {
    add(g, "award-pickup-powerups", 1.0);
}

/// `prizeScreen.onOpenPrize(boxType)`.
pub fn prize_opened(g: &mut Game, box_type: Option<&str>) {
    if matches!(box_type, Some("mystery-box" | "mini-mystery-box")) {
        add(g, "award-open-mystery-boxes", 1.0);
    }
}

/// `jf.onJump` / `Wf.onStart` (roll).
pub fn jumped(g: &mut Game) {
    g.awards.no_jump = false;
}

/// `Pf.onLaneChanged`.
pub fn lane_changed(g: &mut Game) {
    g.awards.no_lanes = false;
}

/// `nav.onGameplayStart`.
pub fn gameplay_start(g: &mut Game) {
    let a = &mut g.awards;
    a.missions_this_run = 0;
    a.sneakers = 0.0;
    let after_tutorial = g.flow.user.tutorial;
    a.no_coins = after_tutorial;
    a.no_lanes = after_tutorial;
    a.no_jump = after_tutorial;
    a.sneakers_on = true;
    a.next_tick = g.clock.now + 1000.0;
}

/// `nav.onGameplayFinish` (`toGameover`).
pub fn gameplay_finish(g: &mut Game) {
    // Rg: exactly the goal's coins in a run that ended
    let id = "award-collect-coins-single-run";
    let (goal, _) = tier(id, progress(g, id).tier);
    if g.state == crate::game::GameState::Gameover && g.coins as f64 == goal {
        set(g, id, goal);
    }
    // Bg
    let id = "award-missions-in-one-run";
    let (goal, _) = tier(id, progress(g, id).tier);
    let n = g.awards.missions_this_run as f64;
    if n >= goal {
        set(g, id, n);
    }
    let a = &mut g.awards;
    a.missions_this_run = 0;
    a.sneakers = 0.0;
    a.no_coins = false;
    a.no_lanes = false;
    a.no_jump = false;
    a.sneakers_on = false;
}

/// Each frame (timers, the ownership counts).
pub fn after_render(g: &mut Game) {
    // Ug / Hg / Wg: owned - 1 (on every shop change)
    let u = &g.flow.user;
    let owned = [
        ("award-own-characters", u.owned_characters.len()),
        ("award-own-boards", u.owned_boards.iter().filter(|b| !b.contains('~')).count()),
        ("award-own-outfits", u.owned_outfits.len()),
    ];
    for (id, n) in owned {
        set(g, id, n as f64 - 1.0);
    }
    let now = g.clock.now;
    // awardPlayer's 2 s timeout
    let due: Vec<i64> = g.awards.pending_keys.iter().filter(|(t, _)| now >= *t).map(|(_, k)| *k).collect();
    if !due.is_empty() {
        g.awards.pending_keys.retain(|(t, _)| now < *t);
        g.flow.user.keys += due.iter().sum::<i64>();
        g.flow.save_user();
    }
    // the 1 s intervals
    while g.awards.next_tick > 0.0 && now >= g.awards.next_tick {
        g.awards.next_tick += 1000.0;
        tick(g);
    }
}

fn tick(g: &mut Game) {
    let score = crate::flow::score(g) as f64;
    let goal = |g: &Game, id: &str| tier(id, progress(g, id).tier).0;
    if g.awards.no_coins {
        if g.coins != 0 {
            g.awards.no_coins = false;
        } else if score >= goal(g, "award-score-no-coins") {
            let v = goal(g, "award-score-no-coins");
            set(g, "award-score-no-coins", v);
        }
    }
    if g.awards.no_lanes && score >= goal(g, "award-score-no-change-lanes") {
        let v = goal(g, "award-score-no-change-lanes");
        set(g, "award-score-no-change-lanes", v);
        g.awards.no_lanes = false;
    }
    if g.awards.no_jump && score >= goal(g, "award-score-no-jump") {
        let v = goal(g, "award-score-no-jump");
        set(g, "award-score-no-jump", v);
        g.awards.no_jump = false;
    }
    if g.awards.sneakers_on && g.hero.sneakers.is_on() {
        g.awards.sneakers += 1.0 / 60.0;
        let id = "award-super-sneakers-minutes";
        if g.awards.sneakers >= goal(g, id) {
            let v = g.awards.sneakers;
            set(g, id, v);
            g.awards.sneakers = 0.0;
        }
    }
}

/// "Collect Award" (`Lg.collect`): the keys, then the next tier at 0.
pub fn collect(g: &mut Game, id: &str) {
    let Some(&id) = IDS.iter().find(|&&i| i == id) else { return };
    if all_tiers_completed(g, id) || !ready(g, id) {
        return;
    }
    let p = progress(g, id);
    let (_, reward) = tier(id, p.tier);
    // awardPlayer: the prize screen with the keys (it opens at once and
    // credits them), and 2 s later the keys once more
    let prize = crate::prizes::Prize { kind: "keys".into(), amount: reward, box_type: None };
    if let Some(mut ps) = crate::flow::PrizeScreen::open(vec![prize]) {
        crate::flow::prize_press(g, &mut ps);
        g.flow.overlay = Some(crate::menu::Overlay::Prize(ps));
    }
    g.awards.pending_keys.push((g.clock.now + 2000.0, reward));
    g.flow.user.awards.insert(id.to_string(), Progress { value: 0.0, tier: p.tier + 1 });
    g.flow.save_user();
    // afterCollect: the ownership awards recount
    after_render(g);
}

/// The Awards tab's order (`reorderItems`): progress ratio, then tier
/// (both descending), ready ones first among equals.
pub fn ordered(g: &Game) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = IDS.to_vec();
    v.sort_by(|a, b| {
        let (ra, rb) = (ratio(g, a), ratio(g, b));
        rb.partial_cmp(&ra)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(progress(g, b).tier.cmp(&progress(g, a).tier))
            .then(ready(g, b).cmp(&ready(g, a)))
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_twelve_awards_of_four_tiers() {
        for id in IDS {
            for t in 0..4 {
                let (goal, reward) = tier(id, t);
                assert!(goal.is_finite() && goal > 0.0 && reward > 0, "{id} {t}");
            }
            assert_eq!(tier(id, 4), (f64::INFINITY, 0));
            assert!(!text(id, 0.0).starts_with("award-"), "{id} text");
        }
        assert_eq!(text("award-open-mystery-boxes-desc", 3.0), "Open 3 Mystery Boxes.");
    }
}
