//! Small vector icons painted with the egui painter, so they follow the theme colours
//! and stay sharp at any interface scale.

use eframe::egui::{pos2, vec2, Align2, Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};
use std::f32::consts::TAU;

/// The icon set. A few are kept for screens that don't use them yet.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Play,
    Stop,
    Record,
    Metronome,
    Gear,
    Help,
    Folder,
    Wave,
    Plus,
    Minus,
    Close,
    Check,
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    Search,
    Volume,
    Target,
    Sparkle,
    Save,
    Trash,
    Loop,
    Hold,
    Shot,
    Note,
    Eye,
    Pencil,
    Tap,
    Gamepad,
    Palette,
    Bolt,
    Grid,
    User,
    Book,
}

fn arc(center: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = 14;
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            center + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

/// Paint `icon` inside the square `rect`.
pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let w = (s * 0.1).max(1.4);
    let st = Stroke::new(w, color);
    // Point in a -1..1 box around the centre.
    let at = |x: f32, y: f32| c + vec2(x, y) * s * 0.5;
    let line = |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|(x, y)| at(*x, *y)).collect(), st);
    let poly = |pts: &[(f32, f32)]| Shape::convex_polygon(pts.iter().map(|(x, y)| at(*x, *y)).collect(), color, Stroke::NONE);
    match icon {
        Icon::Play => {
            p.add(poly(&[(-0.45, -0.62), (0.62, 0.0), (-0.45, 0.62)]));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(s * 0.62, s * 0.62)), CornerRadius::same(2), color);
        }
        Icon::Record => {
            p.circle_filled(c, s * 0.36, color);
        }
        Icon::Metronome => {
            p.add(Shape::closed_line(vec![at(-0.22, -0.7), at(0.22, -0.7), at(0.55, 0.7), at(-0.55, 0.7)], st));
            p.add(line(&[(0.0, 0.45), (0.38, -0.5)]));
            p.circle_filled(at(0.22, -0.12), w * 1.1, color);
        }
        Icon::Gear => {
            p.circle_stroke(c, s * 0.26, st);
            for k in 0..8 {
                let a = TAU * k as f32 / 8.0;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * s * 0.32, c + d * s * 0.46], Stroke::new(w * 1.5, color));
            }
        }
        Icon::Help => {
            p.circle_stroke(c, s * 0.42, st);
            p.text(c + vec2(0.0, s * 0.01), Align2::CENTER_CENTER, "?", crate::theme::semibold(s * 0.58), color);
        }
        Icon::Folder => {
            p.add(Shape::closed_line(
                vec![at(-0.7, -0.5), at(-0.15, -0.5), at(0.05, -0.28), at(0.7, -0.28), at(0.7, 0.55), at(-0.7, 0.55)],
                st,
            ));
        }
        Icon::Wave => {
            for (k, h) in [0.35f32, 0.75, 0.5, 0.9, 0.4].iter().enumerate() {
                let x = -0.6 + k as f32 * 0.3;
                p.line_segment([at(x, -h * 0.7), at(x, h * 0.7)], Stroke::new(w * 1.2, color));
            }
        }
        Icon::Plus => {
            p.add(line(&[(-0.55, 0.0), (0.55, 0.0)]));
            p.add(line(&[(0.0, -0.55), (0.0, 0.55)]));
        }
        Icon::Minus => {
            p.add(line(&[(-0.55, 0.0), (0.55, 0.0)]));
        }
        Icon::Close => {
            p.add(line(&[(-0.45, -0.45), (0.45, 0.45)]));
            p.add(line(&[(-0.45, 0.45), (0.45, -0.45)]));
        }
        Icon::Check => {
            p.add(line(&[(-0.55, 0.05), (-0.15, 0.45), (0.6, -0.4)]));
        }
        Icon::ChevronLeft => {
            p.add(line(&[(0.25, -0.5), (-0.25, 0.0), (0.25, 0.5)]));
        }
        Icon::ChevronRight => {
            p.add(line(&[(-0.25, -0.5), (0.25, 0.0), (-0.25, 0.5)]));
        }
        Icon::ChevronDown => {
            p.add(line(&[(-0.5, -0.22), (0.0, 0.28), (0.5, -0.22)]));
        }
        Icon::Search => {
            p.circle_stroke(at(-0.12, -0.12), s * 0.27, st);
            p.add(line(&[(0.28, 0.28), (0.66, 0.66)]));
        }
        Icon::Volume => {
            p.add(poly(&[(-0.7, -0.2), (-0.38, -0.2), (-0.38, 0.2), (-0.7, 0.2)]));
            p.add(poly(&[(-0.38, -0.2), (0.0, -0.55), (0.0, 0.55), (-0.38, 0.2)]));
            p.add(Shape::line(arc(at(0.05, 0.0), s * 0.2, -0.9, 0.9), st));
            p.add(Shape::line(arc(at(0.05, 0.0), s * 0.36, -0.9, 0.9), st));
        }
        Icon::Target => {
            p.circle_stroke(c, s * 0.4, st);
            p.circle_filled(c, s * 0.13, color);
        }
        Icon::Sparkle => {
            let pts: Vec<Pos2> = (0..8)
                .map(|k| {
                    let a = TAU * k as f32 / 8.0 - TAU / 4.0;
                    let r = if k % 2 == 0 { 0.5 } else { 0.17 };
                    c + vec2(a.cos(), a.sin()) * s * r
                })
                .collect();
            // A star is concave, so paint it as four kites.
            for k in 0..4 {
                let i = k * 2;
                p.add(Shape::convex_polygon(vec![c, pts[(i + 7) % 8], pts[i], pts[(i + 1) % 8]], color, Stroke::NONE));
            }
        }
        Icon::Save => {
            p.rect_stroke(Rect::from_center_size(c, vec2(s * 0.74, s * 0.74)), CornerRadius::same(2), st, StrokeKind::Middle);
            p.rect_filled(Rect::from_min_max(at(-0.36, -0.74), at(0.3, -0.3)), CornerRadius::ZERO, color);
            p.rect_stroke(Rect::from_min_max(at(-0.4, 0.12), at(0.4, 0.74)), CornerRadius::ZERO, st, StrokeKind::Inside);
        }
        Icon::Trash => {
            p.add(line(&[(-0.6, -0.42), (0.6, -0.42)]));
            p.add(line(&[(-0.2, -0.42), (-0.2, -0.66), (0.2, -0.66), (0.2, -0.42)]));
            p.add(line(&[(-0.44, -0.42), (-0.36, 0.66), (0.36, 0.66), (0.44, -0.42)]));
        }
        Icon::Loop => {
            p.add(Shape::line(arc(c, s * 0.36, 0.5, TAU - 0.9), st));
            let tip = c + vec2((TAU - 0.9).cos(), (TAU - 0.9).sin()) * s * 0.36;
            p.add(Shape::convex_polygon(vec![tip + vec2(-s * 0.2, -s * 0.02), tip + vec2(s * 0.12, -s * 0.16), tip + vec2(s * 0.06, s * 0.2)], color, Stroke::NONE));
        }
        Icon::Hold => {
            p.add(line(&[(-0.6, -0.35), (-0.6, 0.35)]));
            p.add(line(&[(0.6, -0.35), (0.6, 0.35)]));
            p.line_segment([at(-0.6, 0.0), at(0.6, 0.0)], Stroke::new(w * 1.6, color));
        }
        Icon::Shot => {
            p.add(line(&[(-0.6, 0.0), (0.25, 0.0)]));
            p.add(poly(&[(0.15, -0.3), (0.65, 0.0), (0.15, 0.3)]));
        }
        Icon::Note => {
            p.circle_filled(at(-0.25, 0.42), s * 0.17, color);
            p.add(line(&[(-0.1, 0.42), (-0.1, -0.6), (0.5, -0.4)]));
        }
        Icon::Eye => {
            let top: Vec<Pos2> = (0..=12).map(|i| {
                let x = -0.7 + 1.4 * i as f32 / 12.0;
                at(x, -0.42 * (1.0 - (x / 0.7).powi(2)))
            }).collect();
            let bot: Vec<Pos2> = top.iter().map(|q| pos2(q.x, 2.0 * c.y - q.y)).collect();
            p.add(Shape::line(top, st));
            p.add(Shape::line(bot, st));
            p.circle_filled(c, s * 0.14, color);
        }
        Icon::Pencil => {
            p.add(line(&[(-0.6, 0.6), (-0.5, 0.25), (0.35, -0.6), (0.6, -0.35), (-0.25, 0.5), (-0.6, 0.6)]));
        }
        Icon::Tap => {
            p.circle_filled(c, s * 0.14, color);
            p.circle_stroke(c, s * 0.3, Stroke::new(w * 0.8, color));
            p.circle_stroke(c, s * 0.45, Stroke::new(w * 0.6, color.gamma_multiply(0.6)));
        }
        Icon::Gamepad => {
            p.rect_stroke(Rect::from_center_size(c, vec2(s * 0.92, s * 0.52)), CornerRadius::same((s * 0.2) as u8), st, StrokeKind::Middle);
            p.add(line(&[(-0.52, 0.0), (-0.22, 0.0)]));
            p.add(line(&[(-0.37, -0.15), (-0.37, 0.15)]));
            p.circle_filled(at(0.28, -0.08), w * 0.9, color);
            p.circle_filled(at(0.5, 0.1), w * 0.9, color);
        }
        Icon::Palette => {
            p.circle_stroke(c, s * 0.42, st);
            for (x, y) in [(-0.3, -0.25), (0.15, -0.38), (0.4, 0.05)] {
                p.circle_filled(at(x, y), w * 1.1, color);
            }
        }
        Icon::Bolt => {
            p.add(poly(&[(0.15, -0.75), (-0.5, 0.12), (-0.02, 0.12)]));
            p.add(poly(&[(-0.15, 0.75), (0.5, -0.12), (0.02, -0.12)]));
        }
        Icon::Grid => {
            for (x, y) in [(-0.38, -0.38), (0.38, -0.38), (-0.38, 0.38), (0.38, 0.38)] {
                p.rect_filled(Rect::from_center_size(at(x, y), vec2(s * 0.3, s * 0.3)), CornerRadius::same(2), color);
            }
        }
        Icon::User => {
            p.circle_stroke(at(0.0, -0.28), s * 0.2, st);
            p.add(Shape::line(arc(at(0.0, 0.72), s * 0.38, -TAU * 0.45, -TAU * 0.05), st));
        }
        Icon::Book => {
            p.add(line(&[(0.0, -0.5), (0.0, 0.6)]));
            p.add(line(&[(0.0, -0.5), (-0.3, -0.62), (-0.7, -0.55), (-0.7, 0.5), (-0.3, 0.45), (0.0, 0.6)]));
            p.add(line(&[(0.0, -0.5), (0.3, -0.62), (0.7, -0.55), (0.7, 0.5), (0.3, 0.45), (0.0, 0.6)]));
        }
    }
}
