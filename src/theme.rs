//! Look and feel: palettes (themes), corner rounding, spacing density, fonts,
//! controller colourways and a few painting helpers.

use crate::input::Input;
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind,
    TextStyle, TextureHandle, Visuals,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

pub const OK: Color32 = Color32::from_rgb(74, 222, 128);
pub const WARN: Color32 = Color32::from_rgb(251, 191, 36);
pub const DANGER: Color32 = Color32::from_rgb(248, 113, 113);
pub const REC: Color32 = Color32::from_rgb(255, 70, 70);
/// Text drawn on top of bright accent fills.
pub const ON_ACCENT: Color32 = Color32::from_rgb(14, 15, 18);

// ----- palettes --------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeKind {
    /// Neutral graphite, like studio gear.
    #[default]
    Studio,
    Midnight,
    Oled,
    Taiga,
    Sunset,
    Light,
    /// Built from a background colour the user picks.
    Custom,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 7] = [
        ThemeKind::Studio,
        ThemeKind::Midnight,
        ThemeKind::Oled,
        ThemeKind::Taiga,
        ThemeKind::Sunset,
        ThemeKind::Light,
        ThemeKind::Custom,
    ];

    pub fn name(self) -> &'static str {
        use crate::i18n::l;
        match self {
            ThemeKind::Studio => l("Графит", "Graphite"),
            ThemeKind::Midnight => l("Полночь", "Midnight"),
            ThemeKind::Oled => "OLED",
            ThemeKind::Taiga => l("Тайга", "Taiga"),
            ThemeKind::Sunset => l("Закат", "Sunset"),
            ThemeKind::Light => l("День", "Daylight"),
            ThemeKind::Custom => l("Своя", "Custom"),
        }
    }

    pub fn palette(self, custom_bg: [u8; 3]) -> Palette {
        match self {
            ThemeKind::Studio => STUDIO,
            ThemeKind::Midnight => MIDNIGHT,
            ThemeKind::Oled => OLED,
            ThemeKind::Taiga => TAIGA,
            ThemeKind::Sunset => SUNSET,
            ThemeKind::Light => LIGHT,
            ThemeKind::Custom => Palette::from_bg(custom_bg),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Window background (top) and its slightly darker bottom.
    pub bg: Color32,
    pub bg2: Color32,
    /// Bars and the sidebar.
    pub surface: Color32,
    pub card: Color32,
    pub card_hi: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub dark: bool,
    /// Corner rounding of this theme (multiplied by the user's setting).
    pub round: f32,
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

const STUDIO: Palette = Palette {
    bg: c(29, 29, 32),
    bg2: c(22, 22, 25),
    surface: c(38, 38, 42),
    card: c(47, 47, 52),
    card_hi: c(60, 60, 66),
    border: c(74, 74, 82),
    text: c(240, 240, 238),
    dim: c(170, 170, 176),
    faint: c(118, 118, 126),
    dark: true,
    round: 0.7,
};
const MIDNIGHT: Palette = Palette {
    bg: c(13, 15, 22),
    bg2: c(8, 9, 14),
    surface: c(18, 21, 30),
    card: c(25, 29, 40),
    card_hi: c(34, 39, 53),
    border: c(44, 50, 67),
    text: c(233, 236, 242),
    dim: c(144, 152, 170),
    faint: c(90, 98, 116),
    dark: true,
    round: 1.0,
};
const OLED: Palette = Palette {
    bg: c(0, 0, 0),
    bg2: c(0, 0, 0),
    surface: c(9, 9, 11),
    card: c(18, 19, 23),
    card_hi: c(29, 31, 37),
    border: c(40, 42, 50),
    text: c(236, 238, 243),
    dim: c(140, 146, 160),
    faint: c(84, 90, 104),
    dark: true,
    round: 1.0,
};
const TAIGA: Palette = Palette {
    bg: c(16, 23, 21),
    bg2: c(10, 15, 14),
    surface: c(21, 30, 27),
    card: c(28, 39, 35),
    card_hi: c(38, 52, 47),
    border: c(51, 67, 60),
    text: c(232, 241, 235),
    dim: c(152, 172, 162),
    faint: c(98, 118, 108),
    dark: true,
    round: 1.0,
};
const SUNSET: Palette = Palette {
    bg: c(27, 18, 25),
    bg2: c(18, 11, 18),
    surface: c(35, 24, 33),
    card: c(46, 32, 43),
    card_hi: c(60, 43, 56),
    border: c(78, 57, 72),
    text: c(247, 237, 239),
    dim: c(188, 164, 174),
    faint: c(130, 108, 120),
    dark: true,
    round: 1.25,
};
const LIGHT: Palette = Palette {
    bg: c(238, 240, 244),
    bg2: c(226, 229, 236),
    surface: c(249, 250, 252),
    card: c(255, 255, 255),
    card_hi: c(231, 234, 240),
    border: c(212, 217, 226),
    text: c(22, 26, 34),
    dim: c(86, 94, 110),
    faint: c(140, 148, 164),
    dark: false,
    round: 1.0,
};

impl Palette {
    /// Derive a whole theme from one background colour.
    pub fn from_bg(bg: [u8; 3]) -> Self {
        let base = rgb(bg);
        let dark = luminance(base) < 128.0;
        let toward = if dark { Color32::WHITE } else { Color32::BLACK };
        let step = |t: f32| mix(base, toward, t);
        let text = if dark { c(240, 240, 244) } else { c(20, 22, 28) };
        Self {
            bg: base,
            bg2: mix(base, Color32::BLACK, if dark { 0.3 } else { 0.05 }),
            surface: step(0.05),
            card: step(0.09),
            card_hi: step(0.16),
            border: step(0.24),
            text,
            dim: mix(text, base, 0.36),
            faint: mix(text, base, 0.6),
            dark,
            round: 1.0,
        }
    }
}

static PALETTE: RwLock<Palette> = RwLock::new(STUDIO);
static ROUND: AtomicU32 = AtomicU32::new(0x3F80_0000); // 1.0
static DENSITY: AtomicU32 = AtomicU32::new(0x3F80_0000);
static COLORED_GLYPHS: AtomicBool = AtomicBool::new(false);

pub fn palette() -> Palette {
    *PALETTE.read().unwrap()
}
pub fn bg() -> Color32 {
    palette().bg
}
pub fn surface() -> Color32 {
    palette().surface
}
pub fn card() -> Color32 {
    palette().card
}
pub fn card_hi() -> Color32 {
    palette().card_hi
}
pub fn border() -> Color32 {
    palette().border
}
pub fn text() -> Color32 {
    palette().text
}
pub fn dim() -> Color32 {
    palette().dim
}
pub fn faint() -> Color32 {
    palette().faint
}

pub fn set_round(scale: f32) {
    ROUND.store(scale.to_bits(), Ordering::Relaxed);
}
pub fn set_density(scale: f32) {
    DENSITY.store(scale.to_bits(), Ordering::Relaxed);
}
pub fn set_colored_glyphs(on: bool) {
    COLORED_GLYPHS.store(on, Ordering::Relaxed);
}

/// Corner radius for the active theme and the user's rounding setting.
pub fn cr(base: f32) -> CornerRadius {
    let k = palette().round * f32::from_bits(ROUND.load(Ordering::Relaxed));
    CornerRadius::same((base * k).round().clamp(0.0, 255.0) as u8)
}

/// A spacing value scaled by the density setting.
pub fn sp(x: f32) -> f32 {
    x * f32::from_bits(DENSITY.load(Ordering::Relaxed))
}

/// `sp` for margins.
pub fn spi(x: f32) -> i8 {
    sp(x).round().clamp(0.0, 127.0) as i8
}

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn luminance(c: Color32) -> f32 {
    0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32
}

/// Readable text colour on top of `fill`.
pub fn on_color(fill: Color32) -> Color32 {
    if luminance(fill) > 135.0 { ON_ACCENT } else { Color32::WHITE }
}

/// Scale a colour's brightness (for the controller light bar).
pub fn scaled(c: [u8; 3], k: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * k.clamp(0.0, 1.0)).round() as u8)
}

// ----- fonts -----------------------------------------------------------------

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

/// The condensed display face used for titles and numbers.
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}

/// Segoe UI for text and Bahnschrift for titles and numbers, straight from Windows.
/// Both cover Cyrillic. Missing files fall back to egui's built-in font.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let dir = std::path::Path::new(&windir).join("Fonts");
    let mut prop = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut bold = prop.clone();
    let mut disp = prop.clone();

    let add = |key: &str, file: &str, fonts: &mut egui::FontDefinitions| -> bool {
        match std::fs::read(dir.join(file)) {
            Ok(bytes) => {
                fonts.font_data.insert(key.into(), Arc::new(egui::FontData::from_owned(bytes)));
                true
            }
            Err(_) => false,
        }
    };
    if add("segoe", "segoeui.ttf", &mut fonts) {
        prop.insert(0, "segoe".into());
        bold.insert(0, "segoe".into());
        disp.insert(0, "segoe".into());
    }
    if add("segoe-sb", "seguisb.ttf", &mut fonts) {
        bold.insert(0, "segoe-sb".into());
        disp.insert(0, "segoe-sb".into());
    }
    if add("bahnschrift", "bahnschrift.ttf", &mut fonts) {
        disp.insert(0, "bahnschrift".into());
    }
    if add("segoe-sym", "seguisym.ttf", &mut fonts) {
        prop.push("segoe-sym".into());
        bold.push("segoe-sym".into());
        disp.push("segoe-sym".into());
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(FontFamily::Name("semibold".into()), bold);
    fonts.families.insert(FontFamily::Name("display".into()), disp);
    ctx.set_fonts(fonts);
}

// ----- visuals ---------------------------------------------------------------

/// Apply a theme and accent colour to every egui style.
pub fn apply(ctx: &egui::Context, kind: ThemeKind, custom_bg: [u8; 3], accent: Color32) {
    let p = kind.palette(custom_bg);
    *PALETTE.write().unwrap() = p;
    ctx.set_theme(if p.dark { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, display(24.0)),
            (TextStyle::Body, FontId::proportional(14.5)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = vec2(sp(10.0), sp(8.0));
        s.button_padding = vec2(sp(14.0), sp(7.0));
        s.interact_size.y = sp(30.0).max(24.0);
        s.slider_width = 190.0;
        s.combo_width = 160.0;
        s.scroll.bar_width = 8.0;

        let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
        v.panel_fill = p.bg;
        v.window_fill = p.surface;
        v.extreme_bg_color = if p.dark { mix(p.bg, Color32::BLACK, 0.25) } else { c(244, 246, 249) };
        v.faint_bg_color = p.card;
        v.window_stroke = Stroke::new(1.0, p.border);
        v.window_corner_radius = cr(14.0);
        v.menu_corner_radius = cr(10.0);
        let shadow_a = if p.dark { 150 } else { 40 };
        v.window_shadow = egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(shadow_a) };
        v.popup_shadow = egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(shadow_a) };
        v.selection.bg_fill = alpha(accent, 0.30);
        v.selection.stroke = Stroke::new(1.5, accent);
        v.hyperlink_color = accent;
        v.slider_trailing_fill = true;
        v.override_text_color = None;

        let hover = if p.dark { mix(p.card_hi, Color32::WHITE, 0.07) } else { mix(p.card_hi, Color32::BLACK, 0.04) };
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = p.card;
        w.noninteractive.weak_bg_fill = p.card;
        w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
        w.noninteractive.fg_stroke = Stroke::new(1.0, p.dim);
        w.noninteractive.corner_radius = cr(10.0);

        w.inactive.bg_fill = p.card_hi;
        w.inactive.weak_bg_fill = p.card_hi;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0, p.text);
        w.inactive.corner_radius = cr(9.0);

        w.hovered.bg_fill = hover;
        w.hovered.weak_bg_fill = hover;
        w.hovered.bg_stroke = Stroke::new(1.0, alpha(accent, 0.55));
        w.hovered.fg_stroke = Stroke::new(1.2, p.text);
        w.hovered.corner_radius = cr(9.0);
        w.hovered.expansion = 0.0;

        w.active.bg_fill = mix(p.card_hi, accent, 0.35);
        w.active.weak_bg_fill = mix(p.card_hi, accent, 0.35);
        w.active.bg_stroke = Stroke::new(1.0, accent);
        w.active.fg_stroke = Stroke::new(1.5, p.text);
        w.active.corner_radius = cr(9.0);
        w.active.expansion = 0.0;

        w.open = w.hovered;
        style.visuals = v;
    });
}

// ----- painting helpers ------------------------------------------------------

fn sheen_id() -> egui::Id {
    egui::Id::unique("sensepad-sheen")
}

/// Create the small gradient texture used for soft highlights. Call once per context.
pub fn init_textures(ctx: &egui::Context) {
    let h = 64;
    let pixels: Vec<Color32> = (0..h)
        .map(|y| {
            let t = 1.0 - y as f32 / (h - 1) as f32;
            Color32::from_white_alpha((t * t * 255.0) as u8)
        })
        .collect();
    let tex = ctx.load_texture("sheen", egui::ColorImage::new([1, h], pixels), egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(sheen_id(), tex));
}

/// Fill a rounded rectangle with `color` fading out towards the bottom.
pub fn fade_down(p: &Painter, rect: Rect, radius: CornerRadius, color: Color32) {
    let tex: Option<TextureHandle> = p.ctx().data(|d| d.get_temp(sheen_id()));
    if let Some(tex) = tex {
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        p.add(egui::epaint::RectShape::filled(rect, radius, color).with_texture(tex.id(), uv));
    }
}

/// Top-to-bottom gradient across the whole rectangle (no rounding).
pub fn gradient(p: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

/// Draw a waveform overview (min/max peaks). `progress` brightens the part already played.
pub fn waveform(p: &Painter, rect: Rect, peaks: &[(f32, f32)], color: Color32, progress: Option<f32>) {
    if peaks.is_empty() || rect.width() < 4.0 {
        return;
    }
    let norm = peaks.iter().map(|(lo, hi)| lo.abs().max(hi.abs())).fold(0.0f32, f32::max).max(1e-4);
    let n = ((rect.width() / 3.0) as usize).clamp(8, peaks.len());
    let bar = (rect.width() / n as f32 * 0.62).max(1.0);
    let mid = rect.center().y;
    let half = rect.height() * 0.5;
    for k in 0..n {
        let a = k * peaks.len() / n;
        let b = ((k + 1) * peaks.len() / n).max(a + 1).min(peaks.len());
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        for &(l, h) in &peaks[a..b] {
            lo = lo.min(l);
            hi = hi.max(h);
        }
        let x = rect.left() + (k as f32 + 0.5) * rect.width() / n as f32;
        let top = mid - (hi / norm).max(0.04) * half;
        let bot = mid - (lo / norm).min(-0.04) * half;
        let col = match progress {
            Some(pr) if (k as f32 + 0.5) / n as f32 <= pr => mix(color, Color32::WHITE, 0.55),
            Some(_) => alpha(color, 0.5),
            None => color,
        };
        p.line_segment([pos2(x, top), pos2(x, bot)], Stroke::new(bar, col));
    }
}

// ----- controller colourways -------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Skin {
    #[default]
    White,
    MidnightBlack,
    CosmicRed,
    StarlightBlue,
    GalacticPurple,
    NovaPink,
    /// User-picked shell / touchpad / centre colours.
    Custom,
}

/// Colours multiplied onto the tintable parts of the full-colour controller.
#[derive(Clone, Copy)]
pub struct SkinTints {
    pub shell: Color32,
    pub touch: Color32,
    pub core: Color32,
}

/// Shell, touchpad and centre colours of the Custom colourway.
static CUSTOM_SKIN: RwLock<[[u8; 3]; 3]> = RwLock::new([[236, 238, 244], [236, 238, 244], [48, 48, 48]]);

pub fn set_custom_skin(c: [[u8; 3]; 3]) {
    *CUSTOM_SKIN.write().unwrap() = c;
}

/// Colours for the hand-drawn controller.
#[derive(Clone, Copy)]
pub struct SkinColors {
    pub shell: Color32,
    pub shell_edge: Color32,
    pub core: Color32,
    pub core_edge: Color32,
    pub touchpad: Color32,
    pub touch_edge: Color32,
    pub button: Color32,
    pub glyph: Color32,
    pub stick: Color32,
    pub label: Color32,
}

impl Skin {
    pub const ALL: [Skin; 7] = [
        Skin::White,
        Skin::MidnightBlack,
        Skin::CosmicRed,
        Skin::StarlightBlue,
        Skin::GalacticPurple,
        Skin::NovaPink,
        Skin::Custom,
    ];

    pub fn name(self) -> &'static str {
        use crate::i18n::l;
        match self {
            Skin::White => l("Белый", "White"),
            Skin::MidnightBlack => "Midnight Black",
            Skin::CosmicRed => "Cosmic Red",
            Skin::StarlightBlue => "Starlight Blue",
            Skin::GalacticPurple => "Galactic Purple",
            Skin::NovaPink => "Nova Pink",
            Skin::Custom => l("Свой", "Custom"),
        }
    }

    pub fn tints(self) -> SkinTints {
        let t = |s: [u8; 3], tp: [u8; 3], co: [u8; 3]| SkinTints { shell: rgb(s), touch: rgb(tp), core: rgb(co) };
        match self {
            Skin::White => t([236, 238, 244], [236, 238, 244], [48, 48, 48]),
            Skin::MidnightBlack => t([70, 72, 80], [78, 80, 88], [30, 31, 35]),
            Skin::CosmicRed => t([190, 40, 52], [54, 52, 58], [40, 36, 40]),
            Skin::StarlightBlue => t([172, 198, 240], [188, 208, 244], [34, 46, 80]),
            Skin::GalacticPurple => t([118, 82, 178], [52, 46, 70], [36, 30, 52]),
            Skin::NovaPink => t([246, 180, 208], [250, 196, 220], [80, 58, 74]),
            Skin::Custom => {
                let c = *CUSTOM_SKIN.read().unwrap();
                t(c[0], c[1], c[2])
            }
        }
    }

    pub fn colors(self) -> SkinColors {
        let t = self.tints();
        let dark_shell = luminance(t.shell) < 110.0;
        SkinColors {
            shell: t.shell,
            shell_edge: mix(t.shell, Color32::BLACK, 0.2),
            core: t.core,
            core_edge: mix(t.core, Color32::WHITE, 0.15),
            touchpad: t.touch,
            touch_edge: mix(t.touch, Color32::BLACK, 0.15),
            button: if dark_shell { c(26, 28, 33) } else { c(52, 55, 63) },
            glyph: if dark_shell { c(160, 165, 176) } else { c(196, 200, 210) },
            stick: mix(t.core, Color32::BLACK, 0.3),
            label: if dark_shell { mix(t.shell, Color32::WHITE, 0.6) } else { mix(t.shell, Color32::BLACK, 0.5) },
        }
    }
}

// ----- glyphs ----------------------------------------------------------------

/// PlayStation face-button colours (used when "Classic colours" is on).
pub fn ps_color(input: Input) -> Color32 {
    match input {
        Input::Cross => c(124, 178, 255),
        Input::Circle => c(255, 105, 105),
        Input::Square => c(236, 136, 214),
        Input::Triangle => c(64, 226, 170),
        _ => text(),
    }
}

/// Colour for a face-button symbol: neutral, or the classic colours if enabled.
pub fn glyph_color(input: Input, mono: Color32) -> Color32 {
    let face = matches!(input, Input::Cross | Input::Circle | Input::Square | Input::Triangle);
    if face && COLORED_GLYPHS.load(Ordering::Relaxed) { ps_color(input) } else { mono }
}

/// Draw a control glyph (face-button shapes, arrows, or a short label).
pub fn draw_glyph(p: &Painter, c: Pos2, size: f32, input: Input, color: Color32) {
    let w = (size * 0.10).max(1.3);
    let s = Stroke::new(w, color);
    let h = size * 0.28;
    match input {
        Input::Cross => {
            p.line_segment([c + vec2(-h, -h), c + vec2(h, h)], s);
            p.line_segment([c + vec2(-h, h), c + vec2(h, -h)], s);
        }
        Input::Circle => {
            p.circle_stroke(c, h * 1.15, s);
        }
        Input::Square => {
            p.rect_stroke(Rect::from_center_size(c, vec2(h * 2.0, h * 2.0)), CornerRadius::same(1), s, StrokeKind::Middle);
        }
        Input::Triangle => {
            let pts = vec![c + vec2(0.0, -h * 1.1), c + vec2(h * 1.1, h * 0.8), c + vec2(-h * 1.1, h * 0.8)];
            p.add(Shape::closed_line(pts, s));
        }
        Input::Up | Input::Down | Input::Left | Input::Right => {
            let (dx, dy) = match input {
                Input::Up => (0.0, -1.0),
                Input::Down => (0.0, 1.0),
                Input::Left => (-1.0, 0.0),
                _ => (1.0, 0.0),
            };
            let d = vec2(dx, dy) * h;
            let n = vec2(-dy, dx) * h;
            let pts = vec![c + d, c - d * 0.6 + n, c - d * 0.6 - n];
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Input::TouchLeft | Input::TouchRight => {
            let r = Rect::from_center_size(c, vec2(size * 0.72, size * 0.46));
            p.rect_stroke(r, CornerRadius::same(3), Stroke::new(w * 0.8, color), StrokeKind::Middle);
            let half = if input == Input::TouchLeft {
                Rect::from_min_max(r.min, pos2(r.center().x, r.max.y))
            } else {
                Rect::from_min_max(pos2(r.center().x, r.min.y), r.max)
            };
            p.rect_filled(half.shrink(w), CornerRadius::same(2), color);
        }
        _ => {
            p.text(c, Align2::CENTER_CENTER, input.short(), semibold(size * 0.42), color);
        }
    }
}

/// A rounded badge showing a binding.
pub fn badge(p: &Painter, rect: Rect, input: Input, fill: Color32) {
    p.rect_filled(rect, cr(8.0), fill);
    draw_glyph(p, rect.center(), rect.height(), input, glyph_color(input, on_color(fill)));
}

/// App logo: four pads in a rounded square.
pub fn logo(p: &Painter, rect: Rect, accent: Color32) {
    p.rect_filled(rect, cr(10.0), accent);
    let inner = rect.shrink(rect.width() * 0.2);
    let g = inner.width() * 0.12;
    let cell = (inner.width() - g) / 2.0;
    for (i, a) in [0.95, 0.55, 0.55, 0.75].iter().enumerate() {
        let (cx, cy) = ((i % 2) as f32, (i / 2) as f32);
        let r = Rect::from_min_size(inner.min + vec2(cx * (cell + g), cy * (cell + g)), vec2(cell, cell));
        p.rect_filled(r, cr(3.0), alpha(on_color(accent), *a));
    }
}

/// Window icon generated at startup.
pub fn app_icon() -> egui::IconData {
    let n = 64usize;
    let mut rgba = vec![0u8; n * n * 4];
    let accent = [94u8, 234, 212];
    let rr = |x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32| -> bool {
        let cx = x.clamp(x0 + r, x1 - r);
        let cy = y.clamp(y0 + r, y1 - r);
        (x - cx).powi(2) + (y - cy).powi(2) <= r * r
    };
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let i = (y * n + x) * 4;
            if !rr(fx, fy, 2.0, 2.0, 62.0, 62.0, 14.0) {
                continue;
            }
            let mut px = [accent[0], accent[1], accent[2], 255];
            for (cx, cy) in [(14.0, 14.0), (34.0, 14.0), (14.0, 34.0), (34.0, 34.0)] {
                if rr(fx, fy, cx, cy, cx + 16.0, cy + 16.0, 4.0) {
                    px = [14, 17, 23, 255];
                }
            }
            rgba[i..i + 4].copy_from_slice(&px);
        }
    }
    egui::IconData { rgba, width: n as u32, height: n as u32 }
}
