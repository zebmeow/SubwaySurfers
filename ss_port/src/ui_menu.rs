//! Drawing the menus of [`crate::menu`]: title screen `mb`, Me panel `hy`
//! (Characters / Boards), boost shop `Mv`, boost panel `Pv`, buy hoverboards
//! `Fv`, not enough currency `Ny`, the resume countdown. Positions are the
//! original's virtual units (docs/js_notes/ui_powerups_shop.md §2-§4),
//! relative to the screen centre unless noted.
//!
//! Settings, My Tour and the missions: `crate::ui_missions`.
//!
//! Differences from the original: no FreeStuff / Top Run buttons (not
//! ported; the bottom row is laid out as with the leaderboard off), no
//! Awards or Free Stuff tabs, the thumb strips are static images (the
//! characters' 2D portraits, pre-rendered board thumbnails), no
//! drag scrolling of these lists (mouse wheel, arrow keys, taps).

use crate::flow::Click;
use crate::game::Game;
use crate::menu::{self, Item, MeState, MeTab, Menu, Overlay, SelectButton};
use crate::shop::Catalog;
use crate::ui::{backdrop_blur, raw, rect_comp, Builder, UiFont, WHITE};
use bevy::prelude::*;

const NAVY: u32 = 0x033B71;
const GREEN: u32 = 0x41972A;

/// Rough Lilita / Titan text width (em-relative), for right-aligned tags.
fn text_w(s: &str, size: f32) -> f32 {
    s.chars().count() as f32 * size * 0.52
}

/// `Am` (44395): white (w+10)x(h+10) r10 frame, coloured w x h r5 fill, icon.
pub(crate) fn am_button(b: &mut Builder, cx: f32, cy: f32, w: f32, h: f32, color: u32, icon: &str, alpha: f32, click: Option<Click>) {
    let s = b.s;
    let (x, y) = b.c(cx - (w + 10.0) / 2.0, cy - (h + 10.0) / 2.0);
    let k = if click.is_some() { b.press_tint(x, y, (w + 10.0) * s, (h + 10.0) * s).to_linear().red } else { 1.0 };
    let tint = |c: Color| {
        let l = c.to_linear();
        Color::LinearRgba(LinearRgba::new(l.red * k, l.green * k, l.blue * k, l.alpha))
    };
    b.rect(x, y, (w + 10.0) * s, (h + 10.0) * s, tint(raw(WHITE, alpha)), 10.0 * s);
    let (x, y) = b.c(cx - w / 2.0, cy - h / 2.0);
    b.rect(x, y, w * s, h * s, tint(raw(color, alpha)), 5.0 * s);
    let (ix, iy) = b.c(cx, cy);
    b.frame_c(icon, ix, iy, 1.0, Color::WHITE.with_alpha(alpha));
    if let Some(c) = click {
        let (x, y) = b.c(cx - (w + 10.0) / 2.0, cy - (h + 10.0) / 2.0);
        b.hit(x, y, (w + 10.0) * s, (h + 10.0) * s, c);
    }
}

/// `Xm` notepad bottom-bar button: `navigation-button-*` x(0.85, 0.9),
/// icon, Titan One 20 label at y + 68.
fn xm_button(b: &mut Builder, cx: f32, bg: &str, icon: &str, label: &str, click: Click) {
    let s = b.s;
    let (x, y) = b.c(cx, 412.0);
    let (fw, fh) = (166.0 * 0.85 * s, 110.0 * 0.9 * s);
    let t = b.press_tint(x - fw / 2.0, y - fh / 2.0, fw, fh);
    b.frame(bg, x - fw / 2.0, y - fh / 2.0, fw, fh, t, false);
    b.frame_c(icon, x, y, 1.0, Color::WHITE);
    let (lx, ly) = b.c(cx, 412.0 + 68.0);
    b.text(label, UiFont::Titan, 20.0, raw(WHITE, 1.0), lx, ly, 0.5, 0.5, None);
    b.hit(x - fw / 2.0, y - fh / 2.0, fw, fh, click);
}

/// Bottom row slots of a 4-button notepad row (`th`, spacing 17.92).
const SLOTS: [f32; 4] = [-238.5, -79.5, 79.5, 238.5];

/// `th` notepad: blur backdrop, panel, bottom row.
pub(crate) fn notepad(b: &mut Builder, buttons: &[(&str, &str, &str, Click)]) {
    backdrop_blur(b);
    let (x, y) = b.c(0.0, -64.18);
    b.frame_c("notepad-panel", x, y, 1.0, Color::WHITE);
    for (i, (bg, icon, label, click)) in buttons.iter().enumerate() {
        xm_button(b, SLOTS[i], bg, icon, label, *click);
    }
}

/// Currencies `Tv`: grey nine-slice with the key and coin tags; `right` is
/// the right edge (units, centre-relative).
pub(crate) fn currencies(b: &mut Builder, right: f32, y: f32, keys: i64, coins: i64) {
    let s = b.s;
    let (ks, cs) = (keys.to_string(), coins.to_string());
    let kw = 27.0 + 6.0 + text_w(&ks, 30.0);
    let cw = 39.0 + 6.0 + text_w(&cs, 30.0);
    let bw = kw + cw + 30.0;
    let left = right - bw;
    let (bx, by) = b.c(left, y - 21.0);
    b.sliced("base-grey", bx, by, bw * s, 42.0 * s, 10.0, 10.0, 10.0, 10.0);
    let (x, yy) = b.c(left + 10.0 + 13.5, y);
    b.frame_c("icon-key", x, yy, 0.75, Color::WHITE);
    let (x, yy) = b.c(left + 10.0 + 27.0 + 4.0, y);
    b.text(&ks, UiFont::Lilita, 30.0, raw(0xF6F6F6, 1.0), x, yy, 0.0, 0.5, Some(1.0));
    let (x, yy) = b.c(left + 20.0 + kw + 19.5, y);
    b.frame_c("icon-coin", x, yy, 0.75, Color::WHITE);
    let (x, yy) = b.c(left + 20.0 + kw + 39.0 + 4.0, y);
    b.text(&cs, UiFont::Lilita, 30.0, raw(0xF6F6F6, 1.0), x, yy, 0.0, 0.5, Some(1.0));
}

/// The menu screen of the current [`Menu`].
pub(crate) fn menus(b: &mut Builder, g: &Game) {
    match &g.flow.menu {
        Menu::None => {}
        Menu::Title => title(b, g),
        Menu::Me(m) => me_panel(b, g, m),
        Menu::Shop { open, scroll } => boost_shop(b, g, *open, *scroll),
        Menu::MyTour { missions } => crate::ui_missions::my_tour(b, g, *missions),
    }
}

/// Modal overlays.
pub(crate) fn overlays(b: &mut Builder, g: &Game) {
    match &g.flow.overlay {
        None => {}
        Some(Overlay::Boosts { open, scroll }) => boost_panel(b, g, *open, *scroll),
        Some(Overlay::BuyBoards { bump }) => buy_boards(b, g, *bump),
        Some(Overlay::NotEnough { currency, needed }) => not_enough(b, currency, *needed),
        Some(Overlay::Prize(p)) => prize_screen(b, g, p),
        Some(Overlay::Settings { prompt }) => crate::ui_missions::settings(b, g, *prompt),
    }
}

/// The prize screen `By` (64472, boosts_mysterybox.md §3.2). The box (`Dy`)
/// and then the prize (`Fy`) are 3D renders (`crate::render_menu_3d`).
pub(crate) fn prize_screen(b: &mut Builder, g: &Game, p: &crate::flow::PrizeScreen) {
    use crate::flow::PrizeState;
    let (s, vw, vh) = (b.s, b.vw(), b.vh());
    let (cx, cy) = (vw / 2.0, vh / 2.0);
    let size = vw.max(vh) * 1.25;
    b.frame("prizescreen-bg", (cx - size / 2.0) * s, (cy - vh * 0.05 - size / 2.0) * s, size * s, size * s, Color::WHITE, false);
    // "Prizes" tag (the odds panel is not ported) and the wallet
    let boxed = p.current.box_type.is_some();
    if boxed {
        let (tx, ty) = (cx - 100.0 - 60.0, 45.0 + 40.0);
        b.rect((tx - 100.0) * s, (ty - 45.0) * s, 200.0 * s, 90.0 * s, raw(0xE47F00, 1.0), 12.0 * s);
        b.frame_c("mystery-box-icon", (tx - 50.0) * s, ty * s, 0.5, Color::WHITE);
        b.text("Prizes", UiFont::Lilita, 32.0, raw(WHITE, 1.0), (tx + 40.0) * s, ty * s, 0.5, 0.5, Some(2.0));
    }
    let u = &g.flow.user;
    currencies(b, 60.0 + 200.0, 85.0 - vh / 2.0, u.keys, u.coins);
    let since = p.opened.map(|o| (p.t - o) as f32 / 60.0);
    // rays (`lh` with stripes, baked: `ui::bake_rays`): 0.3 s in, a turn
    // in 4 s, then 1 s out
    if let Some(t) = since {
        // GSAP's default ease (Power1.easeOut) on each tween
        let ease = |p: f32| {
            let p = p.clamp(0.0, 1.0);
            1.0 - (1.0 - p) * (1.0 - p)
        };
        let a = if t < 4.0 { ease(t / 0.3) } else { 1.0 - ease(t - 4.0) };
        if a > 0.0 {
            // the mask grows to 650 in 0.3 s (here the whole burst); x1.2
            let r = 650.0 * 1.2 * ease(t / 0.3);
            b.rotation = ease(t / 4.0) * std::f32::consts::TAU;
            match b.ui.rays.clone() {
                Some(rays) => b.image(&rays, (cx - r / 2.0) * s, (cy - r / 2.0) * s, r * s, r * s, Color::srgba(1.0, 1.0, 1.0, a)),
                None => b.frame("background-superglow", (cx - r / 2.0) * s, (cy - r / 2.0) * s, r * s, r * s, raw(0xFFFF66, a), false),
            }
            b.rotation = 0.0;
        }
    }
    // the box (`Dy` at the centre): its shadow 448 x 224, anchor (0.5, 0) at
    // y 200, the
    // 640-unit render with anchor (0.5, 1) at y 296
    if boxed {
        let pose = crate::render_menu_3d::box_pose(p.t as f64 / 60.0, p.opened.map(|o| (p.t - o) as f64 / 60.0));
        let k = pose.shadow_k as f32;
        if k > 0.0 && pose.shadow_alpha > 0.0 {
            let (w, h) = (448.0 * k, 224.0 * k);
            let blob = b.ui.blob.clone();
            b.image(&blob, (cx - w / 2.0) * s, (cy + 200.0) * s, w * s, h * s, Color::srgba(1.0, 1.0, 1.0, pose.shadow_alpha as f32));
        }
        // (the stage shows the prize from 0.5 s)
        if since.is_none_or(|t| t < 0.5) {
            let view = b.ui.hs_view.clone();
            b.image(&view, (cx - 320.0) * s, (cy + 296.0 - 640.0) * s, 640.0 * s, 640.0 * s, Color::WHITE);
        }
    }
    // the prize: grows 0.5 - 2.5 s while spinning, its text fades in 2.5 - 3 s
    if let Some(t) = since {
        if t >= 0.5 {
            // `Fy` at the centre: its shadow (`Z_`, anchor 0.5) at y shadowY,
            // growing to shadowW x 0.35 shadowW and alpha 0.3 in 2 s; the
            // 640-unit render (anchor 0.5) at y = 320 - (top + bottom) / 2,
            // which centres the prize's measured pixels
            let tt = ((t - 0.5) / 2.0).min(1.0);
            let k = 1.0 - (1.0 - tt) * (1.0 - tt);
            let (sprite_y, shadow_w, shadow_y) = match crate::render_menu_3d::prize_bounds(&p.current.kind) {
                Some([top, bottom, left, right]) => (320.0 - (top + bottom) / 2.0, (right - left) * 1.15, (bottom - top) / 2.0 + 50.0),
                None => (-200.0, 640.0 * 0.7, 180.0),
            };
            let (w, h) = (shadow_w * k, shadow_w * 0.35 * k);
            let blob = b.ui.blob.clone();
            b.image(&blob, (cx - w / 2.0) * s, (cy + shadow_y - h / 2.0) * s, w * s, h * s, Color::srgba(1.0, 1.0, 1.0, 0.3 * k));
            let view = b.ui.hs_view.clone();
            b.image(&view, (cx - 320.0) * s, (cy + sprite_y - 320.0) * s, 640.0 * s, 640.0 * s, Color::WHITE);
        }
        let a = ((t - 2.5) / 0.5).clamp(0.0, 1.0);
        if a > 0.0 {
            let label = p.current.label();
            // `prizeText.y = vh / 2 + prizeThumb.height / 2`: the 640-unit
            // render's thumb; measured against the oracle (346.5, any prize)
            b.text(&label, UiFont::Titan, 45.0, raw(WHITE, a), (cx + 1.7) * s, (cy + 347.5) * s, 0.5, 0.5, None);
            b.text(&label, UiFont::Titan, 45.0, raw(0x000000, a), cx * s, (cy + 346.5) * s, 0.5, 0.5, None);
        }
    }
    // hint: pulses 1 <-> 0.1 over 1.3 s
    let (text, show) = match p.state {
        PrizeState::Opening => ("Press Space to open", true),
        PrizeState::Animating => ("", false),
        _ => ("Press Space to continue", true),
    };
    if show {
        let ph = (p.t as f32 / 60.0 / 1.3) % 2.0;
        let a = if ph < 1.0 { 1.0 - 0.9 * ph } else { 0.1 + 0.9 * (ph - 1.0) };
        b.text(text, UiFont::Titan, 45.0, raw(WHITE, a), cx * s, (vh - 50.0) * s, 0.5, 0.5, None);
    }
    b.hit(0.0, 0.0, b.w, b.h, Click::Elsewhere);
}

// ---- title `mb` ----------------------------------------------------------------------------

fn title(b: &mut Builder, g: &Game) {
    let (s, vw, vh) = (b.s, b.vw(), b.vh());
    // boards button `lb` at (60, 170): base-item-large 100x100, icon, count
    let (x, y) = (60.0 * s, 170.0 * s);
    let t = b.press_tint(x - 50.0 * s, y - 50.0 * s, 100.0 * s, 100.0 * s);
    b.frame("base-item-large", x - 50.0 * s, y - 50.0 * s, 100.0 * s, 100.0 * s, t, false);
    b.frame_c("icon-item-hoverboard", x, y, 1.0, Color::WHITE);
    let n = g.flow.user.hoverboards.to_string();
    let off = Vec2::new((std::f32::consts::PI / 6.0).cos(), (std::f32::consts::PI / 6.0).sin()) * 2.0;
    b.text(&n, UiFont::Lilita, 22.0, raw(WHITE, 1.0), x + (33.0 + off.x) * s, y + (38.0 + off.y) * s, 1.0, 1.0, None);
    b.text(&n, UiFont::Lilita, 22.0, raw(0x000000, 1.0), x + 33.0 * s, y + 38.0 * s, 1.0, 1.0, None);
    b.hit(x - 50.0 * s, y - 50.0 * s, 100.0 * s, 100.0 * s, Click::TitleBoards);
    // PRESS TO PLAY: Titan One 40, wobble cos(t * 0.006) * 0.1
    b.rotation = ((g.now_ms() * 0.006).cos() * 0.1) as f32;
    b.text("PRESS TO PLAY", UiFont::Titan, 40.0, raw(WHITE, 1.0), vw / 2.0 * s, (vh - 350.0) * s, 0.5, 0.5, None);
    b.rotation = 0.0;
    // bottom row (`eh` spacing 40, no Top Run): Me 130x90, Shop 200x90
    let row_y = vh / 2.0 - 120.0;
    am_button(b, -125.0, row_y, 130.0, 90.0, 0x006501, "front-icon-me", 1.0, Some(Click::TitleMe));
    am_button(b, 90.0, row_y, 200.0, 90.0, 0x276FAB, "front-icon-shop", 1.0, Some(Click::TitleShop));
    crate::ui_missions::title_buttons(b, g);
    // the rest of the screen starts the run
    b.hit(0.0, 0.0, b.w, b.h, Click::StartGame);
}

// ---- Me panel `hy` -------------------------------------------------------------------------

fn me_panel(b: &mut Builder, g: &Game, m: &MeState) {
    let s = b.s;
    notepad(
        b,
        &[
            ("navigation-button-grey", "icon-white-back", "Menu", Click::Back),
            ("navigation-button-green", "icon-user", "Characters", Click::MeTab(MeTab::Characters)),
            ("navigation-button-green", "icon-boards", "Boards", Click::MeTab(MeTab::Boards)),
            ("navigation-button-green", "icon-white-awards", "Awards", Click::MeAwards),
        ],
    );
    if m.awards {
        crate::ui_missions::awards_tab(b, g);
        return;
    }
    let u = &g.flow.user;
    currencies(b, 654.0 / 2.0 - 20.0, -345.0, u.keys, u.coins);
    let c = Catalog::get();
    // profile block at y -200
    // (icon, locked, owned, focused, [base tint, icon tint]) per feature button
    let tints = |colors: &[String]| -> [u32; 2] {
        let p = |i: usize, d: u32| colors.get(i).and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok()).unwrap_or(d);
        [p(0, 0x3B8ABC), p(1, 0x10364F)]
    };
    #[allow(clippy::type_complexity)]
    let (name, outfit_label, feats): (String, String, Vec<(String, bool, bool, bool, [u32; 2])>) = match m.tab {
        MeTab::Characters => {
            let list = menu::characters();
            let id = &list[m.character];
            let ch = c.character(id);
            let name = title_case(id);
            let outfit = if m.outfit == 0 { String::new() } else { outfit_name(id, m.outfit) };
            let base = ch.map(|c| tints(&c.colors)).unwrap_or([0x3B8ABC, 0x10364F]);
            let mut feats = vec![("icon-character".to_string(), false, menu::owns(u, &Item::Character(id.clone())), m.outfit == 0, base)];
            for k in 1..=ch.map_or(0, |c| c.outfits.len()) {
                let it = Item::Outfit(id.clone(), k);
                let t = ch.and_then(|c| c.outfits.get(k - 1)).map(|o| tints(&o.colors)).unwrap_or(base);
                feats.push(("icon-outfit".to_string(), menu::locked(u, &it), menu::owns(u, &it), m.outfit == k, t));
            }
            (name, outfit, feats)
        }
        MeTab::Boards => {
            let bd = &c.boards[m.board];
            let focused = *m.board_preview.last().unwrap_or(&0);
            let base = [0x3B8ABC, 0x10364F];
            let mut feats = vec![("icon-board".to_string(), false, menu::owns(u, &Item::Board(bd.id.clone())), focused == 0, base)];
            if bd.powerup.is_none() {
                for (k, p) in bd.powerups.iter().enumerate().filter(|(_, p)| p.available) {
                    let it = Item::BoardPower(bd.id.clone(), k + 1);
                    feats.push((format!("icon-{}", p.id), menu::locked(u, &it), menu::owns(u, &it), focused == k + 1, base));
                }
            }
            let label = if focused == 0 { String::new() } else { bd.powerups.get(focused - 1).map(|p| title_case(&p.id)).unwrap_or_default() };
            (board_name(&bd.id), label, feats)
        }
    };
    let (x, y) = b.c(0.0, -200.0);
    b.text(&name, UiFont::Titan, 40.0, raw(NAVY, 1.0), x, y, 0.0, 0.0, None);
    if !outfit_label.is_empty() {
        let (x, y) = b.c(0.0, -150.0);
        b.text(&outfit_label, UiFont::Titan, 20.0, raw(NAVY, 1.0), x, y, 0.0, 0.0, None);
    }
    if m.tab == MeTab::Boards {
        if let Some(p) = &c.boards[m.board].powerup {
            // SPECIAL POWER tag (`ay`)
            let (x, y) = b.c(45.0, -95.0);
            b.frame_c("icon-board-powerup", x, y, 1.0, raw(0xCC5407, 1.0));
            let (x, y) = b.c(90.0, -130.0);
            b.text("SPECIAL POWER", UiFont::Lilita, 28.0, raw(0xCC5407, 1.0), x, y, 0.0, 0.0, None);
            let (x, y) = b.c(90.0, -100.0);
            b.text(&title_case(p), UiFont::Lilita, 28.0, raw(NAVY, 1.0), x, y, 0.0, 0.0, None);
        }
    }
    // feature buttons `ny` (btn-base-small 54x54, spacing 20) at y -80
    if feats.len() > 1 || m.tab == MeTab::Characters && feats.len() > 1 {
        for (i, (icon, locked, owned, focused, tint)) in feats.iter().enumerate() {
            let cx = 27.0 + i as f32 * 74.0;
            let (x, y) = b.c(cx, -80.0);
            // tintBase(colors[0]) / tintIcon(colors[1]); the focused one is a bit bigger
            let k = if *focused { 1.15 } else { 1.0 };
            b.frame("btn-base-small", x - 27.0 * k * s, y - 27.0 * k * s, 54.0 * k * s, 54.0 * k * s, raw(tint[0], 1.0), false);
            b.frame_c(icon, x, y, 0.8 * k, raw(tint[1], 1.0));
            let (mx, my) = b.c(cx + 22.0, -80.0 + 22.0);
            if *locked {
                b.frame_c("icon-lock-outfit", mx, my, 0.75, Color::WHITE);
            } else if *owned && i > 0 {
                b.frame_c("icon-owned", mx, my, 0.75, Color::WHITE);
            }
            b.hit(x - 27.0 * s, y - 27.0 * s, 54.0 * s, 54.0 * s, Click::MeFeature(i));
        }
    }
    // the big button `iy` (346x120 frame, 322x94 green r12) x0.9 at (150, 60)
    if let Some((_, state)) = menu::select_button(g) {
        let (label, alpha, currency) = match &state {
            SelectButton::Select => ("SELECT".to_string(), 1.0, None),
            SelectButton::Selected => ("SELECTED".to_string(), 0.5, None),
            SelectButton::Locked => ("LOCKED".to_string(), 0.5, None),
            SelectButton::TurnOn => ("TURN ON".to_string(), 1.0, None),
            SelectButton::TurnOff => ("TURN OFF".to_string(), 1.0, None),
            SelectButton::Buy(c, cur) => (c.to_string(), 1.0, Some(*cur)),
        };
        let k = 0.9;
        let (cx, cy) = (150.0, 60.0);
        let (fx, fy) = b.c(cx - 161.0 * k, cy - 47.0 * k);
        b.rect(fx, fy, 322.0 * k * s, 94.0 * k * s, raw(GREEN, alpha), 12.0 * k * s);
        let (bx, by) = b.c(cx - 173.0 * k + 5.0, cy - 60.0 * k + 6.0);
        b.sliced("box-border-grey", bx, by, 346.0 * k * s, 120.0 * k * s, 15.0, 15.0, 25.0, 25.0);
        let lx = if currency.is_some() { cx - 15.0 } else { cx };
        let (tx, ty) = b.c(lx, cy);
        b.text(&label, UiFont::Titan, 40.0 * k, raw(WHITE, alpha), tx, ty, 0.5, 0.5, None);
        if let Some(cur) = currency {
            let icon = if cur == "keys" { "icon-key" } else { "icon-coin" };
            let (ix, iy) = b.c(lx + text_w(&label, 36.0) / 2.0 + 15.0, cy);
            b.frame_c(icon, ix, iy, 0.9, Color::WHITE);
        }
        if alpha == 1.0 {
            let (hx, hy) = b.c(cx - 173.0 * k, cy - 60.0 * k);
            b.hit(hx, hy, 346.0 * k * s, 120.0 * k * s, Click::MeSelect);
        }
    }
    // the 3D preview (`_v` at (-170, 400) x1.3, anchor (0.5, 1)): the
    // 640-unit stage render, feet-side edge on y 75
    let view = b.ui.hs_view.clone();
    let size = 640.0 * 1.3;
    let (x, y) = b.c(-170.0 - size / 2.0, 75.0 - size);
    b.image(&view, x, y, size * s, size * s, Color::WHITE);
    // thumb strip (`sy`): thumb i at x = i*130 + 70 from the notepad's left
    // edge (+ scroll), feet on y 317, scale 0.45 (0.55 focused), clipped
    let (list, focus, selected): (Vec<String>, usize, usize) = match m.tab {
        MeTab::Characters => {
            let l = menu::characters();
            let sel = l.iter().position(|c| *c == u.selected_character).unwrap_or(0);
            (l.iter().map(|id| format!("preview-{id}")).collect(), m.character, sel)
        }
        MeTab::Boards => {
            let sel = c.boards.iter().position(|bd| bd.id == u.selected_board).unwrap_or(0);
            (c.boards.iter().map(|bd| format!("board-{}", bd.id)).collect(), m.board, sel)
        }
    };
    let (mx, my) = b.c(-317.0, 317.0 - 200.0);
    b.clip_begin(mx, my, 634.0 * s, 230.0 * s);
    for (i, key) in list.iter().enumerate() {
        let cx = -327.0 + m.scroll + i as f32 * 130.0 + 70.0;
        if !(-400.0..400.0).contains(&cx) {
            continue;
        }
        let k = if i == focus { 0.55 } else { 0.45 };
        let size = 384.0 * k * if m.tab == MeTab::Boards { 0.6 } else { 1.0 };
        if let Some(img) = b.ui.thumbs.get(key).cloned() {
            let (x, y) = b.c(cx - size / 2.0, 317.0 - size);
            b.image(&img, x, y, size * s, size * s, Color::WHITE);
        }
        let owned = match m.tab {
            MeTab::Characters => menu::owns(u, &Item::Character(menu::characters()[i].clone())),
            MeTab::Boards => menu::owns(u, &Item::Board(c.boards[i].id.clone())),
        };
        if owned {
            let (x, y) = b.c(cx, 317.0 - 45.0);
            b.frame_c(if i == selected { "icon-selected" } else { "icon-owned" }, x, y, 1.0, Color::WHITE);
        }
        let (hx, hy) = b.c(cx - 60.0, 317.0 - 190.0);
        b.hit(hx, hy, 120.0 * s, 190.0 * s, Click::MeThumb(i));
    }
    b.clip_end();
}

fn title_case(id: &str) -> String {
    id.split(['-', '_'])
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Board names (lang-en.json).
fn board_name(id: &str) -> String {
    match id {
        "greatwhite" => "Great White".into(),
        "bigkahuna" => "Big Kahuna".into(),
        "skullfire" => "Skull Fire".into(),
        "hotrod" => "Hot Rod".into(),
        other => title_case(other),
    }
}

/// Outfit names (lang-en.json, ui_powerups_shop.md §3.3).
fn outfit_name(id: &str, k: usize) -> String {
    let n = match (id, k) {
        ("jake", 1) => "Dark",
        ("jake", 2) => "Star",
        ("tricky", 1) => "Camo",
        ("tricky", 2) => "Heart",
        ("lucy", 1) => "Goth",
        ("lucy", 2) => "Steam",
        ("tagbot", 1) => "Space",
        ("tagbot", 2) => "Toy",
        ("ninja", 1) => "Yang",
        ("ninja", 2) => "Flame",
        ("tasha", 1) => "Cheer",
        ("tasha", 2) => "Gym",
        ("king", 1) => "Count",
        ("king", 2) => "Royal",
        ("brody", 1) => "Posh",
        ("brody", 2) => "Chill",
        _ => return format!("Outfit {k}"),
    };
    format!("{n} Outfit")
}

// ---- boost list `kv` / cards `Ov` --------------------------------------------------------

fn boost_title(id: &str) -> &'static str {
    match id {
        "hoverboard" => "Hoverboard",
        "mysteryBox" => "Mystery Box",
        "scoreBooster" => "Score Booster",
        "headstart" => "Headstart",
        "jetpack" => "Jetpack",
        "sneakers" => "Super Sneakers",
        "magnet" => "Coin Magnet",
        "multiplier" => "Score Multiplier",
        _ => "",
    }
}

fn boost_description(id: &str) -> &'static [&'static str] {
    match id {
        "hoverboard" => &["Protect yourself from crashing", "for 30 seconds. Activate by", "pressing Space."],
        "scoreBooster" => &["Use up to 3 times to increase your", "Multiplier by 5, 6, or 7", "for one run."],
        "headstart" => &["Use up to 3 times to skip ahead and", "start running at higher", "speed."],
        "jetpack" => &["Increase the duration of the", "Spray Can Jetpack powerup."],
        "sneakers" => &["Increase the duration of the", "Super Sneakers powerup."],
        "magnet" => &["Increase the duration of the", "Coin Magnet powerup."],
        "multiplier" => &["Increase the duration of the", "Double Multiplier powerup."],
        _ => &[],
    }
}

/// Height of card `i` (119 collapsed; open: body + 30 more).
fn card_h(i: usize, open: Option<usize>) -> f32 {
    if open != Some(i) {
        return 119.0;
    }
    let id = match menu::boost_item(i) {
        Some(Item::Consumable(id)) | Some(Item::Upgrade(id)) => id,
        _ => "",
    };
    let body = (boost_description(id).len().max(1) as f32) * 30.0;
    119.0 + body + 30.0
}

/// The boost list with its origin at `y0` (centre-relative units), clipped
/// to [clip_top, clip_bottom].
fn boost_list(b: &mut Builder, g: &Game, y0: f32, open: Option<usize>, clip: (f32, f32)) {
    let s = b.s;
    let u = &g.flow.user;
    let (cx0, cy0) = b.c(-327.0, clip.0);
    b.clip_begin(cx0, cy0, 654.0 * s, (clip.1 - clip.0) * s);
    let mut y = y0 + 10.0;
    let title_at = |b: &mut Builder, text: &str, y: f32| {
        let (x, yy) = b.c(0.0, y);
        b.text(text, UiFont::Titan, 70.0, raw(0x004A80, 1.0), x, yy, 0.5, 1.0, None);
    };
    title_at(b, "Single Use", y);
    y += 74.0;
    for i in 0..menu::BOOST_CARDS {
        if i == menu::CONSUMABLES.len() {
            title_at(b, "Upgrades", y + 10.0);
            y += 84.0;
        }
        let h = card_h(i, open);
        // the card's collapsed centre sits at the running y
        boost_card(b, g, i, y, h, open == Some(i), clip);
        y += h + 10.0;
    }
    b.clip_end();
    let _ = u;
}

fn boost_card(b: &mut Builder, g: &Game, i: usize, cy: f32, h: f32, open: bool, clip: (f32, f32)) {
    let s = b.s;
    let u = &g.flow.user;
    let Some(item) = menu::boost_item(i) else { return };
    let id = match &item {
        Item::Consumable(id) | Item::Upgrade(id) => *id,
        _ => return,
    };
    let top = cy - 59.5;
    if top > clip.1 || top + h < clip.0 {
        return;
    }
    // panel: top / stretched middle / flipped bottom
    let (x, y) = b.c(-307.0, top);
    b.frame("upgrade-panel-top", x, y, 614.0 * s, 34.0 * s, Color::WHITE, false);
    let mid = h - 34.0 - 34.0 + 2.0;
    let (x, y) = b.c(-307.0, top + 33.0);
    b.frame("upgrade-panel-middle", x, y, 614.0 * s, mid * s, Color::WHITE, false);
    let (x, y) = b.c(-307.0, top + 33.0 + mid - 1.0);
    b.frame("upgrade-panel-bottom", x, y, 614.0 * s, 34.0 * s, Color::WHITE, true);
    // icon tile + icon
    let (x, y) = b.c(-246.75, cy);
    b.frame_c("base-item", x, y, 1.15, Color::WHITE);
    b.frame_c(&format!("icon-item-{}", id.to_lowercase()), x, y, 1.035, Color::WHITE);
    let (x, y) = b.c(-196.5, cy - 44.5);
    b.text(boost_title(id), UiFont::Lilita, 42.0, raw(0xEECC32, 1.0), x, y, 0.0, 0.0, None);
    let cost = match &item {
        Item::Consumable(_) => {
            let sub = if id == "mysteryBox" { "Open immediately".to_string() } else { format!("You have: {}", menu::consumable_count(u, id)) };
            let (x, y) = b.c(-196.5, cy + 5.0);
            b.text(&sub, UiFont::Lilita, 25.0, raw(WHITE, 1.0), x, y, 0.0, 0.0, None);
            Some(menu::consumable_cost(id))
        }
        _ => {
            // tier pips `Dv`: upgrades-slot + #FFCC00 28x15 r5 at (27 + 41.5 i, 15)
            let (x, y) = b.c(-196.5, cy + 5.0);
            b.frame("upgrades-slot", x, y, 267.0 * s, 30.0 * s, Color::WHITE, false);
            for k in 0..menu::upgrade_level(u, id) {
                let (px, py) = b.c(-196.5 + 27.0 + 41.5 * k as f32 - 14.0, cy + 5.0 + 15.0 - 7.5);
                b.rect(px, py, 28.0 * s, 15.0 * s, raw(0xFFCC00, 1.0), 5.0 * s);
            }
            menu::upgrade_cost(u, id)
        }
    };
    if !open {
        // price tag (right-aligned number + coin) and the hint arrow
        if let Some(c) = cost {
            let (x, y) = b.c(261.0, top + 40.0);
            b.frame_c("icon-coin", x, y, 0.75, Color::WHITE);
            let (x, y) = b.c(236.0, top + 40.0);
            b.text(&c.to_string(), UiFont::Lilita, 30.0, raw(0xF6F6F6, 1.0), x, y, 1.0, 0.5, Some(1.0));
        }
        let (x, y) = b.c(163.0, cy + 15.5);
        b.frame("arrow-down-hint", x, y, 104.0 * s, 37.0 * s, Color::WHITE, false);
    } else {
        for (k, line) in boost_description(id).iter().enumerate() {
            let (x, y) = b.c(-287.0, cy + 59.5 + 30.0 * k as f32);
            b.text(line, UiFont::Lilita, 25.0, raw(WHITE, 1.0), x, y, 0.0, 0.0, None);
        }
        // buy button `Ev`: w = len(cost) * 42, h 70, green + coin + value
        if let Some(c) = cost {
            let txt = c.to_string();
            let w = txt.len() as f32 * 42.0;
            let bcx = 307.0 - (w + 24.0) / 2.0 - 15.0;
            let bcy = top + h - 50.0;
            let (fx, fy) = b.c(bcx - w / 2.0, bcy - 35.0);
            b.rect(fx, fy, w * s, 70.0 * s, raw(GREEN, 1.0), 12.0 * s);
            let (bx, by) = b.c(bcx - (w + 24.0) / 2.0 + 5.0, bcy - 48.0 + 6.0);
            b.sliced("box-border-grey", bx, by, (w + 24.0) * s, 96.0 * s, 15.0, 15.0, 25.0, 25.0);
            let tw = text_w(&txt, 35.0);
            let (ix, iy) = b.c(bcx - tw / 2.0 - 5.0, bcy);
            b.frame_c("icon-coin", ix, iy, 0.75, Color::WHITE);
            let (tx, ty) = b.c(bcx + 18.0, bcy + 2.0);
            b.text(&txt, UiFont::Lilita, 35.0, raw(WHITE, 1.0), tx, ty, 0.5, 0.5, Some(1.0));
            let (hx, hy) = b.c(bcx - (w + 24.0) / 2.0, bcy - 48.0);
            b.hit(hx, hy, (w + 24.0) * s, 96.0 * s, Click::BoostBuy(i));
        }
    }
    // the whole card toggles it
    let vis_top = top.max(clip.0);
    let vis_bot = (top + h).min(clip.1);
    if vis_bot > vis_top {
        let (x, y) = b.c(-307.0, vis_top);
        b.hit(x, y, 614.0 * s, (vis_bot - vis_top) * s, Click::BoostCard(i));
    }
}

fn boost_shop(b: &mut Builder, g: &Game, open: Option<usize>, scroll: f32) {
    notepad(
        b,
        &[
            ("navigation-button-grey", "icon-white-back", "Menu", Click::Back),
            ("navigation-button-blu", "icon-white-upgrades", "Boosts", Click::Elsewhere),
        ],
    );
    let u = &g.flow.user;
    currencies(b, 654.0 / 2.0 - 20.0, -345.0, u.keys, u.coins);
    // list at yScrollMax -242 (+ scroll), mask y [-312, 342]
    boost_list(b, g, -242.0 + scroll, open, (-312.0, 342.0));
}

/// `Pv` over the results notepad.
fn boost_panel(b: &mut Builder, g: &Game, open: Option<usize>, scroll: f32) {
    let (w, h, s) = (b.w, b.h, b.s);
    b.rect(0.0, 0.0, w, h, raw(0x000000, 0.75), 0.0);
    b.hit(0.0, 0.0, w, h, Click::CloseOverlay);
    rect_comp(b, 0.0, 0.0, 690.0, 935.0, 712.0, 959.0, raw(0xEEEEEE, 1.0), "box-border-grey", None);
    let (x, y) = b.c(-345.0, -467.5);
    b.hit(x, y, 690.0 * s, 935.0 * s, Click::Elsewhere);
    let u = &g.flow.user;
    currencies(b, 345.0 - 20.0, -935.0 / 2.0 + 31.0, u.keys, u.coins);
    boost_list(b, g, -292.5 + scroll, open, (-377.5, 467.5));
    let (x, y) = b.c(-335.0, -457.5);
    b.frame_c("btn-close", x, y, 1.0, Color::WHITE);
    b.hit(x - 43.0 * s, y - 43.0 * s, 86.0 * s, 86.0 * s, Click::CloseOverlay);
    // first match wins: the close button and the cards before the panel
    // and the backdrop (pushed first above)
    let mut hits = std::mem::take(&mut b.hits);
    let background: Vec<_> = hits.drain(..2).collect();
    hits.extend(background.into_iter().rev());
    b.hits = hits;
}

// ---- Fv / Ny / countdown -----------------------------------------------------------------

/// "Need hoverboards?" (`Fv`, §3.6).
pub(crate) fn buy_boards(b: &mut Builder, g: &Game, bump: u32) {
    let (w, h, s) = (b.w, b.h, b.s);
    b.rect(0.0, 0.0, w, h, raw(0x000000, 0.75), 0.0);
    rect_comp(b, 0.0, 0.0, 540.0, 540.0, 562.0, 564.0, raw(0xEEEEEE, 1.0), "box-border-grey", None);
    let (x, y) = b.c(-255.0, 40.0 - 220.0);
    b.rect(x, y, 510.0 * s, 440.0 * s, raw(0x2795D2, 1.0), 0.0);
    let k = if bump > 0 { 1.0 + 0.2 * (bump as f32 / 15.0) } else { 1.0 };
    let (x, y) = b.c(0.0, -20.0);
    b.frame_c("hoverboard-large", x, y, k, Color::WHITE);
    let (x, y) = b.c(0.0, -130.0);
    b.text("Need hoverboards?", UiFont::Titan, 36.0, raw(WHITE, 1.0), x, y, 0.5, 0.5, Some(1.0));
    let (x, y) = b.c(0.0, 80.0);
    b.text(&format!("You have: {}", g.flow.user.hoverboards), UiFont::Lilita, 26.0, raw(WHITE, 1.0), x, y, 0.5, 0.5, None);
    // buy `Ev` 172x64 + coin + 300
    let (fx, fy) = b.c(-86.0, 140.0 - 32.0);
    b.rect(fx, fy, 172.0 * s, 64.0 * s, raw(GREEN, 1.0), 12.0 * s);
    let (bx, by) = b.c(-98.0 + 5.0, 140.0 - 45.0 + 6.0);
    b.sliced("box-border-grey", bx, by, 196.0 * s, 90.0 * s, 15.0, 15.0, 25.0, 25.0);
    let (ix, iy) = b.c(-30.0, 140.0);
    b.frame_c("icon-coin", ix, iy, 0.75, Color::WHITE);
    let (tx, ty) = b.c(18.0, 142.0);
    b.text(&crate::flow::BOARD_COST.to_string(), UiFont::Lilita, 35.0, raw(WHITE, 1.0), tx, ty, 0.5, 0.5, Some(1.0));
    let (hx, hy) = b.c(-98.0, 140.0 - 45.0);
    b.hit(hx, hy, 196.0 * s, 90.0 * s, Click::BuyBoard);
    for (k, line) in ["Protect yourself from crashing for 30 seconds.", "Press Space to activate when running."].iter().enumerate() {
        let (x, y) = b.c(0.0, 215.0 + 20.0 * k as f32);
        b.text(line, UiFont::Lilita, 20.0, raw(0xACE5FE, 1.0), x, y, 0.5, 0.5, None);
    }
    let u = &g.flow.user;
    currencies(b, 270.0 - 50.0 + 60.0, -220.0, u.keys, u.coins);
    let (x, y) = b.c(-260.0, -260.0);
    b.frame_c("btn-close", x, y, 1.0, Color::WHITE);
    b.hit(x - 43.0 * s, y - 43.0 * s, 86.0 * s, 86.0 * s, Click::CloseOverlay);
    b.hit(0.0, 0.0, w, h, Click::CloseOverlay);
}

/// Not enough coins / keys (`Ny`, §3.7): any tap closes it.
fn not_enough(b: &mut Builder, currency: &str, needed: i64) {
    let (w, h) = (b.w, b.h);
    b.rect(0.0, 0.0, w, h, raw(0x000000, 0.95), 0.0);
    rect_comp(b, 0.0, 0.0, 600.0, 300.0, 622.0, 324.0, raw(0xEEEEEE, 1.0), "box-border-grey", None);
    let (x, y) = b.c(168.0, -20.0);
    b.frame_c("background-splat", x, y, 1.0, raw(0x70569A, 1.0));
    let (x, y) = b.c(165.0, -23.0);
    b.frame_c("background-splat", x, y, 1.0, raw(0xAC97E8, 1.0));
    let (x, y) = b.c(168.0, -40.0);
    b.frame_c(if currency == "keys" { "icon-key-large" } else { "icon-coin-large" }, x, y, 1.0, Color::WHITE);
    let (x, y) = b.c(-260.0, -100.0);
    b.text("Not enough", UiFont::Titan, 50.0, raw(0x074B7E, 1.0), x, y, 0.0, 0.0, None);
    let (x, y2) = b.c(-260.0, -42.0);
    b.text(&format!("{currency}!"), UiFont::Titan, 50.0, raw(0x074B7E, 1.0), x, y2, 0.0, 0.0, None);
    let (x, y3) = b.c(-260.0, 36.0);
    let unit = if currency == "keys" { "Keys" } else { "Coins" };
    b.text(&format!("{needed} {unit} needed"), UiFont::Titan, 30.0, raw(0x19669B, 1.0), x, y3, 0.0, 0.0, None);
    let (x, y) = b.c(-290.0, -140.0);
    b.frame_c("btn-close", x, y, 1.0, Color::WHITE);
    b.hit(0.0, 0.0, w, h, Click::CloseOverlay);
}

/// `hud.runCountdown(3)` (`Nm`): "Starting in\n{n}", Titan One 80 white,
/// centred, no backdrop.
pub(crate) fn countdown(b: &mut Builder, n: u32) {
    let (x, y) = b.c(0.0, -45.0);
    b.text("Starting in", UiFont::Titan, 80.0, raw(WHITE, 1.0), x, y, 0.5, 0.5, None);
    let (x, y) = b.c(0.0, 45.0);
    b.text(&n.to_string(), UiFont::Titan, 80.0, raw(WHITE, 1.0), x, y, 0.5, 0.5, None);
}

/// The pause panel `ph` (46385, pause_reset.md §2.2): two blur backdrops,
/// the notepad with the Word Hunt section and the missions section `fh`,
/// and the bottom row: Menu (house), Settings and RESUME.
pub(crate) fn pause_panel(b: &mut Builder, g: &crate::game::Game) {
    let s = b.s;
    backdrop_blur(b);
    notepad(
        b,
        &[
            ("navigation-button-grey", "icon-white-house", "Menu", Click::PauseMenu),
            ("navigation-button-blu", "icon-settings", "Settings", Click::PauseSettings),
        ],
    );
    // Word Hunt / Missions headers and the dashed separator
    let navy = raw(0x07294A, 1.0);
    let (x, y) = b.c(-282.0, -392.0);
    b.text("Word Hunt", UiFont::Titan, 40.0, navy, x, y, 0.0, 0.0, None);
    let (x, y) = b.c(-282.0, -392.0 + 42.2);
    b.text("COLLECT LETTERS", UiFont::Lilita, 35.0, raw(0x6FB500, 1.0), x, y, 0.0, 0.0, None);
    word_hunt_section(b, g);
    for i in 0..22 {
        let (x, y) = b.c(-297.0 + i as f32 * (19.364 + 8.0), -125.0 - 2.5);
        b.rect(x, y, 19.364 * s, 5.0 * s, navy, 0.0);
    }
    crate::ui_missions::pause_missions(b, g);
    // RESUME: large-navigation-button-light-green x0.83 at (141.76, 412)
    let (x, y) = b.c(141.76, 412.0);
    let (fw, fh) = (320.0 * 0.83 * s, 120.0 * 0.83 * s);
    let t = b.press_tint(x - fw / 2.0, y - fh / 2.0, fw, fh);
    b.frame("large-navigation-button-light-green", x - fw / 2.0, y - fh / 2.0, fw, fh, t, false);
    b.text("RESUME", UiFont::Lilita, 55.0, raw(WHITE, 1.0), x, y, 0.5, 0.5, None);
    b.hit(x - fw / 2.0, y - fh / 2.0, fw, fh, Click::PauseResume);
}

// ---- Word Hunt (`crate::word_hunt`) ------------------------------------------------------

/// Titan One advance widths (em) of A-Z (from assets/fonts/titan-one.ttf).
const TITAN_ADVANCE: [f32; 26] = [
    0.738, 0.705, 0.656, 0.731, 0.610, 0.586, 0.674, 0.727, 0.359, 0.410, 0.709, 0.544, 0.880, 0.734, 0.756, 0.707, 0.746, 0.716, 0.654, 0.651,
    0.726, 0.727, 1.016, 0.721, 0.698, 0.627,
];

/// pixi Text width of one upper-case letter (advance + drop shadow).
fn letter_width(c: char, size: f32, shadow: f32) -> f32 {
    let i = (c as u8).wrapping_sub(b'A') as usize;
    TITAN_ADVANCE.get(i).copied().unwrap_or(0.7) * size + shadow
}

/// The letters' x in their container (`dg` / `rh` layout): each after the
/// previous half widths plus 15, centred in `base_w`; and the container's
/// scale to fit `max_w`.
fn letter_layout(word: &str, size: f32, shadow: f32, base_w: f32, max_w: f32) -> (Vec<f32>, f32) {
    let ws: Vec<f32> = word.chars().map(|c| letter_width(c, size, shadow)).collect();
    let mut xs = Vec::new();
    let mut acc = 0.0;
    for (i, w) in ws.iter().enumerate() {
        acc += if i > 0 { ws[i - 1] / 2.0 } else { 0.0 } + w / 2.0;
        xs.push(acc - base_w / 2.0 + i as f32 * 15.0);
    }
    let span = xs[xs.len() - 1] + ws[ws.len() - 1] / 2.0 - (xs[0] - ws[0] / 2.0);
    let scale = if span > max_w { max_w / span } else { 1.0 };
    let shift = (base_w - span) / 2.0;
    (xs.into_iter().map(|x| x + shift).collect(), scale)
}

const LETTER_YELLOW: u32 = 0xFFCC00;

/// The HUD drop-down `dg` (47450): at the top centre, 300 x 90 `#3A8BBA`;
/// the word in Titan One 53 (collected letters yellow); after the last
/// letter the "Word Hunt Complete" panel with the reward.
pub(crate) fn word_banner(b: &mut Builder, g: &crate::game::Game) {
    let (Some(banner), Some(hunt)) = (g.hud.word_banner, g.word_hunt.as_ref()) else { return };
    let s = b.s;
    let word = hunt.word.to_ascii_uppercase();
    let drop = banner.drop() as f32;
    if drop <= 0.0 {
        return;
    }
    let (w, h) = (300.0, 90.0);
    let cx = b.vw() / 2.0;
    let cy = -h / 2.0 + (h + 20.0) * drop;
    let border = 5.0;
    b.rect((cx - w / 2.0 - border) * s, (cy - h / 2.0 - border) * s, (w + 2.0 * border) * s, (h + 2.0 * border) * s, raw(WHITE, 1.0), 22.0 * s);
    b.rect((cx - w / 2.0) * s, (cy - h / 2.0) * s, w * s, h * s, raw(0x3A8BBA, 1.0), 18.0 * s);
    if banner.complete_shown {
        // onCompletePanel: reward box (100 x 90 #226184 + 50 x 90) and the text
        let bx = cx + 56.0 - w / 2.0;
        b.rect((bx - 50.0) * s, (cy - h / 2.0) * s, 125.0 * s, h * s, raw(0x226184, 1.0), 18.0 * s);
        match hunt.reward() {
            Some(crate::word_hunt::Reward::Coins(n)) => {
                b.frame_c("icon-coin-large", (bx - 22.0) * s, cy * s, 0.45, Color::WHITE);
                b.text(&n.to_string(), UiFont::Lilita, 34.0, raw(WHITE, 1.0), (bx + 8.0) * s, cy * s, 0.0, 0.5, Some(2.0));
            }
            Some(crate::word_hunt::Reward::Box(kind)) => b.frame_c(&format!("icon-item-{kind}"), bx * s, cy * s, 0.6, Color::WHITE),
            None => {}
        }
        let (tx, ty) = ((cx + 30.0) * s, cy * s);
        b.text_shadow("Word Hunt", UiFont::Titan, 30.0, raw(WHITE, 1.0), tx, ty - 17.0 * s, 0.2, 0.5, Some((2.0, raw(0x000000, 1.0))));
        b.text_shadow("Complete", UiFont::Titan, 30.0, raw(WHITE, 1.0), tx, ty + 17.0 * s, 0.2, 0.5, Some((2.0, raw(0x000000, 1.0))));
        return;
    }
    let (xs, scale) = letter_layout(&word, 53.0, 2.0, w, 260.0);
    let (tint, grow) = banner.progress();
    for (i, c) in word.chars().enumerate() {
        let collected = i < banner.letter;
        let (col, k) = if i == banner.letter {
            let a = raw(WHITE, 1.0).to_linear();
            let y = raw(LETTER_YELLOW, 1.0).to_linear();
            (Color::LinearRgba(a.mix(&y, tint as f32)), grow as f32)
        } else if collected {
            (raw(LETTER_YELLOW, 1.0), 1.0)
        } else {
            (raw(WHITE, 1.0), 1.0)
        };
        b.text_shadow(&c.to_string(), UiFont::Titan, 53.0 * scale * k, col, (cx + xs[i] * scale) * s, cy * s, 0.5, 0.5, Some((2.0, raw(0x000000, 1.0))));
    }
}

/// The pause panel's Word Hunt section `ah` (45977, pause_reset.md): the day
/// timer, the letters box `rh` (the word in Titan One 72: collected
/// `#FFCC00` / `#36373B` shadow, the rest `#36373B` / `#6990BE`) with the
/// dashed divider and the mystery box reward.
pub(crate) fn word_hunt_section(b: &mut Builder, g: &crate::game::Game) {
    let s = b.s;
    let top = -392.0;
    // day timer `ih` at (208.5, 42.2): "{23-UTCh}h {59-UTCm}m" (wall clock)
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) % 86400;
    let (hh, mm) = (23 - secs / 3600, 59 - (secs % 3600) / 60);
    let (tx, ty) = (208.5, top + 42.2);
    let (x, y) = b.c(tx - 73.5, ty - 19.5);
    b.rect(x, y, 147.0 * s, 39.0 * s, raw(0x6FB500, 1.0), 19.5 * s);
    let (ix, iy) = b.c(tx - 52.55, ty);
    b.frame_c("timer-empty-icon", ix, iy, 0.9, Color::WHITE);
    let (lx, ly) = b.c(tx - 33.6, ty);
    b.text(&format!("{hh}h {mm}m"), UiFont::Lilita, 28.0, raw(WHITE, 1.0), lx, ly, 0.0, 0.5, None);
    // letters box `rh` at (0, 163.84) from the section
    let by = top + 163.84;
    let (x, y) = b.c(-282.0, by - 61.0);
    b.rect(x, y, 564.0 * s, 122.0 * s, raw(0x5181AA, 1.0), 20.0 * s);
    let Some(hunt) = g.word_hunt.as_ref() else { return };
    let word = hunt.word.to_ascii_uppercase();
    // letters fit the space left of the divider, centred in it
    let room = 564.0 / 2.0 + 110.0 - 2.0;
    let (xs, _) = letter_layout(&word, 72.0, 3.0, 564.0, f32::MAX);
    let first = xs[0] - letter_width(word.chars().next().unwrap(), 72.0, 3.0) / 2.0;
    let last_c = word.chars().last().unwrap();
    let span = xs[xs.len() - 1] + letter_width(last_c, 72.0, 3.0) / 2.0 - first;
    let scale = if span > room - 40.0 { (room - 40.0) / span } else { 1.0 };
    let width = span * scale;
    let cx0 = -282.0 + width / 2.0 + (room - width) / 2.0;
    let mid = first + span / 2.0;
    for (i, c) in word.chars().enumerate() {
        let got = i < hunt.index;
        let (fill, shadow) = if got { (LETTER_YELLOW, 0x36373B) } else { (0x36373B, 0x6990BE) };
        let (x, y) = b.c(cx0 + (xs[i] - mid) * scale, by);
        b.text_shadow(&c.to_string(), UiFont::Titan, 72.0 * scale, raw(fill, 1.0), x, y, 0.5, 0.5, Some((3.0, raw(shadow, 1.0))));
    }
    // divider: 6 white dashes, 4 wide, at x 110
    for k in 0..6 {
        let (x, y) = b.c(110.0 - 2.0, by - 61.0 + k as f32 * (122.0 / 6.0) + 4.0);
        b.rect(x, y, 4.0 * s, 122.0 / 6.0 * 0.6 * s, raw(WHITE, 1.0), 0.0);
    }
    // reward: mystery box icon and "Mystery"
    let (x, y) = b.c(200.0, by - 14.0);
    b.frame_c("mystery-box-icon", x, y, 0.6, Color::WHITE);
    let (x, y) = b.c(200.0, by + 42.0);
    b.text("Mystery", UiFont::Lilita, 24.0, raw(WHITE, 1.0), x, y, 0.5, 0.5, Some(2.0));
}
