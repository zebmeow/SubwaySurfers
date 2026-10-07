//! Random numbers.
//!
//! The game draws everything from `Math.random`. For trace parity the oracle
//! (oracle/hooks.js) gives every JS call site its own mulberry32 stream,
//! seeded with `fnv1a(site, fnv1a(seed.to_string()))`, where `site` is the two
//! innermost stack frames, e.g.
//! `at R.item (assets/index-QNpTjs8S.js:1:56829) < at Eg.getSequence (assets/index-QNpTjs8S.js:1:1186553)`.
//!
//! Ported code calls [`Rng::random`] with the [`Site`] of the JS statement it
//! transliterates. [`OracleRng`] reproduces the oracle's streams exactly;
//! [`SingleStreamRng`] ignores the site (one stream, like the shipped game).

use std::collections::HashMap;

/// A JS `Math.random()` call site. The key is the oracle stream key
/// (callee frame < caller frame). Keys are only meaningful for the exact
/// bundle `index-QNpTjs8S.js`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Site(pub &'static str);

pub trait Rng {
    /// `Math.random()` at `site`: uniform in [0, 1).
    fn random(&mut self, site: Site) -> f64;
}

/// mulberry32, bit-exact with oracle/hooks.js.
#[derive(Clone, Debug)]
pub struct Mulberry32(u32);

impl Mulberry32 {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
    }
}

/// FNV-1a over UTF-16 code units (JS `charCodeAt`), as in oracle/hooks.js.
pub fn fnv1a(s: &str, mut h: u32) -> u32 {
    for c in s.encode_utf16() {
        h = (h ^ c as u32).wrapping_mul(0x0100_0193);
    }
    h
}

pub const FNV_OFFSET: u32 = 0x811c_9dc5;

/// Per-call-site streams, identical to the oracle's.
#[derive(Clone, Debug)]
pub struct OracleRng {
    seed_hash: u32,
    streams: HashMap<&'static str, (Mulberry32, u32)>,
}

impl OracleRng {
    pub fn new(seed: i32) -> Self {
        Self { seed_hash: fnv1a(&seed.to_string(), FNV_OFFSET), streams: HashMap::new() }
    }
    /// Number of draws made at each site so far.
    pub fn counts(&self) -> impl Iterator<Item = (&'static str, u32)> + '_ {
        self.streams.iter().map(|(k, (_, n))| (*k, *n))
    }
}

impl Rng for OracleRng {
    fn random(&mut self, site: Site) -> f64 {
        let seed_hash = self.seed_hash;
        let (stream, n) = self
            .streams
            .entry(site.0)
            .or_insert_with(|| (Mulberry32::new(fnv1a(site.0, seed_hash)), 0));
        *n += 1;
        stream.next_f64()
    }
}

/// One stream for all sites (what the shipped game effectively has).
#[derive(Clone, Debug)]
pub struct SingleStreamRng(Mulberry32);

impl SingleStreamRng {
    pub fn new(seed: u32) -> Self {
        Self(Mulberry32::new(seed))
    }
}

impl Rng for SingleStreamRng {
    fn random(&mut self, _site: Site) -> f64 {
        self.0.next_f64()
    }
}
