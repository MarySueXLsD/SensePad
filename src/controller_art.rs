//! The DualSense line-art picture with live highlights on top.
//! The picture (`assets/dualsense.png`) is built into the program.

use crate::dualsense::PadSnapshot;
use crate::i18n::{input_label, l, lf};
use crate::input::Input;
use crate::kit::PadConfig;
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Color32, Id, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TextureHandle, Ui, Vec2};

const ART: &[u8] = include_bytes!("../assets/dualsense.png");
const ART_W: f32 = 1188.0;
const ART_H: f32 = 820.0;

/// Decode the built-in picture (white lines on transparent) into a texture.
pub fn load_texture(ctx: &egui::Context) -> Option<TextureHandle> {
    let img = image::load_from_memory_with_format(ART, image::ImageFormat::Png).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    Some(ctx.load_texture("dualsense-line-art", color, egui::TextureOptions::LINEAR))
}

pub struct ArtOpts<'a> {
    pub accent: Color32,
    pub line: Color32,
    pub mute_led: bool,
    pub pad_names: &'a [String],
}

enum Hot {
    Circle(Pos2, f32),
    Rect(Rect, f32),
}

/// Control positions measured on the picture (pixels of the 1188×820 asset).
fn spots() -> [(Input, Hot); 20] {
    let c = |x: f32, y: f32, r: f32| Hot::Circle(pos2(x, y), r);
    let r = |x0: f32, y0: f32, x1: f32, y1: f32, rad: f32| Hot::Rect(Rect::from_min_max(pos2(x0, y0), pos2(x1, y1)), rad);
    [
        (Input::L2, r(150.0, 10.0, 306.0, 40.0, 14.0)),
        (Input::L1, r(150.0, 40.0, 306.0, 72.0, 14.0)),
        (Input::R2, r(882.0, 10.0, 1040.0, 40.0, 14.0)),
        (Input::R1, r(882.0, 40.0, 1040.0, 72.0, 14.0)),
        (Input::TouchLeft, r(364.0, 40.0, 594.0, 282.0, 22.0)),
        (Input::TouchRight, r(594.0, 40.0, 824.0, 282.0, 22.0)),
        (Input::Create, r(299.0, 84.0, 337.0, 142.0, 18.0)),
        (Input::Options, r(851.0, 83.0, 890.0, 141.0, 18.0)),
        (Input::Up, r(189.0, 142.0, 261.0, 231.0, 24.0)),
        (Input::Down, r(189.0, 255.0, 261.0, 344.0, 24.0)),
        (Input::Left, r(123.0, 208.0, 212.0, 279.0, 24.0)),
        (Input::Right, r(237.0, 206.0, 327.0, 278.0, 24.0)),
        (Input::Triangle, c(962.0, 156.0, 41.0)),
        (Input::Square, c(875.0, 243.0, 41.0)),
        (Input::Circle, c(1050.0, 243.0, 41.0)),
        (Input::Cross, c(962.0, 331.0, 41.0)),
        (Input::L3, c(402.0, 404.0, 80.0)),
        (Input::R3, c(783.0, 404.0, 80.0)),
        (Input::Ps, c(594.0, 395.0, 34.0)),
        (Input::Mute, r(562.0, 459.0, 626.0, 478.0, 10.0)),
    ]
}

/// The light-bar grooves along both sides of the touchpad.
const LIGHT_L: [(f32, f32); 5] = [(353.0, 75.0), (357.0, 100.0), (367.0, 150.0), (377.0, 200.0), (388.0, 250.0)];
const LIGHT_R: [(f32, f32); 5] = [(835.0, 75.0), (830.0, 100.0), (820.0, 150.0), (809.0, 200.0), (799.0, 250.0)];
const TOUCH_AREA: (f32, f32, f32, f32) = (368.0, 42.0, 820.0, 280.0);

/// Paint the controller into `area`. Returns a control the user clicked, if any.
pub fn show(
    ui: &mut Ui,
    area: Rect,
    tex: &TextureHandle,
    snap: &PadSnapshot,
    pads: &[PadConfig],
    selected: usize,
    o: &ArtOpts,
) -> Option<Input> {
    let scale = (area.width() / ART_W).min(area.height() / ART_H);
    let img = Rect::from_center_size(area.center(), vec2(ART_W, ART_H) * scale);
    let at = |x: f32, y: f32| img.min + vec2(x, y) * scale;
    let connected = snap.conn.is_some();
    let p = ui.painter().clone();

    // Light bar glow sits behind the line art.
    if connected {
        let col = rgb(snap.lightbar);
        for strip in [LIGHT_L, LIGHT_R] {
            let pts: Vec<Pos2> = strip.iter().map(|(x, y)| at(*x, *y)).collect();
            p.add(Shape::line(pts.clone(), Stroke::new(16.0 * scale + 4.0, alpha(col, 0.16))));
            p.add(Shape::line(pts, Stroke::new((7.0 * scale).max(2.0), col)));
        }
    }

    let tint = if connected { o.line } else { alpha(o.line, 0.45) };
    p.image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);

    let mut clicked = None;
    for (input, hot) in spots() {
        let (rect, circle) = match hot {
            Hot::Circle(c, r) => (Rect::from_center_size(at(c.x, c.y), Vec2::splat(2.0 * r * scale)), Some(r * scale)),
            Hot::Rect(r, _) => (Rect::from_min_max(at(r.min.x, r.min.y), at(r.max.x, r.max.y)), None),
        };
        let round = match hot {
            Hot::Rect(_, rad) => egui::CornerRadius::same((rad * scale).clamp(0.0, 255.0) as u8),
            Hot::Circle(..) => egui::CornerRadius::ZERO,
        };
        let resp = ui.interact(rect, Id::unique(("art", input as u8)), Sense::click());
        let amount = match input {
            Input::L2 => snap.l2,
            Input::R2 => snap.r2,
            _ if snap.down(input) => 1.0,
            _ => 0.0,
        };
        let draw = |fill: Color32, stroke: Stroke| match circle {
            Some(r) => {
                p.circle(rect.center(), r, fill, stroke);
            }
            None => {
                p.rect(rect, round, fill, stroke, StrokeKind::Inside);
            }
        };
        if amount > 0.01 {
            draw(alpha(o.accent, 0.2 + 0.5 * amount), Stroke::NONE);
        }
        if input == Input::Mute && o.mute_led && connected {
            draw(alpha(Color32::from_rgb(255, 140, 50), 0.85), Stroke::NONE);
        }
        if resp.hovered() {
            draw(Color32::TRANSPARENT, Stroke::new(2.0, o.accent));
        }

        // Pad-colour dots for pads bound to this control.
        let bound: Vec<(usize, &PadConfig)> = pads.iter().enumerate().filter(|(_, c)| c.binding == Some(input)).collect();
        let anchor = match circle {
            Some(r) => rect.center() + vec2(r * 0.72, -r * 0.72),
            None => rect.right_top() + vec2(-4.0, 4.0),
        };
        for (n, (_, cfg)) in bound.iter().take(3).enumerate() {
            let dot = anchor + vec2(-(n as f32) * 8.0, 0.0);
            p.circle_filled(dot, 3.6, rgb(cfg.color));
            p.circle_stroke(dot, 3.6, Stroke::new(1.0, bg()));
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
        let resp = resp.on_hover_text(tip);
        if resp.clicked() && input.bindable() {
            clicked = Some(input);
        }
    }

    // Stick positions.
    for (c, (x, y)) in [(at(402.0, 404.0), (snap.lx, snap.ly)), (at(783.0, 404.0), (snap.rx, snap.ry))] {
        let off = vec2(x, y) * 58.0 * scale;
        if off.length() > 2.0 {
            p.line_segment([c, c + off], Stroke::new(2.0, alpha(o.accent, 0.5)));
            p.circle_filled(c + off, 9.0 * scale + 2.0, o.accent);
        }
    }

    // Live touch points on the touchpad.
    let (x0, y0, x1, y1) = TOUCH_AREA;
    let pad = Rect::from_min_max(at(x0, y0), at(x1, y1));
    for t in snap.touches.iter().flatten() {
        let pos = pad.min + vec2(t.0 * pad.width(), t.1 * pad.height());
        p.circle_filled(pos, 14.0 * scale + 3.0, alpha(o.accent, 0.25));
        p.circle_filled(pos, 6.0 * scale + 2.0, o.accent);
    }
    clicked
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_picture_decodes_at_measured_size() {
        let img = image::load_from_memory(super::ART).unwrap();
        assert_eq!((img.width() as f32, img.height() as f32), (super::ART_W, super::ART_H));
    }
}
