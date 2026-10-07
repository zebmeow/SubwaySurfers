//! The menus around a run: title screen `mb`, Me panel `hy` (Characters /
//! Boards), boost shop `Mv` and boost panel `Pv`, buy hoverboards `Fv`, not
//! enough currency `Ny`, and the purchase controller `T_` (56664). This is
//! the state and the actions; `crate::ui_menu` draws it.
//! See docs/js_notes/ui_powerups_shop.md §2-§5.

use crate::flow::UserData;
use crate::game::Game;
use crate::shop::Catalog;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeTab {
    Characters,
    Boards,
}

/// Me panel state (`my` / `py` selection model).
#[derive(Clone, Debug, PartialEq)]
pub struct MeState {
    pub tab: MeTab,
    /// Focused character (index into [`characters`]) and previewed outfit.
    pub character: usize,
    pub outfit: usize,
    /// Focused board (index into the catalog boards) and the previewed powers
    /// (`profile.boardPowers`, last = focused button).
    pub board: usize,
    pub board_preview: Vec<usize>,
    /// Thumb strip scroll (virtual units, <= 0).
    pub scroll: f32,
    /// The Awards tab is open (`hy.setSection("awards")`).
    pub awards: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Menu {
    /// In a run (or the death flow).
    None,
    Title,
    Me(MeState),
    /// Boost shop: open card, list scroll.
    Shop { open: Option<usize>, scroll: f32 },
    /// My Tour `My` (64029): the Word Hunt or Missions section.
    MyTour { missions: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Overlay {
    /// Boost panel `Pv` over the results notepad.
    Boosts { open: Option<usize>, scroll: f32 },
    /// Buy hoverboards `Fv` (from the title; mid-run it is `Screen::BuyBoards`).
    BuyBoards { bump: u32 },
    /// Not enough currency `Ny`: currency ("coins" / "keys") and the shortfall.
    NotEnough { currency: String, needed: i64 },
    /// A box bought in the shop, opened at once (`nav.toPrizeScreen`).
    Prize(crate::flow::PrizeScreen),
    /// Settings `cb` (65835); `prompt`: the nickname prompt `rb` is open.
    Settings { prompt: bool },
}

/// The Characters tab list: the original roster `v_`.
pub fn characters() -> Vec<String> {
    crate::render_actors::character_roster()
}

/// Everything the shop sells.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Character(String),
    /// Outfit `k` (1-based) of a character.
    Outfit(String, usize),
    Board(String),
    /// Board powerup `k` (1-based index into its `powerups`).
    BoardPower(String, usize),
    Upgrade(&'static str),
    Consumable(&'static str),
}

/// Boost list order (`kv`): single use, then upgrades.
pub const CONSUMABLES: [&str; 4] = ["hoverboard", "mysteryBox", "scoreBooster", "headstart"];
pub const UPGRADES: [&str; 4] = ["jetpack", "sneakers", "magnet", "multiplier"];

/// `h_` (54796): single-use prices.
pub fn consumable_cost(id: &str) -> i64 {
    match id {
        "hoverboard" => 300,
        "mysteryBox" => 500,
        "scoreBooster" => 3000,
        "headstart" => 2000,
        _ => 0,
    }
}

pub fn upgrade_level(u: &UserData, id: &str) -> u32 {
    let p = &u.upgrades;
    match id {
        "jetpack" => p.jetpack_tier,
        "sneakers" => p.sneakers_tier,
        "magnet" => p.magnet_tier,
        "multiplier" => p.multiplier_tier,
        _ => 0,
    }
}

fn upgrade_level_mut<'a>(u: &'a mut UserData, id: &str) -> Option<&'a mut u32> {
    let p = &mut u.upgrades;
    Some(match id {
        "jetpack" => &mut p.jetpack_tier,
        "sneakers" => &mut p.sneakers_tier,
        "magnet" => &mut p.magnet_tier,
        "multiplier" => &mut p.multiplier_tier,
        _ => return None,
    })
}

pub fn consumable_count(u: &UserData, id: &str) -> i64 {
    match id {
        "hoverboard" => u.hoverboards,
        "mysteryBox" => u.mystery_boxes,
        "scoreBooster" => u.score_boosters,
        "headstart" => u.headstarts,
        _ => 0,
    }
}

/// Next upgrade price (`levels[level].cost`), None when maxed.
pub fn upgrade_cost(u: &UserData, id: &str) -> Option<i64> {
    crate::powerups::UPGRADE_COSTS.get(upgrade_level(u, id) as usize).copied()
}

pub fn owns(u: &UserData, item: &Item) -> bool {
    match item {
        Item::Character(id) => u.owned_characters.contains(id),
        Item::Outfit(id, k) => u.owned_outfits.contains(&format!("{id}-outfit-{k}")),
        Item::Board(id) => u.owned_boards.contains(id),
        Item::BoardPower(b, k) => Catalog::get()
            .board(b)
            .and_then(|bd| bd.powerups.get(k - 1))
            .is_some_and(|p| u.owned_boards.contains(&format!("{b}~{}", p.id))),
        Item::Upgrade(id) => upgrade_level(u, id) >= 6,
        Item::Consumable(_) => false,
    }
}

/// (cost, currency) of an item.
pub fn price(u: &UserData, item: &Item) -> (i64, &'static str) {
    let c = Catalog::get();
    let cur = |s: &str| if s == "keys" { "keys" } else { "coins" };
    match item {
        Item::Character(id) => c.character(id).map(|x| (x.cost, cur(&x.currency))).unwrap_or((0, "coins")),
        Item::Outfit(id, k) => c.character(id).and_then(|x| x.outfits.get(k - 1)).map(|o| (o.cost, cur(&o.currency))).unwrap_or((0, "keys")),
        Item::Board(id) => c.board(id).map(|b| (b.cost, cur(&b.currency))).unwrap_or((0, "coins")),
        Item::BoardPower(b, k) => c.board(b).and_then(|bd| bd.powerups.get(k - 1)).map(|p| (p.cost, cur(&p.currency))).unwrap_or((0, "keys")),
        Item::Upgrade(id) => (upgrade_cost(u, id).unwrap_or(-1), "coins"),
        Item::Consumable(id) => (consumable_cost(id), "coins"),
    }
}

/// An outfit / board powerup is `locked` while its owner is not purchased.
pub fn locked(u: &UserData, item: &Item) -> bool {
    match item {
        Item::Outfit(id, _) => !owns(u, &Item::Character(id.clone())),
        Item::BoardPower(b, _) => !owns(u, &Item::Board(b.clone())),
        _ => false,
    }
}

fn wallet<'a>(u: &'a mut UserData, currency: &str) -> &'a mut i64 {
    if currency == "keys" { &mut u.keys } else { &mut u.coins }
}

/// `T_.purchase` (56808): `canBuy` (cost <= wallet, and boosts below max),
/// pay, apply, save. Returns false (and changes nothing) if it cannot.
pub fn purchase(u: &mut UserData, item: &Item) -> bool {
    let (cost, currency) = price(u, item);
    let can = match item {
        Item::Upgrade(id) => upgrade_level(u, id) < 6,
        _ => true,
    } && cost >= 0
        && cost <= *wallet(u, currency);
    if !can {
        return false;
    }
    *wallet(u, currency) -= cost;
    match item {
        Item::Character(id) => {
            if !u.owned_characters.contains(id) {
                u.owned_characters.push(id.clone());
            }
        }
        Item::Outfit(id, k) => u.owned_outfits.push(format!("{id}-outfit-{k}")),
        Item::Board(id) => {
            if !u.owned_boards.contains(id) {
                u.owned_boards.push(id.clone());
            }
        }
        Item::BoardPower(b, k) => {
            if let Some(p) = Catalog::get().board(b).and_then(|bd| bd.powerups.get(k - 1)) {
                u.owned_boards.push(format!("{b}~{}", p.id));
            }
        }
        Item::Upgrade(id) => {
            if let Some(l) = upgrade_level_mut(u, id) {
                *l += 1;
            }
        }
        Item::Consumable(id) => match *id {
            "hoverboard" => u.hoverboards += 1,
            // opened at once by the caller (no stored count)
            "mysteryBox" => {}
            "scoreBooster" => u.score_boosters += 1,
            "headstart" => u.headstarts += 1,
            _ => {}
        },
    }
    true
}

// ---- navigation ------------------------------------------------------------------------

/// `$.nav.toMePanel()`: opens on the last tab used, focused on the saved selection.
pub fn open_me(g: &mut Game) {
    let tab = match &g.flow.menu {
        Menu::Me(m) => m.tab,
        _ => g.flow.last_me_tab,
    };
    let u = &g.flow.user;
    let list = characters();
    let character = list.iter().position(|c| *c == u.selected_character).unwrap_or(0);
    let boards = &Catalog::get().boards;
    let board = boards.iter().position(|b| b.id == u.selected_board).unwrap_or(0);
    g.flow.menu = Menu::Me(MeState {
        tab,
        character,
        outfit: if u.selected_character == list[character] { u.selected_outfit } else { 0 },
        board,
        board_preview: u.board_powers.clone(),
        scroll: 0.0,
        awards: false,
    });
    scroll_to_focus(g);
}

pub fn open_shop(g: &mut Game) {
    g.flow.menu = Menu::Shop { open: None, scroll: 0.0 };
}

pub fn back_to_title(g: &mut Game) {
    if let Menu::Me(m) = &g.flow.menu {
        g.flow.last_me_tab = m.tab;
    }
    g.flow.menu = Menu::Title;
}

fn scroll_to_focus(g: &mut Game) {
    if let Menu::Me(m) = &mut g.flow.menu {
        let (i, n) = match m.tab {
            MeTab::Characters => (m.character, characters().len()),
            MeTab::Boards => (m.board, Catalog::get().boards.len()),
        };
        // easeToSlot(max(3, idx + 1) - 3): keep the focused thumb in view
        let content = n as f32 * 130.0 + 70.0;
        let max_scroll = (content - 634.0).max(0.0);
        m.scroll = -((i as f32 * 130.0 + 70.0) - 317.0).clamp(0.0, max_scroll);
    }
}

pub fn set_tab(g: &mut Game, tab: MeTab) {
    if let Menu::Me(m) = &mut g.flow.menu {
        m.tab = tab;
    }
    scroll_to_focus(g);
}

/// Thumb tap: focus that character / board (outfit and powers preview reset).
pub fn focus(g: &mut Game, i: usize) {
    let u = g.flow.user.clone();
    if let Menu::Me(m) = &mut g.flow.menu {
        match m.tab {
            MeTab::Characters => {
                if i < characters().len() && i != m.character {
                    m.character = i;
                    m.outfit = if characters()[i] == u.selected_character { u.selected_outfit } else { 0 };
                }
            }
            MeTab::Boards => {
                if i < Catalog::get().boards.len() && i != m.board {
                    m.board = i;
                    let id = &Catalog::get().boards[i].id;
                    m.board_preview = if *id == u.selected_board { u.board_powers.clone() } else { vec![0] };
                }
            }
        }
    }
    scroll_to_focus(g);
}

/// Left / right keys in the Me panel.
pub fn step_focus(g: &mut Game, d: i32) {
    if let Menu::Me(m) = &g.flow.menu {
        let (i, n) = match m.tab {
            MeTab::Characters => (m.character, characters().len()),
            MeTab::Boards => (m.board, Catalog::get().boards.len()),
        };
        let j = (i as i32 + d).clamp(0, n as i32 - 1) as usize;
        focus(g, j);
    }
}

/// Feature button `k` (0 = base, 1-2 = outfits / board powerups).
pub fn feature(g: &mut Game, k: usize) {
    if let Menu::Me(m) = &mut g.flow.menu {
        match m.tab {
            MeTab::Characters => m.outfit = k,
            MeTab::Boards => {
                // focusPowerup: keep only turned-on powers, then move k to the end
                let sel = g.flow.user.board_powers.clone();
                let on_board = Catalog::get().boards[m.board].id == g.flow.user.selected_board;
                m.board_preview.retain(|p| on_board && sel.contains(p));
                m.board_preview.retain(|&p| p != k);
                m.board_preview.push(k);
            }
        }
    }
}

/// What the big button does for the current focus (`refreshSelected`).
#[derive(Clone, Debug, PartialEq)]
pub enum SelectButton {
    Select,
    Selected,
    Locked,
    TurnOn,
    TurnOff,
    Buy(i64, &'static str),
}

pub fn select_button(g: &Game) -> Option<(Item, SelectButton)> {
    let Menu::Me(m) = &g.flow.menu else { return None };
    let u = &g.flow.user;
    match m.tab {
        MeTab::Characters => {
            let id = characters()[m.character].clone();
            let item = if m.outfit == 0 { Item::Character(id.clone()) } else { Item::Outfit(id.clone(), m.outfit) };
            let state = if locked(u, &item) {
                SelectButton::Locked
            } else if !owns(u, &item) && !(m.outfit == 0 && id == "jake") {
                let (c, cur) = price(u, &item);
                SelectButton::Buy(c, cur)
            } else if u.selected_character == id && u.selected_outfit == m.outfit {
                SelectButton::Selected
            } else {
                SelectButton::Select
            };
            Some((item, state))
        }
        MeTab::Boards => {
            let b = &Catalog::get().boards[m.board];
            let k = *m.board_preview.last().unwrap_or(&0);
            if k == 0 {
                let item = Item::Board(b.id.clone());
                let state = if !owns(u, &item) {
                    let (c, cur) = price(u, &item);
                    SelectButton::Buy(c, cur)
                } else if u.selected_board == b.id {
                    SelectButton::Selected
                } else {
                    SelectButton::Select
                };
                Some((item, state))
            } else {
                let item = Item::BoardPower(b.id.clone(), k);
                let state = if locked(u, &item) {
                    SelectButton::Locked
                } else if owns(u, &item) {
                    if u.selected_board == b.id && u.board_powers.contains(&k) {
                        SelectButton::TurnOff
                    } else {
                        SelectButton::TurnOn
                    }
                } else {
                    let (c, cur) = price(u, &item);
                    SelectButton::Buy(c, cur)
                };
                Some((item, state))
            }
        }
    }
}

/// The big button: select / buy (buying also equips) / turn a power on or off.
pub fn press_select(g: &mut Game) {
    let Some((item, state)) = select_button(g) else { return };
    match state {
        SelectButton::Selected | SelectButton::Locked => {}
        SelectButton::Buy(cost, currency) => {
            if purchase(&mut g.flow.user, &item) {
                confirm(g);
                g.flow.save_user();
            } else {
                // Ny: the shortfall in that currency
                let have = if currency == "keys" { g.flow.user.keys } else { g.flow.user.coins };
                g.flow.overlay = Some(Overlay::NotEnough { currency: currency.to_string(), needed: cost - have });
            }
        }
        SelectButton::Select | SelectButton::TurnOn => {
            confirm(g);
            g.flow.save_user();
        }
        SelectButton::TurnOff => {
            if let Menu::Me(m) = &mut g.flow.menu {
                let k = *m.board_preview.last().unwrap_or(&0);
                g.flow.user.board_powers.retain(|&p| p != k);
                if g.flow.user.board_powers.is_empty() {
                    g.flow.user.board_powers.push(0);
                }
                m.board_preview = g.flow.user.board_powers.clone();
            }
            g.flow.save_user();
        }
    }
}

/// `confirmSelection`: equip the focused character+outfit / board+powers.
fn confirm(g: &mut Game) {
    let Menu::Me(m) = &mut g.flow.menu else { return };
    let u = &mut g.flow.user;
    match m.tab {
        MeTab::Characters => {
            u.selected_character = characters()[m.character].clone();
            u.selected_outfit = m.outfit;
        }
        MeTab::Boards => {
            u.selected_board = Catalog::get().boards[m.board].id.clone();
            let mut powers: Vec<usize> = m.board_preview.clone();
            powers.dedup();
            u.board_powers = powers;
            m.board_preview = u.board_powers.clone();
        }
    }
}

// ---- boost list (Mv / Pv) ----------------------------------------------------------------

/// Card `i` of the boost list (consumables, then upgrades).
pub fn boost_item(i: usize) -> Option<Item> {
    if i < CONSUMABLES.len() {
        Some(Item::Consumable(CONSUMABLES[i]))
    } else {
        UPGRADES.get(i - CONSUMABLES.len()).map(|&u| Item::Upgrade(u))
    }
}

pub const BOOST_CARDS: usize = CONSUMABLES.len() + UPGRADES.len();

fn boost_state(g: &mut Game) -> Option<&mut Option<usize>> {
    match (&mut g.flow.overlay, &mut g.flow.menu) {
        (Some(Overlay::Boosts { open, .. }), _) => Some(open),
        (None, Menu::Shop { open, .. }) => Some(open),
        _ => None,
    }
}

/// Tap a card: close every other one, toggle this one.
pub fn toggle_card(g: &mut Game, i: usize) {
    if let Some(open) = boost_state(g) {
        *open = if *open == Some(i) { None } else { Some(i) };
    }
}

/// `Ov.onBuy`: a failed purchase does nothing (no popup in the boost shop).
pub fn buy_card(g: &mut Game, i: usize) {
    let Some(item) = boost_item(i) else { return };
    if purchase(&mut g.flow.user, &item) {
        g.flow.save_user();
        if item == Item::Consumable("mysteryBox") {
            crate::missions::add_stat(g, 1, "mission-buy-mystery");
            let p = crate::prizes::roll(g, "mystery-box");
            g.flow.overlay = crate::flow::PrizeScreen::open(vec![p]).map(Overlay::Prize);
        }
    }
}

pub fn scroll(g: &mut Game, dy: f32) {
    let s = match (&mut g.flow.overlay, &mut g.flow.menu) {
        (Some(Overlay::Boosts { scroll, .. }), _) => scroll,
        (None, Menu::Shop { scroll, .. }) => scroll,
        (None, Menu::Me(m)) => {
            let n = match m.tab {
                MeTab::Characters => characters().len(),
                MeTab::Boards => Catalog::get().boards.len(),
            };
            let max = (n as f32 * 130.0 + 70.0 - 634.0).max(0.0);
            m.scroll = (m.scroll - dy).clamp(-max, 0.0);
            return;
        }
        _ => return,
    };
    *s = (*s - dy).clamp(-900.0, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_tiers_and_costs() {
        let mut u = UserData { coins: 200_000, ..Default::default() };
        let costs: Vec<i64> = (0..7).map(|_| {
            let c = upgrade_cost(&u, "magnet").unwrap_or(-1);
            purchase(&mut u, &Item::Upgrade("magnet"));
            c
        }).collect();
        assert_eq!(costs, vec![500, 1500, 3000, 10000, 30000, 60000, -1]);
        assert_eq!(u.upgrades.magnet_tier, 6);
        assert_eq!(u.coins, 200_000 - 105_000);
        assert!(!purchase(&mut u, &Item::Upgrade("magnet")), "maxed");
        assert_eq!(crate::powerups::timed_duration(u.upgrades.magnet_tier), 40.0);
    }

    #[test]
    fn locked_outfits_and_wallet() {
        let mut u = UserData { coins: 2, keys: 5, ..Default::default() };
        assert!(locked(&u, &Item::Outfit("tricky".into(), 1)));
        assert!(!purchase(&mut u, &Item::Character("tricky".into())), "3 coins needed");
        u.coins = 3;
        assert!(purchase(&mut u, &Item::Character("tricky".into())));
        assert!(!locked(&u, &Item::Outfit("tricky".into(), 1)));
        assert!(purchase(&mut u, &Item::Outfit("tricky".into(), 1)), "2 keys");
        assert_eq!((u.coins, u.keys), (0, 3));
    }
}
