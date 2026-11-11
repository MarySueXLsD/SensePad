//! The controller bar: which pad every button plays, the live controller, and the DJ effects.

use super::widgets::*;
use crate::app::{func_name, fx_name, Action, SensePadApp};
use crate::controller_art::{self, ArtOpts};
use crate::controller_skin::{self, SkinOpts};
use crate::controller_view::{self, ViewOpts};
use crate::dualsense::{Link, PadSnapshot};
use crate::i18n::{input_label, l, lf};
use crate::icons::Icon;
use crate::input::Input;
use crate::kit::{ControllerStyle, FxKind, StickMode, PAD_COUNT};
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Id, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder};

const LEFT: [Input; 10] = [
    Input::L2, Input::L1, Input::Up, Input::Down, Input::Left, Input::Right, Input::L3, Input::TouchLeft, Input::Create, Input::Mute,
];
const RIGHT: [Input; 9] =
    [Input::R2, Input::R1, Input::Triangle, Input::Square, Input::Circle, Input::Cross, Input::R3, Input::TouchRight, Input::Options];

/// What a row of the mapping columns shows.
enum Mapped<'a> {
    Pad(&'a str, Color32),
    Func(&'static str),
    Nothing,
}

fn tips() -> [&'static str; 8] {
    [
        l("Нажмите ● и сыграйте бит — он зациклится. Наслаивайте поверх сколько угодно.", "Press ● and play a beat — it loops. Layer as much as you like on top."),
        l("Левый стик влево-вправо — фильтр, как на диджейском пульте.", "Left stick left and right is a filter, like on a DJ mixer."),
        l("Правый стик вверх — статтер: дробит последний звук в такт.", "Right stick up is a stutter: it chops the last sound in time."),
        l("Кликните по кнопке на картинке геймпада — и она сыграет выбранный пэд.", "Click a button on the controller picture and it plays the selected pad."),
        l("Брейки и лупы сами подстраиваются под темп. Меняйте BPM смело.", "Breaks and loops follow the tempo. Change the BPM freely."),
        l("Правый клик по пэду — быстрое меню: звук, кнопка, очистить.", "Right-click a pad for a quick menu: sound, button, clear."),
        l("Потяните край боковой панели или этой полосы, чтобы изменить размер.", "Drag the edge of the sidebar or of this bar to resize it."),
        l("F1 — инструкция. Там же можно пройти знакомство ещё раз.", "F1 opens the guide. You can replay the welcome tour from there."),
    ]
}

impl SensePadApp {
    pub(crate) fn controller_panel(&mut self, ui: &mut Ui, snap: &PadSnapshot) {
        let accent = self.accent();
        let full = ui.available_rect_before_wrap();
        ui.allocate_rect(full, Sense::hover());
        let gap = sp(16.0);
        let fx_w = if self.settings.show_fx && full.width() > 900.0 { 310.0 } else { 0.0 };
        let cols = self.settings.show_mapping && full.width() - fx_w > 620.0;
        let col_w = if cols { ((full.width() - fx_w - 3.0 * gap) * 0.23).clamp(150.0, 240.0) } else { 0.0 };
        let left_col = Rect::from_min_size(full.min, vec2(col_w, full.height()));
        let fx_rect = Rect::from_min_max(pos2(full.max.x - fx_w, full.min.y), full.max);
        let right_x = full.max.x - fx_w - if fx_w > 0.0 { gap } else { 0.0 } - col_w;
        let right_col = Rect::from_min_size(pos2(right_x, full.min.y), vec2(col_w, full.height()));
        let pad_gap = if cols { gap } else { 0.0 };
        let center = Rect::from_min_max(pos2(left_col.max.x + pad_gap, full.min.y), pos2(right_col.min.x - pad_gap, full.max.y));
        let status_h = 26.0;
        let draw = Rect::from_min_max(center.min, pos2(center.max.x, center.max.y - status_h));

        let pad_names: Vec<String> = (0..PAD_COUNT).map(|i| self.pad_name(i)).collect();
        let clicked = self.draw_controller(ui, draw, snap, self.selected, &pad_names);
        if let Some(input) = clicked {
            if self.learning.is_some() {
                self.cancel_learning();
            }
            self.actions.push(Action::Bind(self.selected, Some(input)));
        }

        // Status line under the controller: connection, then a rotating tip.
        let mut status = match &snap.conn {
            Some(c) => {
                let mut s = format!("{} · {}", c.model, if c.link == Link::Usb { "USB" } else { "Bluetooth" });
                if let Some(b) = snap.battery {
                    s.push_str(&format!(" · {b}%{}", if snap.charging { " ⚡" } else { "" }));
                }
                s
            }
            None => l("Геймпад не подключён — играйте с клавиатуры: 1-4, Q-R, A-F, Z-V", "No controller — play from the keyboard: 1-4, Q-R, A-F, Z-V").to_string(),
        };
        if self.settings.show_hints && snap.conn.is_some() {
            let all = tips();
            status = all[(self.started.elapsed().as_secs() / 12) as usize % all.len()].to_string();
        }
        let sp_ = pos2(center.center().x, center.max.y - status_h * 0.5 + 2.0);
        let p = ui.painter();
        let galley = p.layout_no_wrap(status.clone(), FontId::proportional(12.5), dim());
        if galley.size().x <= center.width() - 30.0 {
            let gx = sp_.x - galley.size().x * 0.5;
            p.circle_filled(pos2(gx - 10.0, sp_.y), 4.0, if snap.conn.is_some() { OK } else { WARN });
            p.galley(pos2(gx, sp_.y - galley.size().y * 0.5), galley, dim());
        } else {
            p.circle_filled(pos2(center.left() + 8.0, sp_.y), 4.0, if snap.conn.is_some() { OK } else { WARN });
            clipped(p, pos2(center.left() + 18.0, sp_.y), Align2::LEFT_CENTER, &status, FontId::proportional(12.5), dim(), center.width() - 24.0);
        }

        if cols {
            let shift = ui.input(|i| i.modifiers.shift);
            let mut map_actions = Vec::new();
            let rows = LEFT.len().max(RIGHT.len());
            for (col, inputs) in [(left_col, &LEFT[..]), (right_col, &RIGHT[..])] {
                ui.painter().rect(col, cr(10.0), card(), Stroke::new(1.0, border()), StrokeKind::Inside);
                let inner = col.shrink(8.0);
                let gap_y = 4.0;
                let row_h = ((inner.height() - gap_y * (rows - 1) as f32) / rows as f32).clamp(15.0, 30.0);
                for (k, input) in inputs.iter().copied().enumerate() {
                    let row = Rect::from_min_size(inner.min + vec2(0.0, k as f32 * (row_h + gap_y)), vec2(inner.width(), row_h));
                    if row.bottom() > inner.bottom() + 1.0 {
                        break;
                    }
                    let bound = self.kit.pads.iter().position(|p| p.binding == Some(input));
                    let func = self.settings.functions.of(input);
                    let info = match (bound, func) {
                        (Some(i), _) => Mapped::Pad(pad_names[i].as_str(), rgb(self.kit.pads[i].color)),
                        (None, Some(f)) => Mapped::Func(func_name(f)),
                        _ => Mapped::Nothing,
                    };
                    if map_row(ui, row, input, info, snap.down(input), bound == Some(self.selected), accent) {
                        match (bound, func) {
                            (Some(i), _) if !shift => map_actions.push(Action::SelectPad(i)),
                            (None, Some(_)) if !shift => map_actions.push(Action::OpenSettings(3)),
                            _ => map_actions.push(Action::Bind(self.selected, Some(input))),
                        }
                    }
                }
            }
            self.actions.extend(map_actions);
        }
        if fx_w > 0.0 {
            self.fx_card(ui, fx_rect, snap);
        }
    }

    /// The live controller in the style chosen in Settings. The optional picture styles
    /// fall back to SensePad's own drawing when their artwork isn't installed.
    pub(crate) fn draw_controller(&self, ui: &mut Ui, area: Rect, snap: &PadSnapshot, selected: usize, pad_names: &[String]) -> Option<Input> {
        let accent = self.accent();
        let mute_led = self.learning.is_some() || self.audio.recording();
        match (self.settings.controller_style, &self.skin_tex, &self.art) {
            (ControllerStyle::Colour, Some(tex), _) => {
                let opts = SkinOpts { tints: self.settings.skin.tints(), accent, mute_led, pad_names };
                controller_skin::show(ui, area, tex, snap, &self.kit.pads, selected, &opts)
            }
            (ControllerStyle::LineArt, _, Some(tex)) => {
                let opts = ArtOpts { accent, line: text(), mute_led, pad_names };
                controller_art::show(ui, area, tex, snap, &self.kit.pads, selected, &opts)
            }
            _ => {
                let opts = ViewOpts { skin: self.settings.skin, accent, player_leds: self.prof().player_leds, mute_led, pad_names };
                controller_view::show(ui, area, snap, &self.kit.pads, selected, &opts)
            }
        }
    }

    /// Live meters for the stick effects. In Knob mode the bars can be dragged.
    fn fx_card(&mut self, ui: &mut Ui, rect: Rect, snap: &PadSnapshot) {
        let accent = self.accent();
        let sticks = self.settings.sticks;
        let fx_vals: Vec<f32> = sticks.axes().iter().map(|k| self.audio.fx.get(*k)).collect();
        let touching = snap.touches[0].is_some() && sticks.touch_fx;
        let mut knob_set: Option<(FxKind, f32)> = None;
        let mut open_sticks = false;
        let mut reset = false;
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(rect.height() - 2.0 * sp(14.0) - 2.0);
                ui.spacing_mut().item_spacing.y = sp(7.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(l("Эффекты на стиках", "Stick effects")).font(display(15.0)).color(dim()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_btn(ui, Icon::Gear, 24.0, l("Настроить стики", "Set up the sticks")).clicked() {
                            open_sticks = true;
                        }
                        if sticks.mode == StickMode::Knob && icon_btn(ui, Icon::Close, 24.0, l("Сбросить эффекты", "Reset the effects")).clicked() {
                            reset = true;
                        }
                    });
                });
                let axis_names = [l("Левый ↔", "Left ↔"), l("Левый ↕", "Left ↕"), l("Правый ↔", "Right ↔"), l("Правый ↕", "Right ↕")];
                for (n, kind) in sticks.axes().into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (lr, _) = ui.allocate_exact_size(vec2(74.0, 16.0), Sense::hover());
                        ui.painter().text(lr.left_center(), Align2::LEFT_CENTER, axis_names[n], FontId::proportional(11.5), dim());
                        let (nr, _) = ui.allocate_exact_size(vec2(88.0, 16.0), Sense::hover());
                        ui.painter().text(nr.left_center(), Align2::LEFT_CENTER, fx_name(kind), semibold(12.0), text());
                        let (br, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::click_and_drag());
                        let p = ui.painter();
                        p.rect_filled(br, egui::CornerRadius::same(4), card_hi());
                        let v = fx_vals[n];
                        if kind != FxKind::None {
                            let fillr = if kind.bipolar() {
                                let cx = br.center().x;
                                let x = cx + v.clamp(-1.0, 1.0) * br.width() * 0.5;
                                Rect::from_min_max(pos2(cx.min(x), br.top()), pos2(cx.max(x), br.bottom()))
                            } else {
                                Rect::from_min_max(br.min, pos2(br.left() + br.width() * v.clamp(0.0, 1.0), br.bottom()))
                            };
                            p.rect_filled(fillr, egui::CornerRadius::same(4), accent);
                            if kind.bipolar() {
                                p.line_segment([pos2(br.center().x, br.top() - 2.0), pos2(br.center().x, br.bottom() + 2.0)], Stroke::new(1.0, faint()));
                            }
                        }
                        if sticks.mode == StickMode::Knob && kind != FxKind::None && (resp.dragged() || resp.clicked()) {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                let x = ((pos.x - br.left()) / br.width()).clamp(0.0, 1.0);
                                knob_set = Some((kind, if kind.bipolar() { x * 2.0 - 1.0 } else { x }));
                            }
                        }
                    });
                }
                if sticks.touch_fx {
                    let line = lf("Тачпад: ↔ {} · ↕ {}", "Touchpad: ↔ {} · ↕ {}", &[&fx_name(sticks.touch_x), &fx_name(sticks.touch_y)]);
                    ui.label(RichText::new(line).size(11.5).color(if touching { accent } else { dim() }));
                }
                let note = match sticks.mode {
                    StickMode::Spring => l("Отпустили стик — эффект ушёл.", "Let go of the stick and the effect is gone."),
                    StickMode::Knob => l("Стики крутят ручки. Полосы можно двигать мышью.", "Sticks turn knobs. Drag the bars with the mouse."),
                };
                hint(ui, note);
            });
        });
        if let Some((k, v)) = knob_set {
            self.shared.set_knob(k, v);
        }
        if reset {
            self.actions.push(Action::ResetFx);
        }
        if open_sticks {
            self.actions.push(Action::OpenSettings(4));
        }
    }
}

/// One row of the mapping columns: a control and what it does. Returns true when clicked.
fn map_row(ui: &mut Ui, row: Rect, input: Input, info: Mapped, down: bool, selected: bool, accent: Color32) -> bool {
    let resp = ui.interact(row, Id::unique(("map", input as u8)), Sense::click());
    let p = ui.painter();
    let label_w = row.height().max(24.0) + 8.0;
    let lr = Rect::from_min_size(row.min, vec2(label_w, row.height()));
    let br = Rect::from_min_max(pos2(lr.max.x + 4.0, row.min.y), row.max);
    let shaped = matches!(
        input,
        Input::Cross | Input::Circle | Input::Square | Input::Triangle | Input::Up | Input::Down | Input::Left | Input::Right | Input::TouchLeft | Input::TouchRight
    );
    if shaped {
        draw_glyph(p, lr.center(), row.height() * 0.95, input, if down { accent } else { glyph_color(input, text()) });
    } else {
        p.text(lr.center(), Align2::CENTER_CENTER, input.short(), semibold(12.0), if down { accent } else { text() });
    }
    let fill_c = if down { alpha(accent, 0.35) } else if resp.hovered() { card_hi() } else { mix(bg(), card(), 0.3) };
    let stroke = if selected { Stroke::new(1.5, accent) } else { Stroke::new(1.0, border()) };
    p.rect(br, cr(6.0), fill_c, stroke, StrokeKind::Inside);
    let tx = br.left() + 20.0;
    match info {
        Mapped::Pad(name, color) => {
            p.circle_filled(pos2(br.left() + 10.0, br.center().y), 3.5, color);
            clipped(p, pos2(tx, br.center().y), Align2::LEFT_CENTER, name, FontId::proportional(13.0), text(), br.width() - 26.0);
        }
        Mapped::Func(name) => {
            p.rect_filled(Rect::from_center_size(pos2(br.left() + 10.0, br.center().y), vec2(6.0, 6.0)), egui::CornerRadius::same(1), dim());
            clipped(p, pos2(tx, br.center().y), Align2::LEFT_CENTER, name, FontId::proportional(12.5), dim(), br.width() - 26.0);
        }
        Mapped::Nothing => {
            p.text(pos2(br.left() + 10.0, br.center().y), Align2::LEFT_CENTER, "—", FontId::proportional(13.0), faint());
        }
    }
    let tip = format!(
        "{}\n{}",
        input_label(input),
        l("Клик — выбрать её пэд. Shift+клик — отдать кнопку выбранному пэду.", "Click to select its pad. Shift+click gives the button to the selected pad.")
    );
    resp.on_hover_text(tip).clicked()
}
