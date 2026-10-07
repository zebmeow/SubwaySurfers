//! The original shop catalogs (`data/shop_catalog.json`, extracted verbatim
//! from deobfuscated.js: boards `m_` 54399, boosts `g_` 54826, characters
//! `__` 54928, the Me-panel roster `v_` 55959) and the purchase rules of
//! `T_` (56664). See docs/js_notes/ui_powerups_shop.md §3-§5.

use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Clone, Debug, Deserialize)]
pub struct BoardPower {
    pub id: String,
    #[serde(default)]
    pub cost: i64,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Board {
    pub id: String,
    pub root: String,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub cost: i64,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub powerups: Vec<BoardPower>,
    /// Built-in power (daredevil, teleporter, ...).
    #[serde(default)]
    pub powerup: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Outfit {
    pub id: String,
    #[serde(default)]
    pub cost: i64,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub texture: String,
    #[serde(default)]
    pub colors: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Character {
    pub id: String,
    #[serde(default)]
    pub texture: String,
    #[serde(default)]
    pub cost: i64,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub outfits: Vec<Outfit>,
    #[serde(default)]
    pub colors: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoostLevel {
    pub cost: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Boost {
    pub id: String,
    #[serde(default)]
    pub levels: Vec<BoostLevel>,
    #[serde(default)]
    pub currency: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Catalog {
    pub boards: Vec<Board>,
    pub boosts: Vec<Boost>,
    pub characters: Vec<Character>,
    pub roster: Vec<String>,
}

impl Catalog {
    pub fn get() -> &'static Catalog {
        static C: OnceLock<Catalog> = OnceLock::new();
        C.get_or_init(|| serde_json::from_str(include_str!("../data/shop_catalog.json")).expect("shop_catalog.json"))
    }
    pub fn board(&self, id: &str) -> Option<&Board> {
        self.boards.iter().find(|b| b.id == id)
    }
    pub fn character(&self, id: &str) -> Option<&Character> {
        self.characters.iter().find(|c| c.id == id)
    }
    /// The Me-panel characters (`v_` mapped through `__`).
    pub fn roster(&self) -> Vec<&Character> {
        self.roster.iter().filter_map(|id| self.character(id)).collect()
    }
}

/// `setBoard` (32625): the active powers of board `id` with the selected
/// `boardPowers` (1-based indices into its `powerups`, 0 = none).
pub fn board_powers(id: &str, board_powers: &[usize]) -> Vec<String> {
    let c = Catalog::get();
    let Some(b) = c.board(id).or_else(|| c.board("hoverboard")) else { return Vec::new() };
    if let Some(p) = &b.powerup {
        return vec![p.clone()];
    }
    board_powers.iter().filter(|&&p| p != 0).filter_map(|&p| b.powerups.get(p - 1)).map(|p| p.id.clone()).collect()
}

/// `of` (32479): the board animation set.
#[derive(Clone, Copy, Debug)]
pub struct BoardAnims {
    pub run: &'static str,
    pub jump: &'static [&'static str],
    pub resume: &'static str,
    pub grind: &'static [&'static str],
    pub grind_land: &'static [&'static str],
    pub roll: &'static str,
    pub dodge_right: &'static str,
    pub dodge_left: &'static str,
    pub start: &'static str,
}

const H_JUMPS: &[&str] = &[
    "h_jump2_kickflip_flip",
    "h_jump3_bs360grab",
    "h_jump4_360_flip",
    "h_jump5_Impossible_flip",
    "h_jump6_nollie",
    "h_jump7_heelflip_flip",
    "h_jump8_pop_shuvit_flip",
    "h_jump9_fs360grab",
    "h_jump10_heel360_flip",
    "h_jump11_fs_salto",
    "h_jump",
];

pub const ANIMS_HOVERBOARD: BoardAnims = BoardAnims {
    run: "h_run",
    jump: H_JUMPS,
    resume: "h_jump9_fs360grab",
    grind: &["h_Grind1", "h_Grind2", "h_Grind3"],
    grind_land: &["h_Grind1_land", "h_Grind2_land", "h_Grind3_land"],
    roll: "h_roll",
    dodge_right: "h_right",
    dodge_left: "h_left",
    start: "h_skate_on",
};

pub const ANIMS_STAY_LOW: BoardAnims = BoardAnims {
    run: "Lowrider_Run",
    jump: &["Lowrider_Jump1", "Lowrider_Jump_3_Starfish_Frontflip", "Lowrider_Jump_2_BarrelRoll", "Lowrider_Jump_4_Handstand"],
    resume: "Lowrider_Jump_4_Handstand",
    grind: &["Lowrider_Grind1", "Lowrider_Grind2"],
    grind_land: &["Lowrider_Grind1_land", "Lowrider_Fat_Grind2_land"],
    roll: "Lowrider_Down",
    dodge_right: "Lowrider_Changelane_Right1",
    dodge_left: "Lowrider_Changelane_Left1",
    start: "Lowrider_Board_on",
};

pub const ANIMS_ZAP: BoardAnims = BoardAnims { dodge_right: "", dodge_left: "", ..ANIMS_HOVERBOARD };

/// `of[id] || of[stay-low | zap-sideways power] || of.hoverboard`.
pub fn board_anims(id: &str, powers: &[String]) -> BoardAnims {
    if id == "hoverboard" {
        return ANIMS_HOVERBOARD;
    }
    if powers.iter().any(|p| p == "stay-low") {
        ANIMS_STAY_LOW
    } else if powers.iter().any(|p| p == "zap-sideways") {
        ANIMS_ZAP
    } else {
        ANIMS_HOVERBOARD
    }
}
