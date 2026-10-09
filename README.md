# Subway Surfers Rust / Bevy port

Everything is in this folder:

| Folder | Contents |
|---|---|
| `ss_port/` | the game's source code ([`ss_port/README.md`](ss_port/README.md)) |
| `oracle/site/` | the original web build's files (models, textures, sounds, levels) that the game reads |
| `oracle/traces/`, `oracle/dumps/` | recordings of the original game, used by the tests |

## Play

You need Rust (https://rustup.rs). Then double-click **Play.command**, or:

```bash
cd ss_port
cargo run --release
```

The first build compiles the engine and takes a few minutes; after that it
starts in seconds. Controls: arrows / WASD, Space (hoverboard), Esc (pause),
R (restart).

## Test

```bash
cd ss_port
cargo test --release
```

**Don't delete or move the `oracle` folder.** It holds the original game's
files (`oracle/site`), which the game reads at startup next to `ss_port`.
