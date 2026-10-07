//! Mystery box prizes: the roll tables `Yi` / `Xi` / `Zi` (10112-10444,
//! `data/prize_tables.json`, verbatim), `aa(kind)` and the credit
//! `openMysteryBox` (72586). See docs/js_notes/boosts_mysterybox.md §2-§4.

use crate::game::Game;
use crate::rng::Site;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum Amount {
    Fixed(i64),
    Range([i64; 2]),
}

#[derive(Clone, Debug, Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: String,
    amount: Amount,
    chance: f64,
}

#[derive(Debug, Deserialize)]
struct Tables {
    mini: Vec<Entry>,
    normal: Vec<Entry>,
    #[serde(rename = "super")]
    sup: Vec<Entry>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("../data/prize_tables.json")).expect("prize_tables.json"))
}

/// A rolled prize (`{type, amount, boxType}`).
#[derive(Clone, Debug, PartialEq)]
pub struct Prize {
    /// coins, keys, hoverboard, headstart, scoreBooster
    pub kind: String,
    pub amount: i64,
    /// "mystery-box", "super-mystery-box", "mini-mystery-box" (None: plain prize)
    pub box_type: Option<String>,
}

impl Prize {
    /// `amount + " " + Ry[type]` (+ "s" when amount > 1).
    pub fn label(&self) -> String {
        let name = match self.kind.as_str() {
            "coins" => "Coin",
            "keys" => "Key",
            "hoverboard" => "Hoverboard",
            "headstart" => "Headstart",
            "scoreBooster" => "Score Booster",
            other => other,
        };
        format!("{} {}{}", self.amount, name, if self.amount > 1 { "s" } else { "" })
    }
}

/// RNG call sites: `ta` / `yt` under `ra` (normal), `na` (mini), `ia` (super).
pub mod sites {
    use super::Site;
    pub const TA_RA: Site = Site("at ta (assets/index-QNpTjs8S.js:1:300105) < at ra (assets/index-QNpTjs8S.js:1:300593)");
    pub const YT_RA: Site = Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at ra (assets/index-QNpTjs8S.js:1:300679)");
    pub const TA_IA: Site = Site("at ta (assets/index-QNpTjs8S.js:1:300105) < at ia (assets/index-QNpTjs8S.js:1:300834)");
    pub const YT_IA: Site = Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at ia (assets/index-QNpTjs8S.js:1:300920)");
    pub const TA_NA: Site = Site("at ta (assets/index-QNpTjs8S.js:1:300105) < at na (assets/index-QNpTjs8S.js:1:300352)");
    pub const YT_NA: Site = Site("at yt (assets/index-QNpTjs8S.js:1:41124) < at na (assets/index-QNpTjs8S.js:1:300438)");
}

/// `ta(table)` weighted pick, then the amount (`yt(a, b + 1, true)` for ranges).
fn roll_table(g: &mut Game, table: &[Entry], pick: Site, range: Site, box_type: &str) -> Prize {
    let total: f64 = table.iter().map(|e| e.chance).sum();
    let mut r = g.rng.random(pick) * total;
    let mut chosen = &table[table.len() - 1];
    for e in table {
        r -= e.chance;
        if r < 0.0 {
            chosen = e;
            break;
        }
    }
    let amount = match chosen.amount {
        Amount::Fixed(a) => a,
        Amount::Range([a, b]) => (a as f64 + g.rng.random(range) * ((b + 1 - a) as f64)).floor() as i64,
    };
    Prize { kind: chosen.kind.clone(), amount, box_type: Some(box_type.to_string()) }
}

/// `aa(kind)` (10432): always one normal roll first, then mini / super replace it.
pub fn roll(g: &mut Game, kind: &str) -> Prize {
    let t = tables();
    let p = roll_table(g, &t.normal, sites::TA_RA, sites::YT_RA, "mystery-box");
    let p = match kind {
        "mini-mystery-box" => roll_table(g, &t.mini, sites::TA_NA, sites::YT_NA, "mini-mystery-box"),
        "super-mystery-box" => roll_table(g, &t.sup, sites::TA_IA, sites::YT_IA, "super-mystery-box"),
        _ => p,
    };
    if p.kind == "coins" {
        crate::missions::add_stat(g, p.amount, "mission-coins-mystery");
    }
    p
}

/// `rx.openMysteryBox` (72586): credit and save.
pub fn credit(g: &mut Game, p: &Prize) {
    let u = &mut g.flow.user;
    match p.kind.as_str() {
        "coins" => u.coins += p.amount,
        "keys" => u.keys += p.amount,
        "hoverboard" => u.hoverboards += p.amount,
        "headstart" => u.headstarts += p.amount,
        "scoreBooster" => u.score_boosters += p.amount,
        _ => {}
    }
    g.flow.save_user();
}

#[cfg(test)]
mod tests {
    /// The oracle's first draws on these keys (oracle `__oracle.logRng`, seed 1,
    /// `nav.toPrizeScreen()`): equal values = same per-site stream = same key.
    #[test]
    fn roll_sites_match_oracle_streams() {
        use crate::rng::{OracleRng, Rng};
        let mut r = OracleRng::new(1);
        assert_eq!(r.random(super::sites::TA_RA), 0.05340254586189985);
    }

    #[test]
    fn tables_load() {
        let t = super::tables();
        assert_eq!((t.mini.len(), t.normal.len(), t.sup.len()), (13, 17, 22));
    }
}
