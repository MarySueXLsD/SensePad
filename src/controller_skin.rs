//! Full-colour DualSense view built from a GamepadViewer-style skin (the PS5 skin by
//! CrazyGrape; credits are in the README). The PNGs in `assets/gv` are built into the program.
//!
//! The body is split into tintable layers (tools/split_gv_skin.py), so any colourway works:
//! shell, touchpad and centre are multiplied by the chosen colours, the light bar shows the
//! controller's real LED colour, sticks move with the real stick input, and pressed buttons
//! use the skin's pressed-frame sprites.

use crate::dualsense::PadSnapshot;
use crate::i18n::{input_label, l, lf};
use crate::input::Input;
use crate::kit::PadConfig;
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Color32, Id, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, Vec2};

const W: f32 = 807.0;
const H: f32 = 651.0;
/// How far the stick sprites travel at full tilt (skin pixels).
const STICK_TRAVEL: f32 = 18.0;

macro_rules! asset {
    ($name:literal) => {
        include_bytes!(concat!("../assets/gv/", $name))
    };
}

pub struct SkinTextures {
    core: TextureHandle,
    shell: TextureHandle,
    touch: TextureHandle,
    light: TextureHandle,
    base: TextureHandle,
    sticks: TextureHandle,
    buttons: TextureHandle,
    dpad: TextureHandle,
    bumpers: TextureHandle,
    triggers: TextureHandle,
    arrows: TextureHandle,
    ps: TextureHandle,
}

fn tex(ctx: &egui::Context, name: &str, png: &[u8]) -> Option<TextureHandle> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

impl SkinTextures {
    pub fn load(ctx: &egui::Context) -> Option<Self> {
        Some(Self {
            core: tex(ctx, "gv-core", asset!("layer_core.png"))?,
            shell: tex(ctx, "gv-shell", asset!("layer_shell.png"))?,
            touch: tex(ctx, "gv-touch", asset!("layer_touch.png"))?,
            light: tex(ctx, "gv-light", asset!("layer_light.png"))?,
            base: tex(ctx, "gv-base", asset!("layer_base.png"))?,
            sticks: tex(ctx, "gv-sticks", asset!("nXaGdI2.png"))?,
            buttons: tex(ctx, "gv-buttons", asset!("DVqDSsJ.png"))?,
            dpad: tex(ctx, "gv-dpad", asset!("hCmzXWK.png"))?,
            bumpers: tex(ctx, "gv-bumpers", asset!("2YssqRT.png"))?,
            triggers: tex(ctx, "gv-triggers", asset!("LsxmGBD.png"))?,
            arrows: tex(ctx, "gv-arrows", asset!("YJRVQxC.png"))?,
            ps: tex(ctx, "gv-ps", asset!("PS_Color.png"))?,
        })
    }
}

pub use crate::theme::SkinTints as Tints;

pub struct SkinOpts<'a> {
    pub tints: Tints,
    pub accent: Color32,
    pub mute_led: bool,
    pub pad_names: &'a [String],
}

/// Draw `src` (pixels of `tex`) into `dst`; `flip` mirrors horizontally.
fn sprite(p: &Painter, tex: &TextureHandle, src: Rect, dst: Rect, tint: Color32, flip: bool) {
    let [tw, th] = tex.size();
    let (tw, th) = (tw as f32, th as f32);
    let (u0, u1) = (src.min.x / tw, src.max.x / tw);
    let uv = Rect::from_min_max(pos2(if flip { u1 } else { u0 }, src.min.y / th), pos2(if flip { u0 } else { u1 }, src.max.y / th));
    p.image(tex.id(), dst, uv, tint);
}

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

enum Hit {
    Rect(Rect),
    Circle(Pos2, f32),
}

/// Clickable areas (skin pixels), taken from the visualizer's CSS layout.
fn hits() -> [(Input, Hit); 20] {
    [
        (Input::L2, Hit::Rect(r(94.0, 0.0, 111.0, 106.0))),
        (Input::R2, Hit::Rect(r(602.0, 0.0, 111.0, 106.0))),
        (Input::L1, Hit::Rect(r(93.0, 108.0, 110.0, 38.0))),
        (Input::R1, Hit::Rect(r(603.0, 108.0, 110.0, 38.0))),
        (Input::TouchLeft, Hit::Rect(r(240.0, 120.0, 163.0, 175.0))),
        (Input::TouchRight, Hit::Rect(r(403.0, 120.0, 163.0, 175.0))),
        (Input::Create, Hit::Rect(r(193.0, 140.0, 31.0, 57.0))),
        (Input::Options, Hit::Rect(r(582.0, 140.0, 31.0, 57.0))),
        (Input::Up, Hit::Rect(r(117.0, 196.0, 52.0, 63.0))),
        (Input::Down, Hit::Rect(r(117.0, 277.0, 52.0, 63.0))),
        (Input::Left, Hit::Rect(r(70.0, 243.0, 64.0, 52.0))),
        (Input::Right, Hit::Rect(r(152.0, 242.0, 63.0, 52.0))),
        (Input::Triangle, Hit::Circle(pos2(663.0, 207.0), 29.0)),
        (Input::Square, Hit::Circle(pos2(602.0, 268.0), 29.0)),
        (Input::Circle, Hit::Circle(pos2(725.0, 269.0), 29.0)),
        (Input::Cross, Hit::Circle(pos2(663.0, 330.0), 29.0)),
        (Input::L3, Hit::Circle(pos2(270.0, 383.0), 52.0)),
        (Input::R3, Hit::Circle(pos2(537.0, 383.0), 52.0)),
        (Input::Ps, Hit::Circle(pos2(403.0, 377.0), 28.0)),
        (Input::Mute, Hit::Rect(r(380.0, 419.0, 47.0, 16.0))),
    ]
}

/// Pressed-frame sprites: (input, source rect in sheet, destination rect in skin space).
fn face_frames() -> [(Input, Rect, Rect); 4] {
    let ox = 573.0;
    let oy = 178.0;
    [
        (Input::Cross, r(0.0, 59.0, 58.0, 58.0), r(ox + 61.0, oy + 123.0, 58.0, 58.0)),
        (Input::Circle, r(59.0, 59.0, 58.0, 58.0), r(ox + 123.0, oy + 62.0, 58.0, 58.0)),
        (Input::Square, r(118.0, 59.0, 58.0, 58.0), r(ox, oy + 61.0, 58.0, 58.0)),
        (Input::Triangle, r(177.0, 59.0, 58.0, 58.0), r(ox + 61.0, oy, 58.0, 58.0)),
    ]
}

fn dpad_frames() -> [(Input, Rect, Rect); 4] {
    let (ox, oy) = (71.0, 196.0);
    [
        (Input::Up, r(0.0, 68.0, 52.0, 63.0), r(ox + 46.0, oy, 52.0, 63.0)),
        (Input::Down, r(54.0, 68.0, 52.0, 63.0), r(ox + 46.0, oy + 81.0, 52.0, 63.0)),
        (Input::Left, r(108.0, 68.0, 64.0, 52.0), r(ox - 1.0, oy + 47.0, 64.0, 52.0)),
        (Input::Right, r(175.0, 68.0, 63.0, 52.0), r(ox + 81.0, oy + 46.0, 63.0, 52.0)),
    ]
}

const TOUCH_AREA: Rect = Rect { min: Pos2 { x: 244.0, y: 122.0 }, max: Pos2 { x: 562.0, y: 292.0 } };

/// Paint the controller into `area`. Returns a control the user clicked, if any.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    area: Rect,
    t: &SkinTextures,
    snap: &PadSnapshot,
    pads: &[PadConfig],
    selected: usize,
    o: &SkinOpts,
) -> Option<Input> {
    let scale = (area.width() / W).min(area.height() / H);
    let img = Rect::from_center_size(area.center(), vec2(W, H) * scale);
    let at = |x: f32, y: f32| img.min + vec2(x, y) * scale;
    let tr = |rr: Rect| Rect::from_min_max(at(rr.min.x, rr.min.y), at(rr.max.x, rr.max.y));
    let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let connected = snap.conn.is_some();
    let p = ui.painter().clone();

    // Body layers.
    let light = if connected { rgb(snap.lightbar) } else { mix(Color32::from_rgb(90, 92, 100), o.accent, 0.35) };
    p.image(t.core.id(), img, full, o.tints.core);
    p.image(t.shell.id(), img, full, o.tints.shell);
    p.image(t.touch.id(), img, full, o.tints.touch);
    p.image(t.light.id(), img, full, light);
    p.image(t.base.id(), img, full, Color32::WHITE);

    // Touchpad halves light up in the accent colour when clicked.
    for (input, left) in [(Input::TouchLeft, true), (Input::TouchRight, false)] {
        if snap.down(input) {
            let (u0, u1) = if left { (0.0, 403.0 / W) } else { (403.0 / W, 1.0) };
            let dst = Rect::from_min_max(pos2(img.min.x + u0 * img.width(), img.min.y), pos2(img.min.x + u1 * img.width(), img.max.y));
            p.image(t.touch.id(), dst, Rect::from_min_max(pos2(u0, 0.0), pos2(u1, 1.0)), alpha(o.accent, 0.55));
        }
    }

    // Pressed frames.
    let white = Color32::WHITE;
    for (input, src, dst) in face_frames().into_iter().chain(dpad_frames()) {
        if snap.down(input) {
            sprite(&p, &t.buttons_or_dpad(input), src, tr(dst).translate(vec2(0.0, scale)), white, false);
        }
    }
    if snap.down(Input::Create) {
        sprite(&p, &t.arrows, r(0.0, 0.0, 27.0, 57.0), tr(r(195.0, 140.0, 27.0, 57.0)), white, false);
    }
    if snap.down(Input::Options) {
        sprite(&p, &t.arrows, r(28.0, 0.0, 27.0, 57.0), tr(r(584.0, 140.0, 27.0, 57.0)), white, false);
    }
    if snap.down(Input::L1) {
        sprite(&p, &t.bumpers, r(0.0, 0.0, 110.0, 35.0), tr(r(93.0, 115.0, 110.0, 35.0)), white, false);
    }
    if snap.down(Input::R1) {
        sprite(&p, &t.bumpers, r(0.0, 0.0, 110.0, 35.0), tr(r(603.0, 115.0, 110.0, 35.0)), white, true);
    }
    for (amount, src, x) in [(snap.l2, r(0.0, 0.0, 111.0, 108.0), 94.0), (snap.r2, r(113.0, 0.0, 111.0, 108.0), 602.0)] {
        if amount > 0.03 {
            sprite(&p, &t.triggers, src, tr(r(x, 0.0, 111.0, 108.0)), alpha(white, amount.min(1.0)), false);
        }
    }
    if snap.down(Input::Ps) {
        sprite(&p, &t.ps, r(0.0, 0.0, 320.0, 320.0), tr(r(373.0, 345.0, 60.0, 60.0)), white, false);
    }
    if o.mute_led && connected {
        p.rect_filled(tr(r(386.0, 423.0, 35.0, 8.0)), egui::CornerRadius::same(3), Color32::from_rgb(255, 140, 50));
    }

    // Sticks move with the real sticks; pressing them shows the pressed frame.
    for (input, x0, (sx, sy)) in [(Input::L3, 220.0, (snap.lx, snap.ly)), (Input::R3, 487.0, (snap.rx, snap.ry))] {
        let off = vec2(sx, sy) * STICK_TRAVEL;
        let frame = if snap.down(input) { r(102.0, 0.0, 100.0, 100.0) } else { r(0.0, 0.0, 100.0, 100.0) };
        sprite(&p, &t.sticks, frame, tr(r(x0 + off.x, 333.0 + off.y, 100.0, 100.0)), white, false);
    }

    // Live touch points.
    let pad = tr(TOUCH_AREA);
    for tp in snap.touches.iter().flatten() {
        let pos = pad.min + vec2(tp.0 * pad.width(), tp.1 * pad.height());
        p.circle_filled(pos, 16.0 * scale + 3.0, alpha(o.accent, 0.25));
        p.circle_filled(pos, 7.0 * scale + 2.0, o.accent);
    }

    // Interaction: hover outline, binding dots, tooltip, click to bind.
    let mut clicked = None;
    for (input, hit) in hits() {
        let (rect, circle) = match hit {
            Hit::Rect(rr) => (tr(rr), None),
            Hit::Circle(c, rad) => (Rect::from_center_size(at(c.x, c.y), Vec2::splat(2.0 * rad * scale)), Some(rad * scale)),
        };
        let resp = ui.interact(rect, Id::unique(("gv", input as u8)), Sense::click());
        if resp.hovered() {
            match circle {
                Some(rad) => {
                    p.circle_stroke(rect.center(), rad + 2.0, Stroke::new(2.0, o.accent));
                }
                None => {
                    p.rect_stroke(rect, egui::CornerRadius::same(8), Stroke::new(2.0, o.accent), StrokeKind::Outside);
                }
            }
        }
        let bound: Vec<(usize, &PadConfig)> = pads.iter().enumerate().filter(|(_, c)| c.binding == Some(input)).collect();
        let anchor = match circle {
            Some(rad) => rect.center() + vec2(rad * 0.75, -rad * 0.75),
            None => rect.right_top() + vec2(-4.0, 4.0),
        };
        for (n, (_, cfg)) in bound.iter().take(3).enumerate() {
            let dot = anchor + vec2(-(n as f32) * 8.0, 0.0);
            p.circle_filled(dot, 4.0, rgb(cfg.color));
            p.circle_stroke(dot, 4.0, Stroke::new(1.2, Color32::from_black_alpha(160)));
        }

        let mut tip = input_label(input).to_string();
        if bound.is_empty() {
            tip.push_str(l(" — свободна", " — not bound"));
        } else {
            for (i, _) in &bound {
                tip.push_str(&format!("\n→ {}", o.pad_names[*i]));
            }
        }
        if input == Input::Ps {
            tip.push('\n');
            tip.push_str(l("Занята Windows и Steam", "Kept by Windows and Steam"));
        } else if let Some(name) = o.pad_names.get(selected) {
            tip.push('\n');
            tip.push_str(&lf("Клик — назначить на «{}»", "Click to bind to “{}”", &[name]));
        }
        if resp.on_hover_text(tip).clicked() && input.bindable() {
            clicked = Some(input);
        }
    }
    clicked
}

impl SkinTextures {
    fn buttons_or_dpad(&self, input: Input) -> TextureHandle {
        match input {
            Input::Up | Input::Down | Input::Left | Input::Right => self.dpad.clone(),
            _ => self.buttons.clone(),
        }
    }
}

/// A small static preview (used for the colourway picker in Settings).
pub fn preview(p: &Painter, rect: Rect, t: &SkinTextures, tints: Tints, light: Color32) {
    let scale = (rect.width() / W).min(rect.height() / H);
    let img = Rect::from_center_size(rect.center(), vec2(W, H) * scale);
    let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    p.image(t.core.id(), img, full, tints.core);
    p.image(t.shell.id(), img, full, tints.shell);
    p.image(t.touch.id(), img, full, tints.touch);
    p.image(t.light.id(), img, full, light);
    p.image(t.base.id(), img, full, Color32::WHITE);
    for x0 in [220.0, 487.0] {
        let dst = Rect::from_min_max(img.min + vec2(x0, 333.0) * scale, img.min + vec2(x0 + 100.0, 433.0) * scale);
        sprite(p, &t.sticks, r(0.0, 0.0, 100.0, 100.0), dst, Color32::WHITE, false);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn layers_share_the_body_size() {
        let layers: [(&str, &[u8]); 5] = [
            ("core", asset!("layer_core.png")),
            ("shell", asset!("layer_shell.png")),
            ("touch", asset!("layer_touch.png")),
            ("light", asset!("layer_light.png")),
            ("base", asset!("layer_base.png")),
        ];
        for (name, png) in layers {
            let img = image::load_from_memory(png).unwrap();
            assert_eq!((img.width() as f32, img.height() as f32), (super::W, super::H), "{name}");
        }
    }
}
