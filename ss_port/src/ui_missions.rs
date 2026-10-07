//! Drawing the missions (`crate::missions`) and the screens around them:
//! the pause panel's missions section `fh` (46317), the My Tour panel `My`
//! (64029) with its missions section `Ty` / `Sy` (63263), the mission
//! notification `Py` (64217), Settings `cb` (65835) with the nickname prompt
//! `rb`, and the title's settings / sound / My Tour buttons (66182).
//! Positions are the original's units relative to the screen centre unless
//! noted.

use crate::flow::Click;
use crate::game::Game;
use crate::missions::{Mission, Toast};
use crate::ui::{backdrop_blur, measure, raw, rect_comp, Builder, UiFont, WHITE};
use crate::ui_menu::{am_button, currencies, notepad, word_hunt_section};
use bevy::prelude::*;

const NAVY: u32 = 0x07294A;
const SET_BLUE: u32 = 0x3A7FBF;

/// pixi `wordWrap` at spaces (a word longer than the width keeps its line).
pub(crate) fn wrap(text: &str, font: UiFont, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let try_line = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && measure(font, size, &try_line) > width {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = try_line;
            }
        }
        lines.push(line);
    }
    lines
}

/// A mission's description (`descriptionLabel`: Lilita 35 white, drop
/// shadow 2, wrap 430, line height 32) with the yellow amount label over
/// its first line at the width of the text before the first digit; its top
/// at `top`, left at `left` (units).
fn description(b: &mut Builder, m: &Mission, left: f32, top: f32) {
    let text = m.description();
    for (i, line) in wrap(&text, UiFont::Lilita, 35.0, 430.0).iter().enumerate() {
        let (x, y) = b.c(left, top + 32.0 * i as f32);
        b.text(line, UiFont::Lilita, 35.0, raw(WHITE, 1.0), x, y, 0.0, 0.0, Some(2.0));
    }
    if let Some(d) = text.find(|c: char| c.is_ascii_digit()) {
        let w = measure(UiFont::Lilita, 35.0, &text[..d]);
        let (x, y) = b.c(left + w - 1.0, top);
        b.text(&m.amount.to_string(), UiFont::Lilita, 35.0, raw(0xFFCC00, 1.0), x, y, 0.0, 0.0, None);
    }
}

/// pixi text height of `n` lines at line height 32 (Lilita 35, shadow 2).
fn description_height(m: &Mission) -> f32 {
    let n = wrap(&m.description(), UiFont::Lilita, 35.0, 430.0).len() as f32;
    35.0 * (0.92225 + 0.21925) + (n - 1.0) * 32.0 + 2.0
}

/// The progress box: the 100 x 55 border at (cx, cy), the fill (progress /
/// amount of 100 x 54), the count; or the green completed box with the tick.
fn progress_box(b: &mut Builder, m: &Mission, cx: f32, cy: f32, completed_box: bool) {
    let s = b.s;
    let (x, y) = b.c(cx - 50.0, cy - 27.5);
    b.rect(x, y, 100.0 * s, 55.0 * s, raw(0x7993AE, 1.0), 6.0 * s);
    let k = (m.progress as f32 / m.amount as f32).clamp(0.0, 1.0);
    if k > 0.0 {
        let (x, y) = b.c(cx - 50.0, cy - 27.0);
        b.rect(x, y, 100.0 * k * s, 54.0 * s, raw(0x67BFC3, 1.0), 6.0 * s);
    }
    // the completed box (`completedBg`) covers the count: one or the other
    let done = completed_box && m.completed;
    if !done {
        let label = m.progress.to_string();
        let w = measure(UiFont::Lilita, 35.0, &label);
        let size = if w > 90.0 { 35.0 * 90.0 / w } else { 35.0 };
        let (x, y) = b.c(cx, cy);
        b.text(&label, UiFont::Lilita, size, raw(WHITE, 1.0), x, y, 0.5, 0.5, None);
    }
    if done {
        let (x, y) = b.c(cx - 50.0, cy - 27.0);
        b.rect(x, y, 100.0 * s, 54.0 * s, raw(0x70B501, 1.0), 6.0 * s);
        b.frame_c("mission-completed-checkmark", x + 50.0 * s, y + 27.0 * s, 1.0, Color::WHITE);
    }
}

/// `sh`: the set's progress counter (166 x 40, three slots, the
/// multiplier reward icon at the right end), centred at (cx, cy).
fn progress_counter(b: &mut Builder, missions: &[Mission], cx: f32, cy: f32) {
    let s = b.s;
    let (x, y) = b.c(cx - 83.0, cy - 20.0);
    b.rect(x, y, 166.0 * s, 40.0 * s, raw(0xACBFD3, 1.0), 14.0 * s);
    let icon_w = 60.0;
    let w = (166.0 - icon_w / 2.0 - 40.0) / 3.0;
    for i in 0..3 {
        let x0 = cx - 83.0 + (w + 10.0) * (i as f32 + 1.0) - w / 2.0;
        let (x, y) = b.c(x0 - w / 2.0, cy - 8.0);
        b.rect(x, y, w * s, 16.0 * s, raw(0x7692AD, 1.0), 5.0 * s);
        if missions.get(i).is_some_and(|m| m.completed) {
            let (x, y) = b.c(x0 - (w - 4.0) / 2.0 - 2.0, cy - 8.0);
            b.rect(x, y, w * s, 16.0 * s, raw(0x64AA1D, 1.0), 8.0 * s);
            let (x, y) = b.c(x0 - (w - 4.0) / 2.0 + 2.0, cy - 4.0);
            b.rect(x, y, (w - 8.0) * s, 8.0 * s, raw(0x8FD326, 1.0), 4.0 * s);
        }
    }
    let (x, y) = b.c(cx + 83.0, cy);
    b.frame_c("mission-multiplier-reward", x, y, 1.0, Color::WHITE);
}

/// The pause panel's missions section `fh` at y -110: "Missions",
/// "MISSION SET n", the counter, the three modules `uh` (580 x 96).
pub(crate) fn pause_missions(b: &mut Builder, g: &Game) {
    let s = b.s;
    let m = &g.missions;
    let top = -110.0;
    let (x, y) = b.c(-290.0, top);
    b.text("Missions", UiFont::Titan, 40.0, raw(NAVY, 1.0), x, y, 0.0, 0.0, None);
    let (x, y) = b.c(-290.0, top + 42.2);
    let label = crate::missions::text("mission-set", 0).replace("{{num}}", &m.set_label().to_string());
    b.text(&label, UiFont::Lilita, 35.0, raw(SET_BLUE, 1.0), x, y, 0.0, 0.0, None);
    progress_counter(b, m.current(), 270.0 - 98.0, top + 42.2);
    for (i, ms) in m.current().iter().enumerate() {
        let cy = top + 150.0 + (96.0 + 15.0) * i as f32;
        let (x, y) = b.c(-290.0, cy - 48.0);
        b.rect(x, y, 580.0 * s, 96.0 * s, raw(0x567EA9, 1.0), 18.0 * s);
        progress_box(b, ms, 220.0, cy, true);
        description(b, ms, -275.0, cy - description_height(ms) / 2.0);
    }
}

/// My Tour `My`: the notepad (Back, Word Hunt, Missions) with one section.
pub(crate) fn my_tour(b: &mut Builder, g: &Game, missions: bool) {
    notepad(
        b,
        &[
            ("navigation-button-grey", "icon-white-back", "Menu", Click::Back),
            ("navigation-button-red", "word-hunt-icon", "Word Hunt", Click::TourSection(false)),
            ("navigation-button-red", "icon-white-missions", "Missions", Click::TourSection(true)),
        ],
    );
    let u = &g.flow.user;
    currencies(b, 327.0 - 20.0, -417.0 - 64.18 + 70.0, u.keys, u.coins);
    if missions {
        tour_missions(b, g);
    } else {
        let (x, y) = b.c(-282.0, -392.0);
        b.text("Word Hunt", UiFont::Titan, 40.0, raw(NAVY, 1.0), x, y, 0.0, 0.0, None);
        let (x, y) = b.c(-282.0, -392.0 + 42.2);
        b.text("COLLECT LETTERS", UiFont::Lilita, 35.0, raw(0x6FB500, 1.0), x, y, 0.0, 0.0, None);
        word_hunt_section(b, g);
    }
}

/// `Ty`: the splat with "x n", "Missions" / "MISSION SET n", the counter,
/// the three modules `Sy` (580 x 172) with their skip buttons.
fn tour_missions(b: &mut Builder, g: &Game) {
    let s = b.s;
    let m = &g.missions;
    let (w, h) = (654.0, 834.0);
    // header icon: anchor (1, 0) at (w/2 - 45, -h/2 + 15), x1.1; the green
    // copy on top at (-3, -2)
    let (ix, iy) = (w / 2.0 - 45.0, -h / 2.0 + 15.0);
    let (sw, sh) = (200.0 * 1.1, 180.0 * 1.1);
    let (x, y) = b.c(ix - sw, iy);
    b.frame("missions-splat", x, y, sw * s, sh * s, Color::WHITE, false);
    let (x, y) = b.c(ix - sw - 3.3, iy - 2.2);
    b.frame("missions-splat", x, y, sw * s, sh * s, raw(0x6FB500, 1.0), false);
    // the multiplier: Lilita 100, black stroke 15, yellow gradient; rotated
    let n = m.set_label().to_string();
    let nw = measure(UiFont::Lilita, 100.0, &n);
    let xw = measure(UiFont::Lilita, 100.0, "x");
    let (cx, cy) = (ix - sw / 2.0 - nw / 4.0, iy + sh / 2.0 - 30.0);
    b.rotation = std::f32::consts::PI / 20.0;
    // (each label turns about its own centre: the builder rotates nodes)
    for (t, ox, tw) in [("x", -xw / 2.0, xw), (n.as_str(), nw / 2.3, nw)] {
        let mx = cx + ox + tw / 2.0;
        for k in 0..8 {
            let a = k as f32 * std::f32::consts::FRAC_PI_4;
            let (x, y) = b.c(mx + a.cos() * 7.5, cy + a.sin() * 7.5);
            b.text(t, UiFont::Lilita, 100.0, raw(0x000000, 1.0), x, y, 0.5, 0.5, None);
        }
        let (x, y) = b.c(mx + 9.0, cy + 3.0);
        b.text(t, UiFont::Lilita, 98.0, raw(0xFFAB00, 1.0), x, y, 0.5, 0.5, None);
        let (x, y) = b.c(mx + 9.0, cy + 1.0);
        b.text(t, UiFont::Lilita, 98.0, raw(0xFFD73C, 1.0), x, y, 0.5, 0.5, None);
    }
    b.rotation = 0.0;
    let (hx, hy) = (-w / 2.0 + 40.0, -h / 2.0 + 25.0);
    let (x, y) = b.c(hx, hy);
    b.text("Missions", UiFont::Titan, 40.0, raw(NAVY, 1.0), x, y, 0.0, 0.0, None);
    let (x, y) = b.c(hx, hy + 42.2);
    let label = crate::missions::text("mission-set", 0).replace("{{num}}", &m.set_label().to_string());
    b.text(&label, UiFont::Lilita, 35.0, raw(SET_BLUE, 1.0), x, y, 0.0, 0.0, None);
    progress_counter(b, m.current(), hx + 83.0, hy + 42.2 + 40.4 + 31.5);
    for (i, ms) in m.current().iter().enumerate() {
        let cy = -135.0 + (172.0 + 15.0) * i as f32;
        let (x, y) = b.c(-290.0, cy - 86.0);
        b.rect(x, y, 580.0 * s, 172.0 * s, raw(0xB6CBE0, 1.0), 18.0 * s);
        if ms.completed {
            // the green glow panel `lh` with "Completed" and the tick
            b.rect(x, y, 580.0 * s, 172.0 * s, raw(0x70B501, 1.0), 20.0 * s);
            let tw = measure(UiFont::Lilita, 45.0, "Completed");
            let (x, y) = b.c(-15.0, cy + 43.0);
            b.text_shadow("Completed", UiFont::Lilita, 45.0, raw(0x4A8400, 1.0), x, y, 0.5, 0.5, Some((1.0, raw(WHITE, 1.0))));
            let (x, y) = b.c(-15.0 + tw / 2.0 + 25.0 - 5.0, cy + 43.0);
            b.frame_c("mission-completed-checkmark", x, y, 1.0, Color::WHITE);
        } else {
            let (x, y) = b.c(-270.0, cy + 43.0);
            b.text(&crate::missions::text("skip-mission", 0), UiFont::Lilita, 30.0, raw(0x668AB6, 1.0), x, y, 0.0, 0.5, None);
            // `Ev`: 178 x 52 green, the price and a coin
            let (bx, by) = (270.0 - 89.0, cy + 43.0);
            let (x, y) = b.c(bx - 89.0, by - 26.0);
            b.rect(x, y, 178.0 * s, 52.0 * s, raw(0x6FB500, 1.0), 12.0 * s);
            let price = crate::missions::SKIP_COST.to_string();
            let pw = measure(UiFont::Lilita, 25.0, &price);
            let (x, y) = b.c(bx - (pw + 30.0) / 2.0, by);
            b.text(&price, UiFont::Lilita, 25.0, raw(WHITE, 1.0), x, y, 0.0, 0.5, Some(1.0));
            let (x, y) = b.c(bx - (pw + 30.0) / 2.0 + pw + 18.0, by);
            b.frame_c("icon-coin", x, y, 0.6, Color::WHITE);
            let (x, y) = b.c(bx - 89.0, by - 26.0);
            b.hit(x, y, 178.0 * s, 52.0 * s, Click::MissionSkip(i));
        }
        // the module's top bar (over the completed panel)
        let (x, y) = b.c(-290.0, cy - 86.0);
        b.rect(x, y, 580.0 * s, 86.0 * s, raw(0x567EA9, 1.0), 18.0 * s);
        progress_box(b, ms, 220.0, cy - 43.0, false);
        description(b, ms, -275.0, cy - 43.0 - description_height(ms) / 2.0);
    }
}

/// `Mm` (44629): a 100 x 100 `base-item-flat` with an icon, at (x, y) px.
fn round_button(b: &mut Builder, x: f32, y: f32, icon: &str, diagonal: bool, click: Click) {
    let s = b.s;
    let t = b.press_tint(x - 50.0 * s, y - 50.0 * s, 100.0 * s, 100.0 * s);
    b.frame("base-item-flat", x - 50.0 * s, y - 50.0 * s, 100.0 * s, 100.0 * s, t, false);
    b.frame_c(icon, x, y, 1.0, Color::WHITE);
    if diagonal {
        b.frame_c("diagonal", x, y, 1.0, Color::WHITE);
    }
    b.hit(x - 50.0 * s, y - 50.0 * s, 100.0 * s, 100.0 * s, click);
}

/// The title's top row: settings (60, 60), My Tour (vw/2, 60), sound
/// (vw - 60, 60); FreeStuff is not ported.
pub(crate) fn title_buttons(b: &mut Builder, g: &Game) {
    let (s, vw, vh) = (b.s, b.vw(), b.vh());
    round_button(b, 60.0 * s, 60.0 * s, "icon-settings", false, Click::TitleSettings);
    round_button(b, (vw - 60.0) * s, 60.0 * s, "icon-sound", g.flow.user.muted, Click::TitleSound);
    am_button(b, 0.0, 60.0 - vh / 2.0, 240.0, 100.0, 0x932F3A, "front-icon-mytour", 1.0, Some(Click::TitleMyTour));
}

/// Settings `cb`: 586.5 x 850 panel, "Settings", the Nickname / Sound /
/// Privacy Policy rows (`ib`, x0.85, 130 apart from -250), the close button
/// and the version.
pub(crate) fn settings(b: &mut Builder, g: &Game, prompt: bool) {
    let s = b.s;
    backdrop_blur(b);
    let w = 586.5;
    rect_comp(b, 0.0, 0.0, w, 850.0, w + 22.0, 874.0, raw(0xEEEEEE, 1.0), "box-border-grey", None);
    let (x, y) = b.c(0.0, -360.0);
    b.text("Settings", UiFont::Titan, 60.0, raw(0x004A80, 1.0), x, y, 0.5, 0.5, None);
    let u = &g.flow.user;
    let rows: [(&str, &str, String, Option<Click>, bool); 3] = [
        ("icon-user", "Nickname", u.name.clone(), Some(Click::SettingsNickname), false),
        ("icon-sound", "Sound", if u.muted { "Off".into() } else { "On".into() }, Some(Click::SettingsSound), u.muted),
        // `window.open(privacy_policy_link)`: no browser here
        ("icon-info", "Privacy Policy", String::new(), None, false),
    ];
    let k = 0.85;
    for (i, (icon, title, sub, click, diag)) in rows.iter().enumerate() {
        let oy = i as f32 * 130.0 - 250.0;
        let p = |x: f32, y: f32| (x * k, oy + y * k);
        // the button: grey border 116 x 118 (+1, +1) around a 100 x 100 fill
        let (bx, by) = p(-215.0, 0.0);
        let (x, y) = b.c(bx - 58.0 * k + k, by - 59.0 * k + k);
        b.sliced("box-border-grey-small", x, y, 116.0 * k * s, 118.0 * k * s, 15.0, 15.0, 20.0, 20.0);
        let (x, y) = b.c(bx - 50.0 * k, by - 50.0 * k);
        b.rect(x, y, 100.0 * k * s, 100.0 * k * s, raw(0x3689BE, 1.0), 6.0 * k * s);
        let (x, y) = b.c(bx, by);
        b.frame_c(icon, x, y, k, Color::WHITE);
        if *diag {
            b.frame_c("diagonal", x, y, k, Color::WHITE);
        }
        let ty = if sub.is_empty() { -20.0 } else { -30.0 };
        let (tx, tyy) = p(-135.0, ty);
        let (x, y) = b.c(tx, tyy);
        b.text(title, UiFont::Titan, 40.0 * k, raw(0x004A80, 1.0), x, y, 0.0, 0.0, None);
        if !sub.is_empty() {
            let (sx, sy) = p(-135.0, ty + 55.0);
            let (x, y) = b.c(sx, sy);
            b.text(sub, UiFont::Titan, 30.0 * k, raw(0x3A8BBF, 1.0), x, y, 0.0, 0.0, None);
        }
        if let Some(c) = click {
            let (x, y) = b.c(-275.0 * k, oy - 65.0 * k);
            b.hit(x, y, 550.0 * k * s, 130.0 * k * s, *c);
        }
    }
    let (x, y) = b.c(-w / 2.0 + 10.0, -415.0);
    b.frame_c("btn-close", x, y, 1.0, Color::WHITE);
    b.hit(x - 43.0 * s, y - 43.0 * s, 86.0 * s, 86.0 * s, Click::CloseOverlay);
    let (x, y) = b.c(w / 2.0 - 10.0, 415.0);
    b.text("v1.0.12-fix-pogo", UiFont::Titan, 18.0, raw(0x3A8BBF, 0.5), x, y, 1.0, 1.0, None);
    let (x, y) = b.c(-w / 2.0, -425.0);
    b.hit(x, y, w * s, 850.0 * s, Click::Elsewhere);
    if prompt {
        nickname_prompt(b, g);
    }
    let (ww, hh) = (b.w, b.h);
    b.hit(0.0, 0.0, ww, hh, Click::Elsewhere);
}

/// The nickname prompt `rb` (an HTML overlay in CSS px, not scaled): a
/// 75 % black backdrop (a tap closes it), a 400 x 120 #EEEEEE panel with
/// "Set your nickname" (Lilita 1.5em #004A80) and the input (2em #3A8BBF on
/// white, 80 % wide), Enter closes it.
fn nickname_prompt(b: &mut Builder, g: &Game) {
    let (w, h) = (b.w, b.h);
    b.rect(0.0, 0.0, w, h, raw(0x000000, 0.75), 0.0);
    let (px, py) = (w / 2.0 - 200.0, h / 2.0 - 60.0);
    b.rect(px, py, 400.0, 120.0, raw(0xEEEEEE, 1.0), 16.0);
    let lilita_px = |px_size: f32, s: f32| px_size / s;
    let s = b.s;
    b.text("Set your nickname", UiFont::Lilita, lilita_px(24.0, s), raw(0x004A80, 1.0), w / 2.0, py + 15.0, 0.5, 0.0, None);
    let (ix, iy, iw, ih) = (px + 40.0, py + 15.0 + 30.0 + 5.0, 320.0, 40.0);
    b.rect(ix, iy, iw, ih, raw(WHITE, 1.0), 0.0);
    let caret = if (g.now_ms() / 500.0) as i64 % 2 == 0 { "|" } else { " " };
    let text = format!("{}{caret}", g.flow.nickname_input);
    b.text(&text, UiFont::Lilita, lilita_px(32.0, s), raw(0x3A8BBF, 1.0), w / 2.0, iy + ih / 2.0, 0.5, 0.5, None);
    b.hit(px, py, 400.0, 120.0, Click::Elsewhere);
    b.hit(0.0, 0.0, w, h, Click::PromptClose);
}

/// The notification `Py` at (vw/2, 100 + slide): `base-blue` (nine-slice
/// 20), the tick, two lines of Lilita 30; height 130.
pub(crate) fn toast(b: &mut Builder, t: &Toast) {
    let Some(off) = t.offset() else { return };
    let (s, vw) = (b.s, b.vw());
    let lines: Vec<&str> = t.text.split('\n').collect();
    let label_w = lines.iter().map(|l| measure(UiFont::Lilita, 30.0, l)).fold(0.0, f32::max);
    let icon_size = b.ui.frames.get(t.icon).map_or(Vec2::new(50.0, 40.0), |r| Vec2::new(r.width() as f32, r.height() as f32));
    let bh = t.height.unwrap_or(icon_size.y + 20.0);
    let side = icon_size.x.max(bh);
    let bw = label_w + side + 50.0;
    let (cx, cy) = (vw / 2.0, 100.0 + off as f32);
    b.sliced("base-blue", (cx - bw / 2.0) * s, (cy - bh / 2.0) * s, bw * s, bh * s, 20.0, 20.0, 20.0, 20.0);
    let icon_x = -bw / 2.0 + side / 2.0 + 10.0;
    b.frame_c(t.icon, (cx + icon_x) * s, cy * s, 1.0, Color::WHITE);
    let lx = icon_x + side / 2.0 + 10.0;
    let lh = 30.0 * (0.92225 + 0.21925);
    let n = lines.len() as f32;
    for (i, l) in lines.iter().enumerate() {
        let y = cy - lh * n / 2.0 + lh * (i as f32 + 0.5);
        b.text(l, UiFont::Lilita, 30.0, raw(WHITE, 1.0), (cx + lx) * s, y * s, 0.0, 0.5, None);
    }
}

/// The Me panel's Awards section `ty` (61464): "Awards" (Lilita 60) at the
/// top, the 12 items `ey` (594 x 190, 200 apart) in a 654-tall mask,
/// scrolled by `flow.awards_list`.
pub(crate) fn awards_tab(b: &mut Builder, g: &Game) {
    use crate::awards;
    let s = b.s;
    let (x, y) = b.c(0.0, -834.0 / 2.0 + 10.0);
    b.text("Awards", UiFont::Lilita, 60.0, raw(0x003C6E, 1.0), x, y, 0.5, 0.0, None);
    let (mx, my) = b.c(-327.0, -327.0);
    b.clip_begin(mx, my, 654.0 * s, 654.0 * s);
    let top = g.flow.awards_list.target;
    for (k, id) in awards::ordered(g).into_iter().enumerate() {
        let cy = top + k as f32 * 200.0;
        if cy + 95.0 < -327.0 || cy - 95.0 > 327.0 {
            continue;
        }
        award_item(b, g, id, cy);
    }
    b.clip_end();
}

/// `ey`: an award's row centred at y `cy`.
fn award_item(b: &mut Builder, g: &Game, id: &'static str, cy: f32) {
    use crate::awards;
    let s = b.s;
    let (w, h) = (594.0, 190.0);
    let (x, y) = b.c(-w / 2.0, cy - h / 2.0);
    b.rect(x, y, w * s, h * s, raw(0xA9D7EE, 1.0), 0.0);
    let p = awards::progress(g, id);
    let done = awards::all_tiers_completed(g, id);
    // trophies `$v` (112 x 170): the big spray can of the tier, four small
    // ones (earned / this tier's outline / empty)
    let (tx, ty) = (-w / 2.0 + 10.0, cy - h / 2.0 + 10.0);
    let (x, y) = b.c(tx, ty);
    b.rect(x, y, 112.0 * s, 170.0 * s, raw(0x66737F, 1.0), 10.0 * s);
    let (ix, iy) = b.c(tx + 56.0, ty + 125.0 / 2.0 - 8.0);
    b.frame_c("base-item-large", ix, iy, 1.0, Color::WHITE);
    let big = awards::TIERS.get(p.tier as usize).copied().unwrap_or("diamond");
    b.frame_c(&format!("spraycan-big-{big}"), ix, iy, 1.0, Color::WHITE);
    b.frame_c("stars", ix, iy, 1.0, Color::WHITE);
    for i in 0..4u8 {
        let frame = if i == p.tier {
            "spraycan-outline".to_string()
        } else if i > p.tier {
            "spraycan-empty".to_string()
        } else {
            format!("spraycan-{}", awards::TIERS[i as usize])
        };
        let (x, y) = b.c(tx + i as f32 * 25.0 + 18.0, ty + 170.0 - 58.0 * 0.8 / 2.0 - 5.0);
        b.frame_c(&frame, x, y, 0.8, Color::WHITE);
    }
    // title (Lilita 36, wrap) and description (Lilita 24)
    // `trophies.width`: the 126-wide icon (centred at 56) overhangs the box
    let tw = 126.0;
    let lx = -w / 2.0 + tw + 10.0;
    let wrap_w = w - tw - 20.0;
    let title = awards::text(id, 0.0);
    let lines = wrap(&title, UiFont::Lilita, 36.0, wrap_w);
    for (i, l) in lines.iter().enumerate() {
        let (x, y) = b.c(lx, cy - h / 2.0 + 10.0 + 36.0 * i as f32);
        b.text(l, UiFont::Lilita, 36.0, raw(0x1A5C8D, 1.0), x, y, 0.0, 0.0, None);
    }
    let title_h = 36.0 * (0.92225 + 0.21925) + (lines.len() as f32 - 1.0) * 36.0;
    let (goal, reward) = awards::tier(id, p.tier);
    let desc = if done { awards::text("award-completed", 0.0) } else { awards::text(&format!("{id}-desc"), goal) };
    for (i, l) in wrap(&desc, UiFont::Lilita, 24.0, wrap_w).iter().enumerate() {
        let (x, y) = b.c(lx, cy - h / 2.0 + 10.0 + title_h + 5.0 + 20.0 * i as f32);
        b.text(l, UiFont::Lilita, 24.0, raw(0x2F89B7, 1.0), x, y, 0.0, 0.0, None);
    }
    if done {
        return;
    }
    // the keys box `Zv` (462 x 50) with the reward
    let ky = cy + h / 2.0 - 50.0 - 10.0;
    let (x, y) = b.c(lx, ky);
    b.rect(x, y, wrap_w * s, 50.0 * s, raw(0x66737F, 1.0), 10.0 * s);
    let label = reward.to_string();
    let lw = measure(UiFont::Lilita, 35.0, &label);
    let (x, y) = b.c(lx + wrap_w - 50.0, ky + 25.0);
    b.text(&label, UiFont::Lilita, 35.0, raw(WHITE, 1.0), x, y, 1.0, 0.5, None);
    let (x, y) = b.c(lx + wrap_w - 50.0 + lw + 3.0, ky + 25.0);
    b.frame_c("icon-key", x, y, 0.8, Color::WHITE);
    let ratio = awards::ratio(g, id) as f32;
    if ratio >= 1.0 {
        // "Collect Award": grey border 366 x 76 around the 342 x 50 green
        let (bx, by) = (lx + 366.0 / 2.0 - 10.0, ky + 25.0 - 3.0);
        rect_comp(b, bx, by, 342.0, 50.0, 366.0, 76.0, raw(0x41972A, 1.0), "box-border-grey", None);
        let (x, y) = b.c(bx, by);
        b.text(&awards::text("award-collect", 0.0), UiFont::Lilita, 30.0, raw(WHITE, 1.0), x, y, 0.5, 0.5, None);
        let (hx, hy) = b.c(bx - 183.0, by - 38.0);
        let i = awards::IDS.iter().position(|&a| a == id).unwrap_or(0);
        b.hit(hx, hy, 366.0 * s, 76.0 * s, Click::AwardCollect(i));
    } else {
        // the progress bar `Qv` (362 x 34) with its percentage
        let (px, py) = (lx + 8.0, ky + 8.0);
        let (x, y) = b.c(px, py);
        b.rect(x, y, 362.0 * s, 34.0 * s, raw(0xF8EAC8, 1.0), 0.0);
        let r = ratio.clamp(0.0, 1.0);
        if r > 0.0 {
            b.rect(x, y, 362.0 * r * s, 34.0 * s, raw(0xF5BD41, 1.0), 0.0);
        }
        let (x, y) = b.c(px + 181.0, py + 17.0);
        b.text(&format!("{}%", (r * 100.0).round()), UiFont::Lilita, 30.0, raw(0x1A5C8D, 1.0), x, y, 0.5, 0.5, None);
    }
}
