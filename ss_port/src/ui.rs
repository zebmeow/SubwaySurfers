//! The 2D UI: the in-game HUD (`fg`) and the death-flow screens ("Save
//! me!", Not enough keys, New High Score, results notepad), drawn from
//! [`crate::flow`] state with the original's atlas (`assets/ui/ui.webp`) and
//! fonts (Lilita One, Titan One; TTF conversions of the shipped woff2).
//! Layout follows docs/js_notes/ui_hud.md and ui_gameover.md: a virtual
//! space scaled by `S = (H / base) * 0.5` (base 500 in landscape).
//!
//! The UI is a Bevy UI overlay: no world bend, no fog. Like pixi drawing
//! into the same non-sRGB canvas, it blends into the gamma-encoded target
//! (the sRGB decode runs after the UI pass), so colours are given as raw
//! sRGB values and the atlas is loaded without sRGB decoding.
//!
//! Rebuilt every frame (immediate mode); clicks are hit-tested against the
//! rectangles laid out in the same pass.
//!
//! The menus (title, Me panel, boost shop, modals) are in [`crate::ui_menu`].
//!
//! Not drawn: the 3D character renders of the high-score and results
//! screens, the celebration stripes, the in-run boost buttons (headstart,
//! score booster), the pause panel.

use crate::flow::{self, Click, Screen};
use crate::sim_plugin::Sim;
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;
use std::collections::HashMap;

/// Atlas frame rectangles (`ui.webp.json`).
#[derive(Resource, Default)]
pub struct UiAssets {
    pub atlas: Handle<Image>,
    pub frames: HashMap<String, URect>,
    pub lilita: Handle<Font>,
    pub titan: Handle<Font>,
    /// The clock pie (redrawn per frame).
    pub pie: Handle<Image>,
    /// 1x1 white, for solid fills (pixi `Graphics`).
    pub white: Handle<Image>,
    /// Me-panel thumbs: `preview-<character>` (character-previews/) and
    /// `board-<id>` (assets/ui/thumbs, tools/thumbs.py).
    pub thumbs: HashMap<String, Handle<Image>>,
    /// The New High Score character (`Xv`): `crate::render_menu_3d` renders the
    /// active character into it (640 x 640, raw values like the atlas).
    pub hs_view: Handle<Image>,
    /// `Z_()`: the thumbs' 2D floor shadow (128 x 128 radial gradient).
    pub blob: Handle<Image>,
    /// The prize screen's rays (`lh` with `raysAlpha`, masked), baked from
    /// the atlas once it has loaded ([`bake_rays`]).
    pub rays: Option<Handle<Image>>,
}

/// Clickable rectangles of the last build (screen px) and what they do.
#[derive(Resource, Default)]
pub struct UiHits(pub Vec<(Rect, Click)>);

/// The button held down (its hit rectangle, px): `Am.onPointerDown` tints
/// its base #AAAAAA until the pointer comes up.
#[derive(Resource, Default)]
pub struct UiPress(pub Option<Rect>);

/// A hit that is a button (pixi `buttonMode`: pointer cursor, press tint):
/// not the whole-window taps (the title's tap area, backdrops) or a panel's
/// catch-all.
fn is_button(r: &Rect, c: Click, w: f32, h: f32) -> bool {
    let whole = r.width() >= w - 1.0 && r.height() >= h - 1.0;
    !whole && !matches!(c, Click::Elsewhere | Click::StartGame | Click::PromptClose)
}

/// The pointer cursor over buttons.
fn hover_cursor(mut commands: Commands, windows: Query<(Entity, &Window)>, hits: Res<UiHits>, mut shown: Local<Option<bool>>) {
    let Ok((e, win)) = windows.single() else { return };
    let over = win.cursor_position().is_some_and(|p| hits.0.iter().find(|(r, _)| r.contains(p)).is_some_and(|(r, c)| is_button(r, *c, win.width(), win.height())));
    if *shown != Some(over) {
        *shown = Some(over);
        let icon = if over { bevy::window::SystemCursorIcon::Pointer } else { bevy::window::SystemCursorIcon::Default };
        commands.entity(e).insert(bevy::window::CursorIcon::System(icon));
    }
}

#[derive(Component)]
struct UiRoot;

/// Font metrics: pixi's measured ascent/descent (glyph bounds of `|ÉqÅM`
/// + 0.4875 px per 50 px, as Chrome measures) and the hhea metrics Bevy
/// lays lines out with, in em.
#[derive(Clone, Copy)]
pub enum UiFont {
    Lilita,
    Titan,
}

impl UiFont {
    fn metrics(self) -> (f32, f32, f32, f32) {
        match self {
            UiFont::Lilita => (0.92225, 0.21925, 0.922, 0.219),
            UiFont::Titan => (0.91, 0.145, 0.970, 0.175),
        }
    }
}

/// The width of `text` in units at `size` (the font's advance widths, as
/// pixi's `measureText` without kerning).
pub(crate) fn measure(font: UiFont, size: f32, text: &str) -> f32 {
    use std::collections::HashMap as Map;
    type Adv = Map<char, f32>;
    static T: std::sync::OnceLock<(Adv, Adv)> = std::sync::OnceLock::new();
    let (lilita, titan) = T.get_or_init(|| {
        let load = |name: &str| -> Adv {
            let Ok(bytes) = std::fs::read(crate::install::Install::locate().port.join("assets/fonts").join(name)) else { return Map::new() };
            let Ok(face) = ttf_parser::Face::parse(&bytes, 0) else { return Map::new() };
            let upem = face.units_per_em() as f32;
            (' '..='~')
                .chain("ÀÁÂÃÄÅÇÈÉÊËÌÍÎÏÑÒÓÔÕÖÙÚÛÜàáâãäåçèéêëìíîïñòóôõöùúûüß…×".chars())
                .filter_map(|c| face.glyph_index(c).and_then(|gi| face.glyph_hor_advance(gi)).map(|a| (c, a as f32 / upem)))
                .collect()
        };
        (load("lilita-one.ttf"), load("titan-one.ttf"))
    });
    let t = match font {
        UiFont::Lilita => lilita,
        UiFont::Titan => titan,
    };
    text.chars().map(|c| t.get(&c).copied().unwrap_or(0.55)).sum::<f32>() * size
}

/// Raw sRGB colour (the target holds encoded values).
pub(crate) fn raw(rgb: u32, a: f32) -> Color {
    Color::LinearRgba(LinearRgba::new(((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0, a))
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiAssets>()
            .init_resource::<UiHits>()
            .add_systems(Startup, load_assets)
            .init_resource::<UiPress>()
            .add_systems(Update, (click_ui, type_text, bake_rays, hover_cursor))
            // The rebuilt tree must be stacked (and laid out) in the frame it
            // is built. Bevy leaves `UiSystems::Stack` unordered in
            // PostUpdate: when `ui_stack_system` ran first, every new node
            // kept stack index 0 for that frame, so the renderer sorted all
            // text above all images (a modal's card under the pause
            // panel's text, a mission's count over its tick), a flicker on
            // every rebuild.
            .add_systems(PostUpdate, build_ui.before(bevy::ui::UiSystems::Prepare).before(bevy::ui::UiSystems::Stack));
    }
}

fn load_assets(mut ui: ResMut<UiAssets>, assets: Res<AssetServer>, mut fonts: ResMut<Assets<Font>>, mut images: ResMut<Assets<Image>>, mut commands: Commands) {
    let inst = crate::install::Install::locate();
    // pixi: no mipmaps, linear filter; raw values (no sRGB decode)
    ui.atlas = assets
        .load_builder()
        .with_settings(|s: &mut ImageLoaderSettings| {
            s.is_srgb = false;
            s.sampler = ImageSampler::linear();
        })
        .load("assets/ui/ui.webp");
    if let Ok(text) = std::fs::read_to_string(inst.site.join("assets/ui/ui.webp.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            for (name, f) in v["frames"].as_object().into_iter().flatten() {
                let r = &f["frame"];
                let g = |k: &str| r[k].as_u64().unwrap_or(0) as u32;
                ui.frames.insert(name.trim_end_matches(".png").to_string(), URect::new(g("x"), g("y"), g("x") + g("w"), g("y") + g("h")));
            }
        }
    }
    let font = |name: &str| std::fs::read(inst.port.join("assets/fonts").join(name)).ok().and_then(|b| Some(Font::from_bytes(b)));
    if let Some(f) = font("lilita-one.ttf") {
        ui.lilita = fonts.add(f);
    }
    if let Some(f) = font("titan-one.ttf") {
        ui.titan = fonts.add(f);
    }
    ui.pie = images.add(pie_image(1.0));
    {
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        let img = Image::new(Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, TextureDimension::D2, vec![255; 4], TextureFormat::Rgba8Unorm, default());
        ui.white = images.add(img);
    }
    ui.hs_view = images.add(crate::render_menu_3d::target_image());
    ui.blob = images.add(crate::render_menu_3d::blob_image());
    // Me-panel thumbs (raw values, like the atlas)
    for id in crate::shop::Catalog::get().roster.clone() {
        let h = assets
            .load_builder()
            .with_settings(|s: &mut ImageLoaderSettings| {
                s.is_srgb = false;
                s.sampler = ImageSampler::linear();
            })
            .load(format!("assets/character-previews/preview-{id}.webp"));
        ui.thumbs.insert(format!("preview-{id}"), h);
    }
    if let Ok(dir) = std::fs::read_dir(inst.port.join("assets/ui/thumbs")) {
        use bevy::image::{CompressedImageFormats, ImageType};
        for e in dir.flatten() {
            let path = e.path();
            let Some(name) = path.file_stem().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            let Ok(bytes) = std::fs::read(&path) else { continue };
            if let Ok(img) = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, false, ImageSampler::linear(), bevy::asset::RenderAssetUsages::default()) {
                ui.thumbs.insert(name, images.add(img));
            }
        }
    }
    commands.spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, UiRoot));
}

/// The clock's elapsed wedge (#EEEEEE, clockwise from 12 o'clock) on a
/// transparent 64x64 image covering radius 43 units.
fn pie_image(remaining: f64) -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let n = 64usize;
    let mut data = vec![0u8; n * n * 4];
    let elapsed = (1.0 - remaining).clamp(0.0, 1.0) * std::f64::consts::TAU;
    for y in 0..n {
        for x in 0..n {
            let dx = (x as f64 + 0.5) - n as f64 / 2.0;
            let dy = (y as f64 + 0.5) - n as f64 / 2.0;
            if dx * dx + dy * dy > (n as f64 / 2.0).powi(2) {
                continue;
            }
            // angle clockwise from 12 o'clock
            let a = dx.atan2(-dy).rem_euclid(std::f64::consts::TAU);
            if a < elapsed {
                let o = (y * n + x) * 4;
                data[o..o + 4].copy_from_slice(&[0xEE, 0xEE, 0xEE, 0xFF]);
            }
        }
    }
    let mut img = Image::new(Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8Unorm, default());
    img.sampler = ImageSampler::linear();
    img
}

/// One frame's UI under construction (screen px, top-left origin).
pub(crate) struct Builder<'a, 'w, 's> {
    pub(crate) commands: &'a mut Commands<'w, 's>,
    pub(crate) root: Entity,
    pub(crate) ui: &'a UiAssets,
    /// UI scale `S`, window size.
    pub(crate) s: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) hits: Vec<(Rect, Click)>,
    /// Pixel sizes of loaded images (for cropping whole images).
    pub(crate) image_sizes: &'a HashMap<AssetId<Image>, Vec2>,
    pub(crate) z: i32,
    /// Current parent (root, or a clip container) and its origin (px).
    parent: Option<(Entity, Vec2)>,
    /// Rotation for the next nodes (radians, pixi `rotation`).
    pub(crate) rotation: f32,
    /// Active clip rectangle (px): images and fills are cropped to it by
    /// hand (Bevy drops image nodes under a clipping parent here); text goes
    /// into the clipping container.
    pub(crate) clip: Option<Rect>,
    /// The held button's hit rectangle ([`UiPress`]).
    pub(crate) pressed: Option<Rect>,
}

impl Builder<'_, '_, '_> {
    pub(crate) fn vw(&self) -> f32 {
        self.w / self.s
    }
    pub(crate) fn vh(&self) -> f32 {
        self.h / self.s
    }
    /// Siblings draw in pixi tree order (explicit z: Bevy batches sliced
    /// images separately otherwise).
    pub(crate) fn spawn(&mut self, b: impl Bundle) {
        self.z += 1;
        let e = self.commands.spawn((b, GlobalZIndex(self.z))).id();
        if self.rotation != 0.0 {
            self.commands.entity(e).insert(UiTransform::from_rotation(Rot2::radians(self.rotation)));
        }
        match self.parent {
            Some((p, o)) => {
                // re-express the absolute position relative to the clip origin
                self.commands.entity(e).entry::<Node>().and_modify(move |mut n| {
                    if let Val::Px(l) = n.left {
                        n.left = Val::Px(l - o.x);
                    }
                    if let Val::Px(t) = n.top {
                        n.top = Val::Px(t - o.y);
                    }
                });
                self.commands.entity(p).add_child(e);
            }
            None => {
                self.commands.entity(self.root).add_child(e);
            }
        }
    }
    /// Start clipping children to a rectangle (pixi mask).
    pub(crate) fn clip_begin(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let mut n = Self::node(x, y, w, h);
        n.overflow = Overflow::clip();
        self.z += 1;
        let e = self.commands.spawn((n, GlobalZIndex(self.z))).id();
        self.commands.entity(self.root).add_child(e);
        self.parent = Some((e, Vec2::new(x, y)));
        self.clip = Some(Rect::new(x, y, x + w, y + h));
    }
    pub(crate) fn clip_end(&mut self) {
        self.parent = None;
        self.clip = None;
    }
    /// Spawn at the root (outside any clip container).
    fn spawn_root(&mut self, b: impl Bundle) {
        let parent = self.parent.take();
        self.spawn(b);
        self.parent = parent;
    }
    /// The visible rectangle: the clip, else the window.
    fn bounds(&self) -> Rect {
        self.clip.unwrap_or(Rect::new(0.0, 0.0, self.w, self.h))
    }
    pub(crate) fn node(x: f32, y: f32, w: f32, h: f32) -> Node {
        Node { position_type: PositionType::Absolute, left: Val::Px(x), top: Val::Px(y), width: Val::Px(w), height: Val::Px(h), ..default() }
    }
    /// Atlas frame stretched to (x, y, w, h) px.
    pub(crate) fn frame(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32, tint: Color, flip_y: bool) {
        let Some(r) = self.ui.frames.get(name).copied() else { return };
        let src = Rect::new(r.min.x as f32, r.min.y as f32, r.max.x as f32, r.max.y as f32);
        self.atlas_piece(src, x, y, w, h, tint, flip_y);
    }
    /// An atlas sub-rectangle `src` stretched to (x, y, w, h) px, cropped to
    /// the window / clip with the source cropped to match (Bevy lays out
    /// oversized nodes badly; pixi just draws off-screen).
    #[allow(clippy::too_many_arguments)]
    fn atlas_piece(&mut self, src: Rect, x: f32, y: f32, w: f32, h: f32, tint: Color, flip_y: bool) {
        let mut src = src;
        let (mut x, mut y, mut w, mut h) = (x, y, w, h);
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let bd = self.bounds();
        if x < bd.min.x || y < bd.min.y || x + w > bd.max.x || y + h > bd.max.y {
            let (cx0, cy0) = (x.max(bd.min.x), y.max(bd.min.y));
            let (cx1, cy1) = ((x + w).min(bd.max.x), (y + h).min(bd.max.y));
            if cx1 <= cx0 || cy1 <= cy0 {
                return;
            }
            let (sw, sh) = (src.width() / w, src.height() / h);
            let u0 = src.min.x + (cx0 - x) * sw;
            let u1 = src.min.x + (cx1 - x) * sw;
            // a vertical flip mirrors which source rows map to the clipped band
            let (v0, v1) = if flip_y {
                (src.max.y - (cy1 - y) * sh, src.max.y - (cy0 - y) * sh)
            } else {
                (src.min.y + (cy0 - y) * sh, src.min.y + (cy1 - y) * sh)
            };
            src = Rect::new(u0, v0, u1, v1);
            (x, y, w, h) = (cx0, cy0, cx1 - cx0, cy1 - cy0);
        }
        let img = ImageNode {
            image: self.ui.atlas.clone(),
            rect: Some(src),
            color: tint,
            flip_y,
            image_mode: NodeImageMode::Stretch,
            ..default()
        };
        self.spawn_root((Self::node(x, y, w, h), img));
    }
    /// Atlas frame `name` stretched to w x h px around (cx, cy), rotated by
    /// `angle` (radians, clockwise on screen). Not cropped: only dropped
    /// when it cannot reach the window.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn frame_rotated(&mut self, name: &str, cx: f32, cy: f32, w: f32, h: f32, angle: f32, tint: Color, flip_y: bool) {
        let Some(r) = self.ui.frames.get(name).copied() else { return };
        let bd = self.bounds();
        let rad = 0.5 * (w * w + h * h).sqrt();
        if cx + rad < bd.min.x || cx - rad > bd.max.x || cy + rad < bd.min.y || cy - rad > bd.max.y || w <= 0.0 || h <= 0.0 {
            return;
        }
        let img = ImageNode {
            image: self.ui.atlas.clone(),
            rect: Some(Rect::new(r.min.x as f32, r.min.y as f32, r.max.x as f32, r.max.y as f32)),
            color: tint,
            flip_y,
            image_mode: NodeImageMode::Stretch,
            ..default()
        };
        let keep = self.rotation;
        self.rotation = angle;
        self.spawn_root((Self::node(cx - w / 2.0, cy - h / 2.0, w, h), img));
        self.rotation = keep;
    }
    /// Atlas frame centred at (cx, cy) px, `scale` x its size in units.
    pub(crate) fn frame_c(&mut self, name: &str, cx: f32, cy: f32, scale: f32, tint: Color) {
        let Some(r) = self.ui.frames.get(name).copied() else { return };
        let (w, h) = (r.width() as f32 * scale * self.s, r.height() as f32 * scale * self.s);
        self.frame(name, cx - w / 2.0, cy - h / 2.0, w, h, tint, false);
    }
    /// Nine-slice frame (`Z.rectImg`, pixi `NineSlicePlane`): borders in
    /// source px, corners drawn at source size x the UI scale; nine plain
    /// stretched sub-rectangles of the atlas frame.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sliced(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32, l: f32, t: f32, r: f32, b: f32) {
        let Some(fr) = self.ui.frames.get(name).copied() else { return };
        let (fx0, fy0, fx1, fy1) = (fr.min.x as f32, fr.min.y as f32, fr.max.x as f32, fr.max.y as f32);
        let src_x = [fx0, fx0 + l, fx1 - r, fx1];
        let src_y = [fy0, fy0 + t, fy1 - b, fy1];
        let s = self.s;
        let dst_x = [x, x + l * s, x + w - r * s, x + w];
        let dst_y = [y, y + t * s, y + h - b * s, y + h];
        for j in 0..3 {
            for i in 0..3 {
                let (dw, dh) = (dst_x[i + 1] - dst_x[i], dst_y[j + 1] - dst_y[j]);
                if dw <= 0.0 || dh <= 0.0 {
                    continue;
                }
                self.atlas_piece(Rect::new(src_x[i], src_y[j], src_x[i + 1], src_y[j + 1]), dst_x[i], dst_y[j], dw, dh, Color::WHITE, false);
            }
        }
    }
    /// A whole image stretched to (x, y, w, h) px.
    pub(crate) fn image(&mut self, img: &Handle<Image>, x: f32, y: f32, w: f32, h: f32, tint: Color) {
        let bd = self.bounds();
        let (x0, y0, x1, y1) = (x.max(bd.min.x), y.max(bd.min.y), (x + w).min(bd.max.x), (y + h).min(bd.max.y));
        if x1 <= x0 || y1 <= y0 || w <= 0.0 || h <= 0.0 {
            return;
        }
        // the whole image's UV sub-rectangle, in source pixels (Bevy needs the size)
        let rect = self.image_sizes.get(&img.id()).map(|sz| {
            let (sx, sy) = (sz.x / w, sz.y / h);
            Rect::new((x0 - x) * sx, (y0 - y) * sy, (x1 - x) * sx, (y1 - y) * sy)
        });
        let node = ImageNode { image: img.clone(), color: tint, rect, image_mode: NodeImageMode::Stretch, ..default() };
        self.spawn_root((Self::node(x0, y0, x1 - x0, y1 - y0), node));
    }
    /// A solid (optionally rounded) fill: a tinted white image.
    pub(crate) fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color, radius: f32) {
        let bd = self.bounds();
        let (x0, y0, x1, y1) = (x.max(bd.min.x), y.max(bd.min.y), (x + w).min(bd.max.x), (y + h).min(bd.max.y));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (x, y, w, h) = (x0, y0, x1 - x0, y1 - y0);
        let mut n = Self::node(x, y, w, h);
        n.border_radius = BorderRadius::all(Val::Px(radius));
        let img = ImageNode { image: self.ui.white.clone(), color, image_mode: NodeImageMode::Stretch, ..default() };
        self.spawn_root((n, img));
    }
    /// pixi `Text` at (x, y) px with anchor (ax, ay); size in units.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text(&mut self, s: &str, font: UiFont, size: f32, color: Color, x: f32, y: f32, ax: f32, ay: f32, shadow: Option<f32>) {
        self.text_shadow(s, font, size, color, x, y, ax, ay, shadow.map(|d| (d, raw(0x000000, 1.0))));
    }
    /// `text` with a coloured drop shadow (distance, colour).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text_shadow(&mut self, s: &str, font: UiFont, size: f32, color: Color, x: f32, y: f32, ax: f32, ay: f32, shadow: Option<(f32, Color)>) {
        let px = size * self.s;
        let (pa, pd, ha, hd) = font.metrics();
        let pad = 0.4875 / 50.0;
        // pixi box: measured ascent + descent; Bevy line of the same height
        // puts the baseline at (H - (ha + hd)) / 2 + ha: shift to pixi's
        let box_h = (pa + pd + 2.0 * pad) * px;
        let pixi_base = (pa + pad) * px;
        let bevy_base = (box_h - (ha + hd) * px) / 2.0 + ha * px;
        let wide = 4000.0;
        let left = x - wide * ax;
        let top = y - box_h * ay + (pixi_base - bevy_base);
        let justify = if ax <= 0.0 {
            Justify::Left
        } else if ax >= 1.0 {
            Justify::Right
        } else {
            Justify::Center
        };
        let handle = match font {
            UiFont::Lilita => self.ui.lilita.clone(),
            UiFont::Titan => self.ui.titan.clone(),
        };
        let tf = TextFont { font: handle.into(), font_size: bevy::text::FontSize::Px(px), ..default() };
        let mut bundle = (Self::node(left, top, wide, box_h), Text::new(s), tf, TextColor(color), TextLayout::justify(justify), bevy::text::LineHeight::Px(box_h));
        bundle.0.overflow = Overflow::visible();
        if let Some((d, shadow_color)) = shadow {
            // pixi v8 drop shadow: angle PI/6, distance d units
            let off = Vec2::new((std::f32::consts::PI / 6.0).cos(), (std::f32::consts::PI / 6.0).sin()) * d * self.s;
            self.spawn((bundle, TextShadow { offset: off, color: shadow_color }));
        } else {
            self.spawn(bundle);
        }
    }
    /// The base tint of a button whose hit rectangle is (x, y, w, h) px:
    /// #AAAAAA while it is held.
    pub(crate) fn press_tint(&self, x: f32, y: f32, w: f32, h: f32) -> Color {
        let held = self.pressed.is_some_and(|r| (r.min.x - x).abs() < 1.0 && (r.min.y - y).abs() < 1.0 && (r.width() - w).abs() < 1.0 && (r.height() - h).abs() < 1.0);
        if held {
            raw(0xAAAAAA, 1.0)
        } else {
            Color::WHITE
        }
    }
    /// Units relative to the screen centre -> px.
    pub(crate) fn c(&self, ux: f32, uy: f32) -> (f32, f32) {
        (self.w / 2.0 + ux * self.s, self.h / 2.0 + uy * self.s)
    }
    pub(crate) fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, c: Click) {
        self.hits.push((Rect::new(x, y, x + w, y + h), c));
    }
}

/// The UI scale `S` (px per unit) of a w x h window.
pub(crate) fn ui_scale(w: f32, h: f32) -> f32 {
    let base = if w / h >= 1.0 {
        500.0
    } else if w / h >= 0.5 {
        667.0
    } else {
        760.0
    };
    (h / base) * 0.5
}

#[allow(clippy::too_many_arguments)]
fn build_ui(
    mut commands: Commands,
    sim: Option<NonSend<Sim>>,
    ui: Res<UiAssets>,
    mut hits: ResMut<UiHits>,
    roots: Query<Entity, With<UiRoot>>,
    windows: Query<&Window>,
    mut images: ResMut<Assets<Image>>,
    mut built: Local<Option<(i64, u64, Vec2, usize, usize)>>,
    press: Res<UiPress>,
) {
    let (Some(sim), Ok(root), Ok(win)) = (sim, roots.single(), windows.single()) else { return };
    // immediate mode, but only when the state can have changed: a new
    // simulation frame, another simulation, a resize, or a click (menus)
    let key = (sim.game.frame, sim.serial, Vec2::new(win.width(), win.height()), ui.thumbs.len(), hits.0.len());
    if built.as_ref() == Some(&key) && !sim.game.flow.ui_dirty.get() {
        return;
    }
    sim.game.flow.ui_dirty.set(false);
    *built = Some(key);
    commands.entity(root).despawn_related::<Children>();
    let (w, h) = (win.width(), win.height());
    let s = ui_scale(w, h);
    let sizes: HashMap<AssetId<Image>, Vec2> = ui.thumbs.values().chain([&ui.hs_view]).filter_map(|h| images.get(h).map(|i| (h.id(), Vec2::new(i.width() as f32, i.height() as f32)))).collect();
    let mut b = Builder { commands: &mut commands, root, ui: &ui, s, w, h, hits: Vec::new(), image_sizes: &sizes, z: 0, parent: None, rotation: 0.0, clip: None, pressed: press.0 };
    let g = &sim.game;
    if g.hud.visible {
        hud(&mut b, g);
    }
    crate::ui_menu::menus(&mut b, g);
    match &g.flow.screen {
        Screen::SaveMe { secs, tall } => {
            if let Some(mut img) = images.get_mut(&ui.pie) {
                *img = pie_image(secs / 6.0);
            }
            save_me(&mut b, g, *tall);
        }
        Screen::NotEnough { needed } => not_enough(&mut b, *needed),
        Screen::HighScore { alpha, .. } => high_score(&mut b, flow::score(g), *alpha, &g.flow.celebration),
        Screen::Results { score, coins, frames, doubled, list, .. } => results(&mut b, g, *score, *coins, *frames, *doubled, list.target),
        Screen::BuyBoards { bump, .. } => crate::ui_menu::buy_boards(&mut b, g, *bump),
        Screen::Countdown { n, .. } => crate::ui_menu::countdown(&mut b, *n),
        Screen::Paused => crate::ui_menu::pause_panel(&mut b, g),
        Screen::Prize(p) => crate::ui_menu::prize_screen(&mut b, g, p),
        _ => {}
    }
    // modal overlays on top: their hits are tested first
    let below = std::mem::take(&mut b.hits);
    crate::ui_menu::overlays(&mut b, g);
    b.hits.extend(below);
    // the notification layer (`Py`, layer 1) over everything
    if let Some(t) = g.missions.toasts.front() {
        crate::ui_missions::toast(&mut b, t);
    }
    hits.0 = b.hits;
}

pub(crate) const WHITE: u32 = 0xFFFFFF;

/// The in-game HUD (`fg`, docs/js_notes/ui_hud.md §8).
fn hud(b: &mut Builder, g: &crate::game::Game) {
    let s = b.s;
    let vw = b.vw();
    let half = raw(WHITE, 0.5).with_alpha(0.5);
    let plate = raw(0x000000, 1.0).with_alpha(0.5);
    let _ = half;
    // score: base-long (302x82) left edge VW-220, centre y 60, alpha .5
    b.frame("base-long", (vw - 220.0) * s, (60.0 - 41.0) * s, 302.0 * s, 82.0 * s, Color::LinearRgba(LinearRgba::new(1.0, 1.0, 1.0, 0.5)), false);
    let digits = format!("{:06}", g.hud.score.max(0));
    for (i, ch) in digits.chars().rev().take(6).collect::<Vec<_>>().into_iter().rev().enumerate() {
        b.text(&ch.to_string(), UiFont::Lilita, 50.0, raw(WHITE, 1.0), (vw - 185.0 + 30.0 * i as f32) * s, 60.0 * s, 0.5, 0.5, None);
    }
    // multiplier badge: base-short centred (VW-280, 60)
    b.frame_c("base-short", (vw - 280.0) * s, 60.0 * s, 1.0, Color::LinearRgba(LinearRgba::new(1.0, 1.0, 1.0, 0.5)));
    b.text(&g.hud.badge, UiFont::Lilita, 50.0, raw(0xFEDB04, 1.0), (vw - 280.0) * s, 60.0 * s, 0.5, 0.5, None);
    if g.hud.badge_overlay > 0.0 {
        b.text(&g.hud.badge, UiFont::Lilita, 50.0, raw(0x777777, g.hud.badge_overlay as f32), (vw - 280.0) * s, 60.0 * s, 0.5, 0.5, None);
    }
    // coins: plate grows left 30 units per digit, digits in the last slots
    let coins = g.hud.coins.max(0).to_string();
    let len = coins.len().min(6) as f32;
    b.frame("base-long", (vw - 170.0 + 70.0 - 30.0 * len) * s, (160.0 - 41.0) * s, 302.0 * s, 82.0 * s, Color::LinearRgba(LinearRgba::new(1.0, 1.0, 1.0, 0.5)), false);
    let start = 6 - len as usize;
    for (k, ch) in coins.chars().take(6).enumerate() {
        let i = (start + k) as f32;
        b.text(&ch.to_string(), UiFont::Lilita, 50.0, raw(WHITE, 1.0), (vw - 245.0 + 30.0 * i) * s, 160.0 * s, 0.5, 0.5, None);
    }
    b.frame_c("icon-coin-large", (vw - 45.0) * s, 160.0 * s, 0.75, Color::WHITE);
    let _ = plate;
    // pause button: base-item-flat as a 100x100 tile at (60, 60), icon on top
    // pause button (Escape): disabled (alpha 0.5) during the resume countdown
    let a = if matches!(g.flow.screen, Screen::Countdown { .. }) { 0.5 } else { 1.0 };
    let t = b.press_tint((60.0 - 50.0) * s, (60.0 - 50.0) * s, 100.0 * s, 100.0 * s);
    b.frame("base-item-flat", (60.0 - 50.0) * s, (60.0 - 50.0) * s, 100.0 * s, 100.0 * s, t.with_alpha(a), false);
    b.frame_c("icon-pause", 60.0 * s, 60.0 * s, 1.0, Color::WHITE.with_alpha(a));
    b.hit((60.0 - 50.0) * s, (60.0 - 50.0) * s, 100.0 * s, 100.0 * s, Click::PauseButton);
    meters(b, g);
    boost_buttons(b, g);
    crate::ui_menu::word_banner(b, g);
    tutorial(b, g);
}

/// The tutorial's view (`jg.view`, the screen centre; hidden while paused):
/// the arrow `kg` (`tutorial-arrow` x2, alpha 0.5, turned to its direction,
/// sliding 300 -> -300 along it) and the message `Ag` (Lilita 50, white,
/// black stroke 5).
fn tutorial(b: &mut Builder, g: &crate::game::Game) {
    if !g.tutorial_enabled || !g.tutorial.shown || g.state == crate::game::GameState::Paused {
        return;
    }
    let (s, vw, vh) = (b.s, b.vw(), b.vh());
    let (cx, cy) = (vw / 2.0, vh / 2.0);
    if let Some(a) = g.tutorial.arrow.as_ref().filter(|a| !a.pending) {
        let rot = std::f32::consts::FRAC_PI_2 * a.dir as f32;
        // the image's local (0, y) turned with the arrow
        let y = a.y() as f32;
        let (x, yy) = (cx - y * rot.sin(), cy + y * rot.cos());
        b.rotation = rot;
        b.frame_c("tutorial-arrow", x * s, yy * s, 2.0, Color::srgba(1.0, 1.0, 1.0, 0.5));
        b.rotation = 0.0;
    }
    if let Some(m) = &g.tutorial.msg {
        if m.scale_y <= 0.05 {
            return;
        }
        let lines: Vec<&str> = m.text.split('\n').collect();
        let lh = 50.0 * (0.92225 + 0.21925);
        let n = lines.len() as f32;
        for (i, l) in lines.iter().enumerate() {
            let y = cy + m.y as f32 + (i as f32 - (n - 1.0) / 2.0) * lh;
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                b.text(l, UiFont::Lilita, 50.0, raw(0x000000, 1.0), (cx + a.cos() * 2.5) * s, (y + a.sin() * 2.5) * s, 0.5, 0.5, None);
            }
            b.text(l, UiFont::Lilita, 50.0, raw(WHITE, 1.0), cx * s, y * s, 0.5, 0.5, None);
        }
    }
}

/// Boost buttons `Wm` (boosts_mysterybox.md §1.3-§1.5): 160x220 black
/// nine-slice at (100, VH - 130 - 240 i - meters), sliding in from x -300,
/// boost art x0.75, count, key hint, white flashes; and the gauge `Dm`.
fn boost_buttons(b: &mut Builder, g: &crate::game::Game) {
    use crate::boosts::BoostKind;
    let (s, vh) = (b.s, b.vh());
    let meters = if g.hud.items.is_empty() { 0.0 } else { 90.0 * g.hud.items.len() as f32 };
    for (i, bt) in g.hud.boosts.iter().enumerate() {
        let k = bt.slide as f32;
        let slide = if bt.leaving { -500.0 * (1.0 - k) } else { -300.0 * (1.0 - k) };
        let (cx, cy) = (100.0 + slide, vh - 130.0 - 240.0 * i as f32 - meters);
        let (x0, y0) = ((cx - 80.0) * s, (cy - 110.0) * s);
        b.rect(x0, y0, 160.0 * s, 220.0 * s, raw(0x000000, 0.65), 20.0 * s);
        let (art, key, count) = match bt.kind {
            BoostKind::Headstart => ("boost-headstart", "Key V", g.flow.user.headstarts),
            BoostKind::Multiplier => ("boost-multiplier", "Key C", g.flow.user.score_boosters),
        };
        b.frame_c(art, cx * s, cy * s, 0.75, Color::WHITE);
        b.text(&count.to_string(), UiFont::Lilita, 32.0, raw(WHITE, 1.0), (cx + 70.0) * s, (cy + 105.0) * s, 1.0, 1.0, Some(2.0));
        b.text(key, UiFont::Lilita, 20.0, raw(0x999999, 1.0), (cx - 70.0) * s, (cy - 105.0) * s, 0.0, 0.0, None);
        // flashes: idle (3 blinks to .4 after 1 / 2 s) and on press (.9, 0.6 s)
        let t = bt.age as f32;
        let mut over = 0.0;
        let idle0 = 0.3 + (i as f32 + 1.0);
        if t > idle0 && t < idle0 + 1.2 {
            let ph = ((t - idle0) / 0.2) as i32;
            let f = ((t - idle0) % 0.2) / 0.2;
            over = if ph % 2 == 0 { 0.4 * f } else { 0.4 * (1.0 - f) };
        }
        if let Some(p) = bt.pressed_at {
            let d = (bt.age - p) as f32;
            if d < 0.2 {
                over = 0.9 * d / 0.2;
            } else if d < 0.6 {
                over = 0.9 * (1.0 - (d - 0.2) / 0.4);
            }
        }
        if over > 0.0 {
            b.rect(x0, y0, 160.0 * s, 220.0 * s, raw(WHITE, over), 20.0 * s);
        }
        if !bt.leaving {
            b.hit(x0, y0, 160.0 * s, 220.0 * s, Click::Boost(bt.kind));
        }
    }
    // the gauge at (VW/2, 300): slices at angle pi/7 * a, radius 150
    if g.now_ms() < g.hud.gauge.visible_until {
        let cx = b.vw() / 2.0;
        for (label, a) in [("3", -0.6f32), ("2", -1.6), ("1", -2.6), ("+7", 0.6), ("+6", 1.6), ("+5", 2.6)] {
            let th = std::f32::consts::PI / 7.0 * a;
            let (x, y) = (cx + 150.0 * th.sin(), 300.0 - 150.0 * th.cos());
            let lit = g.hud.gauge.lit.iter().any(|l| l == label);
            let tint = match (lit, a < 0.0) {
                (false, _) => raw(0x000000, 0.4),
                (true, true) => raw(0xFDE934, 1.0),
                (true, false) => raw(0x66D6FD, 1.0),
            };
            b.rotation = th;
            b.frame_c("base-slice", x * s, y * s, 1.0, tint);
            b.rotation = 0.0;
            let col = match (lit, a < 0.0) {
                (false, _) => raw(WHITE, 1.0),
                (true, true) => raw(0x971711, 1.0),
                (true, false) => raw(0x1D419F, 1.0),
            };
            b.text(label, UiFont::Lilita, 32.0, col, x * s, y * s, 0.5, 0.5, if lit { None } else { Some(2.0) });
        }
    }
}

/// Powerup meters (`Km` rows stacked by `fg.organizeTimers`, ui_powerups_shop.md
/// §1.2-§1.3): row i (oldest = 0) centred on its icon at (55, VH - 55 - 90 i).
fn meters(b: &mut Builder, g: &crate::game::Game) {
    let (s, vh) = (b.s, b.vh());
    for (i, t) in g.hud.items.iter().enumerate() {
        let (ox, oy) = (55.0, vh - 55.0 - 90.0 * i as f32);
        let at = |x: f32, y: f32| ((ox + x) * s, (oy + y) * s);
        let r = t.ratio.clamp(0.0, 1.0) as f32;
        let (x, y) = at(18.5, -26.5);
        b.rect(x, y, 237.0 * s, 53.0 * s, raw(0x999999, 1.0), 6.5 * s);
        let (x, y) = at(20.0, -25.0);
        b.rect(x, y, 234.0 * s, 50.0 * s, raw(WHITE, 1.0), 5.0 * s);
        let (x, y) = at(-35.0, -35.0);
        b.frame("base-item", x, y, 70.0 * s, 70.0 * s, Color::WHITE, false);
        let (x, y) = at(0.0, 0.0);
        b.frame_c(&format!("icon-item-{}", t.name), x, y, 1.0, Color::WHITE);
        let (x, y) = at(41.0, -17.0);
        b.rect(x, y, 200.0 * s, 34.0 * s, raw(WHITE, 0.75), 0.0);
        let (x, y) = at(45.0, -13.0);
        b.frame("item-duration-full-bar", x, y, 192.0 * s, 26.0 * s, Color::WHITE, false);
        if r < 1.0 {
            b.frame("item-duration-full-bar", x, y, 192.0 * s, 26.0 * s, raw(0xFF0000, (1.0 - r) * 0.5), false);
            let cw = (1.0 - r) * 200.0;
            let (x, y) = at(237.0 - cw, -14.0);
            b.rect(x, y, cw * s, 28.0 * s, raw(WHITE, 1.0), 0.0);
        }
        if t.name == "hoverboard" {
            // boards left: Lilita 18 black, white drop shadow (distance 2), anchor (1, 1) at (26, 30)
            let n = g.flow.user.hoverboards.to_string();
            let off = Vec2::new((std::f32::consts::PI / 6.0).cos(), (std::f32::consts::PI / 6.0).sin()) * 2.0;
            let (x, y) = at(26.0 + off.x, 30.0 + off.y);
            b.text(&n, UiFont::Lilita, 18.0, raw(WHITE, 1.0), x, y, 1.0, 1.0, None);
            let (x, y) = at(26.0, 30.0);
            b.text(&n, UiFont::Lilita, 18.0, raw(0x000000, 1.0), x, y, 1.0, 1.0, None);
        }
    }
}

/// `Qm` BLUR backdrop: flipped bands top and bottom, black 50 % over all.
pub(crate) fn backdrop_blur(b: &mut Builder) {
    let (w, h, s) = (b.w, b.h, b.s);
    b.frame("base-blurry", 0.0, 0.0, w, 230.0 * s, Color::WHITE, true);
    b.frame("base-blurry", 0.0, h - 230.0 * s, w, 230.0 * s, Color::WHITE, false);
    b.rect(0.0, 0.0, w, h, raw(0x000000, 0.5), 0.0);
}

/// `Z.rectComp(fill, border)`: rounded fill, nine-slice border offset (5, 6).
pub(crate) fn rect_comp(b: &mut Builder, cx: f32, cy: f32, fw: f32, fh: f32, bw: f32, bh: f32, fill: Color, border: &str, fill_frame: Option<&str>) {
    let s = b.s;
    let (fx, fy) = b.c(cx - fw / 2.0, cy - fh / 2.0);
    match fill_frame {
        Some(f) => b.sliced(f, fx, fy, fw * s, fh * s, 15.0, 15.0, 20.0, 20.0),
        None => b.rect(fx, fy, fw * s, fh * s, fill, 16.0 * s),
    }
    let (bx, by) = b.c(cx - fw / 2.0 - (bw - fw) / 2.0 + 5.0, cy - fh / 2.0 - (bh - fh) / 2.0 + 6.0);
    b.sliced(border, bx, by, bw * s, bh * s, 15.0, 15.0, 25.0, 25.0);
}

/// A Save me button base (`Am` with `Z.rectComp`, measured §3.1): grey
/// border (-153, y-54) 316x120, coloured fill (-148, y-48) 300x100.
fn button_base(b: &mut Builder, y: f32, fill: &str) {
    let s = b.s;
    let (fx, fy) = b.c(-148.0, y - 48.0);
    b.sliced(fill, fx, fy, 300.0 * s, 100.0 * s, 15.0, 15.0, 20.0, 20.0);
    let (bx, by) = b.c(-153.0, y - 54.0);
    b.sliced("box-border-grey", bx, by, 316.0 * s, 120.0 * s, 15.0, 15.0, 25.0, 25.0);
}

/// "Save me!" (`eb`, ui_gameover.md §2.6, §3.1).
fn save_me(b: &mut Builder, g: &crate::game::Game, tall: bool) {
    let s = b.s;
    backdrop_blur(b);
    let ph = if tall { 400.0 } else { 260.0 };
    rect_comp(b, 0.0, 0.0, 550.0, ph, 572.0, ph + 24.0, raw(0xEEEEEE, 1.0), "box-border-grey", None);
    // any tap on the backdrop or panel declines; buttons are tested first
    let keys_y = if tall { 110.0 } else { 40.0 };
    let (kx, ky) = b.c(-153.0, keys_y - 54.0);
    b.hit(kx, ky, 316.0 * s, 120.0 * s, Click::ReviveKeys);
    if tall {
        let (fx, fy) = b.c(-153.0, -20.0 - 54.0);
        b.hit(fx, fy, 316.0 * s, 120.0 * s, Click::ReviveFree);
        button_base(b, -20.0, "box-fill-green");
        let (ix, iy) = b.c(60.0, -30.0);
        b.frame_c("icon-tv", ix, iy, 1.0, Color::WHITE);
        let (tx, ty) = b.c(-50.0, -20.0);
        b.text("Free!", UiFont::Lilita, 40.0, raw(WHITE, 1.0), tx, ty, 0.5, 0.5, None);
    }
    button_base(b, keys_y, "box-fill-blue");
    let (ix, iy) = b.c(30.0, keys_y);
    b.frame_c("icon-key", ix, iy, 1.0, Color::WHITE);
    let (tx, ty) = b.c(-30.0, keys_y);
    b.text(&g.flow.paid_cost().to_string(), UiFont::Lilita, 40.0, raw(WHITE, 1.0), tx, ty, 0.5, 0.5, None);
    let (tx, ty) = b.c(0.0, -ph / 2.0 + 60.0);
    b.text("Save me!", UiFont::Titan, 40.0, raw(0x004A80, 1.0), tx, ty, 0.5, 0.5, None);
    // clock: base, blue fill, white elapsed wedge
    let cy = -ph / 2.0 + 20.0;
    let (cx0, cy0) = b.c(-256.0, cy - 1.0);
    b.frame_c("clock-base", cx0, cy0, 1.0, Color::WHITE);
    let (cx1, cy1) = b.c(-255.0, cy);
    b.frame_c("clock-fill", cx1, cy1, 1.0, Color::WHITE);
    let r = 43.0 * s;
    let pie = b.ui.pie.clone();
    b.spawn((Builder::node(cx1 - r, cy1 - r, 2.0 * r, 2.0 * r), ImageNode::new(pie)));
    // currencies: keys tag (base-grey nine-slice, key icon, count)
    let keys = g.flow.user.keys.to_string();
    let extra = (keys.len() as f32 - 1.0) * 15.0;
    let (bx, by) = b.c(171.3 - extra, -ph / 2.0 + 35.0 - 21.0);
    b.sliced("base-grey", bx, by, (85.7 + extra) * s, 42.0 * s, 10.0, 10.0, 10.0, 10.0);
    let (ix, iy) = b.c(199.3 - extra, -ph / 2.0 + 35.0);
    b.frame_c("icon-key", ix, iy, 0.75, Color::WHITE);
    let (tx, ty) = b.c(220.4 - extra, -ph / 2.0 + 35.0);
    b.text(&keys, UiFont::Lilita, 30.0, raw(0xF6F6F6, 1.0), tx, ty, 0.0, 0.5, Some(1.0));
    let (px, py) = b.c(-286.0, -ph / 2.0 - 12.0);
    b.hit(px, py, 582.0 * s, (ph + 24.0) * s, Click::Elsewhere);
    b.hit(0.0, 0.0, b.w, b.h, Click::Elsewhere);
}

/// Not enough keys (`Ny`, §2.7).
fn not_enough(b: &mut Builder, needed: i64) {
    let (w, h) = (b.w, b.h);
    b.rect(0.0, 0.0, w, h, raw(0x000000, 0.95), 0.0);
    rect_comp(b, 0.0, 0.0, 600.0, 300.0, 622.0, 324.0, raw(0xEEEEEE, 1.0), "box-border-grey", None);
    let (x, y) = b.c(168.0, -20.0);
    b.frame_c("background-splat", x, y, 1.0, raw(0x70569A, 1.0));
    let (x, y) = b.c(165.0, -23.0);
    b.frame_c("background-splat", x, y, 1.0, raw(0xAC97E8, 1.0));
    let (x, y) = b.c(168.0, -40.0);
    b.frame_c("icon-key-large", x, y, 1.0, Color::WHITE);
    let (x, y) = b.c(-260.0, -100.0);
    b.text("Not enough", UiFont::Titan, 50.0, raw(0x074B7E, 1.0), x, y, 0.0, 0.0, None);
    let (x, y2) = b.c(-260.0, -100.0 + 58.0);
    b.text("keys!", UiFont::Titan, 50.0, raw(0x074B7E, 1.0), x, y2, 0.0, 0.0, None);
    let (x, y3) = b.c(-260.0, -100.0 + 2.0 * 58.0 + 20.0);
    b.text(&format!("{needed} Keys needed"), UiFont::Titan, 30.0, raw(0x19669B, 1.0), x, y3, 0.0, 0.0, None);
    let (x, y) = b.c(-290.0, -140.0);
    b.frame_c("btn-close", x, y, 1.0, Color::WHITE);
    b.hit(0.0, 0.0, w, h, Click::Elsewhere);
}

/// New High Score (`Gv`, §2.8). The character render and stripes are not drawn.
/// New High Score (`Gv`, §2.8, §3.2): the bg, the speed stripes `Kv` in the
/// bg's (flipped) 512-space, the 3D character `Xv` (container (vw/2, vh/2)
/// x1.25, its 640-px view at the lerped offset and scale), the texts.
fn high_score(b: &mut Builder, score: i64, alpha: f64, c: &crate::celebration::Celebration) {
    let (s, vw, vh) = (b.s, b.vw(), b.vh());
    let m = vw.max(vh);
    let (bw, bh) = (m * 4.0 * s, m * 1.3 * s);
    let (cx, cy) = (vw * 1.5 * s, vh * 0.6 * s);
    b.frame("highscorescreen-bg", cx - bw / 2.0, cy - bh / 2.0, bw, bh, Color::WHITE, true);
    // a stripe: 512 x 32 at anchor (0.05, 0.5), scale (0.15, scale_y),
    // rotated, in a parent scaled (bw/512, -bh/512). The parent's unequal
    // scale shears the thin axis; drawn as the rectangle along the mapped
    // long axis with the same area.
    let (px, py) = (bw / 512.0, -bh / 512.0);
    for st in &c.stripes {
        let (cs, sn) = (st.rotation.cos() as f32, st.rotation.sin() as f32);
        let long = Vec2::new(px * 76.8 * cs, py * 76.8 * sn);
        let thick = Vec2::new(px * -32.0 * st.scale_y as f32 * sn, py * 32.0 * st.scale_y as f32 * cs);
        let len = long.length();
        if len <= 0.0 {
            continue;
        }
        let width = long.perp_dot(thick).abs() / len;
        let anchor = Vec2::new(cx + px * st.x as f32, cy + py * st.y as f32);
        let centre = anchor + long * 0.45;
        let flip = long.perp_dot(thick) < 0.0;
        b.frame_rotated("celebration-stripe", centre.x, centre.y, len, width, long.y.atan2(long.x), raw(WHITE, 0.75), flip);
    }
    // the character: texture 640 units square, anchor 0.5
    let size = 640.0 * 1.25 * c.view[2] as f32 * s;
    let (ox, oy) = ((vw / 2.0 + 1.25 * c.view[0] as f32) * s, (vh / 2.0 + 1.25 * c.view[1] as f32) * s);
    let view = b.ui.hs_view.clone();
    b.image(&view, ox - size / 2.0, oy - size / 2.0, size, size, Color::WHITE);
    b.text("New High Score!", UiFont::Lilita, 50.0, raw(0x000000, 1.0), vw / 2.0 * s, 60.0 * s, 0.5, 0.5, None);
    b.text(&score.to_string(), UiFont::Lilita, 70.0, raw(WHITE, 1.0), vw / 2.0 * s, 120.0 * s, 0.5, 0.5, None);
    let a = alpha.clamp(0.0, 1.0) as f32;
    if a > 0.0 {
        b.text("Press Space to continue", UiFont::Lilita, 40.0, raw(WHITE, a), vw / 2.0 * s, (vh - 80.0) * s, 0.5, 0.5, None);
    }
    let (w, h) = (b.w, b.h);
    b.hit(0.0, 0.0, w, h, Click::Elsewhere);
}

/// The mock leaderboard (`Ve`) with the player, sorted, 11 rows.
fn leaderboard(me: i64, name: &str) -> Vec<(String, i64, bool)> {
    let mut v: Vec<(String, i64, bool)> = [
        ("Yutani", 999999),
        ("Spike", 810312),
        ("Fresh", 555555),
        ("Tricky", 345678),
        ("Lucy", 171110),
        ("Ninja", 123456),
        ("Brody", 65900),
        ("Tagbot", 15505),
        ("Tasha", 1000),
        ("King", 500),
    ]
    .iter()
    .map(|(n, s)| (n.to_string(), *s, false))
    .collect();
    v.push((name.to_string(), me, true));
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.truncate(11);
    v
}

/// Results notepad (`Wv`, §2.9, §3.3) with its 3D idle character (`_v`:
/// anchor (0.5, 1) at (-190, -H/2 + 390) in the panel: 36 below the
/// notepad's centre, measured against the oracle; the 640-unit stage render,
/// `crate::render_menu_3d`).
#[allow(clippy::too_many_arguments)]
fn results(b: &mut Builder, g: &crate::game::Game, score: i64, coins: i64, frames: u32, doubled: bool, list_y: f32) {
    let s = b.s;
    backdrop_blur(b);
    backdrop_blur(b);
    let (x, y) = b.c(0.0, -64.18);
    b.frame_c("notepad-panel", x, y, 1.0, Color::WHITE);
    let view = b.ui.hs_view.clone();
    let (vx, vy) = b.c(-190.0 - 320.0, -64.18 + 36.0 - 640.0);
    b.image(&view, vx, vy, 640.0 * s, 640.0 * s, Color::WHITE);
    // leaderboard rows (clipped to the mask)
    // the Double Up module while it shows; the list (`Hv`) scrolls (`vv`)
    let double_up = coins > 0 && !doubled;
    let (mask_top, mask_h) = if double_up { (35.0, 308.0) } else { (-65.6, 408.0) };
    let rows = leaderboard(g.flow.user.high_score.max(score), &g.flow.user.name);
    // the list's mask (pixi `mask` = a clipping container here)
    let (mx, my) = b.c(-317.0, mask_top);
    b.clip_begin(mx, my, 634.0 * s, mask_h * s);
    for (i, (name, sc, me)) in rows.iter().enumerate() {
        let cy = list_y + 52.0 * i as f32;
        if cy + 26.0 < mask_top || cy - 26.0 > mask_top + mask_h {
            continue;
        }
        let rank = if *me && i > 9 { 0 } else { i + 1 };
        let fill = if *me {
            0xFFF155
        } else if rank % 2 == 1 {
            0xCADAE4
        } else {
            0xADC7D8
        };
        let (rx, ry) = b.c(-317.0, cy - 26.0);
        b.rect(rx, ry, 634.0 * s, 52.0 * s, raw(fill, 1.0), 0.0);
        let col = raw(0x0A2B53, 1.0);
        let (ix, iy) = b.c(-247.0, cy);
        b.text(&rank.to_string(), UiFont::Lilita, 26.0, col, ix, iy, 1.0, 0.5, None);
        let (ax, ay) = b.c(-197.0, cy);
        b.frame_c("thumb-generic", ax, ay, 0.431, Color::WHITE);
        let (nx, ny) = b.c(-147.0, cy);
        b.text(name, UiFont::Lilita, 26.0, col, nx, ny, 0.0, 0.5, None);
        let (sx, sy) = b.c(287.0, cy);
        b.text(&sc.to_string(), UiFont::Lilita, 26.0, col, sx, sy, 1.0, 0.5, None);
    }
    b.clip_end();
    if double_up {
        let (x, y) = b.c(0.0, -20.0);
        b.frame_c("doubleup-module-panel", x, y, 1.0, Color::WHITE);
        if let Some(r) = b.ui.frames.get("doubleup-module-panel").copied() {
            let (w, h) = (r.width() as f32 * s, r.height() as f32 * s);
            b.hit(x - w / 2.0, y - h / 2.0, w, h, Click::DoubleUp);
        }
        let (x, y) = b.c(197.0, -30.0);
        b.frame_c("icon-tv", x, y, 1.0, Color::WHITE);
        let (x, y) = b.c(-222.0, -37.0);
        b.frame_c("icon-king", x, y, 1.0, Color::WHITE);
        let (x, y) = b.c(-190.0, -60.0);
        b.text("Free Double Up!", UiFont::Lilita, 35.0, raw(0xFFCC00, 1.0), x, y, 0.0, 0.5, None);
        let (x, y) = b.c(-190.0, -20.0);
        b.text(&format!("Get {coins} Coins"), UiFont::Lilita, 30.0, raw(WHITE, 1.0), x, y, 0.0, 0.5, None);
    }
    // scoreboard
    let (sx, sy) = (130.0, -167.0);
    let (x, y) = b.c(sx, sy);
    b.frame_c("scoreboard", x, y, 1.0, Color::WHITE);
    let (x, y) = b.c(sx - 15.0, sy - 110.0);
    b.rotation = -0.07;
    b.text("Score", UiFont::Titan, 60.0, raw(0x004A80, 1.0), x, y, 0.5, 0.5, None);
    b.rotation = 0.0;
    // the stars ride the rotated title (measured, §3.3)
    for (ux, uy) in [(-3.1, -268.7), (233.1, -285.2)] {
        let (x, y) = b.c(ux, uy);
        b.frame_c("icon-star", x, y, 1.0, Color::WHITE);
    }
    let (x, y) = b.c(sx, sy - 33.0);
    b.text(&score.to_string(), UiFont::Lilita, 55.0, raw(WHITE, 1.0), x, y, 0.5, 0.5, Some(1.0));
    let (x, y) = b.c(sx, sy + 35.0);
    // `scoreboard.update({coins: coins * 2})` after a Double Up
    let shown = if doubled { coins * 2 } else { coins };
    b.text(&shown.to_string(), UiFont::Lilita, 45.0, raw(WHITE, 1.0), x, y, 0.5, 0.5, Some(1.0));
    let (x, y) = b.c(sx - 100.0, sy + 35.0);
    b.frame_c("icon-coin", x, y, 0.75, Color::WHITE);
    // currencies (keys and coins tween from 0 with quad.out over <= 1 s)
    let t = (frames as f32 / 60.0).min(1.0);
    let ease = 1.0 - (1.0 - t) * (1.0 - t);
    let (bx, by) = b.c(sx - 80.5, -367.0 - 21.0);
    b.sliced("base-grey", bx, by, 161.0 * s, 42.0 * s, 10.0, 10.0, 10.0, 10.0);
    let (x, y) = b.c(sx - 52.0, -367.0);
    b.frame_c("icon-key", x, y, 0.75, Color::WHITE);
    let (x, y) = b.c(sx - 36.0, -367.0);
    b.text(&((g.flow.user.keys as f32 * ease).round() as i64).to_string(), UiFont::Lilita, 30.0, raw(0xF6F6F6, 1.0), x, y, 0.0, 0.5, Some(1.0));
    let (x, y) = b.c(sx + 21.5, -367.0);
    b.frame_c("icon-coin", x, y, 0.75, Color::WHITE);
    let (x, y) = b.c(sx + 39.0, -367.0);
    b.text(&((g.flow.user.coins as f32 * ease).round() as i64).to_string(), UiFont::Lilita, 30.0, raw(0xF6F6F6, 1.0), x, y, 0.0, 0.5, Some(1.0));
    // bottom row: Menu, Boosts, PLAY
    for (ux, bg, icon, label) in [(-238.5, "navigation-button-grey", "icon-white-house", "Menu"), (-79.4, "navigation-button-blu", "icon-white-upgrades", "Boosts")] {
        let (x, y) = b.c(ux, 412.0);
        let (fw, fh) = (166.0 * 0.85 * s, 110.0 * 0.9 * s);
        b.frame(bg, x - fw / 2.0, y - fh / 2.0, fw, fh, Color::WHITE, false);
        b.frame_c(icon, x, y, 1.0, Color::WHITE);
        let (lx, ly) = b.c(ux, 412.0 + 68.0);
        b.text(label, UiFont::Titan, 20.0, raw(WHITE, 1.0), lx, ly, 0.5, 0.5, None);
        b.hit(x - fw / 2.0, y - fh / 2.0, fw, fh, if label == "Menu" { Click::ResultsMenu } else { Click::ResultsBoosts });
    }
    let (x, y) = b.c(141.7, 412.0);
    let (fw, fh) = (320.0 * 0.83 * s, 120.0 * 0.83 * s);
    let t = b.press_tint(x - fw / 2.0, y - fh / 2.0, fw, fh);
    b.frame("large-navigation-button-light-green", x - fw / 2.0, y - fh / 2.0, fw, fh, t, false);
    b.text("PLAY", UiFont::Lilita, 55.0, raw(WHITE, 1.0), x, y, 0.5, 0.5, None);
    b.hit(x - fw / 2.0, y - fh / 2.0, fw, fh, Click::Play);
}

/// The prize screen's ray burst `By.rays` (`lh`, 46100): four
/// `background-stripes-hq` quarters (rotated 0, 90, 180, 270 degrees about
/// their corner, scaled to 747.5 across), raysAlpha 0.75, and the
/// `background-superglow` x7, all tinted #FFFF66, masked by a 650-wide
/// `background-superglow`; baked once into a 512 x 512 image of the 650
/// units. (The original adds them; here a white layer approximates it.)
fn bake_rays(mut ui: ResMut<UiAssets>, mut images: ResMut<Assets<Image>>) {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    if ui.rays.is_some() {
        return;
    }
    let (Some(stripes), Some(glow)) = (ui.frames.get("background-stripes-hq").copied(), ui.frames.get("background-superglow").copied()) else { return };
    let Some(atlas) = images.get(&ui.atlas) else { return };
    let Some(data) = atlas.data.as_ref() else { return };
    let aw = atlas.width() as usize;
    // alpha x red of an atlas frame at uv (0..1); 0 outside
    let sample = |f: URect, u: f32, v: f32| -> f32 {
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return 0.0;
        }
        let x = f.min.x as usize + (u * f.width() as f32) as usize;
        let y = f.min.y as usize + (v * f.height() as f32) as usize;
        let i = (y * aw + x) * 4;
        data.get(i).zip(data.get(i + 3)).map_or(0.0, |(r, a)| (*r as f32 / 255.0) * (*a as f32 / 255.0))
    };
    let n = 512usize;
    let ext = 650.0f32;
    let quarter = 747.5 / 2.0;
    let mut px = vec![0u8; n * n * 4];
    for j in 0..n {
        for i in 0..n {
            let p = Vec2::new((i as f32 + 0.5) / n as f32 * ext - ext / 2.0, (j as f32 + 0.5) / n as f32 * ext - ext / 2.0);
            let mask = sample(glow, p.x / ext + 0.5, p.y / ext + 0.5);
            if mask <= 0.0 {
                continue;
            }
            let g = sample(glow, p.x / 700.0 + 0.5, p.y / 700.0 + 0.5);
            let mut st = 0.0f32;
            for k in 0..4 {
                let a = -(k as f32) * std::f32::consts::FRAC_PI_2;
                let q = Vec2::new(p.x * a.cos() - p.y * a.sin(), p.x * a.sin() + p.y * a.cos());
                st += sample(stripes, q.x / quarter, q.y / quarter);
            }
            // the added light (#FFFF66 x intensity) over the bright
            // background mostly lifts the blue: drawn as cream at 65 % of
            // the intensity
            let v = ((0.75 * st + g) * mask * 0.65).clamp(0.0, 1.0);
            let o = (j * n + i) * 4;
            px[o..o + 4].copy_from_slice(&[255, 255, 200, (v * 255.0).round() as u8]);
        }
    }
    let mut img = Image::new(Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8Unorm, Default::default());
    img.sampler = bevy::image::ImageSampler::linear();
    ui.rays = Some(images.add(img));
}

/// The nickname prompt's typing (`flow::type_key`).
pub fn type_text(mut keys: MessageReader<bevy::input::keyboard::KeyboardInput>, mut sim: Option<NonSendMut<Sim>>) {
    use bevy::input::keyboard::Key as K;
    let Some(sim) = sim.as_mut() else {
        keys.clear();
        return;
    };
    for e in keys.read() {
        if !e.state.is_pressed() || !flow::typing(&sim.game) {
            continue;
        }
        let k = match &e.logical_key {
            K::Character(c) => c.chars().next().map(flow::TypedKey::Char),
            K::Space => Some(flow::TypedKey::Char(' ')),
            K::Backspace => Some(flow::TypedKey::Backspace),
            K::Enter | K::Escape => Some(flow::TypedKey::Enter),
            _ => None,
        };
        if let Some(k) = k {
            flow::type_key(&mut sim.game, k);
            sim.game.flow.ui_dirty.set(true);
        }
    }
}

/// Mouse clicks -> the screen's tap targets (first hit wins).
pub fn click_ui(
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    windows: Query<&Window>,
    hits: Res<UiHits>,
    mut sim: Option<NonSendMut<Sim>>,
    mut press: ResMut<UiPress>,
) {
    // the results leaderboard: drag and wheel over its area
    let over_list = |win: &Window| -> bool {
        let Some(p) = win.cursor_position() else { return false };
        let s = ui_scale(win.width(), win.height());
        let (ux, uy) = ((p.x - win.width() / 2.0) / s, (p.y - win.height() / 2.0) / s);
        (-317.0..317.0).contains(&ux) && (-112.0..343.0).contains(&uy)
    };
    if let (Some(sim), Ok(win)) = (sim.as_mut(), windows.single()) {
        if crate::flow::results_list(&mut sim.game).is_some() {
            let y = win.cursor_position().map(|p| p.y);
            if buttons.just_pressed(MouseButton::Left) && over_list(win) {
                if let Some(y) = y {
                    flow::pointer(&mut sim.game, flow::Pointer::Down(y));
                }
            } else if buttons.pressed(MouseButton::Left) {
                if let Some(y) = y {
                    flow::pointer(&mut sim.game, flow::Pointer::Move(y));
                }
            }
            if buttons.just_released(MouseButton::Left) {
                flow::pointer(&mut sim.game, flow::Pointer::Up);
            }
            if over_list(win) {
                for e in wheel.read() {
                    // browser deltaY: ~100 px a notch, positive downwards
                    let d = match e.unit {
                        bevy::input::mouse::MouseScrollUnit::Line => -e.y * 100.0,
                        bevy::input::mouse::MouseScrollUnit::Pixel => -e.y,
                    };
                    flow::pointer(&mut sim.game, flow::Pointer::Wheel(d));
                }
            }
        }
    }
    // the menus' lists scroll with the wheel (virtual units)
    if let Some(sim) = sim.as_mut() {
        for e in wheel.read() {
            let dy = match e.unit {
                bevy::input::mouse::MouseScrollUnit::Line => e.y * 60.0,
                bevy::input::mouse::MouseScrollUnit::Pixel => e.y * 2.0,
            };
            let dx = match e.unit {
                bevy::input::mouse::MouseScrollUnit::Line => e.x * 60.0,
                bevy::input::mouse::MouseScrollUnit::Pixel => e.x * 2.0,
            };
            crate::menu::scroll(&mut sim.game, -(dy + dx));
            sim.game.flow.ui_dirty.set(true);
        }
    }
    if buttons.just_released(MouseButton::Left) && press.0.take().is_some() {
        if let Some(sim) = sim.as_ref() {
            sim.game.flow.ui_dirty.set(true);
        }
    }
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let (Ok(win), Some(sim)) = (windows.single(), sim.as_mut()) else { return };
    let Some(p) = win.cursor_position() else { return };
    if let Some((r, c)) = hits.0.iter().find(|(r, _)| r.contains(p)) {
        if is_button(r, *c, win.width(), win.height()) {
            press.0 = Some(*r);
        }
        flow::click(&mut sim.game, *c);
        sim.game.flow.ui_dirty.set(true);
    }
}
