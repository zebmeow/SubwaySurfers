//! Static game data: route-chunk scene trees (chunks-*.json) and route
//! section tables. Port of `Kt` (deobfuscated.js:1979-2174) and the section
//! tables `Vt` (1651-1978).

use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

pub type Node = Rc<Value>;

/// Node helpers (chunk JSON: `{name, components:{...}, children:[...]}`).
pub fn name(n: &Value) -> &str {
    n["name"].as_str().unwrap_or("")
}
pub fn comp<'a>(n: &'a Value, c: &str) -> Option<&'a Value> {
    n.get("components").and_then(|cs| cs.get(c)).filter(|v| !v.is_null())
}
pub fn children(n: &Value) -> &[Value] {
    n.get("children").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}
pub fn has_children(n: &Value) -> bool {
    n.get("children").is_some_and(|c| !c.is_null())
}
/// `components.Transform.position.{x,y,z}` (absent -> 0).
pub fn pos(n: &Value) -> (f64, f64, f64) {
    let p = &n["components"]["Transform"]["position"];
    (p["x"].as_f64().unwrap_or(0.0), p["y"].as_f64().unwrap_or(0.0), p["z"].as_f64().unwrap_or(0.0))
}
/// JS truthiness of a JSON value.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// `Kt.chunkMap` + `Kt.chunk`.
pub struct ChunkData {
    map: HashMap<String, Node>,
}

impl ChunkData {
    /// Loads the three bundles in cache order (idle, basic, full).
    pub fn load(site_root: &std::path::Path) -> Result<Self, String> {
        let mut map = HashMap::new();
        for f in ["chunks-idle.json", "chunks-basic.json", "chunks-full.json"] {
            let p = site_root.join("assets/data").join(f);
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let json: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            // chunkMap walks {cacheKey: json} with a depth budget of 5.
            let wrapper = serde_json::json!({ f: json });
            walk(&wrapper, 5, &mut map);
        }
        Ok(Self { map })
    }

    /// `Kt.chunk(name)`: returns (node, `__name` = the stripped request name).
    pub fn chunk(&self, name: &str) -> Option<(Node, String)> {
        let n = name.replacen("routeChunk_", "", 1).replacen("route_chunk_", "", 1);
        let found = self
            .map
            .get(&n)
            .or_else(|| self.map.get(&format!("routeChunk_{n}")))
            .or_else(|| self.map.get(&format!("route_chunk_{n}")))
            .or_else(|| self.map.get(&format!("default_{n}")))?;
        Some((found.clone(), n))
    }
}

fn walk(v: &Value, depth: i32, map: &mut HashMap<String, Node>) {
    if depth <= 0 || !v.is_object() && !v.is_array() {
        return;
    }
    let depth = depth - 1;
    if v.get("name").and_then(Value::as_str) == Some("intro") {
        map.insert("intro".into(), Rc::new(v.clone()));
    } else if let Some(rc) = comp(v, "RouteChunk").filter(|_| v.get("components").is_some()) {
        let key = rc["_reportedName"].as_str().filter(|s| !s.is_empty()).unwrap_or_else(|| name(v)).to_string();
        map.insert(key, Rc::new(v.clone()));
    } else if let Some(ch) = v.get("children").filter(|c| !c.is_null()) {
        match ch {
            Value::Array(a) => a.iter().for_each(|c| walk(c, depth, map)),
            Value::Object(o) => o.values().for_each(|c| walk(c, depth, map)),
            _ => {}
        }
    } else {
        match v {
            Value::Array(a) => a.iter().for_each(|c| walk(c, depth, map)),
            Value::Object(o) => o.values().for_each(|c| walk(c, depth, map)),
            _ => {}
        }
    }
}

// ---- route sections (Vt) ----------------------------------------------------

#[derive(Debug)]
pub struct Section {
    pub name: &'static str,
    pub start: &'static [&'static str],
    pub mid: &'static [&'static str],
    pub end: &'static [&'static str],
}

macro_rules! sec {
    ($name:literal, [$($s:literal),*], [$($m:literal),*], [$($e:literal),*]) => {
        Section { name: $name, start: &[$($s),*], mid: &[$($m),*], end: &[$($e),*] }
    };
}

/// Level tables in key (= iteration) order: easy `Mt`, normal `Pt`, hard, expert `Nt`.
pub static LEVEL_TABLES: [&[Section]; 4] = [
    &[
        sec!("route_section_default_train_tops_2", [], ["routeChunk_default_train_tops_2"], []),
        sec!("route_section_default_s-s", [], ["routeChunk_default_s-s"], []),
        sec!("route_section_default_b-s-b", [], ["routeChunk_default_b-s-b"], []),
        sec!("route_section_default_s-b-s-b", [], ["routeChunk_default_s-b-s-b"], []),
        sec!("route_section_default_s-s-s-s", [], ["routeChunk_default_s-s-s-s"], []),
        sec!("route_section_default_tunnel_notrain", [], ["routeChunk_default_tunnel_notrain"], []),
        sec!("route_section_default_choice", [], ["routeChunk_default_choice"], []),
        sec!("route_section_default_train_tops_1", [], ["routeChunk_default_train_tops_1"], []),
    ],
    &[
        sec!("route_section_default_train_tops_moving", [], ["routeChunk_default_train_tops_moving"], []),
        sec!("route_section_default_epic", [], ["routeChunk_default_epic"], []),
        sec!("route_section_default_train_tops_moving_multiple", [], ["routeChunk_default_train_tops_moving_multiple"], []),
        sec!(
            "route_section_default_2_tracks",
            ["routeChunk_default_2_tracks_start"],
            ["routeChunk_default_2_tracks_mid_var_1", "routeChunk_default_2_tracks_mid_var_2"],
            ["routeChunk_default_2_tracks_end"]
        ),
        sec!("route_section_default_ramp_1", [], ["routeChunk_default_ramp_1"], []),
        sec!("route_section_default_tunnel", [], ["routeChunk_default_tunnel"], []),
        sec!("route_section_default_train_tops_moving_combined", [], ["routeChunk_default_train_tops_moving_combined"], []),
        sec!("route_section_default_1_track", [], ["routeChunk_short_1_track"], []),
    ],
    &[
        sec!("route_section_default_4_units_3_tracks_choice", [], ["routeChunk_4_units_3_tracks_choice"], []),
        sec!("route_section_default_4_units_3_tracks_s-b-s-b", [], ["routeChunk_4_units_3_tracks_s-b-s-b"], []),
        sec!("route_section_default_4_units_3_tracks_b-s-b", [], ["routeChunk_4_units_3_tracks_b-s-b"], []),
        sec!("route_section_default_epic_various", [], ["routeChunk_epic_various"], []),
        sec!("route_section_default_4_units_3_tracks_s-s", [], ["routeChunk_4_units_3_tracks_s-s"], []),
        sec!("route_section_default_4_units_3_tracks_s-s-s-s", [], ["routeChunk_4_units_3_tracks_s-s-s-s"], []),
    ],
    &[
        sec!("route_section_default_short_train_tops_moving_multiple", [], ["routeChunk_short_train_tops_moving_multiple"], []),
        sec!(
            "route_section_default_short_2_tracks",
            ["routeChunk_short_2_tracks_start"],
            ["routeChunk_short_2_tracks_mid_var_1", "routeChunk_short_2_tracks_mid_var_2"],
            ["routeChunk_short_2_tracks_end"]
        ),
        sec!("route_section_default_short_train_tops_moving", [], ["routeChunk_short_train_tops_moving"], []),
        sec!("route_section_default_ramp_2", [], ["routeChunk_ramp_2"], []),
        sec!("route_section_default_short_train_tops_moving_combined", [], ["routeChunk_short_train_tops_moving_combined"], []),
        sec!("route_section_default_short_1_track", [], ["routeChunk_short_1_track"], []),
    ],
];

/// `Vt.main`, `Vt.special`, `Vt.custom.basic_game` (only reachable by name).
pub static OTHER_SECTIONS: &[Section] = &[
    sec!("routeSection_default_start", [], ["routeChunk_default_pogostick_start"], []),
    sec!("routeSection_default_fallback", [], ["routeChunk_default_fallback"], []),
    sec!("routeSection_default_start_short", [], ["routeChunk_default_fallback"], []),
    sec!("routeSection_default_jetpack_landing", [], ["routeChunk_default_jetpack_landing"], []),
    sec!(
        "routeSection_default_no_gameplay",
        [],
        ["routeChunk_default_no_gameplay_1", "routeChunk_default_no_gameplay_2", "routeChunk_default_no_gameplay_epic"],
        []
    ),
    sec!("routeSection_tutorial", [], ["routeChunk_tutorial"], []),
    sec!("routeSection_default_bonus_short", [], ["routeChunk_bonus_short"], []),
    sec!("routeSection_default_bonus_long", [], ["routeChunk_bonus_long"], []),
    sec!("routeSection_default_pogostick_start", [], ["routeChunk_default_pogostick_start"], []),
    sec!("basic_game", [], ["routeChunk_basic_b-s-b"], []),
];

/// `Tg.sectionsMid` keys (only membership matters: selection is uniform).
pub static SECTIONS_MID: &[&str] = &[
    "default_b-s-b", "default_choice", "default_s-b-s-b", "default_s-s", "default_s-s-s-s",
    "default_train_tops_1", "default_train_tops_2", "default_tunnel_notrain", "default_ramp_1", "default_epic",
    "default_1_track", "default_2_tracks", "default_train_tops_moving", "default_train_tops_moving_combined",
    "default_train_tops_moving_multiple", "default_tunnel", "default_4_units_3_tracks_b-s-b",
    "default_4_units_3_tracks_choice", "default_4_units_3_tracks_s-b-s-b", "default_4_units_3_tracks_s-s",
    "default_4_units_3_tracks_s-s-s-s", "default_short_1_track", "default_ramp_2", "default_short_2_tracks",
    "default_short_train_tops_moving_combined", "default_short_train_tops_moving_multiple",
    "default_short_train_tops_moving", "default_pogostick_start", "default_bonus_short", "default_bonus_long",
];

/// `Kt.section(name)`.
pub fn section(name: &str) -> Option<&'static Section> {
    let n = name.replacen("routeSection_", "", 1).replacen("route_section_", "", 1);
    let all = || LEVEL_TABLES.iter().flat_map(|t| t.iter()).chain(OTHER_SECTIONS.iter());
    let by = |k: String| all().find(|s| s.name == k);
    by(n.clone()).or_else(|| by(format!("route_section_{n}"))).or_else(|| by(format!("routeSection_{n}")))
}

/// Section short name (`__shortname`).
pub fn short_name(s: &Section) -> String {
    s.name.replacen("routeSection_", "", 1).replacen("route_section_", "", 1)
}
