//! Subway Surfers (Poki web build) ported 1:1 from the decompiled JS.
//!
//! The simulation core is plain Rust that mirrors the JS classes (line
//! references to `decompiled/js_src/deobfuscated.js` throughout); the Bevy
//! layer only renders it. Parity is checked against oracle traces
//! (`oracle/traces/*/trace.jsonl`).

pub mod anim;
pub mod audio;
pub mod awards;
pub mod boosts;
pub mod celebration;
pub mod char_idle;
pub mod camera;
pub mod data;
pub mod describe;
pub mod entities;
pub mod environment;
pub mod flow;
pub mod fx;
pub mod game;
pub mod guard;
pub mod hero;
pub mod hero_fx;
pub mod install;
pub mod math3;
pub mod menu;
pub mod missions;
pub mod mount;
pub mod particles;
pub mod physics;
pub mod powerups;
pub mod prizes;
pub mod rng;
pub mod scene;
pub mod skin;
pub mod shop;
pub mod sim_plugin;
pub mod theme;
pub mod library;
pub mod pk;
pub mod render;
pub mod render_actors;
pub mod render_fx;
pub mod render_menu_3d;
pub mod trace;
pub mod tutorial;
pub mod ui;
pub mod ui_missions;
pub mod word_hunt;
pub(crate) mod ui_menu;
