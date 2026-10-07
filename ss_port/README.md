# Subway Surfers — 1:1 Rust / Bevy Source Port

A frame-accurate reimplementation of the **Subway Surfers web game** (the
Poki WebGL build) in Rust, using the [Bevy](https://bevyengine.org) engine.
The game's logic was reverse-engineered from the shipped JavaScript bundle
and rebuilt module by module. Every gameplay system is checked against
**oracle traces**: recordings of the original game running deterministically
in a browser, frame by frame.

The original game's models, textures, sounds, music and level chunks are
not part of this repository. At run time the port reads them from a local
copy of the official web build (see [Building](#building)).

---

## Highlights

- **Deterministic, trace-verified simulation.** The game runs at a fixed
  60 Hz with the original's per-frame physics sub-steps. Seeded per-call-site
  random streams reproduce the original's `Math.random()` exactly. Hero
  movement, obstacles, power-ups, scenery and restarts match the oracle
  recordings frame for frame (`tests/trace_*_parity.rs`).
- **The Bali route, procedurally generated as in the original.** It uses
  the route and section system (`Eg` / `Tg`) and the chunk mounting with
  its randomizers, trains, barriers, tunnels, stations and environment
  pieces. The same seed always gives the same layout.
- **The original's look.**
  - A custom WGSL material reproduces the game's unlit shading, fog, water
    and foam.
  - The world bends in clip space as in the original vertex shader:
    `pos.xy += w² · uBend`.
  - Characters are skinned on the CPU from the original `.pk` models with
    their own clips, facial animation and powerup props.
- **The full game loop.**
  - All five power-ups: Jetpack, Super Sneakers, Coin Magnet, 2x Multiplier
    and Pogo Stick. Hoverboards with crash shield, mystery boxes, headstart
    and score booster.
  - The Inspector and his dog, stumbles, crashes, Save me! with keys,
    New High Score and results.
- **Progression.**
  - 87 missions in 29 sets, raising the score multiplier from x1 to x30.
  - The daily Word Hunt.
  - 12 awards with four tiers each.
  - The first-run tutorial route with checkpoint rewinds.
- **Menus.**
  - The title screen; the Me panel with the original nine characters (Jake,
    Tricky, Yutani, Lucy, Tagbot, Ninja, Tasha, King, Brody), their outfits
    and the 16 boards, previewed live in 3D.
  - The Shop (mystery boxes, boosts, power-up upgrades), My Tour (Word Hunt,
    missions), Settings and the pause panel.
- **Audio.** The Bali theme and the 27 original sound effects, triggered
  where the original triggers them. Coin pitch rises along jump arcs.
- **Persistence.** The original `GameSettings` save (coins, keys, high
  score, characters, missions, awards, settings), written atomically to the
  user's data folder.

## Building

Requirements: a recent stable Rust toolchain, and a GPU with Metal, Vulkan
or DirectX 12.

The port expects this layout:

```
work/
├── ss_port/            this repository
├── oracle/site/        a local copy of the official web build (assets, bundles, index.html)
└── oracle/traces/      oracle recordings (needed by the parity tests only)
```

Build and run:

```bash
cd ss_port
cargo run --release
```

For a Windows build from macOS or Linux, use
[cargo-xwin](https://github.com/rust-cross/cargo-xwin):

```bash
cargo xwin build --release --target x86_64-pc-windows-msvc
```

The result is a GUI-subsystem executable with the game icon. It reads
`assets/`, `data/` and `site/` from the folder next to it.

## Controls

| Key | Action |
|---|---|
| Left / Right (A / D) | change lane |
| Up (W) | jump |
| Down (S) | roll / fast drop |
| Space | hoverboard; continue on the game-over screens |
| V / C | headstart / score booster (at the start of a run, if owned) |
| K | revive with keys on "Save me!" |
| Esc | pause / resume |
| R | restart the run |

The menus take mouse clicks, the mouse wheel, the arrow keys and Space.

### Command-line options

| Option | Effect |
|---|---|
| `--seed N` | level seed (default 1) |
| `--lock-seed` | every run starts from a fresh boot of the seed, so it has the same layout |
| `--god` | invincible |
| `--character ID[:OUTFIT]` | draw the hero as a roster character, e.g. `tricky:1` |
| `--pause-on-blur` | pause and mute when the window loses focus (as the original does) |
| `--aa off\|msaa\|fxaa\|both` | anti-aliasing (default MSAA) |
| `--debug-hud` | developer status line at the bottom (FPS, frame, state, speed) |
| `--vsync` / `--no-smooth` | present with VSync / no interpolation between simulation frames |
| `--replay [TRACE]` | replay the key presses of an oracle trace |
| `--replay --screenshot OUT.png --at F` | render frame F of a replay to a PNG |
| `--hd --dump-frames DIR --from F --at T` | render frames F to T off screen at 1920 × 1080 |

`tools/showcase.py` turns frame dumps into a captioned 1080p60 video with
the theme music.

## Architecture

The simulation is a library (`src/lib.rs`) with no dependency on rendering.
The Bevy app (`src/main.rs`) steps it at 60 Hz and draws it, interpolating
between simulation frames.

| Area | Modules |
|---|---|
| Core loop, level and route systems, chunk mounting | `game.rs`, `mount.rs`, `entities.rs`, `environment.rs`, `data.rs`, `library.rs` |
| Hero, physics, camera, pursuers | `hero.rs`, `physics.rs`, `camera.rs`, `guard.rs`, `anim.rs`, `skin.rs` |
| Power-ups, boosts, prizes | `powerups.rs`, `boosts.rs`, `prizes.rs` |
| Flow, menus, shop, progression | `flow.rs`, `menu.rs`, `shop.rs`, `missions.rs`, `awards.rs`, `word_hunt.rs`, `tutorial.rs`, `celebration.rs` |
| Particles and effects | `particles.rs`, `fx.rs`, `hero_fx.rs` |
| Randomness | `rng.rs`: per-call-site streams keyed like the oracle's |
| Rendering | `render.rs`, `render_actors.rs`, `render_fx.rs`, `render_menu_3d.rs`, `shaders/` |
| 2D UI | `ui.rs`, `ui_menu.rs`, `ui_missions.rs` (pixi-equivalent layout from the original atlas) |
| Audio | `audio.rs`: the simulation queues commands, the app mixes them |
| Oracle and traces | `trace.rs`, `sim_plugin.rs`: drivers for live keys and recorded traces |

Source comments cite the matching functions of the original bundle (the
minified class names and line numbers in the deobfuscated source).
[`docs/PORTING_NOTES.md`](docs/PORTING_NOTES.md) records each system in
detail, together with every known difference from the original.

## Verification

```bash
cargo test --release
```

The 25 test suites include:

- **Trace parity:** hero, obstacles, power-ups, scenery, restarts, effects
  and RNG streams, against the oracle recordings.
- **Game logic:** jumps, death flow, missions, awards, the Word Hunt, the
  tutorial, outfits, menus and the shop, audio triggers, and seed
  determinism.

The simulation is deterministic: the same seed and the same key presses
always give the same run.

## Legal

Subway Surfers, its characters, artwork, models, sounds, music and level
data are © **SYBO Games** and **Kiloo**. The web version is published by
**Poki**. This is an unofficial, non-commercial fan project made for
educational and preservation purposes. It is not affiliated with or endorsed
by SYBO, Kiloo or Poki.

The game's models, textures, sounds, music and level chunks are not
distributed here: they are read from a copy of the official web build that
you provide. The repository does contain material derived from the original:

- small data tables extracted from its bundle (`data/`: missions, shop
  catalog, awards, theme and effect settings);
- board thumbnails rendered from its models (`assets/ui/thumbs/`);
- the Poki logo used as the app icon (`assets/icon/`).

If you are a rights holder and have concerns, please open an issue.
