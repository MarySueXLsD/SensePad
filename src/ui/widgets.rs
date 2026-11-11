//! Shared building blocks for every screen.

use crate::icons::{self, Icon};
use crate::theme::*;
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontId, Frame, Layout, Margin, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui,
};

pub fn card_frame() -> Frame {
    Frame::new().fill(card()).corner_radius(cr(14.0)).inner_margin(Margin::same(spi(14.0))).stroke(Stroke::new(1.0, border()))
}

/// A padded card with a title, used for groups of settings.
pub fn section<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame()
        .inner_margin(Margin::symmetric(spi(16.0), spi(14.0)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = sp(8.0);
            if !title.is_empty() {
                ui.label(RichText::new(title).font(display(15.0)).color(dim()));
            }
            add(ui)
        })
        .inner
}

pub fn field_label(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).size(13.0).color(dim()));
}

/// Small explanatory text under a control.
pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(faint()));
}

pub fn btn(ui: &mut Ui, label: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(label).color(text())).fill(card_hi()).corner_radius(cr(9.0)))
}

pub fn btn_primary(ui: &mut Ui, label: &str, accent: Color32) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(on_color(accent)).font(semibold(14.0)))
            .fill(accent)
            .corner_radius(cr(9.0)),
    )
}

pub fn btn_danger(ui: &mut Ui, label: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(label).color(DANGER)).fill(card_hi()).corner_radius(cr(9.0)))
}

/// Square button with only an icon.
pub fn icon_btn(ui: &mut Ui, icon: Icon, size: f32, tip: &str) -> Response {
    icon_btn_colored(ui, icon, size, tip, text(), None)
}

pub fn icon_btn_colored(ui: &mut Ui, icon: Icon, size: f32, tip: &str, color: Color32, fill: Option<Color32>) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let p = ui.painter();
    let bg = match fill {
        Some(f) if resp.hovered() => mix(f, Color32::WHITE, 0.12),
        Some(f) => f,
        None if resp.hovered() => card_hi(),
        None => Color32::TRANSPARENT,
    };
    p.rect_filled(r, cr(9.0), bg);
    let pad = size * 0.27;
    icons::paint(p, r.shrink(pad), icon, color);
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

/// Button with an icon and a label. `fill` = None draws a quiet button.
pub fn icon_label_btn(ui: &mut Ui, icon: Icon, label: &str, fill: Option<Color32>) -> Response {
    let h = sp(34.0).max(28.0);
    let fg = fill.map(on_color).unwrap_or_else(text);
    let font = if fill.is_some() { semibold(14.0) } else { FontId::proportional(14.0) };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, fg);
    let w = galley.size().x + 16.0 + 7.0 + sp(26.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let p = ui.painter();
    let base = fill.unwrap_or_else(card_hi);
    let bg = if resp.is_pointer_button_down_on() {
        mix(base, Color32::BLACK, 0.15)
    } else if resp.hovered() {
        mix(base, Color32::WHITE, 0.1)
    } else {
        base
    };
    p.rect_filled(r, cr(9.0), bg);
    let x = r.left() + sp(13.0);
    icons::paint(p, Rect::from_center_size(pos2(x + 8.0, r.center().y), vec2(16.0, 16.0)), icon, fg);
    p.galley(pos2(x + 16.0 + 7.0, r.center().y - galley.size().y * 0.5), galley, fg);
    resp
}

/// iOS-style toggle switch.
pub fn toggle(ui: &mut Ui, on: &mut bool, accent: Color32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let p = ui.painter();
    p.rect_filled(rect, egui::CornerRadius::same(11), mix(mix(card_hi(), border(), 0.6), accent, t));
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    p.circle_filled(pos2(x, rect.center().y), 8.0, if *on { on_color(accent) } else { dim() });
    resp
}

/// Label (and optional explanation) on the left, a switch on the right.
pub fn toggle_row(ui: &mut Ui, label: &str, note: &str, on: &mut bool, accent: Color32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.set_max_width((ui.available_width() - 56.0).max(80.0));
            ui.label(RichText::new(label).color(text()));
            if !note.is_empty() {
                hint(ui, note);
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed = toggle(ui, on, accent).changed();
        });
    });
    changed
}

pub fn setting_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(text()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| add(ui));
    });
}

/// One choice out of a few, shown as a row of equal buttons.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)], accent: Color32) -> bool {
    let before = options.iter().position(|(v, _)| v == value);
    Frame::new().fill(mix(bg(), Color32::BLACK, if palette().dark { 0.25 } else { 0.06 })).corner_radius(cr(11.0)).inner_margin(Margin::same(3)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let w = (ui.available_width() - 3.0 * (options.len() as f32 - 1.0)) / options.len() as f32;
            for (v, label) in options {
                let sel = *value == *v;
                let b = egui::Button::new(RichText::new(*label).font(semibold(13.0)).color(if sel { on_color(accent) } else { dim() }))
                    .fill(if sel { accent } else { Color32::TRANSPARENT })
                    .corner_radius(cr(8.0))
                    .min_size(vec2(w.max(10.0), sp(28.0).max(24.0)));
                if ui.add(b).clicked() {
                    *value = *v;
                }
            }
        });
    });
    before != options.iter().position(|(v, _)| v == value)
}

/// Small selectable tag.
pub fn chip(ui: &mut Ui, label: &str, selected: bool, accent: Color32) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(12.5).color(if selected { on_color(accent) } else { text() }))
            .fill(if selected { accent } else { card_hi() })
            .corner_radius(cr(12.0))
            .min_size(vec2(0.0, 24.0)),
    )
}

pub fn swatch(ui: &mut Ui, color: Color32, selected: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let p = ui.painter();
    p.circle_filled(r.center(), 10.0, color);
    if selected {
        p.circle_stroke(r.center(), 12.5, Stroke::new(2.0, text()));
    } else if resp.hovered() {
        p.circle_stroke(r.center(), 12.5, Stroke::new(1.0, dim()));
    }
    resp
}

/// Preset colours plus a picker for any colour. Returns true when the value changed.
pub fn color_row(ui: &mut Ui, presets: &[[u8; 3]], current: &mut [u8; 3], pick_tip: &str) -> bool {
    let before = *current;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for c in presets {
            if swatch(ui, rgb(*c), *current == *c).clicked() {
                *current = *c;
            }
        }
        egui::color_picker::color_edit_button_srgb(ui, current).on_hover_text(pick_tip);
    });
    before != *current
}

/// Rounded status tag with a coloured dot.
pub fn pill(ui: &mut Ui, dot: Color32, label: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), text());
    let size = vec2(galley.size().x + 34.0, 30.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect(r, egui::CornerRadius::same(15), card(), Stroke::new(1.0, border()), StrokeKind::Inside);
    p.circle_filled(pos2(r.left() + 15.0, r.center().y), 4.0, dot);
    p.galley(pos2(r.left() + 25.0, r.center().y - galley.size().y / 2.0), galley, text());
    resp
}

/// Label on the left, value on the right (double-click it to reset), full-width slider below.
pub fn value_slider(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
    step: Option<f64>,
    fmt: impl Fn(f32) -> String,
) -> bool {
    let before = *value;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(text()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let shown = egui::Label::new(RichText::new(fmt(*value)).font(semibold(13.0)).color(text())).sense(Sense::click());
            if ui.add(shown).double_clicked() {
                *value = default;
            }
        });
    });
    ui.spacing_mut().slider_width = ui.available_width();
    let mut slider = egui::Slider::new(value, range).show_value(false);
    if let Some(s) = step {
        slider = slider.step_by(s);
    }
    ui.add(slider);
    before != *value
}

/// A little keycap, for shortcuts in the help.
pub fn keycap(ui: &mut Ui, label: &str) {
    let galley = ui.painter().layout_no_wrap(label.to_string(), semibold(12.0), text());
    let (r, _) = ui.allocate_exact_size(vec2(galley.size().x + 14.0, 22.0), Sense::hover());
    let p = ui.painter();
    p.rect(r, cr(6.0), card_hi(), Stroke::new(1.0, border()), StrokeKind::Inside);
    p.galley(pos2(r.center().x - galley.size().x * 0.5, r.center().y - galley.size().y * 0.5), galley, text());
}

/// Text that fits `max_width`, cut with an ellipsis if needed.
pub fn clipped(p: &egui::Painter, pos: egui::Pos2, anchor: Align2, s: &str, font: FontId, color: Color32, max_width: f32) {
    let mut shown = s.to_string();
    let mut galley = p.layout_no_wrap(shown.clone(), font.clone(), color);
    if galley.size().x > max_width {
        let mut chars: Vec<char> = s.chars().collect();
        while chars.len() > 1 {
            chars.pop();
            shown = chars.iter().collect::<String>() + "…";
            galley = p.layout_no_wrap(shown.clone(), font.clone(), color);
            if galley.size().x <= max_width {
                break;
            }
        }
    }
    let rect = anchor.anchor_size(pos, galley.size());
    p.galley(rect.min, galley, color);
}
