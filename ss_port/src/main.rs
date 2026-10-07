// Windows release builds open as a desktop app (no console window).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! Playable port: the simulation runs at a fixed 60 frames per second and
//! is rendered through the port's own camera rig.
//!
//!   cargo run --release                      play (arrows / WASD, space)
//!   cargo run --release -- --seed 7          another level seed
//!   cargo run --release -- --god             invincible (also --invincible). Without it every
//!                                            run is normal: crashes end the run, Save me!,
//!                                            game over (screenshots too; replays follow the trace)
//!   cargo run --release -- --theme bali      city theme (data/theme_<id>.json; this build has bali only)
//!   cargo run --release -- --character tricky   draw the hero as a roster character (`<id>:<outfit>`
//!                                            for an outfit; alias --hero; default: the saved selection)
//!   cargo run --release -- --replay [TRACE]  replay the keydowns of an oracle trace
//!   cargo run --release -- --replay --screenshot OUT.png --at F
//!
//!   cargo run --release -- --aa fxaa           anti-aliasing: off, msaa (default), fxaa, both
//!   cargo run --release -- --debug-hud       the developer status line (FPS, frame, state...)
//!   cargo run --release -- --pause-on-blur     the original's pause (and mute) on focus loss
//!                                            (default: Alt-Tab does not pause)
//!   cargo run --release -- --lock-seed       every run the seed's same layout (R, PLAY)
//!   cargo run --release -- --vsync             present with VSync (default: uncapped; the
//!                                              simulation stays at 60 Hz, render frames interpolate)
//!
//! Keys (the original bindings): Left/A, Right/D change lane, Up/W jump,
//! Down/S roll, Space the hoverboard (and continue on the death screens),
//! Escape pause, V / C the headstart / score booster, K revive with keys.
//! R restarts the run, T switches to the next city theme (a restart in the
//! new city; with one theme it reports it).
//!
//! In replay mode only the logged keys come from the trace; with
//! `--screenshot` the viewer fast-forwards to frame F, waits for textures,
//! saves the window and exits.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use ss_port::hero::Key;
use ss_port::render::SceneRenderPlugin;
use ss_port::render_actors::{character_roster, ActiveCharacter};
use ss_port::sim_plugin::{trace_hunt_letter, DataPaths, Driver, KeyQueue, ReplayDriver, Sim};
use ss_port::install::Install;
use ss_port::theme::ThemeRegistry;
use ss_port::trace::Trace;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Resource, Clone)]
struct Args {
    seed: i32,
    god: bool,
    letter: Option<String>,
    replay: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    at: i64,
    /// `--dump-frames DIR --from F --at T`: every frame F..=T as
    /// DIR/00000.png ... (a screenshot run that keeps going).
    dump_frames: Option<PathBuf>,
    from: i64,
    /// `--hd`: screenshots / dumps at 1920 x 1080 (the 960 x 540 layout at
    /// scale 2).
    hd: bool,
    theme: Option<String>,
    /// `--character`; None: the saved selection (jake for a fresh user).
    character: Option<ActiveCharacter>,
    /// `--press Space@794`: keydowns before given frames (screenshot runs).
    press: Vec<(Key, i64)>,
    /// `--chunk NAME@F`: queue a route chunk before frame F (screenshot runs).
    chunks: Vec<(String, i64)>,
    /// `--collect-letter F` (repeatable): collect the Word Hunt's next letter
    /// before frame F, with today's word on a fresh save (screenshot runs).
    collect_letters: Vec<i64>,
    /// `--mission-stat ID=N@F` (screenshot runs): `missions.addStat(N, ID)`
    /// before frame F.
    mission_stats: Vec<(String, i64, i64)>,
    /// `--click NAME@F` (screenshot runs): a UI click before frame F
    /// (pause-settings, close, resume).
    clicks: Vec<(String, i64)>,
    /// `--power KIND@F` (screenshot / dump runs): turn a power-up on before
    /// frame F (jetpack, magnet, sneakers, multiplier, hoverboard).
    powers: Vec<(String, i64)>,
    /// `--menu title|me|me:<id>|me-boards|shop|boosts|buyboards`: boot to the title
    /// and open that screen (screenshot runs).
    menu: Option<String>,
    /// `--aa off|msaa|fxaa|both` (interactive default msaa; screenshots off).
    aa: ss_port::render::AntiAlias,
    /// `--vsync`: present with VSync (interactive default: uncapped).
    vsync: bool,
    /// Interpolate render frames between simulation frames (off: `--no-smooth`, screenshots).
    smooth: bool,
    /// `--pause-on-blur`: the original's `Game.onBlur` (pause, mute) when the
    /// window loses focus. Off by default: the game plays on in the background.
    pause_on_blur: bool,
    /// `--lock-seed`: every run starts from a fresh boot of the seed (the
    /// same layout each run; the original's streams go on from run to run).
    lock_seed: bool,
    /// `--debug-hud`: the developer status line at the bottom (FPS, frame,
    /// state, coins, distance, speed). Off by default: not in the original.
    debug_hud: bool,
}

fn parse_args(inst: &Install) -> Args {
    let a: Vec<String> = std::env::args().collect();
    let get = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).filter(|v| !v.starts_with("--")).cloned();
    let has = |k: &str| a.iter().any(|x| x == k);
    let replay = has("--replay").then(|| get("--replay").map(PathBuf::from).unwrap_or_else(|| inst.repo.clone().unwrap_or_else(|| inst.port.clone()).join("oracle/traces/seed1-god/trace.jsonl")));
    Args {
        seed: get("--seed").and_then(|v| v.parse().ok()).unwrap_or(1),
        // normal (mortal) play unless asked for: crashes stumble, kill, and
        // lead to Save me! and the game over. Replays take the trace's own
        // setting (`configure_from_trace`); --mortal is accepted (the default)
        god: has("--god") || has("--invincible"),
        // `Na()` picks the word-hunt letter by date; the traces used "S"
        letter: Some(get("--letter").unwrap_or_else(|| "S".into())),
        replay,
        screenshot: get("--screenshot").map(PathBuf::from).or_else(|| get("--dump-frames").map(|d| PathBuf::from(d).join("last.png"))),
        dump_frames: get("--dump-frames").map(PathBuf::from),
        from: get("--from").and_then(|v| v.parse().ok()).unwrap_or(0),
        hd: has("--hd"),
        at: get("--at").and_then(|v| v.parse().ok()).unwrap_or(600),
        theme: get("--theme").or_else(|| get("--city")),
        character: get("--character").or_else(|| get("--hero")).map(|n| {
            ActiveCharacter::from_name(&n).unwrap_or_else(|| {
                eprintln!("Unknown character '{n}'. Available: {:?}", character_roster());
                std::process::exit(2);
            })
        }),
        press: a
            .windows(2)
            .filter(|w| w[0] == "--press")
            .filter_map(|w| {
                let (k, f) = w[1].split_once('@')?;
                Some((Key::from_dom(k)?, f.parse().ok()?))
            })
            .collect(),
        menu: get("--menu"),
        collect_letters: a.windows(2).filter(|w| w[0] == "--collect-letter").filter_map(|w| w[1].parse().ok()).collect(),
        powers: a
            .windows(2)
            .filter(|w| w[0] == "--power")
            .filter_map(|w| {
                let (n, f) = w[1].rsplit_once('@')?;
                Some((n.to_string(), f.parse().ok()?))
            })
            .collect(),
        clicks: a
            .windows(2)
            .filter(|w| w[0] == "--click")
            .filter_map(|w| {
                let (n, f) = w[1].rsplit_once('@')?;
                Some((n.to_string(), f.parse().ok()?))
            })
            .collect(),
        mission_stats: a
            .windows(2)
            .filter(|w| w[0] == "--mission-stat")
            .filter_map(|w| {
                let (stat, f) = w[1].rsplit_once('@')?;
                let (id, n) = stat.split_once('=')?;
                Some((id.to_string(), n.parse().ok()?, f.parse().ok()?))
            })
            .collect(),
        aa: match get("--aa") {
            Some(v) => ss_port::render::AntiAlias::parse(&v).unwrap_or_else(|| {
                eprintln!("--aa takes off, msaa, fxaa or both");
                std::process::exit(2);
            }),
            // screenshots stay comparable with the original (no AA there)
            None if has("--screenshot") => ss_port::render::AntiAlias::Off,
            None => ss_port::render::AntiAlias::Msaa,
        },
        vsync: has("--vsync"),
        pause_on_blur: has("--pause-on-blur"),
        lock_seed: has("--lock-seed"),
        debug_hud: has("--debug-hud"),
        // render frames between simulation frames interpolate (not in screenshots)
        smooth: !has("--screenshot") && !has("--no-smooth"),
        chunks: a
            .windows(2)
            .filter(|w| w[0] == "--chunk")
            .filter_map(|w| {
                let (n, f) = w[1].rsplit_once('@')?;
                Some((n.to_string(), f.parse().ok()?))
            })
            .collect(),
    }
}

/// Live keys for the interactive driver.
struct Keys(KeyQueue);

#[derive(Resource, Default)]
struct Clock {
    acc: f32,
}

/// A frame dump in progress: started (textures loaded), the frame being
/// captured, its outcome.
#[derive(Resource, Default)]
struct DumpState {
    started: bool,
    waited: u32,
    /// Frames saved (the next file's number).
    index: usize,
    /// A capture is in flight.
    capturing: bool,
    /// The simulation may step (the last frame is saved).
    step: bool,
    /// Blank captures of the current frame (a hidden window).
    retries: u32,
    /// The capture's outcome: 0 pending, 1 saved, 2 blank (taken again).
    status: std::sync::Arc<std::sync::atomic::AtomicU8>,
    finished: bool,
}

/// `--dump-frames`: once every texture has loaded, each simulation frame is
/// saved as DIR/NNNNN.png until `--at`. A frame is captured, checked and
/// saved before the simulation steps on (`step_sim` waits for `step`); a
/// blank capture (the window not presented: covered, on another space) is
/// taken again, so no frame of the video is lost or black.
#[allow(clippy::too_many_arguments)]
fn dump_frames(mut commands: Commands, args: Res<Args>, sim: NonSend<Sim>, mut st: ResMut<DumpState>, assets: Res<AssetServer>, cache: Res<ss_port::render::RenderCache>, target: Option<Res<DumpTarget>>, mut exit: MessageWriter<AppExit>) {
    use std::sync::atomic::Ordering;
    let Some(dir) = args.dump_frames.clone() else { return };
    if st.finished {
        exit.write(AppExit::Success);
        return;
    }
    if !st.started {
        if !cache.textures_loaded(&assets) {
            return;
        }
        st.waited += 1;
        if st.waited < 60 {
            return;
        }
        let _ = std::fs::create_dir_all(&dir);
        st.started = true;
    }
    if st.capturing {
        match st.status.load(Ordering::SeqCst) {
            0 => return,
            1 => {
                st.capturing = false;
                st.retries = 0;
                st.index += 1;
                if sim.game.frame >= args.at || sim.finished {
                    st.finished = true;
                } else {
                    st.step = true;
                }
                return;
            }
            _ => {
                // blank: this frame again
                st.capturing = false;
                st.retries += 1;
                if st.retries % 120 == 0 {
                    warn!("[Dump] frame {}: the window shows nothing (hidden?), waiting", st.index);
                }
            }
        }
    }
    // the frame on screen (stepped by step_sim, or the first one)
    if st.step {
        return;
    }
    let status = st.status.clone();
    status.store(0, Ordering::SeqCst);
    let mut save = save_to_disk(dir.join(format!("{:05}.png", st.index)));
    let shot = match &target {
        Some(t) => Screenshot(bevy::camera::RenderTarget::Image(t.0.clone())),
        None => Screenshot::primary_window(),
    };
    commands.spawn(shot).observe(move |shot: On<ScreenshotCaptured>| {
        let blank = shot.image.data.as_ref().is_none_or(|d| d.chunks(4).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
        if blank {
            status.store(2, Ordering::SeqCst);
        } else {
            save(shot);
            status.store(1, Ordering::SeqCst);
        }
    });
    st.capturing = true;
}

#[derive(Resource, Default)]
struct ShotState {
    waited: u32,
    /// A capture is in flight.
    taken: bool,
    after: u32,
    attempts: u32,
    /// Capture outcome: 0 pending, 1 saved, 2 blank (retry).
    status: std::sync::Arc<std::sync::atomic::AtomicU8>,
}

#[derive(Component)]
struct Hud;

/// Interactive play opens on the title screen; replays and screenshots start
/// the run at once (as the oracle traces do).
fn interactive(args: &Args) -> bool {
    args.replay.is_none() && args.screenshot.is_none()
}

fn build_sim(paths: &DataPaths, args: &Args) -> (Sim, KeyQueue) {
    let (mut sim, q) = build_sim_at(paths, args, !interactive(args) && args.menu.is_none());
    if let Some(m) = &args.menu {
        use ss_port::menu::{self, MeTab, Menu, Overlay};
        let g = &mut sim.game;
        g.flow.menu = Menu::Title;
        match m.as_str() {
            "me" => menu::open_me(g),
            // `me:<id>[:<outfit>]`: the Characters tab focused on that
            // character's card (and outfit)
            v if v.starts_with("me:") => {
                menu::open_me(g);
                let (id, outfit) = match v[3..].split_once(':') {
                    Some((id, k)) => (id, k.parse().ok()),
                    None => (&v[3..], None),
                };
                if let Some(i) = menu::characters().iter().position(|c| c.eq_ignore_ascii_case(id)) {
                    menu::focus(g, i);
                }
                if let Some(k) = outfit {
                    menu::feature(g, k);
                }
            }
            "me-awards" => {
                menu::open_me(g);
                ss_port::flow::click(g, ss_port::flow::Click::MeAwards);
            }
            "me-boards" => {
                menu::open_me(g);
                menu::set_tab(g, MeTab::Boards);
            }
            "shop" => menu::open_shop(g),
            "results" => {
                g.flow.menu = Menu::None;
                g.flow.screen = ss_port::flow::Screen::Results { score: 12345, coins: 120, play_due_ms: 0.0, play_ready: true, frames: 0, doubled: false, list: ss_port::flow::Scroller::leaderboard(true) };
            }
            // the pause panel (and Settings over it)
            "pause" | "pause-settings" => {
                g.flow.menu = Menu::None;
                g.flow.screen = ss_port::flow::Screen::Paused;
                if m == "pause-settings" {
                    g.flow.overlay = Some(Overlay::Settings { prompt: false });
                }
            }
            "mytour" => g.flow.menu = Menu::MyTour { missions: false },
            "mytour-missions" => g.flow.menu = Menu::MyTour { missions: true },
            "settings" => g.flow.overlay = Some(Overlay::Settings { prompt: false }),
            "settings-prompt" => {
                g.flow.nickname_input = g.flow.user.name.clone();
                g.flow.overlay = Some(Overlay::Settings { prompt: true });
            }
            "highscore" => {
                g.flow.menu = Menu::None;
                g.flow.screen = ss_port::flow::Screen::HighScore { alpha: -5.0, frames: 0 };
                ss_port::celebration::open(g);
            }
            "boosts" => g.flow.overlay = Some(Overlay::Boosts { open: Some(5), scroll: 0.0 }),
            "buyboards" => g.flow.overlay = Some(Overlay::BuyBoards { bump: 0 }),
            // `prize-open:<kind>`: that prize (coins, keys, hoverboard, ...)
            v if v == "prize" || v.starts_with("prize-open") => {
                let mut p = ss_port::prizes::roll(g, "mystery-box");
                if let Some(kind) = v.strip_prefix("prize-open:") {
                    p.kind = kind.to_string();
                }
                let mut ps = ss_port::flow::PrizeScreen::open(vec![p]).unwrap();
                if v.starts_with("prize-open") {
                    ss_port::flow::prize_press(g, &mut ps);
                }
                g.flow.overlay = Some(Overlay::Prize(ps));
            }
            _ => {}
        }
    }
    (sim, q)
}

fn build_sim_at(paths: &DataPaths, args: &Args, start: bool) -> (Sim, KeyQueue) {
    let keys = KeyQueue::default();
    let mut replay_trace = None;
    let (seed, letter, driver): (i32, Option<String>, Box<dyn Driver>) = match &args.replay {
        Some(path) => {
            let trace = Rc::new(Trace::load(path).expect("load trace"));
            let seed = trace.meta["seed"].as_i64().unwrap_or(1) as i32;
            replay_trace = Some(trace.clone());
            (seed, trace_hunt_letter(&trace), Box::new(ReplayDriver::new(trace)))
        }
        None => (args.seed, args.letter.clone(), Box::new(keys.clone())),
    };
    let mut sim = if start { Sim::new(paths, seed, letter, driver) } else { Sim::new_title(paths, seed, letter, driver) }.expect("sim");
    // `--tutorial`: run the tutorial route (screenshot runs; play follows the save)
    if std::env::args().any(|a| a == "--tutorial") && args.replay.is_none() {
        sim.game.tutorial_allowed = true;
        sim.game.tutorial_enabled = true;
    }
    sim.game.god = args.god;
    // a replay runs with the trace's own overrides (god, forcePickup)
    if let Some(t) = &replay_trace {
        sim.configure_from_trace(t);
    }
    // the player's currencies and high score persist in interactive play
    if args.replay.is_none() && args.screenshot.is_none() {
        let save = Install::locate().save_file();
        sim.game.flow.user = ss_port::flow::UserData::load(&save);
        sim.game.flow.save_path = Some(save);
        // the missions' saved progress (`prepareMissions`), tracked from the
        // start; `stats.missionMultiplier` as the stats' reset reads it
        sim.game.missions = ss_port::missions::Missions::prepare(&sim.game.flow.user);
        ss_port::missions::init_tracking(&mut sim.game);
        sim.game.mission_multiplier = sim.game.missions.multiplier() as f64;
        // the tutorial runs until the save says it is done
        sim.game.tutorial_allowed = true;
        sim.game.tutorial_enabled = !sim.game.flow.user.tutorial;
        // today's Word Hunt (Da() at start-up), unless --letter pins a letter
        if !std::env::args().any(|a| a == "--letter") {
            let hunt = ss_port::word_hunt::init(&mut sim.game.flow.user, ss_port::word_hunt::Day::today());
            sim.game.flow.save_user();
            ss_port::word_hunt::attach(&mut sim.game, hunt);
        }
    }
    (sim, keys)
}

fn main() {
    let inst = Install::locate();
    let args = parse_args(&inst);
    let mut themes = ThemeRegistry::discover(&inst.port.join("data"));
    if let Some(id) = &args.theme {
        if let Err(e) = themes.select(id) {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
    // the original game's files: a clear message, not a crash, when the
    // folder is missing (moved or deleted)
    if !inst.site.join("assets/boards").is_dir() || !inst.site.join("bundles").is_dir() {
        eprintln!(
            "The game files are missing: expected the original web build's files in\n  {}\n\
             Keep that folder (`oracle/site` next to `ss_port`, or `site` next to the\n\
             executable) together with the game.",
            inst.site.display()
        );
        std::process::exit(2);
    }
    let paths = DataPaths::installed(&inst, &themes.active);
    let (mut sim, keys) = build_sim(&paths, &args);
    if args.screenshot.is_some() && !args.collect_letters.is_empty() {
        let hunt = ss_port::word_hunt::init(&mut sim.game.flow.user, ss_port::word_hunt::Day::today());
        ss_port::word_hunt::attach(&mut sim.game, hunt);
    }
    if args.screenshot.is_some() {
        // a dump fast-forwards to its first frame, a screenshot to its only one
        let target = if args.dump_frames.is_some() { args.from } else { args.at };
        loop {
            if sim.next_frame >= target {
                break;
            }
            inject(&args, &mut sim, &keys);
            if !sim.step() {
                break;
            }
        }
    }

    // the drawn character: --character, else the saved selection
    let character = args.character.clone().unwrap_or_else(|| {
        let u = &sim.game.flow.user;
        let mut c = ActiveCharacter::from_name(&u.selected_character).unwrap_or_default();
        c.outfit = u.selected_outfit;
        c
    });
    let theme_id = themes.active.clone();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Subway Surfers".into(),
                    // a frame dump draws off screen: no window to show
                    visible: args.dump_frames.is_none(),
                    // screenshots at the original's pixel ratio 1 (960x540
                    // pixels, comparable with oracle/screenshot.mjs)
                    resolution: if args.screenshot.is_some() {
                        if args.hd { bevy::window::WindowResolution::new(1920, 1080).with_scale_factor_override(2.0) } else { bevy::window::WindowResolution::new(960, 540).with_scale_factor_override(1.0) }
                    } else {
                        (960, 540).into()
                    },
                    // uncapped in play: the simulation stays a fixed 60 Hz
                    // accumulator, render frames in between interpolate
                    present_mode: if args.vsync || args.screenshot.is_some() {
                        bevy::window::PresentMode::AutoVsync
                    } else {
                        bevy::window::PresentMode::AutoNoVsync
                    },
                    // Metal (windowed) holds an uncapped surface at the
                    // display rate with the default latency 2; 1 lets it run free
                    desired_maximum_frame_latency: if args.vsync || args.screenshot.is_some() {
                        None
                    } else {
                        std::num::NonZeroU32::new(1)
                    },
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin { file_path: paths.site.to_string_lossy().into_owned(), ..default() }),
    )
    .insert_resource(args.clone())
    .insert_resource(themes)
    .insert_resource(character)
    .init_resource::<ShotState>()
    .init_resource::<DumpState>()
    .init_resource::<Clock>()
    .insert_non_send(sim)
    .insert_non_send(Keys(keys))
    .insert_non_send(paths)
    .add_plugins((SceneRenderPlugin, ss_port::ui::UiPlugin, ss_port::render_menu_3d::MenuStagePlugin, ss_port::render_fx::FxRenderPlugin, bevy::diagnostic::FrameTimeDiagnosticsPlugin::default()))
    .add_systems(Startup, setup)
    .add_systems(Update, window_icon)
    .add_systems(Update, (read_keys, step_sim, ss_port::render_actors::sync_selected_character, follow_camera, hud, screenshot, dump_frames).chain());
    // sound in play only (screenshot and frame-dump runs stay silent)
    if app.world().resource::<Args>().screenshot.is_none() {
        app.add_plugins(ss_port::audio::AudioPlugin { theme: theme_id, mute_unfocused: app.world().resource::<Args>().pause_on_blur });
        // keep updating at full rate in the background (Alt-Tab)
        app.insert_resource(bevy::winit::WinitSettings { focused_mode: bevy::winit::UpdateMode::Continuous, unfocused_mode: bevy::winit::UpdateMode::Continuous });
    }
    app.run();
}

/// A frame dump's render target: the game (and its UI) drawn off screen,
/// so a covered or hidden window cannot blank a frame.
#[derive(Resource)]
struct DumpTarget(bevy::camera::ImageRenderTarget);

fn setup(mut commands: Commands, sim: NonSend<Sim>, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    // three PerspectiveCamera (fov from the rig, near 3, far 1200) + the canvas pipeline
    // (the game's UI draws through this camera, also into a frame dump's
    // off-screen image, where no camera renders to the window)
    let cam = commands.spawn((ss_port::render::camera_bundle(&sim.game.theme, args.aa), Transform::default(), IsDefaultUiCamera)).id();
    if args.dump_frames.is_some() {
        // the window's own size and format (the 960 x 540 layout at its scale)
        let (w, h, scale) = if args.hd { (1920, 1080, 2.0) } else { (960, 540, 1.0) };
        let img = Image::new_target_texture(w, h, bevy::render::render_resource::TextureFormat::Bgra8UnormSrgb, None);
        let target = bevy::camera::ImageRenderTarget { handle: images.add(img), scale_factor: scale };
        commands.entity(cam).insert(bevy::camera::RenderTarget::Image(target.clone()));
        commands.insert_resource(DumpTarget(target));
    }
    commands.spawn((
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(18.0), ..default() },
        // a debug status line at the bottom (the game's own HUD is ss_port::ui)
        Node { position_type: PositionType::Absolute, bottom: Val::Px(6.0), left: Val::Px(10.0), ..default() },
        TextColor(Color::LinearRgba(LinearRgba::new(1.0, 1.0, 1.0, 0.8))),
        Hud,
    ));
}

/// The window icon (assets/icon/icon.png), set once the OS window exists.
fn window_icon(windows: Query<Entity, With<bevy::window::PrimaryWindow>>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Ok(e) = windows.single() else { return };
    let path = Install::locate().port.join("assets/icon/icon.png");
    let Ok(bytes) = std::fs::read(&path) else {
        *done = true;
        return;
    };
    let Ok(img) = bevy::image::Image::from_buffer(
        &bytes,
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        true,
        bevy::image::ImageSampler::Default,
        bevy::asset::RenderAssetUsages::MAIN_WORLD,
    ) else {
        *done = true;
        return;
    };
    let (w, h) = (img.width(), img.height());
    let Some(rgba) = img.data.clone() else { return };
    let Ok(icon) = winit::window::Icon::from_rgba(rgba, w, h) else {
        *done = true;
        return;
    };
    bevy::winit::WINIT_WINDOWS.with_borrow(|ws| {
        if let Some(win) = ws.get_window(e) {
            win.set_window_icon(Some(icon));
            *done = true;
        }
    });
}

fn read_keys(
    mut focus: MessageReader<bevy::window::WindowFocused>,
    input: Res<ButtonInput<KeyCode>>,
    mut keys: NonSendMut<Keys>,
    mut sim: NonSendMut<Sim>,
    args: Res<Args>,
    mut paths: NonSendMut<DataPaths>,
    mut themes: ResMut<ThemeRegistry>,
) {
    // Game.onBlur: with --pause-on-blur, interactive play pauses when the
    // window loses focus (by default it plays on: Alt-Tab does not pause)
    if args.pause_on_blur && args.replay.is_none() && args.screenshot.is_none() {
        for f in focus.read() {
            if !f.focused {
                ss_port::flow::blur(&mut sim.game);
            }
        }
    }
    // the nickname prompt has the keyboard
    if ss_port::flow::typing(&sim.game) {
        return;
    }
    if input.just_pressed(KeyCode::KeyT) {
        let next = themes.next();
        if next == themes.active {
            info!("[Theme] Currently active: {} ({} theme loaded)", themes.active, themes.available.len());
            return;
        }
        // switching city = a new run in that city (new library, chunks, maps)
        themes.active = next;
        *paths = DataPaths::installed(&Install::locate(), &themes.active);
        let (fresh, q) = build_sim(&paths, &args);
        info!("[Theme] Switched to {}", fresh.game.theme.display_name);
        *sim = fresh;
        keys.0 = q;
        return;
    }
    if input.just_pressed(KeyCode::KeyR) {
        // a new run, reset in place (nav.toGame)
        sim.game.to_game();
        return;
    }
    if args.replay.is_some() {
        return;
    }
    let map = [
        (KeyCode::ArrowUp, Key::Up),
        (KeyCode::KeyW, Key::Up),
        (KeyCode::ArrowDown, Key::Down),
        (KeyCode::KeyS, Key::Down),
        (KeyCode::ArrowLeft, Key::Left),
        (KeyCode::KeyA, Key::Left),
        (KeyCode::ArrowRight, Key::Right),
        (KeyCode::KeyD, Key::Right),
        (KeyCode::Space, Key::Action),
        (KeyCode::Escape, Key::Pause),
        (KeyCode::KeyV, Key::Headstart),
        (KeyCode::KeyK, Key::ReviveKeys),
    ];
    // C: the score booster while its button shows (`Wm`'s key)
    if input.just_pressed(KeyCode::KeyC) && booster_showing(&sim) {
        keys.0 .0.borrow_mut().push(Key::ScoreBooster);
    }
    for (code, key) in map {
        if input.just_pressed(code) {
            keys.0 .0.borrow_mut().push(key);
        }
    }
}

fn booster_showing(sim: &Sim) -> bool {
    sim.game.hud.boosts.iter().any(|b| b.kind == ss_port::boosts::BoostKind::Multiplier && !b.leaving)
}

/// Fixed 60 Hz game frames (the simulation's own clock is virtual).
/// The scripted inputs of a screenshot / frame-dump run for the next frame:
/// letters, clicks, mission stats, chunks, key presses.
fn inject(args: &Args, sim: &mut Sim, keys: &KeyQueue) {
    for (name, f) in &args.powers {
        if *f == sim.next_frame {
            let g = &mut sim.game;
            match name.as_str() {
                "jetpack" => ss_port::powerups::jetpack_turn_on(g, 0),
                "magnet" => ss_port::powerups::magnet_turn_on(g),
                "sneakers" => ss_port::powerups::sneakers_turn_on(g),
                "multiplier" => ss_port::powerups::multiplier_turn_on(g),
                "hoverboard" => ss_port::powerups::hoverboard_turn_on(g),
                _ => {}
            }
        }
    }
    if args.collect_letters.contains(&sim.next_frame) {
        ss_port::word_hunt::collect(&mut sim.game);
    }
    for (name, f) in &args.clicks {
        if *f == sim.next_frame {
            use ss_port::flow::Click;
            let c = match name.as_str() {
                "pause-settings" => Some(Click::PauseSettings),
                "close" => Some(Click::CloseOverlay),
                "resume" => Some(Click::PauseResume),
                _ => None,
            };
            if let Some(c) = c {
                ss_port::flow::click(&mut sim.game, c);
            }
        }
    }
    for (id, n, f) in &args.mission_stats {
        if let Some(&k) = ss_port::missions::STATS.iter().find(|&&k| k == id) {
            if *f == sim.next_frame {
                ss_port::missions::add_stat(&mut sim.game, *n, k);
            }
        }
    }
    for (n, f) in &args.chunks {
        if *f == sim.next_frame && sim.game.chunks_data.chunk(n).is_some() {
            sim.game.queue_chunk(Some(n));
        }
    }
    for (k, f) in &args.press {
        if *f == sim.next_frame {
            keys.0.borrow_mut().push(*k);
        }
    }
}

/// A fresh game of `seed`, its run started (the user's progress saved
/// first and loaded again).
fn rebuild_with_seed(sim: &mut Sim, keys: &mut Keys, paths: &DataPaths, args: &mut Args, seed: i32) {
    sim.game.flow.save_user();
    args.seed = seed;
    let (fresh, q) = build_sim_at(paths, args, true);
    *sim = fresh;
    keys.0 = q;
}

#[allow(clippy::too_many_arguments)]
fn step_sim(
    mut sim: NonSendMut<Sim>,
    mut args: ResMut<Args>,
    time: Res<Time>,
    mut clock: ResMut<Clock>,
    paths: NonSend<DataPaths>,
    mut keys: NonSendMut<Keys>,
    mut blend: ResMut<ss_port::render::RenderBlend>,
    mut dump: ResMut<DumpState>,
) {
    // (results PLAY / Menu and the pause panel reset the game in place)
    let _ = &paths;
    // (the title's PLAY starts the run in the next frame: Game::step)
    if args.screenshot.is_some() {
        // a dump steps one frame each time the last one is saved
        let go = match args.dump_frames {
            None => true,
            Some(_) => std::mem::take(&mut dump.step),
        };
        if go && sim.next_frame <= args.at && !sim.finished {
            inject(&args, &mut sim, &keys.0);
            sim.step();
        }
        return;
    }
    // --lock-seed: a second run start (R, PLAY, restart) is a fresh boot of
    // the seed (the first run of a boot has the seed's layout, however long
    // the title showed)
    if args.lock_seed && args.replay.is_none() && sim.game.run_starts >= 2 {
        let seed = args.seed;
        rebuild_with_seed(&mut sim, &mut keys, &paths, &mut args, seed);
        info!("[Seed] Restart with seed {seed}");
    }
    clock.acc = (clock.acc + time.delta_secs()).min(0.25);
    while clock.acc >= 1.0 / 60.0 {
        clock.acc -= 1.0 / 60.0;
        if !sim.finished {
            sim.step();
        }
    }
    // how far into the next 60 Hz step this render frame is
    blend.alpha = if args.smooth { (clock.acc * 60.0).clamp(0.0, 1.0) } else { 1.0 };
}

/// The camera pose: the rig of the last two simulation frames, blended.
fn follow_camera(
    sim: NonSend<Sim>,
    blend: Res<ss_port::render::RenderBlend>,
    mut poses: Local<Option<(i64, u64, Transform, Transform)>>,
    mut cam: Query<(&mut Transform, &mut Projection), With<ss_port::render::MainWorldCamera>>,
) {
    let rig = &sim.game.camera.rig;
    let (p, r) = rig.pose();
    let cur = Transform { translation: p.as_vec3(), rotation: r.as_quat(), ..default() };
    let (frame, serial) = (sim.game.frame, sim.serial);
    let next = match *poses {
        Some((f, s, _, c)) if f == frame && s == serial => (f, s, poses.unwrap().2, c),
        Some((_, s, _, c)) if s == serial => (frame, serial, c, cur),
        _ => (frame, serial, cur, cur),
    };
    *poses = Some(next);
    if let Ok((mut t, mut proj)) = cam.single_mut() {
        *t = ss_port::render::blend_tf(&next.2, &next.3, blend.alpha);
        if let Projection::Perspective(pp) = &mut *proj {
            pp.fov = (rig.fov as f32).to_radians();
        }
    }
}

fn hud(
    sim: NonSend<Sim>,
    args: Res<Args>,
    diags: Res<bevy::diagnostic::DiagnosticsStore>,
    time: Res<Time>,
    mut fps_shown: Local<(f32, f64)>,
    mut q: Query<&mut Text, With<Hud>>,
) {
    // FPS, refreshed every 0.25 s
    fps_shown.0 += time.delta_secs();
    if fps_shown.0 >= 0.25 {
        fps_shown.0 = 0.0;
        fps_shown.1 = diags.get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
        if std::env::var_os("SS_FPS_LOG").is_some() {
            println!("fps {:.1} frame {}", fps_shown.1, sim.game.frame);
        }
    }
    let g = &sim.game;
    if let Ok(mut t) = q.single_mut() {
        let mode = if args.replay.is_some() { "replay" } else if g.god { "god mode" } else { "mortal" };
        if args.screenshot.is_some() || !args.debug_hud {
            t.0.clear();
            return;
        }
        let over = if g.hero.player.dead { "   R: new run" } else { "" };
        t.0 = format!(
            "FPS {:.0}  frame {}  {}  coins {}  distance {:.0}  speed {:.2}  [{}]  {}{}",
            fps_shown.1,
            g.frame,
            g.hero.fsm.current,
            g.coins,
            g.distance(),
            g.stats_speed(),
            mode,
            if g.state == ss_port::game::GameState::Idle { "intro…" } else { "" },
            over
        );
    }
}

fn screenshot(
    mut commands: Commands,
    args: Res<Args>,
    sim: NonSend<Sim>,
    mut st: ResMut<ShotState>,
    assets: Res<AssetServer>,
    cache: Res<ss_port::render::RenderCache>,
    mut exit: MessageWriter<AppExit>,
) {
    use std::sync::atomic::Ordering;
    let Some(path) = args.screenshot.clone() else { return };
    if args.dump_frames.is_some() {
        return;
    }
    if sim.game.frame < args.at {
        return;
    }
    if st.taken {
        match st.status.load(Ordering::SeqCst) {
            // saved: let the write finish, then exit
            1 => {
                st.after += 1;
                if st.after > 30 {
                    exit.write(AppExit::Success);
                }
            }
            // the window had not presented a frame yet (all black): retry
            2 => {
                st.taken = false;
                st.waited = 40;
                st.status.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
        return;
    }
    // wait until every texture finished loading, then a few frames more
    if !cache.textures_loaded(&assets) {
        return;
    }
    st.waited += 1;
    if st.waited < 60 {
        return;
    }
    st.attempts += 1;
    if st.attempts > 10 {
        error!("screenshot: every capture came back blank");
        exit.write(AppExit::error());
        return;
    }
    let status = st.status.clone();
    let mut save = save_to_disk(path);
    commands.spawn(Screenshot::primary_window()).observe(move |shot: On<ScreenshotCaptured>| {
        let blank = shot.image.data.as_ref().is_none_or(|d| d.chunks(4).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
        if blank {
            status.store(2, Ordering::SeqCst);
        } else {
            save(shot);
            status.store(1, Ordering::SeqCst);
        }
    });
    st.taken = true;
}
