//! Sound: the original's `qb` (`$.sound` / `game.sfx`, deobfuscated.js
//! 71979) over the SoundBoy mixer `Kb` (channels sfx / music, 67980).
//!
//! The simulation only queues commands ([`play`], [`stop`], [`music_fade`],
//! ...) in [`Sound`]; the Bevy side ([`AudioPlugin`]) plays them. Nothing
//! here reads or advances an RNG stream or changes the simulation, so the
//! replays are unaffected.
//!
//! The original's rules:
//!
//! * the 28 items of the asset manifest (27 effects in `assets/audio-basic`,
//!   `audio-full`, `audio-idle`, and the city bundle's `theme`), each at
//!   volume 1 unless the call gives one; `loops` / `volume` / `rate` options;
//! * the master volume is `config.volume` 0.25 (`sfx.volume` at a run's start);
//! * muted (`gameSettings.muted`, Settings / the title's sound button):
//!   `play` starts nothing and the master volume is 0 (the music keeps
//!   running silently);
//! * a hidden page sets `systemMuted` (the port: an unfocused window, only
//!   with `--pause-on-blur`; by default the game plays on in the background);
//! * the theme starts with the first run (`playTheme`, once) and loops; the
//!   music channel fades to 0 in 0.5 s (`musicFadeOut`: New High Score, a
//!   prize opening) and back to 1 in 1 s (`musicFadeIn`: their close),
//!   linearly;
//! * `stopAllFx` stops every effect (New High Score, a prize opening).

use crate::game::Game;

/// The master volume (`config.volume`, measured in the running original).
pub const MASTER: f32 = 0.25;

/// One command from the simulation.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// `sfx.play(id, {volume, rate, loops})`, after `delay_ms` (the
    /// original's `setTimeout`s).
    Play { id: &'static str, volume: f32, rate: f32, looped: bool, delay_ms: f32 },
    /// `sfx.stop(id)`: every playing instance.
    Stop(&'static str),
    /// `stopAllFx`.
    StopAllFx,
    /// `playTheme` (the first time only).
    Theme,
    /// `musicChannel.fadeTo(volume, secs)`.
    MusicFade { to: f32, secs: f32 },
}

/// The queue (drained by the app every frame; bounded for headless runs).
#[derive(Clone, Debug, Default)]
pub struct Sound {
    pub cmds: Vec<Cmd>,
    /// `playingTheme`.
    theme_started: bool,
}

const MAX_QUEUE: usize = 256;

fn push(g: &mut Game, c: Cmd) {
    let q = &mut g.sound.cmds;
    if q.len() >= MAX_QUEUE {
        q.remove(0);
    }
    q.push(c);
}

/// `sfx.play(id)`.
pub fn play(g: &mut Game, id: &'static str) {
    play_with(g, id, 1.0, 1.0, false);
}

/// `sfx.play(id, {volume, rate, loops})`.
pub fn play_with(g: &mut Game, id: &'static str, volume: f32, rate: f32, looped: bool) {
    // `qb.play`: nothing while muted
    if g.flow.user.muted {
        return;
    }
    push(g, Cmd::Play { id, volume, rate, looped, delay_ms: 0.0 });
}

/// `setTimeout(() => sfx.play(id), ms)`.
pub fn play_later(g: &mut Game, id: &'static str, ms: f32) {
    if g.flow.user.muted {
        return;
    }
    push(g, Cmd::Play { id, volume: 1.0, rate: 1.0, looped: false, delay_ms: ms });
}

pub fn stop(g: &mut Game, id: &'static str) {
    push(g, Cmd::Stop(id));
}

pub fn stop_all_fx(g: &mut Game) {
    push(g, Cmd::StopAllFx);
}

/// `Game.playTheme` (50252): once.
pub fn play_theme(g: &mut Game) {
    if !g.sound.theme_started {
        g.sound.theme_started = true;
        push(g, Cmd::Theme);
    }
}

/// `musicFadeOut` (0 in 0.5 s) / `musicFadeIn` (1 in 1 s).
pub fn music_fade_out(g: &mut Game) {
    push(g, Cmd::MusicFade { to: 0.0, secs: 0.5 });
}
pub fn music_fade_in(g: &mut Game) {
    push(g, Cmd::MusicFade { to: 1.0, secs: 1.0 });
}

/// `pickup-coin` at volume 0.5, rate `1 + arc * 0.05` (`arc`: the coin's
/// place on a jump curve, 0 elsewhere).
pub fn coin(g: &mut Game, arc: u32) {
    play_with(g, "pickup-coin", 0.5, 1.0 + arc as f32 * 0.05, false);
}

/// The asset path of a sound (relative to the site).
pub fn path(id: &str, theme: &str) -> Option<String> {
    const BASIC: [&str; 14] = [
        "guard-catch",
        "guard-proximity",
        "guard-start",
        "hero-death-hitcam",
        "hero-death",
        "hero-dodge",
        "hero-foot-l",
        "hero-foot-r",
        "hero-hoverboard-crash",
        "hero-jump",
        "hero-revive",
        "hero-roll",
        "hero-stumble",
        "unlock",
    ];
    const FULL: [&str; 9] = [
        "hero-sneakers-foot-l",
        "hero-sneakers-foot-r",
        "hero-sneakers-jump",
        "pickup-coin",
        "pickup-powerdown",
        "pickup-powerup",
        "special-jetpack-start",
        "special-jetpack",
        "special-magnet",
    ];
    const IDLE: [&str; 4] = ["gui-coin", "gui-tap", "mission-notification", "open-prize"];
    if id == "theme" {
        return Some(format!("bundles/{theme}/audio-basic/theme.ogg"));
    }
    let dir = if BASIC.contains(&id) {
        "audio-basic"
    } else if FULL.contains(&id) {
        "audio-full"
    } else if IDLE.contains(&id) {
        "audio-idle"
    } else {
        return None;
    };
    Some(format!("assets/{dir}/{id}.ogg"))
}

// ---- the app side ------------------------------------------------------------------------------

use crate::sim_plugin::Sim;
use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use std::collections::HashMap;

/// A playing sound: its id, own volume, and whether it is the music.
#[derive(Component)]
pub struct Playing {
    pub id: &'static str,
    pub volume: f32,
    pub music: bool,
}

#[derive(Resource)]
pub struct Mixer {
    /// The city bundle of the theme.
    pub theme: String,
    handles: HashMap<&'static str, Handle<AudioSource>>,
    /// The music channel's volume and its running fade (from, to, t, secs).
    music: f32,
    fade: Option<(f32, f32, f32, f32)>,
    /// `systemMuted`: the window is not focused.
    unfocused: bool,
    mute_unfocused: bool,
    /// Delayed plays: seconds left, the command.
    pending: Vec<(f32, Cmd)>,
}

impl Mixer {
    pub fn new(theme: &str) -> Self {
        Self { theme: theme.to_string(), handles: HashMap::new(), music: 1.0, fade: None, unfocused: false, mute_unfocused: false, pending: Vec::new() }
    }
}

pub struct AudioPlugin {
    pub theme: String,
    /// `systemMuted` while the window is unfocused (`--pause-on-blur`).
    pub mute_unfocused: bool,
}

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        let mut mixer = Mixer::new(&self.theme);
        mixer.mute_unfocused = self.mute_unfocused;
        app.insert_resource(mixer).add_systems(PostUpdate, mix);
    }
}

fn handle(mixer: &mut Mixer, assets: &AssetServer, id: &'static str) -> Option<Handle<AudioSource>> {
    if let Some(h) = mixer.handles.get(id) {
        return Some(h.clone());
    }
    let p = path(id, &mixer.theme)?;
    let h: Handle<AudioSource> = assets.load(p);
    mixer.handles.insert(id, h.clone());
    Some(h)
}

#[allow(clippy::too_many_arguments)]
fn mix(
    mut commands: Commands,
    sim: Option<NonSendMut<Sim>>,
    mut mixer: ResMut<Mixer>,
    assets: Res<AssetServer>,
    time: Res<Time>,
    mut focus: MessageReader<bevy::window::WindowFocused>,
    playing: Query<(Entity, &Playing)>,
    mut sinks: Query<(&Playing, &mut AudioSink)>,
) {
    let Some(mut sim) = sim else { return };
    for f in focus.read() {
        mixer.unfocused = !f.focused && mixer.mute_unfocused;
    }
    let muted = sim.game.flow.user.muted;
    let master = if muted || mixer.unfocused { 0.0 } else { MASTER };
    let dt = time.delta_secs();
    // the music fade (linear)
    if let Some((from, to, t, secs)) = mixer.fade {
        let t = t + dt;
        let k = if secs > 0.0 { (t / secs).min(1.0) } else { 1.0 };
        mixer.music = from + (to - from) * k;
        mixer.fade = if k >= 1.0 { None } else { Some((from, to, t, secs)) };
    }
    let mut cmds: Vec<Cmd> = std::mem::take(&mut sim.game.sound.cmds);
    // delayed plays that are due
    let mut due = Vec::new();
    mixer.pending.retain_mut(|(left, c)| {
        *left -= dt;
        if *left <= 0.0 {
            due.push(c.clone());
            false
        } else {
            true
        }
    });
    cmds.splice(0..0, due);
    for c in cmds {
        match c {
            Cmd::Play { id, volume, rate, looped, delay_ms } if delay_ms > 0.0 => {
                mixer.pending.push((delay_ms / 1000.0, Cmd::Play { id, volume, rate, looped, delay_ms: 0.0 }));
            }
            Cmd::Play { id, volume, rate, looped, .. } => {
                let Some(h) = handle(&mut mixer, &assets, id) else {
                    warn!("[Sound] Sound item not found: {id}");
                    continue;
                };
                let mode = if looped { PlaybackMode::Loop } else { PlaybackMode::Despawn };
                let settings = PlaybackSettings { mode, volume: Volume::Linear(volume * master), speed: rate, ..PlaybackSettings::ONCE };
                commands.spawn((AudioPlayer::new(h), settings, Playing { id, volume, music: false }));
            }
            Cmd::Stop(id) => {
                for (e, p) in &playing {
                    if !p.music && p.id == id {
                        commands.entity(e).despawn();
                    }
                }
                mixer.pending.retain(|(_, c)| !matches!(c, Cmd::Play { id: i, .. } if *i == id));
            }
            Cmd::StopAllFx => {
                for (e, p) in &playing {
                    if !p.music {
                        commands.entity(e).despawn();
                    }
                }
                mixer.pending.clear();
            }
            Cmd::Theme => {
                // one loop: a new game (a theme or seed rebuild) asks
                // again while the music plays on
                if playing.iter().any(|(_, p)| p.id == "theme") {
                    continue;
                }
                if let Some(h) = handle(&mut mixer, &assets, "theme") {
                    let settings = PlaybackSettings { mode: PlaybackMode::Loop, volume: Volume::Linear(mixer.music * master), ..PlaybackSettings::LOOP };
                    commands.spawn((AudioPlayer::new(h), settings, Playing { id: "theme", volume: 1.0, music: true }));
                }
            }
            Cmd::MusicFade { to, secs } => {
                let from = mixer.music;
                mixer.fade = Some((from, to, 0.0, secs));
            }
        }
    }
    // every sink follows the master volume, the music its channel
    for (p, mut sink) in &mut sinks {
        let v = p.volume * master * if p.music { mixer.music } else { 1.0 };
        sink.set_volume(Volume::Linear(v));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_has_a_file() {
        let site = crate::install::Install::locate().site;
        let ids = [
            "guard-catch", "guard-proximity", "guard-start", "hero-death-hitcam", "hero-death", "hero-dodge", "hero-foot-l", "hero-foot-r",
            "hero-hoverboard-crash", "hero-jump", "hero-revive", "hero-roll", "hero-stumble", "unlock", "hero-sneakers-foot-l",
            "hero-sneakers-foot-r", "hero-sneakers-jump", "pickup-coin", "pickup-powerdown", "pickup-powerup", "special-jetpack-start",
            "special-jetpack", "special-magnet", "gui-coin", "gui-tap", "mission-notification", "open-prize", "theme",
        ];
        assert_eq!(ids.len(), 28);
        for id in ids {
            let p = site.join(path(id, "bali").unwrap());
            assert!(p.is_file(), "{}", p.display());
        }
        assert_eq!(path("nope", "bali"), None);
    }
}
