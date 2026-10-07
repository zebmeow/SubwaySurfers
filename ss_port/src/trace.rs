//! Reader for oracle traces (`oracle/traces/<name>/trace.jsonl`, format in
//! oracle/README.md). Used by parity tests and by trace-driven runs, where
//! inputs the port doesn't simulate yet (hero distance, pickups) come from the
//! trace.

use serde_json::Value;
use std::collections::BTreeMap;

pub struct Trace {
    pub records: Vec<Value>,
    pub meta: Value,
    /// frame -> `player` record
    pub players: BTreeMap<i64, Value>,
}

impl Trace {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let records: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        let meta = records.iter().find(|r| r["t"] == "meta").cloned().ok_or("no meta record")?;
        let players = records
            .iter()
            .filter(|r| r["t"] == "player")
            .map(|r| (r["f"].as_i64().unwrap(), r.clone()))
            .collect();
        Ok(Self { records, meta, players })
    }

    pub fn of_type<'a>(&'a self, t: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
        self.records.iter().filter(move |r| r["t"] == t)
    }

    pub fn events<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
        self.of_type("event").filter(move |r| r["kind"] == kind)
    }

    /// First and last frame with a `player` record.
    pub fn frame_range(&self) -> (i64, i64) {
        (*self.players.keys().next().unwrap(), *self.players.keys().last().unwrap())
    }

    pub fn config(&self) -> &Value {
        &self.meta["config"]
    }
}
