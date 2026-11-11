//! A live, clickable drawing of a real DualSense (front view). Click a control to bind it.

use crate::dualsense::PadSnapshot;
use crate::i18n::{input_label, l, lf};
use crate::input::Input;
use crate::kit::PadConfig;
use crate::theme::*;
use eframe::egui::{pos2, vec2, Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};

const W: f32 = 560.0;
const H: f32 = 250.0;

pub struct ViewOpts<'a> {
    pub skin: Skin,
    pub accent: Color32,
    pub player_leds: u8,
    pub mute_led: bool,
    pub pad_names: &'a [String],
}

#[derive(Clone, Copy)]
enum Kind {
    Face,
    Dpad,
    Stick,
    Shoulder,
    Trigger,
    SmallButton,
    Ps,
    Mute,
    Touch,
}

struct Ctl {
    input: Input,
    rect: Rect,
    kind: Kind,
}

fn layout() -> Vec<Ctl> {
    let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));
    let c = |x: f32, y: f32, w: f32, h: f32| Rect::from_center_size(pos2(x, y), vec2(w, h));
    vec![
        Ctl { input: Input::L2, rect: r(82.0, 0.0, 182.0, 18.0), kind: Kind::Trigger },
        Ctl { input: Input::R2, rect: r(378.0, 0.0, 478.0, 18.0), kind: Kind::Trigger },
        Ctl { input: Input::L1, rect: r(72.0, 20.0, 190.0, 36.0), kind: Kind::Shoulder },
        Ctl { input: Input::R1, rect: r(370.0, 20.0, 488.0, 36.0), kind: Kind::Shoulder },
        Ctl { input: Input::TouchLeft, rect: r(194.0, 40.0, 280.0, 120.0), kind: Kind::Touch },
        Ctl { input: Input::TouchRight, rect: r(280.0, 40.0, 366.0, 120.0), kind: Kind::Touch },
        Ctl { input: Input::Create, rect: c(172.0, 60.0, 9.0, 22.0), kind: Kind::SmallButton },
        Ctl { input: Input::Options, rect: c(388.0, 60.0, 9.0, 22.0), kind: Kind::SmallButton },
        Ctl { input: Input::Up, rect: c(118.0, 76.0, 19.0, 22.0), kind: Kind::Dpad },
        Ctl { input: Input::Down, rect: c(118.0, 118.0, 19.0, 22.0), kind: Kind::Dpad },
        Ctl { input: Input::Left, rect: c(97.0, 97.0, 22.0, 19.0), kind: Kind::Dpad },
        Ctl { input: Input::Right, rect: c(139.0, 97.0, 22.0, 19.0), kind: Kind::Dpad },
        Ctl { input: Input::Triangle, rect: c(442.0, 74.0, 25.0, 25.0), kind: Kind::Face },
        Ctl { input: Input::Cross, rect: c(442.0, 120.0, 25.0, 25.0), kind: Kind::Face },
        Ctl { input: Input::Square, rect: c(419.0, 97.0, 25.0, 25.0), kind: Kind::Face },
        Ctl { input: Input::Circle, rect: c(465.0, 97.0, 25.0, 25.0), kind: Kind::Face },
        Ctl { input: Input::L3, rect: c(222.0, 168.0, 54.0, 54.0), kind: Kind::Stick },
        Ctl { input: Input::R3, rect: c(338.0, 168.0, 54.0, 54.0), kind: Kind::Stick },
        Ctl { input: Input::Ps, rect: c(280.0, 166.0, 20.0, 20.0), kind: Kind::Ps },
        Ctl { input: Input::Mute, rect: c(280.0, 190.0, 22.0, 7.0), kind: Kind::Mute },
    ]
}

/// Paint the controller into `area`. Returns a control the user clicked, if any.
pub fn show(ui: &mut Ui, area: Rect, snap: &PadSnapshot, pads: &[PadConfig], selected: usize, o: &ViewOpts) -> Option<Input> {
    let scale = (area.width() / W).min(area.height() / H);
    let origin = area.center() - vec2(W, H) * scale * 0.5;
    let at = |p: Pos2| origin + p.to_vec2() * scale;
    let tr = |r: Rect| Rect::from_min_max(at(r.min), at(r.max));
    let sc = |v: f32| v * scale;
    let cr = |v: f32| CornerRadius::same((v * scale).clamp(0.0, 255.0) as u8);
    let connected = snap.conn.is_some();
    let k = o.skin.colors();
    let p = ui.painter().clone();
    let ctls = layout();

    // --- shoulders & triggers (behind the shell)
    for ctl in ctls.iter().filter(|c| matches!(c.kind, Kind::Trigger | Kind::Shoulder)) {
        let rect = tr(ctl.rect);
        let base = if matches!(ctl.kind, Kind::Trigger) { mix(k.shell, Color32::BLACK, 0.12) } else { mix(k.shell, Color32::BLACK, 0.05) };
        let round = if matches!(ctl.kind, Kind::Trigger) { cr(9.0) } else { cr(8.0) };
        p.rect(rect, round, base, Stroke::new(1.0, k.shell_edge), StrokeKind::Inside);
    }

    // --- shell silhouette: outline pass, then fill pass
    let body = Rect::from_min_max(pos2(34.0, 30.0), pos2(526.0, 150.0));
    let grips = [(pos2(116.0, 176.0), 68.0), (pos2(444.0, 176.0), 68.0)];
    for (grow, color) in [(1.6, k.shell_edge), (0.0, k.shell)] {
        for (c, rad) in grips {
            p.circle_filled(at(c), sc(rad) + grow, color);
        }
        p.rect_filled(tr(body).expand(grow), cr(60.0), color);
    }
    // black centre section (sticks + PS button live here)
    let core = Rect::from_min_max(pos2(168.0, 108.0), pos2(392.0, 214.0));
    p.rect(tr(core), cr(48.0), k.core, Stroke::new(1.2, k.core_edge), StrokeKind::Inside);

    // --- light bar strips on both sides of the touchpad
    let light = if connected { rgb(snap.lightbar) } else { mix(k.shell, Color32::BLACK, 0.2) };
    for strip in [Rect::from_min_max(pos2(184.0, 50.0), pos2(189.0, 118.0)), Rect::from_min_max(pos2(371.0, 50.0), pos2(376.0, 118.0))] {
        if connected {
            p.rect_filled(tr(strip).expand(sc(3.0)), cr(5.0), alpha(light, 0.25));
        }
        p.rect_filled(tr(strip), cr(3.0), light);
    }

    // --- player indicator LEDs under the touchpad
    for i in 0..5 {
        let lit = o.player_leds & (1 << i) != 0 && connected;
        let pos = at(pos2(264.0 + i as f32 * 8.0, 128.0));
        p.circle_filled(pos, sc(1.8), if lit { Color32::WHITE } else { mix(k.core, Color32::WHITE, 0.15) });
    }

    let mut clicked = None;
    for ctl in &ctls {
        let rect = tr(ctl.rect);
        let down = snap.down(ctl.input);
        let bound: Vec<(usize, &PadConfig)> =
            pads.iter().enumerate().filter(|(_, c)| c.binding == Some(ctl.input)).collect();
        let resp = ui.interact(rect.expand(2.0), Id::unique(("ctl", ctl.input as u8)), Sense::click());
        let hovered = resp.hovered();
        let ring = Stroke::new(1.5, alpha(o.accent, 0.9));

        match ctl.kind {
            Kind::Trigger | Kind::Shoulder => {
                let round = if matches!(ctl.kind, Kind::Trigger) { cr(9.0) } else { cr(8.0) };
                let amount = match ctl.input {
                    Input::L2 => snap.l2,
                    Input::R2 => snap.r2,
                    _ if down => 1.0,
                    _ => 0.0,
                };
                if amount > 0.01 {
                    let mut f = rect;
                    f.set_width(rect.width() * amount);
                    p.rect_filled(f, round, alpha(o.accent, 0.35 + 0.65 * amount));
                }
                if hovered {
                    p.rect_stroke(rect, round, ring, StrokeKind::Inside);
                }
                p.text(rect.center(), Align2::CENTER_CENTER, ctl.input.short(), semibold(sc(10.0).max(8.0)), if down { ON_ACCENT } else { k.label });
            }
            Kind::Touch => {
                let left = ctl.input == Input::TouchLeft;
                let rr = (16.0 * scale) as u8;
                let round = if left { CornerRadius { nw: rr, sw: rr, ne: 0, se: 0 } } else { CornerRadius { nw: 0, sw: 0, ne: rr, se: rr } };
                let fill = if down { mix(k.touchpad, o.accent, 0.45) } else if hovered { mix(k.touchpad, o.accent, 0.15) } else { k.touchpad };
                p.rect_filled(rect, round, fill);
            }
            Kind::Dpad => {
                let fill = if down { o.accent } else if hovered { mix(k.button, o.accent, 0.3) } else { k.button };
                p.rect_filled(rect, cr(4.0), fill);
                draw_glyph(&p, rect.center(), rect.width() * 0.8, ctl.input, if down { ON_ACCENT } else { k.glyph });
            }
            Kind::Face => {
                let fill = if down { o.accent } else { k.button };
                p.circle_filled(rect.center(), rect.width() * 0.5, fill);
                p.circle_stroke(rect.center(), rect.width() * 0.5, Stroke::new(1.0, alpha(Color32::WHITE, 0.08)));
                if hovered {
                    p.circle_stroke(rect.center(), rect.width() * 0.5 + 1.5, ring);
                }
                let gc = if down { ON_ACCENT } else { glyph_color(ctl.input, k.glyph) };
                draw_glyph(&p, rect.center(), rect.width() * 0.95, ctl.input, gc);
            }
            Kind::Stick => {
                let (x, y) = if ctl.input == Input::L3 { (snap.lx, snap.ly) } else { (snap.rx, snap.ry) };
                let rad = rect.width() * 0.5;
                p.circle_filled(rect.center(), rad, mix(k.core, Color32::BLACK, 0.35));
                p.circle_stroke(rect.center(), rad, Stroke::new(1.0, k.core_edge));
                let thumb = rect.center() + vec2(x, y) * rad * 0.32;
                let cap = if down { mix(k.stick, o.accent, 0.6) } else { k.stick };
                p.circle_filled(thumb, rad * 0.72, cap);
                p.circle_stroke(thumb, rad * 0.72, Stroke::new(sc(2.0), alpha(Color32::WHITE, 0.07)));
                p.circle_stroke(thumb, rad * 0.5, Stroke::new(1.0, alpha(Color32::WHITE, 0.05)));
                if hovered {
                    p.circle_stroke(rect.center(), rad + 1.5, ring);
                }
            }
            Kind::SmallButton => {
                let fill = if down { o.accent } else if hovered { mix(k.button, o.accent, 0.4) } else { k.button };
                p.rect_filled(rect, cr(4.0), fill);
                for i in 0..3 {
                    let y = rect.top() + rect.height() * (0.3 + 0.2 * i as f32);
                    p.line_segment([pos2(rect.left() + sc(2.5), y), pos2(rect.right() - sc(2.5), y)], Stroke::new(1.0, alpha(k.glyph, 0.6)));
                }
            }
            Kind::Ps => {
                let fill = if down { o.accent } else { mix(k.core, Color32::WHITE, 0.08) };
                p.circle_filled(rect.center(), rect.width() * 0.5, fill);
                if hovered {
                    p.circle_stroke(rect.center(), rect.width() * 0.5 + 1.5, ring);
                }
                p.text(rect.center(), Align2::CENTER_CENTER, "PS", semibold(sc(7.5).max(7.0)), if down { ON_ACCENT } else { mix(k.core, Color32::WHITE, 0.55) });
            }
            Kind::Mute => {
                let fill = if o.mute_led && connected { Color32::from_rgb(255, 140, 50) } else if down { o.accent } else { mix(k.core, Color32::WHITE, 0.12) };
                p.rect_filled(rect, cr(3.0), fill);
                if hovered {
                    p.rect_stroke(rect.expand(1.5), cr(4.0), ring, StrokeKind::Outside);
                }
            }
        }

        // Pad-colour dots for every pad bound to this control.
        for (n, (_, cfg)) in bound.iter().take(3).enumerate() {
            let dot = rect.right_top() + vec2(-1.0 - n as f32 * 7.0, -1.0);
            p.circle_filled(dot, 3.2, rgb(cfg.color));
            p.circle_stroke(dot, 3.2, Stroke::new(1.0, bg()));
        }

        let mut tip = input_label(ctl.input).to_string();
        if bound.is_empty() {
            tip.push_str(l(" — свободна", " — not bound"));
        } else {
            for (i, _) in &bound {
                tip.push_str(&format!("\n→ {}", o.pad_names[*i]));
            }
        }
        if ctl.input == Input::Ps {
            tip.push('\n');
            tip.push_str(l("Занята Windows и Steam", "Kept by Windows and Steam"));
        } else if let Some(name) = o.pad_names.get(selected) {
            tip.push('\n');
            tip.push_str(&lf("Клик — назначить на «{}»", "Click to bind to “{}”", &[name]));
        }
        let resp = resp.on_hover_text(tip);
        if resp.clicked() && ctl.input.bindable() {
            clicked = Some(ctl.input);
        }
    }

    // Touchpad outline, centre seam and live touch points.
    let pad_rect = tr(Rect::from_min_max(pos2(194.0, 40.0), pos2(366.0, 120.0)));
    p.rect_stroke(pad_rect, cr(16.0), Stroke::new(1.0, k.touch_edge), StrokeKind::Inside);
    p.add(Shape::dashed_line(
        &[pos2(pad_rect.center().x, pad_rect.top() + sc(10.0)), pos2(pad_rect.center().x, pad_rect.bottom() - sc(10.0))],
        Stroke::new(1.0, alpha(k.touch_edge, 0.6)),
        sc(4.0),
        sc(4.0),
    ));
    for t in snap.touches.iter().flatten() {
        let pos = pad_rect.min + Vec2::new(t.0 * pad_rect.width(), t.1 * pad_rect.height());
        p.circle_filled(pos, sc(8.0), alpha(o.accent, 0.25));
        p.circle_filled(pos, sc(4.0), o.accent);
    }
    if !connected {
        p.rect_filled(area, CornerRadius::ZERO, alpha(bg(), 0.45));
    }
    clicked
}
