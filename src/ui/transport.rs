//! The looper: play, record, tempo, metronome, length, swing — and the step strip.

use super::widgets::*;
use crate::app::{Action, SensePadApp};
use crate::i18n::{l, lf};
use crate::icons::Icon;
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Id, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui};

impl SensePadApp {
    pub(crate) fn transport_bar(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        let playing = self.audio.playing();
        let recording = self.audio.recording();
        let step = self.audio.step();
        let wide = ui.available_width() > 1240.0;
        let mut actions: Vec<Action> = Vec::new();
        let mut bpm = self.kit.bpm;
        let mut swing = self.kit.swing;
        let mut bars = self.kit.pattern.bars;
        let before = self.settings.clone();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = sp(8.0);
            let (icon, fill, fg) = if playing { (Icon::Stop, accent, on_color(accent)) } else { (Icon::Play, card_hi(), text()) };
            if icon_btn_colored(ui, icon, 36.0, l("Старт / стоп лупа · Пробел", "Play / stop the loop · Space"), fg, Some(fill)).clicked() {
                actions.push(Action::Play);
            }
            let (fill, fg) = if recording { (REC, Color32::WHITE) } else { (card_hi(), REC) };
            let tip = l(
                "Запись · Enter\nИграйте на пэдах — удары встанут в сетку и будут повторяться.",
                "Record · Enter\nPlay the pads — your hits snap to the grid and repeat.",
            );
            if icon_btn_colored(ui, Icon::Record, 36.0, tip, fg, Some(fill)).clicked() {
                actions.push(Action::Record);
            }

            // Four lamps: which beat of the bar is playing.
            let (lr, _) = ui.allocate_exact_size(vec2(50.0, 36.0), Sense::hover());
            for b in 0..4 {
                let on = step.is_some_and(|s| (s % 16) / 4 == b);
                let c = pos2(lr.left() + 9.0 + b as f32 * 11.0, lr.center().y);
                let col = if on { if b == 0 { accent } else { text() } } else { border() };
                ui.painter().circle_filled(c, if on { 4.5 } else { 3.0 }, col);
            }

            if icon_btn(ui, Icon::Minus, 26.0, "").clicked() {
                bpm -= 1.0;
            }
            ui.scope(|ui| {
                ui.style_mut().override_font_id = Some(display(24.0));
                ui.spacing_mut().interact_size.x = 56.0;
                ui.add(egui::DragValue::new(&mut bpm).range(60.0..=220.0).speed(0.25).fixed_decimals(0))
                    .on_hover_text(l("Темп. Потяните вверх-вниз или кликните и введите число.", "Tempo. Drag up or down, or click and type."));
            });
            if icon_btn(ui, Icon::Plus, 26.0, "").clicked() {
                bpm += 1.0;
            }
            ui.label(RichText::new("BPM").font(display(13.0)).color(faint()));
            if btn(ui, l("Тап", "Tap")).on_hover_text(l("Нажимайте в ритм — темп подстроится", "Tap in time to set the tempo")).clicked() {
                actions.push(Action::Tap);
            }
            let m = self.settings.metronome;
            let tip = l("Метроном", "Metronome");
            if icon_btn_colored(ui, Icon::Metronome, 34.0, tip, if m { on_color(accent) } else { dim() }, m.then_some(accent)).clicked() {
                self.settings.metronome = !m;
            }

            ui.add_space(sp(6.0));
            ui.label(RichText::new(l("Такты", "Bars")).size(12.5).color(dim()));
            ui.allocate_ui(vec2(112.0, 34.0), |ui| {
                segmented(ui, &mut bars, &[(1, "1"), (2, "2"), (4, "4")], accent);
            });

            if wide {
                ui.add_space(sp(6.0));
                ui.label(RichText::new(l("Свинг", "Swing")).size(12.5).color(dim()));
                ui.spacing_mut().slider_width = 76.0;
                ui.add(egui::Slider::new(&mut swing, 0.0..=0.6).show_value(false))
                    .on_hover_text(lf("Свинг: {}%. Оттягивает каждую вторую шестнадцатую.", "Swing: {}%. Delays every second 16th note.", &[&((swing * 100.0).round() as i32)]));
                ui.add_space(sp(6.0));
                ui.label(RichText::new(l("Лупы стартуют", "Loops start")).size(12.5).color(dim()));
                ui.allocate_ui(vec2(250.0, 34.0), |ui| {
                    segmented(
                        ui,
                        &mut self.settings.launch_quant,
                        &[(0, l("сразу", "at once")), (4, l("в долю", "on the beat")), (16, l("в такт", "on the bar"))],
                        accent,
                    );
                })
                .response
                .on_hover_text(l(
                    "Пока луп играет, пэды в режиме «Петля» могут дождаться следующей доли или такта и вступить ровно.",
                    "While the loop plays, Loop pads can wait for the next beat or bar and come in on time.",
                ));
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = sp(8.0);
                if icon_btn(ui, Icon::Trash, 34.0, l("Очистить луп", "Clear the loop")).clicked() {
                    actions.push(Action::ClearPattern);
                }
                if icon_label_btn(ui, Icon::Sparkle, l("Демо", "Demo"), None)
                    .on_hover_text(l("Сыграть демо-бит этого кита", "Play this kit's demo beat"))
                    .clicked()
                {
                    actions.push(Action::Demo);
                }
                let notes = self.kit.pattern.notes.len();
                if notes > 0 {
                    ui.label(RichText::new(lf("нот в лупе: {}", "{} notes in the loop", &[&notes])).size(12.5).color(faint()));
                }
            });
        });

        if (bpm - self.kit.bpm).abs() > 0.01 {
            self.set_bpm(bpm);
        }
        if (swing - self.kit.swing).abs() > 0.001 {
            self.set_swing(swing);
        }
        if bars != self.kit.pattern.bars {
            actions.push(Action::SetBars(bars));
        }
        if self.settings != before {
            self.on_settings_changed(&before);
        }
        self.actions.extend(actions);
    }

    /// One row of steps for the selected pad. Click a cell to add or remove a hit.
    pub(crate) fn steps_strip(&mut self, ui: &mut Ui, rect: Rect) {
        let pad = self.selected;
        let color = rgb(self.kit.pads[pad].color);
        let total = self.kit.pattern.steps();
        let now = self.audio.step();
        let p = ui.painter().clone();
        p.rect(rect, cr(12.0), card(), Stroke::new(1.0, border()), StrokeKind::Inside);
        let inner = rect.shrink2(vec2(sp(14.0), sp(9.0)));
        let label_w = (inner.width() * 0.2).clamp(96.0, 170.0);

        p.circle_filled(pos2(inner.left() + 5.0, inner.top() + 9.0), 5.0, color);
        clipped(&p, pos2(inner.left() + 17.0, inner.top() + 9.0), Align2::LEFT_CENTER, &self.pad_name(pad), semibold(13.5), text(), label_w - 24.0);
        p.text(pos2(inner.left(), inner.bottom() - 7.0), Align2::LEFT_CENTER, l("шаги в лупе", "steps in the loop"), FontId::proportional(11.5), faint());

        let grid = Rect::from_min_max(pos2(inner.left() + label_w, inner.top()), inner.max);
        let bars = (total / 16).max(1);
        let (bar_gap, cell_gap, ticks_h) = (9.0, 2.0, 6.0);
        let cell_w = ((grid.width() - bar_gap * (bars - 1) as f32 - cell_gap * (total - bars) as f32) / total as f32).max(2.0);
        let mut others = [false; 64];
        for n in self.kit.pattern.notes.iter().filter(|n| n.pad as usize != pad) {
            others[(n.step as usize).min(63)] = true;
        }
        let x_of = |i: usize| grid.left() + i as f32 * (cell_w + cell_gap) + (i / 16) as f32 * (bar_gap - cell_gap);
        let mut toggle = None;
        for i in 0..total {
            let x = x_of(i);
            let cell = Rect::from_min_max(pos2(x, grid.top() + ticks_h), pos2(x + cell_w, grid.bottom()));
            let resp = ui.interact(cell, Id::unique(("step", i)), Sense::click());
            let note = self.kit.pattern.find(i as u8, pad as u8).copied();
            let base = if i % 4 == 0 { card_hi() } else { mix(card(), card_hi(), 0.45) };
            let fill = match note {
                Some(n) => mix(base, color, if n.vel < 70 { 0.5 } else { 1.0 }),
                None if resp.hovered() => mix(base, color, 0.3),
                None => base,
            };
            p.rect_filled(cell, cr(4.0), fill);
            if others[i] {
                p.rect_filled(Rect::from_min_size(pos2(x, grid.top()), vec2(cell_w, 3.0)), egui::CornerRadius::same(1), alpha(dim(), 0.55));
            }
            if let Some(n) = note.filter(|n| n.len > 1) {
                let end = (i + n.len as usize - 1).min(total - 1);
                let y = cell.bottom() - 4.0;
                p.line_segment([pos2(x + cell_w * 0.5, y), pos2(x_of(end) + cell_w, y)], Stroke::new(3.0, alpha(color, 0.75)));
            }
            if now == Some(i) {
                p.rect_stroke(cell, cr(4.0), Stroke::new(2.0, text()), StrokeKind::Outside);
            }
            if resp.clicked() {
                toggle = Some(i);
            }
        }
        if self.kit.pattern.is_empty() {
            p.text(
                grid.center() + vec2(0.0, ticks_h * 0.5),
                Align2::CENTER_CENTER,
                l("Луп пуст. Нажмите ● и сыграйте, или кликайте по шагам.", "The loop is empty. Press ● and play, or click the steps."),
                FontId::proportional(12.5),
                dim(),
            );
        }
        if let Some(i) = toggle {
            self.actions.push(Action::ToggleStep(pad, i));
        }
    }
}
