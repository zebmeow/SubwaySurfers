# Porting notes — trace-driven 1:1 port (Rust + Bevy)

A clean port of the Subway Surfers web build, written as a transliteration of
the decompiled JS (line references to `decompiled/js_src/deobfuscated.js`
throughout) and verified against traces of the original game recorded by
`../oracle`. Reference notes the code follows: `../docs/js_notes/*.md`.

```bash
cargo test                                   # all parity tests
cargo test --test trace_hero_parity -- --nocapture      # hero, with a summary
cargo test --test trace_scenery_parity -- --nocapture   # scenery, with a summary
cargo run --release                          # play: title screen, ME / SHOP, Space starts (arrows/WASD; Space = hoverboard; R restarts)
cargo run --release -- --seed 7              # another seed (play is mortal; --god / --invincible for test runs)
cargo run --release -- --replay              # replay the seed 1 trace's keys
cargo run --release -- --theme bali          # city theme (data/theme_<id>.json; T in game)
cargo run --release -- --character tricky    # draw the hero as any roster id (`id:outfit`; default: the saved selection)
cargo test --test trace_powerups_parity -- --nocapture  # magnet, 2x, sneakers, jetpack, hoverboard + shield vs the oracle
cargo run --release -- --menu shop --screenshot shop.png --at 60   # a menu screen (title|me|me:<id>|me-boards|shop|boosts|buyboards)
cargo run --release -- --replay --screenshot out.png --at 1200   # one frame, then exit
cargo run --release -- --aa fxaa --vsync     # anti-aliasing off|msaa|fxaa|both (default msaa); cap at the display rate
node ../oracle/screenshot.mjs ../oracle/traces/seed1-god/trace.jsonl 1200 orig.png
cargo run --release --example inspect -- 1200 e         # list entities of a class
```

## Status

The whole run is simulated; the only input is the keyboard. The parity
tests replay the oracle autopilot's keydowns at their recorded frames and
take nothing else from the trace.

### Step 2: hero simulation

`tests/trace_hero_parity.rs` runs the same headless app for 3,600 frames
(f6..f3605) and compares the hero every frame with the trace's `player`
record:

| Checked every frame | Result (worst deviation) |
|---|---|
| hero position x, y, z | 3600 / 3600, 5.0e-7 |
| velocity x, y, z | 3600 / 3600, 5.0e-7 |
| ground_y (sensor ground, ramps, train/gate roofs) | 3600 / 3600, 5.0e-7 |
| pose state (`idle`, `running`, `dodging`, `rolling`, `hangtime`, `descending`, `empty`, …) and lane | 3600 / 3600 exact |
| extra: body size, landed, lane_changing, `stats.time/speed/z`, coins | 3600 / 3600 (time bit-exact) |
| extra: camera position, rotation, fov (intro tween, running lerps, pogo, shake RNG) | 3600 / 3600, 5.0e-7 |
| extra: events — collisions (other id, flags, hit box), pickups, trigger enters, runFromIntro | 33 / 53 / 7 / 1, same frames |

5e-7 is the trace's 6-decimal rounding; the f32 values themselves agree.
The trace has no jumps, so `tests/hero_jump.rs` checks a jump against the
`jf` formulas (ascent to just under start + 19 on a sineOut over 0.41 s,
gravity 0.055/frame², running → ascending → hangtime → descending →
running).

### The hero's shadow, pickup pops and revive halo (`src/hero_fx.rs`)

The hero entity's own effects (deobfuscated.js 38286-38634):

| effect | asset | blend | behaviour |
|---|---|---|---|
| blob shadow `rp` | 8 x 8 `character_shadow` plane | MULTIPLY | double sided; held at `ground + 1` (on the track or a train roof); on from `Player.run` until a death |
| coin pop `Yf` | `star7` | SCREEN | on every coin, at (0, 0, -3) in the hero's space; random z turn; scale 0.5 -> 1.25 over 8 frames |
| pickup pop `Zf` | `pow` | additive | every pickup and the hoverboard switching on; scale 0.5, then 1 -> 21 over 13 frames |
| revive halo `$f` | `powRevive` | SCREEN, no depth test or write | 120 frames on revive: scale `2 + 0.2 sin(0.1 t)`, opacity in to 0.6 and out over the last tenth, a slow turn |

SCREEN is the original's `CustomBlending(ONE, ONE_MINUS_SRC_COLOR)`, so
opacity does not scale it. It goes through the world shader with the
clip-space bend like everything else.

The random turns draw on the oracle's call sites. V8 names the caller after
the pickup's class (`Ua.onCollect`, `Wa.onCollect`...).

Matching oracle screenshots:

| frame | effect | pixels off by > 32 |
|---|---|---|
| f301 | shadow | 0.33 % |
| f408 | pickup pop | 0.37 % |
| f662 | coin pop | 0.42 % |
| f315 / f360 | revive halo | 0.21 / 0.27 % |

`tests/hero_fx.rs` checks the timings. The original draws no shadow under
the guard or the dog (`rp` is a hero component only), so the port draws
none either.

### Particle effects (`src/particles.rs`, `src/fx.rs`, `src/render_fx.rs`)

The port runs the original's Unity particle runtime (`An`): Unity curves and
gradients, emission and bursts, Cone / Box / Circle / Rectangle shapes, and
over-lifetime modules for colour, size, rotation, velocity and limited
velocity, plus sprite sheets. It also has the FX rigs (`Tf` / `Df`) and
ribbon trails (`Sf`).

**Effects.** These are the effects the original has, each with its original
config (extracted by `extracted/extract_fx.mjs` into `data/fx/`):

- the pogo's rainbow paint clouds and trail;
- the jetpack and headstart start-burst puffs, paint trails and boosted
  flames;
- hoverboard grind sparks and crash smoke;
- revive smoke;
- Bali gate sparkles and swamp fog.

**Not in the original.** It has no coin-pickup sparkles and no landing or roll
dust, so the port adds none (see `docs/js_notes/particles.md`).

**Simulation.** The particles are simulated in the simulation, once per
frame, with every `Math.random` on the oracle's per-call-site streams (the
systems share the same sites). They update in the original's
component order: the order their entities joined the scene.

**Rendering.** `shaders/fx.wgsl` draws them as three did:

- quads expanded in view space, billboarded or stretched;
- sprite-sheet tiles;
- the texture re-encoded with `pow(1/2.2)`, times vertex colour and
  multiplier;
- non-premultiplied normal or additive blending, with no depth write;
- the clip-space world bend `xy += w² · uBend`;
- camera-facing ribbons for the trails.

Render frames between simulation frames interpolate the particle centres.

**Parity.** `tests/fx_parity.rs` matches every system frame for frame: clock,
count, every live particle's offset, size, rotation, colour, lifetime and tile,
plus the rig world matrices and trail points. It runs against five oracle
fixtures (`oracle/fx_dump.mjs`):

| fixture | covers |
|---|---|
| seed1-god | pogo and sparkles |
| seed1-god-jetpack | jetpack and fog |
| seed1-god-hoverboard | grinding |
| seed1-hoverboard-shield | crash smoke |
| seed1-revive | revive smoke |

### New High Score screen: speed stripes and the 3D character

The New High Score screen (`Gv`) now draws all of its parts.

- **Background.** The rainbow is the original's static, stretched
  `highscorescreen-bg`. The motion comes from 10 `celebration-stripe`
  sprites (`Kv`) that streak toward the bg's centre and respawn on the left.
  The original has no rotating sunburst, so the port doesn't either.
- **Stripes in the sim.** The stripes live in the simulation
  (`src/celebration.rs`). They are built on the first opening with the
  oracle's own `R.range` streams and persist across later openings, as in
  the original.
- **Character.** The hero's model (`ActiveCharacter`: an avatar and outfit)
  is rendered on its own into a 640 x 640 texture
  (`src/render_menu_3d.rs`, three's thumb renderer). The camera uses framing
  `Jv`, fov 30, a transparent clear, MSAA, no bend and no fog, plus the
  radial floor shadow.
- **Clip.** The character plays `run_HighScore_main` at 0.35x speed.
- **Slide-in.** The view starts at (400, 50) x0.5 and lerps to the centre
  at full size by 0.5 % per frame, under the title and score and above
  "Press Space to continue".

`tests/celebration.rs` matches the stripes (position, rotation, scale,
speed, respawns), the view and the message alpha with
`oracle/traces/seed1-highscore/celebration.json` for 309 frames. Re-record
the fixture with `node oracle/hs_dump.mjs`. Against the original's
screenshots at f640/f700/f800, about 1 % of pixels differ by more than 8.

### Word Hunt (`src/word_hunt.rs`)

The daily Word Hunt (`va`, `xa`, `Da`/`Ea`, `Fa`; deobfuscated.js
10445-10688) is ported:

- **The word.** Today's word is `va[(days since 2021-06-28) % 21]`. Its
  letters spawn one at a time on the letter pickup timer (20 s), and each
  letter collected advances the hunt.
- **Saved progress.** Progress is saved per day in `GameSettings.json`
  (`hunt_word`, `hunt_day`, `hunt_index`, `completed_hunts`,
  `word_hunt_started`). A new day starts from the first letter. A Monday's
  new word clears the week's finished list.
- **Reward.** The last letter pays `ya[n]`, where `n` is the hunts finished
  earlier this week: mystery box, mystery box, 1050 coins, 2100 coins,
  super mystery box. Coins go to the wallet, and a box is rolled into the
  run's prizes, shown after the run. The letter timer then stops and any
  letters left in the scene are removed.
- **HUD banner.** Each letter drops the HUD banner: 300 x 90, `#3A8BBA`, the
  word in Titan One with the new letter scaling 1 -> 1.7 -> 1 and turning
  yellow. The last letter then shows the "Word Hunt Complete" panel with
  the reward.
- **Pause panel.** The pause panel lists the day's timer, the letter tiles
  (collected ones yellow) and the reward.

Days are UTC days (the original mixes UTC dates and local midnights).
Replays keep the trace's fixed letter: no parity trace collects a letter.
`--collect-letter F` collects today's next letter before frame F in a
screenshot run. `tests/word_hunt.rs` covers collection (the reward paid
once, a box reward going to the run's prizes) and the banner's timeline;
the unit tests in `src/word_hunt.rs` cover the day word and the weekly reset.

### The menus' 3D views (`src/render_menu_3d.rs`)

One offscreen stage (three's thumb renderer `dv`: a 640 x 640 texture, fov
30, transparent clear, MSAA, layer 1, camera order -1) renders whatever the
current screen shows. The UI draws that texture as the original's sprite.

| screen | subject | framing | placement |
|---|---|---|---|
| New High Score (`Xv`) | the hero running `run_HighScore_main` at 0.35x | `Jv` | the sliding view |
| results notepad (`Wv` `_v`) | the saved character idling (its own idles) | `uv` | anchor (0.5, 1), 36 below the notepad's centre, x -190 |
| Me panel, Characters (`hy` `_v`) | the focused character and outfit idling | `uv` | anchor (0.5, 1) at (-170, 75), x1.3 |
| Me panel, Boards | the selected character on the focused board and powers: one random trick (`gv`), then `h_run` | `mv` | same |
| prize screen (`Dy`) | `mysteryBox_default` / `_super` | `Ey` | anchor (0.5, 1) at the centre + 296; 2D shadow `Z_` at + 200 |

- **Characters** stand on `ensureFloorShadow`. The idles (`_v.setup3D`,
  `src/char_idle.rs`) play each character's own `idle-<id>.pk`
  (`animations-character-idle`) on its own skeleton (`Y_`: the tracks of
  the avatar's nodes, `Root.scale` normalized). They're cut into a breathe
  clip and gesture flavors (`Ep`, extracted to `data/character_idles.json`).
  The director `ov` plays a random gesture, then one breathe, then another
  gesture, without cross-fades. Props (`H_`), such as Jake's sandwich and
  spray can or Tagbot's head, are the avatar's unskinned meshes placed on
  their attach point. They're hidden by outfit, and their replaced nodes
  hide while they show. The board preview poses the avatar through Jake's
  skeleton (`h_run` and the tricks). The board hangs on `attachPoint1`
  turned -pi/2 (`ev`).
- **The box** follows `Dy.tween(1)`: ry 0.25, rx -0.56, rz -0.125, x0.008,
  y 0.02 <-> 0.045 and the shadow 1 <-> 0.5, over 1.5 s, yoyo,
  `Power1.easeOut`. Then `tween(2)` on the open press: y -> 0.04 in 0.3 s;
  ry += 5 pi, scale -> 0 and the shadow fade in 0.5 s (`box_pose`, unit
  tested).
- **Checked against the oracle.** `node oracle/menus3d.mjs <dir>` captures
  the results and prize screens, and `oracle/menus.mjs` the Me panel. Use
  `--menu results`, `--menu prize`, `--menu prize-open`, `--menu me`,
  `--menu me-boards` or `--menu highscore` for the port's side.

Differences:

- **Idle files.** The site capture lacked `assets/animations-character-idle/`
  (the offline original shows a broken pose there). The 40 `idle-<id>.pk`
  files were fetched later from the same CDN path as the rest of
  `oracle/site`.
- **Faces in the idles.** The idles' eye-switch and blendshape tracks (`V_`)
  aren't applied, so faces keep their rest look while idling.
- **Drag to turn.** Dragging the Me-panel preview horizontally turns the
  character (0.01 rad/px). This is a port addition, and the turn resets
  when the focus changes.
- **Prize icon.** The prize itself (`Fy`, another 3D thumb) is still its
  atlas icon.

### Sound (`src/audio.rs`)

This ports the original's sound object `qb` (`$.sound` / `game.sfx`) over
its mixer `Kb`. The simulation only queues commands in `Game.sound`, and
`AudioPlugin` plays them with Bevy's audio. The sound code reads no RNG
stream and changes no simulation state, so every parity test is unchanged.

- **Files.** The 27 effects (`assets/audio-basic`, `audio-full`,
  `audio-idle`) and the city bundle's `bundles/bali/audio-basic/theme.ogg`
  load in place from the original's site. Nothing is copied.
- **Volume.** The master volume is 0.25 (`config.volume`, read in the
  running original). Every sound is at volume 1 unless its call gives one.
- **Music.** The theme loops from the first run (`playTheme`, once). It
  fades linearly to 0 in 0.5 s when the New High Score screen or a prize
  opening starts (`musicFadeOut`), and back to 1 in 1 s when that screen
  closes.
- **Effects,** at the original's call sites:
  - **Hero:** `hero-jump`, `hero-roll`, `hero-dodge` (each lane change),
    `hero-stumble` (dizzy start), `hero-sneakers-jump`, and the sneakers'
    `hero-sneakers-foot-l/r` (every 25 frame units while running).
  - **Death:** `hero-death`, plus `hero-death-hitcam` 600 ms later on a
    train. A hoverboard crash plays `hero-hoverboard-crash` instead.
  - **Revive:** `hero-revive`.
  - **Coins:** `pickup-coin` at volume 0.5, rate `1 + arc * 0.05`. `arc` is
    the coin's place on a jump curve, 1, 2, 3…; line coins are 0.
  - **Pickups:** `pickup-powerup` for every pickup but the jetpack, and for
    the hoverboard. `pickup-powerdown` when a timer runs out (hoverboard,
    jetpack, magnet, 2x, sneakers).
  - **Loops:** `special-jetpack-start` then the `special-jetpack` loop (also
    the pogo's loop) and the `special-magnet` loop, stopped when they end.
  - **Guard:** `guard-start` (intro), `guard-proximity` (near),
    `guard-catch`.
  - **UI:** `gui-tap` (button taps, the title's start, the nickname prompt's
    close), `mission-notification` (a mission toast), `open-prize` (with
    `stopAllFx`), `unlock` (New High Score, with `stopAllFx`).
- **Mute.** The Settings / title sound button sets `muted` in
  `GameSettings.json`. Muted, effects don't start and the master volume is
  0, so the music keeps running silently. The original also mutes a hidden
  page; the port does that on focus loss only with `--pause-on-blur`.

`tests/audio.rs` checks the commands of replayed runs: one theme, one chime
per coin, rolls, sneakers jumps and alternating steps, the guard, the curve
coins' `arc`, mute, and the train death's +600 ms hit.

Differences from the task's sound list. These follow the original:

- **No fade on run start or death.** The music starts at full volume and
  keeps playing through a death.
- **Coin pitch** comes from the coin's place on a jump curve, not from how
  quickly coins are collected.
- **Sound names.** There is no separate crash, dog bark, train horn or music
  toggle. The original's ids are used. `hero-foot-l/r` and `gui-coin` ship
  but nothing plays them.
- **Settings** has one Sound toggle, saved as `muted`.

### Showcase video (`tools/showcase.py`, `--dump-frames`)

`python3 tools/showcase.py` renders `dist/showcase.mp4`: 1920 x 1080, 60
FPS, H.264 (crf 18), with the Bali theme as its soundtrack.

- **Segments.** About a minute of 18 captioned segments:
  - the title screen and the live 3D Me panel (Jake, Tricky, Brody, the
    board preview);
  - oracle trace replays of the Bali route: the intro chase, trains and
    barriers, the Power Jumper, a stumble with the pursuers closing in,
    Super Sneakers, the Coin Magnet, the hoverboard and its crash shield;
  - a scripted jetpack with its sky coin ribbon;
  - Tricky on the route;
  - a real crash: Save me!, the New High Score screen and the results.
- **How it renders.** Each segment is a frame dump of the release binary:
  `--hd --dump-frames DIR --from F --at T`, exactly one 60 Hz simulation
  frame per video frame, once every texture has loaded. The segments get
  their captions (drawn with Lilita One) and short fades in ffmpeg, are
  concatenated, and the theme is laid under them. Frame dumps are deleted
  once their segment is encoded.
- **Sound.** Screenshot and dump runs play no sound.
- **Debug flags.** `--power jetpack|magnet|sneakers|multiplier|hoverboard@F`
  turns a power-up on in such runs; `--hd` renders 1920 x 1080 (the
  960 x 540 layout at scale 2).

### Final polish: effects, tutorial, awards, packaging

- **Dizzy stars** (`$o`): after a stumble, two `Dizzytrail` and two
  `Dizzystar` meshes (effects-tex, opacity 0.5, additive) sit 4 above
  `Head_jnt` (world tilt -0.5) and spin 0.05 rad per frame unit until the
  dizzy time ends. They match the oracle at f1420 of seed1-god. The dizzy
  period at a run's start shows none, as in the original.
- **Spray can:** the library's `sprayCan` (props-tex) on `attachPoint2`,
  turned -pi/2, while the paint idle plays on the title screen.
- **World bend on resize:** `Game.resize` runs `updateWorldBend` before it
  updates `aspectRatio`. A window resize therefore bends x by -5e-4 times
  the previous size's height / width; the first size counts as 1, as the
  oracle had. It applies to every live world and particle material. This
  is the original's rule, not "aspect / 1.777".
- **Prize rays:** the `lh` ray burst: four quarter `background-stripes-hq`,
  the 7x glow and the 650-wide superglow mask, baked once into one image
  that turns and fades on the original's timeline. The original adds
  light; a cream layer at 65 % of the intensity approximates it.
- **Buttons:** the pointer cursor shows over buttons (pixi `buttonMode`;
  not over whole-screen taps), and a held button's base is tinted #AAAAAA
  (`tintBaseOnPress`; the icon and label stay).
- **No pause on blur** (a change from the original). Alt-Tab or a click
  outside the window does not pause the game: it keeps running, at full
  rate and with sound.
  - `--pause-on-blur` restores the original's `Game.onBlur`: the pause
    panel, plus silence while unfocused.
  - Escape (the pause button) works as before.
- **Awards** (`src/awards.rs`, `data/awards.json`): the 12 awards with their
  four tiers. Progress is fed as in the original, saved as `awards`, and
  completion shows the "... complete" notification with the tier's spray
  can. The Me panel's Awards tab is the original's `ty` / `ey` list:
  trophies, title, description, keys, the progress bar or "Collect Award",
  ordered by progress, and scrolled by wheel or drag. Collecting opens the
  prize screen with the keys, which credits them, and `awardPlayer` adds
  them again 2 s later. The original pays twice, and so does the port.
  The three "score without ..." awards run only once the tutorial is done,
  as in the original.
- **Tutorial** (`src/tutorial.rs`): a save without `tutorial` plays the
  tutorial route.
  - **Triggers.** Its `Trigger_<type>` entities (`No`) show the arrow
    (`tutorial-arrow`, the frame after) and the message: "Press Arrow Key
    Up", ...
  - **Hoverboard.** "hoverboard" unlocks the board, which is locked for the
    tutorial run.
  - **Finishing.** Leaving "finished" saves the tutorial as done.
  - **Death.** A death on the route rewinds the hero after 1 s to the
    chunk's checkpoint (`goBackToLastCheckPoint`); there is no game over.
  - **Tests.** `tests/tutorial.rs` checks it against the oracle: the prompt
    at f246, the death at f325, the rewind back to z -131.19 at f462.
  - Replays, screenshots and tests never enable it; `--tutorial` does, for
    screenshots.
- **Saves:** `GameSettings.json` lives in the user's application data
  folder: `%APPDATA%\SubwaySurfers` on Windows, `~/Library/Application
  Support/SubwaySurfers` on macOS, or the XDG data folder. It falls back to
  `save/` next to the game, and a save from an earlier version moves over
  once. Writes are atomic (`.tmp`, then a rename).
- **Windows:** release builds use the GUI subsystem, so no console opens.
  The window is titled "Subway Surfers", and the game's icon (the Poki
  logo from the network capture, `assets/icon/`) is the window icon and is
  embedded in the .exe (`build.rs`, `embed-resource`).

### Missions, Settings, My Tour, Double Up (`src/missions.rs`, `src/ui_missions.rs`)

**Missions** port the original's `Bm` (deobfuscated.js 44897) with its data
side (`prepareMissions`, `progressMission`):

- **Table.** The 87 missions of `y_` form 29 sets of three
  (`data/missions.json`, from `extracted/extract_missions.py`, with the
  English texts).
- **Multiplier.** Every finished set adds 1 to `missionMultiplier`, the
  second score multiplier (`score += distance * (multiplier +
  missionMultiplier)`, the HUD's "x"). It is read at each run's reset, so
  a new player plays at x1 and the cap is x30.
- **Counters.** The 41 counters of `Lm` are fed from the same places as in
  the original: coins (also in the air, with the jetpack / magnet / pogo),
  powerups by type, keys, mystery boxes (picked up, bought, coins from
  them), jumps (on trains), rolls (in the centre lane), dodged barriers,
  hoverboards (with and without a crash), getting caught in the first 10
  s, headstarts, score boosters, the score (with no coins), a beaten high
  score, coins spent (any decrease of the wallet) and finished Word Hunts.
  The "onerun" counters reset with each run.
- **Tracking.** Only the current set is tracked: progress is `min(counter,
  amount)`, saved per set in `GameSettings.json` (`missions[set] = [{id,
  progress, started}]`).
- **Completion.** A finished mission queues the "Mission Complete"
  notification `Py`: 0.5 s wait, slide down 0.3 s, 3 s, slide up. Once all
  three are done, the next set is tracked at once.
- **As in the original:**
  - The bump missions look for "train" / "light" / "blocker" in the
    obstacle's minified class name ("xr", "Ki", ...), so they never count.
    Sets 8, 15, 18, 21, 25, 27 and 29 then need a skip.
  - A Super Sneakers jump is not a jump.
- **Tests.** `tests/missions.rs` covers the counters on a replayed run, the
  one-run reset, set 1 completing to x2, the save, the skip and the x30
  cap. The parity traces stay identical; their runs start with no set
  finished.

**Screens:**

- **Pause panel.** It shows the missions section `fh` (MISSION SET n, the
  set's counter, each mission with its progress box and yellow amount).
  The bottom row is Menu / Settings / RESUME again: the port's RESTART is
  gone, and R still restarts.
- **My Tour.** The title's My Tour button opens it, with Word Hunt and
  Missions tabs. The Missions tab has the splat with "x n" and a "Skip this
  Mission" button on each mission: 1700 coins, a flat price in the
  original, or the "Not enough coins" popup.
- **Settings.** The title's gear and the pause panel open it: Nickname
  (prompt: type, Backspace, Enter / a tap outside), Sound On / Off,
  Privacy Policy and the version "v1.0.12-fix-pogo". The title's sound
  button toggles the same setting.
- **Results.** "Free Double Up!" works (the rewarded ad succeeds): the
  run's coins go to the wallet again, the scoreboard shows them doubled,
  the module hides and the leaderboard takes its room. The leaderboard
  scrolls like the original's `vv`: drag (pointer px), wheel, inertia and
  soft limits; it shows the nickname.
- **Prize screen.** From 0.5 s after the open press it shows the prize's
  own 3D model (`Fy`: `currency_coin`, `currency_key`, `board_default_base`,
  `headstartToken`, `ScoreBooster`). It scales to 0.007 in 2 s and turns
  -4 pi in 4 s, with a shadow sized from the model's projected bounds
  (`measureBounds`); the label sits at the measured y.

Debug: `--menu mytour`, `mytour-missions`, `settings`, `settings-prompt`,
`results`, `prize-open:<kind>`; `--mission-stat ID=N@F` adds to a counter
before frame F in a screenshot run.

Differences:

- **Sound.** The original has a single Sound toggle and no separate music
  toggle (see "Sound").
- **Privacy Policy.** It opens nothing (no browser).
- **Version.** It shows the original's string, not "v1.0.0 (ss_port)".
- **Double Up.** It hides the button, as the original does, rather than
  greying it out, and plays no pop.
- **Skip price.** The skip is 1700 coins flat, not scaled from 1,500 to
  20,000, and it is offered on My Tour as in the original, not in the pause
  menu or shop.
- **Leaderboard drag.** It moves along y only. The original also lets the
  list slide sideways.
- **My Tour Word Hunt tab.** It reuses the pause panel's letters section;
  the original's weekly reward row is not drawn.
- **Missions.** They follow the original's 41 counters, not the 18 listed
  in the task.

### Step 6: obstacles, stations, boosts, mystery boxes, pause, in-place reset

| Checked (oracle traces) | Result |
|---|---|
| `Gi` obstacles (dumpster `Ki` 14, bush `qi` 12, power box `Ji` 8 + bushes) and the station (`ho` roof, `fo/po/mo`, platforms `uo` + side colliders `lo`, the bare `spawnStation` box) in the three `default_1_track*` chunks (`--query chunk=...`) | hero 3 x 1800 / 1800 frames; every entity (positions, meshes, despawns) 401 / 243 / 285 |
| headstart (V, twice) and score booster (C x3), `gdHeadstart=3 gdScoreBooster=3` | 2400 / 2400 frames; 699 entities (the headstart's safe-landing chunks and shuffled pickups) |
| in-place restart: death, then `nav.toGame()` (= PLAY) between frames (`record.mjs --eval`) | 1400 / 1400 frames: `Game.reset` / `idle` / `runWithIntro`, RNG streams continuing (second route s-s-s-s, tunnel_notrain, pogostick_start), second death at f817; 193 entities incl. the static `ground` |
| pause (Escape) and resume (Escape: "Starting in 3/2/1", 3 x 600 ms) | 1200 / 1200 frames: paused f205, running again f376, the 2-frame clock step at f377 |
| mystery box roll keys (`ta < ra :300593`, `yt < ra :300679`) | the port's first draw equals the oracle's (0.05340254586189985) |

Obstacles behave as in the original (not as walls): all three crash from the
front and are cleared by a normal jump (apex ~20); `soft` is never read.
Boost buttons show for the first 8 s of a run when owned (V / C or click);
the headstart is the jetpack at level 1-3 (2000 + 1000 level, +20 s of speed
ramp per press, magnet / 2x / sneakers waiting at the landing); the score
booster sets the multiplier to 6 / 7 / 8. A mystery box is rolled on pickup
(`Xi` table) and opened on the prize screen after a declined Save me (before
the high score); a shop box opens at once. Pause: Escape or the pause
button; the panel's Menu goes to the title without banking anything,
RESUME (or Escape) counts down. PLAY / Menu / R reset the world in place.

### Step 5: powerups, meters, menus, shop

From the original's `If` magnet, `Rf` 2x multiplier, `pp` super sneakers,
`kf` jetpack, `cf` hoverboard (docs/js_notes/powerup_*.md) and the shop /
title / Me-panel UI (ui_powerups_shop.md), in `powerups.rs`, `menu.rs`,
`shop.rs`, `ui_menu.rs`.

| Checked (`tests/trace_powerups_parity.rs`, oracle traces recorded with `record.mjs --query forcePickup=X` / `--press Space@F`) | Result |
|---|---|
| magnet (`seed1-god-magnet`): timer, every attracted coin's path / time / duration, collection | 3600 / 3600 frames, worst 2.4e-4 (one f32 ulp at z ~ -2300) |
| 2x multiplier (`seed1-god-multiplier`): timer, stats.multiplier, score | 3600 / 3600 frames |
| super sneakers (`seed1-god-sneakers`, `-sneakers-jump`): timer, 40-unit jumps, camera, superRun / hangtime clips | 3600 + 2400 frames |
| jetpack (`seed1-god-jetpack`, 5400 frames): take-off to y 100, flight, sky coin ribbon (lane picks), safe-landing chunks, fall | 5400 / 5400 frames |
| hoverboard (`seed1-god-hoverboard`): Space, 30 s timer, h_* clips, grinds, pause/resume around the pogo | 3600 / 3600 frames |
| hoverboard crash shield (`seed1-hoverboard-shield`, mortal): two forced hops, removeObstacles(600), the later real death | 1200 / 1200 frames |
| menus (`tests/menus.rs`): title, shop upgrade, Me panel select / buy / locked / not enough, save, PLAY, no-board popup + countdown | headless, through the UI's clicks |

Every check of the hero parity (position, velocity, state, animation
record, camera, guard, events) runs on these traces too.

Values are the original's, not round numbers: magnet / 2x / sneakers last
`10 + 5 * tier` s (tiers 0-6 at 500, 1500, 3000, 10000, 30000, 60000
coins); the jetpack lasts a distance (`1000 + 200 |speed| + 600 * tier`,
snapped to a chunk boundary with a landing chunk), flies at y 100 and drops
under gravity; the magnet pulls coins within 110 units (any lane, ahead or
behind) along `lerp(start, hero, t^2)`; the sneakers jump is 40 units over
80 units of z; the hoverboard is one Space press (double tap on touch),
costs one board (3 to start, 300 coins each), lasts 30 s and absorbs one
fatal crash by hopping twice and deleting obstacles within 600 units (no
cooldown, no invulnerability timer).

HUD meters (`Km`): one row per active powerup, oldest at the bottom, with
icon, bar, red fade and the board count. Hero props: the board under the
feet (`attachPoint1`, the selected board's features and powers), the
jetpack on `LowerSpine_jnt`, the magnet in `R_Hand_jnt`, the super sneakers
skinned to the hero. The jetpack's particle trails are not drawn.

Menus: the title (over the idle 3D scene: boards button, PRESS TO PLAY,
ME, SHOP), the Me panel (Characters: the 9 original characters,
outfits for keys, locked until the character is owned; Boards: the 16
boards and their powerups), the boost shop (single use + upgrades with tier
pips), the results notepad's Menu and Boosts, "Need hoverboards?" (also
mid-run with none left: pause, then a 3 s countdown), "Not enough". Buying
equips, as in the original. Settings, the sound button and My Tour: see
"Missions, Settings, My Tour, Double Up"; Awards: see "Final polish". Not ported: FreeStuff, Top Run, Free Stuff tab, the headstart / score booster in-run buttons (they are counted),
drag scrolling (wheel and keys instead). The thumb strip: the characters' 2D
portraits; boards pre-rendered by `tools/thumbs.py` (the
original renders them in 3D). The big preview is live 3D (see "The menus'
3D views"). Any avatar is drawn from its own `.pk` with
Jake's animator (all share the 26-joint skeleton). Outfits follow the
original's per-character mesh table `Ep[id].outfitMeshes` (`kp`/`jp`;
`data/outfit_meshes.json`, made by `extracted/extract_outfit_meshes.py`).
Each outfit shows exactly its listed meshes, including ones the `.pk` saves
hidden, such as Brody's `brody_outfit1/2` with his hair, sunglasses and
bandana. Characters missing from the table keep the file's visibility
(`tests/outfits.rs`). `--character brody:2` and `--menu me:brody:2` pick an
outfit.

Save (`GameSettings.json`, in the user's application data folder; see
"Final polish"): `high_score`, `coins`, `keys`,
`selected_character`, `selected_outfit`, `selected_board`, `board_powers`,
`upgrades` (`magnet_tier`, `jetpack_tier`, `sneakers_tier`,
`multiplier_tier`), `hoverboards`, `mystery_boxes`, `score_boosters`,
`headstarts`, `owned_characters`, `owned_outfits`, `owned_boards`, the Word
Hunt's fields, `missions`, `name`, `muted` (the
original's GameSettings + ShopSettings + CharacterSettings + BoardSettings
folded into one file). Older saves load (`highscore` is accepted).

### Step 4: HUD and the death flow

`flow.rs` (state) + `ui.rs` (drawing) port the original's pixi UI with its
atlas `assets/ui/ui.webp` and fonts (Lilita One, Titan One: TTF conversions of
the shipped woff2 in `assets/fonts/`), laid out in the original's virtual
space (`S = (H/500)*0.5`). See `docs/js_notes/ui_hud.md`, `ui_gameover.md`.

| Checked | Result |
|---|---|
| score (`floor(Σ distanceDelta·multiplier · 0.1)`) vs the trace | 3600 / 3600 frames |
| HUD at f1200 vs the oracle: score / badge / coin ink boxes | within 0–2 px (font rasterization), coin icon exact |
| death flow timeline (`tests/death_flow.rs`, seed 1 mortal): death f185, Save me f231, clock out f592, Space accepted from f793, PLAY binding open+121 | as the oracle |
| revive by Space (free, once per run) | runs on, obstacles cleared |
| Save me / results screenshots vs the oracle | panel, buttons, clock, scoreboard, leaderboard colours pixel-equal |

The original HUD has no coin bump, high score or score popup: counters
step every 4th frame; the badge shows `x1` (`x2` with the 2x powerup). The
HUD appears 200 ms after the run starts and hides at death. After a crash:
"Save me!" (Space or the "Free!" button = free revive once per run, `K` or
the key button = revive for 1, 2, 4… keys, anything else declines), then New High Score
(Space once "Press Space to continue" has faded in), then the results
notepad; Space (after 2 s) or PLAY starts a new run. Currencies and the high
score persist in `GameSettings.json` (interactive play only).

Differences: PLAY starts a fresh run (the original resets the level in
place, keeping its RNG streams running). Bevy UI quirks worked around in `ui.rs`: nine-slices
are built from nine stretched quads, solid fills are tinted 1x1 images, and
the sRGB decode runs after the UI pass so the overlay blends like pixi.

### Step 3: actors (Jake, the inspector, the dog)

From the original assets (`avatar_jake.pk`, `model-guard.pk`, `model-dog.pk`,
the `movement`/`idle`/`catch`/`pogostick`/... animation `.pk`s, clip tables
`Cp` extracted verbatim to `data/anim_clips.json`):

| Checked | Result |
|---|---|
| guard state machine (`tests/trace_hero_parity.rs`): state, distance, x, z, visibility | 3600 / 3600 frames, worst 5e-7 |
| hero animation record: clip, playhead, weight, duration, speed, loop, mixing, prev | 3600 / 3600 frames, worst 5e-7 |
| skinning (`tests/actor_skin.rs`): bone worlds + skinned vertices of all three, from the live bone pose | ≤ 6e-6 world units |
| the port's own animated bone pose vs the live game, f40 and f600 | every bone local ≤ 4e-8 |
| screenshots (see `docs/rendering.md`) | f40 intro 0.52 % of pixels off, f1200 0.75 % |

`anim.rs` ports the original's clip cutting, three.js's mixer (actions,
f32 cross-fade endpoints, LoopOnce clamp / LoopRepeat, property-mixer
blending toward the rest pose, slerpFlat) and the `Zo`/`X` controller
quirks (stale clip speeds, sudden pre-advance, same-clip no-op). Skinning
runs on the CPU into world space, so characters use the world material and
get the same clip-space bend (no fog, like the original's
`unlit-high:bend:nofog`). Jake's face blinks and the eyes look around
(`Go`); the pogo stick is attached to `attachPoint1`.

### Step 1: chunk spawner + entity mounts

`tests/trace_scenery_parity.rs` runs a headless Bevy app (MinimalPlugins +
`SimPlugin`, one update per game frame, 3,600 frames) for seed 1 (god mode)
and checks against `oracle/traces/seed1-god/trace.jsonl`:

| Checked | Result |
|---|---|
| chunk placements (frame, name, start, anchor z, length, blocks) | 13 / 13 |
| scenery spawns, in add order, per frame | 662 / 662 |
| per spawn: class, root pos/rot/scale, body, every mesh (name, .pk, visibility, world pos/rot/scale, texture, .pk material, culling, blend mode, depth write) | all equal within 1e-3 (worst 5.0e-7, the trace's rounding) |
| despawn frame of every spawned entity (528 despawns + 134 alive at the end) | 662 / 662 |
| `OracleRng` vs every logged gameplay `Math.random` draw (`tests/rng_parity.rs`) | bit-exact |
| asset library groups / part order / material names (`tests/library_parity.rs`) | identical to the game's |

Scenery = everything except character state: hero `Gp`, guard `im`, camera
rig `om`, pogo effects `Tf`/`Sf`.

## How it's structured

| Module | JS it mirrors |
|---|---|
| `pk.rs`, `library.rs` | `.pk` codec (pk-Bxs1M8oX.js), `Z.convert`, library `bb.refresh`/`getEntity`/`getEntityFromGeometry` |
| `data.rs` | `Kt.chunkMap`/`Kt.chunk`, route section tables `Vt` |
| `game.rs` | Game frame order, LevelSystem `xg` (21-frame culling/placement cadence, reverse queue placement), RouteSystem `Eg` + builder `Tg` |
| `mount.rs` | `bg.mount` (Randomizer/RandomizeOffset/Mirror, reverse child order), class registry `Ro`, factories: blockers, coins, trains (incl. cargo wagons, moving trains, lights), ramps, light signals, gates, pickups, pogo trail |
| `environment.rs` | `Tm` (env flags, tube/epic spawn gates), tracks `K`, fillers `ii` (+ bali hide-base hook), tube `Fo`/`Po` (incl. the forEach+destroy skip), epic `$r`, skyline |
| `theme.rs` | theme config engine `Yr`/`Qr` with the bali trees (`data/theme_bali.json`, extracted verbatim) |
| `entities.rs` | constructors (pool misses), f32 boxes/bodies (`tn`/`U`: `Float32Array` center, size, velocity, origin; `hitTest`), Movable/flicker/halo/spin/shine/bounce components, render phase |
| `hero.rs` | hero `Gp`: controller `hm` (keydown flags, one dispatch per frame), lane `Pf`, jump `jf` (the only gravity), roll `tp`, pogo `Kf`, player `Hf` (forward speed lerp, camera Y, stumble/crash/dizzy, god mode), pose FSM `gp` (`Kp` states and transitions) |
| `physics.rs` | `Sg`: sub-steps, `_hasReset`, reverse entity order, broadphase window, `resolveHit` push-out + flags, ground sensor (ramp formula), collectibles, triggers |
| `camera.rs` | camera system `sm` + rig `om` (idle/main/tunnel levels, shake RNG sites), intro `hg`/`mg` tween |
| `guard.rs` | guard `im` + dog `rm`: chaser `nm`, state machine `qp` (disabled/intro/near/far/goAway/catch) |
| `skin.rs`, `anim.rs` | skinned `.pk` models, CPU skinning; clips, mixer, `Zo`/`X`, face `Go` |
| `render_actors.rs` | drawing the characters and the pogo stick |
| `flow.rs`, `ui.rs` | score, HUD `fg`, death flow (`eb` Save me, `Ny`, `Gv` high score, `Wv` results), revive, user data |
| `scene.rs`, `math3.rs` | three.js object tree, child order, three's Euler/quaternion/decompose in f64 |
| `rng.rs` | `Math.random` per JS call site: `OracleRng` (oracle stream keys) / `SingleStreamRng` |
| `sim_plugin.rs` | Bevy `SimPlugin`, `Sim` resource, keyboard drivers (`ReplayDriver`, `KeyQueue`) |
| `render.rs`, `main.rs` | mirrors the scene graph into Bevy meshes/materials; playable window at a fixed 60 Hz |

Things that turned out to matter for exactness (all ported):
- RNG draws happen per JS call site; pool reuse decides which constructors
  run, and constructors draw theme-config randomness, so the per-class LIFO
  pool and its delayed return queue are modelled exactly.
- Bodies are `Float32Array`s: every write rounds to f32 (center, size,
  velocity, origin, hit boxes); physics moves in
  `ceil(delta)+1+ceil(|hero vz|)` sub-steps, the hero first, then every
  physics entity in reverse add order (trains move inside the same loop).
- The game clock: `Mg` smooths the delta of the oracle's virtual
  `performance.now()`, so `frameTime` is 0.9999999999994, not 1. That is why
  the intro completes at f80, a roll lasts 31 updates and a shake makes two
  extra near-zero RNG draws.
- The first RUNNING frame does not move (`Sg._hasReset`); the step count
  uses the forward velocity after `Player.render`'s lerp, before collisions.
- Pose params are read before this frame's physics: `ascending/descending`
  compare the last sub-step's origin with the box, `landed` uses last
  frame's ground; lane changes from physics (bumps) show as `empty` for a
  frame.
- Phase order: preupdate culling sees last frame's `stats.z`; components set
  velocities, `Body.render` copies the pre-physics center into the entity
  position, then physics moves bodies; chunks are placed in postupdate.
- `z.lerp` clamps t; coin spin seeds from a global counter (`ur`, starting
  at 1); moving-train lights blink on `performance.now() % 400` when the hero
  is in their lane.

## Rendering

`render.rs` + `src/shaders/subway.wgsl` port the original's materials:
the clip-space world bend (`uBend = (−5e-4, −3e-4)`), the smoothstep distance fog
(400 → 900 toward (0.454, 0.894, 0.917)), the flat sky `#87CEEB`, unlit
sRGB output without tone mapping, blending of encoded values like the
WebGL canvas, mipmaps, and the Bali water/foam shaders and boat bob.
`docs/rendering.md` lists every value with its source and the measured
per-frame differences. `docs/compare-1200.png` shows the original (top) and
the port (bottom) at f1200: mean pixel difference 0.75/255; the rest is
the hero and the HUD. Those comparisons use `--screenshot`, which always
renders with anti-aliasing off at the display rate.

### Frame rate and anti-aliasing (not in the original)

The original draws one frame per simulation step under `requestAnimationFrame`.
The port runs uncapped in play:

- **Present mode.** `PresentMode::AutoNoVsync` with
  `desired_maximum_frame_latency = 1`. On windowed Metal the default latency 2
  still holds the surface at 60 Hz. With latency 1, play runs at about 170-240
  FPS on an M2. `--vsync` restores `AutoVsync` and the default latency.
- **Fixed simulation rate.** The simulation stays a fixed 60 Hz accumulator
  (`step_sim`), so gameplay, RNG and parity do not depend on the render rate.
  Replay, `--screenshot` and the tests are deterministic as before.
- **Interpolation.** Between simulation frames, `RenderBlend.alpha`
  (the accumulator fraction) blends the last two simulated transforms of the
  scenery (`sync_scene`), actors and props (`render_actors::apply_blend`)
  and the camera (`follow_camera`).
- **Per-frame cost.** UI rebuilds and skinning run only when a new simulation
  frame arrives. `--no-smooth` turns interpolation off.
- **Anti-aliasing.** `--aa off|msaa|fxaa|both` sets the mode. The default in
  play is MSAA 4x; FXAA is Bevy's post-process at High/High.
- **FPS counter.** With `--debug-hud`, a status line at the bottom shows
  `FPS n` (refreshed every 0.25 s), the frame, state, coins, distance and
  speed. It is off by default.
  `SS_FPS_LOG=1` also prints it, with the simulation frame, to stdout.

## Inputs

Only the keyboard. The hunt letter is date dependent in the game (`Na()`);
the viewer uses "S" (as in the traces) unless `--letter` says otherwise, and
the tests take it from the trace. Play is mortal (crashes end the run);
god mode is opt-in only: `--god` / `--invincible` make the hero invincible
(in play and in `--screenshot` runs alike). Replays take god mode from their
trace.

A run's layout depends only on the seed: how long the title showed doesn't
change it (`tests/seed.rs`). A restart in place builds another layout, as
in the original. `--lock-seed` makes every run start from a fresh boot of
the seed instead, so R and PLAY give the same layout each time; progress is
saved first. Space activates a
hoverboard (and continues on the death screens); the menus take clicks,
the mouse wheel, arrows and Space.

## Not ported yet

- Every chunk node class is ported (`tests/scenery_no_panic.rs` forces the
  rare chunks into a run; `--chunk NAME@F` does the same in the viewer).
- The prize odds panel, Top Run, FreeStuff, the award badges on the Me
  button; board power `speed-up`'s per-getter-call ramp (advanced once per
  frame here).
- Untested by the trace (ported from the code, unvalidated): jumps beyond
  `tests/hero_jump.rs`, SLOPE/TOP/BOTTOM hits, queued lane changes, wall
  bumps, tunnel camera (`lowCamera` gate triggers), trigger exits.
- Rendering: the Me panel's thumb strip in 3D (static images), and the
  idles' eye and blendshape tracks (`V_`).
- Themes: this build ships only Bali (the only bundle the original
  downloads); `ThemeRegistry` discovers `data/theme_<id>.json` files, so
  another city needs its file plus its bundle and theme class.

## Reported, but as the original

Behaviours reported as bugs that the web original shares (checked against
the deobfuscated bundle):

- **The magnet isn't held up.** The original attaches the magnet model to
  `R_Hand_jnt` (`If.addMagnetModel`, 37518) and keeps the normal run
  animation. Its `hold_magnet` clip (movement frames 280-281) is defined
  but never played. The arm-up pose is the mobile game's.
- **Board trails (star trail, pink trail).** The original's board data marks
  the `star-trail` and `pink-trail` upgrades `available: false`, so the
  shop hides them (`setPowerups` shows only available ones). No code draws
  a trail for them.
