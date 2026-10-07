//! Every Math.random draw logged by the oracle (`--log-rng`) must be
//! reproduced by OracleRng: same site key, same nth draw, same value.

use ss_port::rng::{Rng, Site, OracleRng};
use std::collections::HashMap;

#[test]
fn oracle_rng_matches_logged_draws() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../oracle/traces/seed1-god-rng/trace.jsonl");
    let text = std::fs::read_to_string(path).expect("record it: node oracle/record.mjs --seed 1 --god --log-rng --out oracle/traces/seed1-god-rng");
    let mut rng = OracleRng::new(1);
    let mut seen: HashMap<String, u64> = HashMap::new();
    let mut checked = 0;
    for line in text.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["t"] != "rng" {
            continue;
        }
        let site = v["site"].as_str().unwrap().to_string();
        let n = v["n"].as_u64().unwrap();
        // Draws before recording started (menu/boot) aren't logged: skip ahead.
        let have = seen.entry(site.clone()).or_insert(0);
        let leaked: &'static str = Box::leak(site.into_boxed_str());
        while *have + 1 < n {
            rng.random(Site(leaked));
            *have += 1;
        }
        let got = rng.random(Site(leaked));
        *have += 1;
        assert_eq!(got, v["v"].as_f64().unwrap(), "site {leaked} draw {n}");
        checked += 1;
    }
    assert!(checked > 1000, "only {checked} draws checked");
}
