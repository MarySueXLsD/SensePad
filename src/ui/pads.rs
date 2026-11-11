//! The pad grid: sixteen pads that light up when they sound.

use super::widgets::*;
use crate::app::{Action, Learn, SensePadApp, KEY_LABELS};
use crate::factory;
use crate::i18n::{l, lf};
use crate::icons::{self, Icon};
use crate::kit::{PadLayout, PlayMode, PAD_COUNT};
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Frame, Id, Layout, Rect, RichText, Sense, Shadow, Stroke, StrokeKind, Ui};

impl SensePadApp {
    pub(crate) fn pads_view(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        let loaded = (0..PAD_COUNT).filter(|&i| self.kit.pads[i].sample.is_some()).count();
        let about = self.kit.factory.as_deref().and_then(factory::find_kit).map(|k| k.about());

        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.kit.name)
                .font(display(27.0))
                .desired_width(380.0)
                .frame(Frame::NONE)
                .hint_text(l("Название кита", "Kit name"));
            if ui.add(edit).on_hover_text(l("Название кита — кликните, чтобы изменить", "Kit name — click to rename")).changed() {
                self.mark_kit_changed();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(lf("{} из {} пэдов со звуком", "{} of {} pads loaded", &[&loaded, &PAD_COUNT])).size(12.5).color(faint()));
            });
        });
        let (cols, rows) = match self.settings.pad_layout {
            PadLayout::Grid4x4 => (4, 4),
            PadLayout::Wide8x2 => (8, 2),
        };
        let gap = sp(13.0);
        // What the pad rows need at the very least; everything else gives way to them.
        let need = rows as f32 * 58.0 + gap * (rows - 1) as f32;
        if let Some(text_) = about.filter(|_| ui.available_height() > need + 150.0) {
            ui.label(RichText::new(text_).size(13.0).color(dim()));
        }
        ui.add_space(sp(10.0));

        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, Sense::hover());
        let mut strip_h = if self.settings.show_steps { sp(70.0).clamp(58.0, 90.0) } else { 0.0 };
        if area.height() - strip_h - gap < need {
            strip_h = 0.0; // the window is too short: the pads keep the room
        }
        let grid = Rect::from_min_max(area.min, pos2(area.max.x, area.max.y - if strip_h > 0.0 { strip_h + gap } else { 0.0 }));
        let cw = ((grid.width() - gap * (cols - 1) as f32) / cols as f32).max(40.0);
        let ch = ((grid.height() - gap * (rows - 1) as f32) / rows as f32).max(30.0);
        let files_hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        for i in 0..PAD_COUNT {
            let (row, col) = ((i / cols) as f32, (i % cols) as f32);
            let rect = Rect::from_min_size(grid.min + vec2(col * (cw + gap), row * (ch + gap)), vec2(cw, ch));
            self.pad_rects[i] = rect;
            self.pad_tile(ui, i, rect, accent, files_hovering);
        }
        if strip_h > 0.0 {
            let strip = Rect::from_min_max(pos2(area.min.x, area.max.y - strip_h), area.max);
            self.steps_strip(ui, strip);
        }
    }

    fn pad_tile(&mut self, ui: &mut Ui, i: usize, rect: Rect, accent: Color32, files_hovering: bool) {
        let resp = ui.interact(rect, Id::unique(("pad", i)), Sense::click());
        let primary_down = ui.input(|inp| inp.pointer.primary_down());
        if resp.is_pointer_button_down_on() && primary_down {
            if self.mouse_down != Some(i) {
                self.mouse_down = Some(i);
                self.selected = i;
                self.shared.fire(i);
            }
        } else if self.mouse_down == Some(i) {
            self.mouse_down = None;
            self.shared.release(i);
        }

        let name = self.pad_name(i);
        let cfg = &self.kit.pads[i];
        let color = rgb(cfg.color);
        let since = self.since_hit(i);
        let glow = if !since.is_finite() {
            0.0
        } else if self.settings.animations {
            (-since * 5.0).exp() * self.settings.glow
        } else if since < 0.09 {
            self.settings.glow
        } else {
            0.0
        };
        let press = if self.settings.animations { (1.0 - since / 0.07).clamp(0.0, 1.0) } else { 0.0 };
        let active = self.audio.active_mask() & (1 << i) != 0;
        let queued = self.audio.queued_mask() & (1 << i) != 0;
        let selected = self.selected == i;
        let sample = self.samples[i].clone();
        let has = cfg.sample.is_some();
        let drop_target = files_hovering && ui.input(|inp| inp.pointer.hover_pos()).is_some_and(|p| rect.contains(p));
        let time = ui.input(|inp| inp.time) as f32;
        let dark = palette().dark;
        let p = ui.painter();
        let rect = rect.shrink(press * 2.0);
        let round = cr(16.0);

        if glow > 0.02 {
            let g = glow.min(1.5);
            p.add(Shadow { offset: [0, 0], blur: (10.0 + 30.0 * g) as u8, spread: (5.0 * g) as u8, color: alpha(color, 0.6 * glow.min(1.0)) }.as_shape(rect, round));
        } else if dark {
            p.add(Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_black_alpha(50) }.as_shape(rect, round));
        }
        let hover = if resp.hovered() { 0.05 } else { 0.0 };
        let body = if has { mix(card(), color, 0.15 + 0.36 * glow.min(1.0) + hover) } else { mix(bg(), card(), 0.55 + hover * 3.0) };
        p.rect_filled(rect, round, body);
        if has {
            fade_down(p, rect, round, alpha(Color32::WHITE, if dark { 0.055 } else { 0.5 }));
            p.rect_filled(Rect::from_min_size(rect.min + vec2(sp(15.0), 0.0), vec2(30.0, 3.0)), egui::CornerRadius::same(2), color);
        }
        let stroke = if drop_target {
            Stroke::new(2.0, color)
        } else if queued {
            Stroke::new(2.0, alpha(color, 0.45 + 0.55 * (time * 9.0).sin().abs()))
        } else if selected {
            Stroke::new(2.0, accent)
        } else if active {
            Stroke::new(1.5, alpha(color, 0.75))
        } else {
            Stroke::new(1.0, if has { mix(border(), color, 0.25) } else { border() })
        };
        p.rect_stroke(rect, round, stroke, StrokeKind::Inside);

        // Top row: the button, the name, then how it plays and which pad it is.
        let m = sp(12.0);
        let row_y = rect.top() + m + 12.0;
        let mut left = rect.left() + m;
        if let Some(b) = cfg.binding {
            badge(p, Rect::from_min_size(pos2(left, row_y - 12.0), vec2(34.0, 24.0)), b, mix(bg(), Color32::BLACK, if dark { 0.2 } else { 0.0 }));
            left += 42.0;
        }
        let mut right = rect.right() - m;
        if rect.width() > 210.0 {
            let corner = if self.settings.show_keys { format!("{} · {:02}", KEY_LABELS[i], i + 1) } else { format!("{:02}", i + 1) };
            let g = p.layout_no_wrap(corner, display(12.0), faint());
            right -= g.size().x;
            p.galley(pos2(right, row_y - g.size().y * 0.5), g, faint());
            right -= 9.0;
        }
        if active {
            let pulse = 0.5 + 0.5 * (time * 6.0).sin();
            p.circle_filled(pos2(right - 5.0, row_y), 3.0 + pulse * 1.5, color);
            right -= 16.0;
        }
        let mode_icon = match cfg.mode {
            PlayMode::Loop => Some(Icon::Loop),
            PlayMode::Hold => Some(Icon::Hold),
            PlayMode::OneShot => None,
        };
        let sync_icon = if cfg.sync_bpm > 0.0 { Some(Icon::Metronome) } else { None };
        for (icon, col) in [(mode_icon, color), (sync_icon, alpha(color, 0.8))] {
            if let Some(icon) = icon {
                icons::paint(p, Rect::from_center_size(pos2(right - 7.0, row_y), vec2(14.0, 14.0)), icon, col);
                right -= 19.0;
            }
        }
        let notes = self.kit.pattern.notes.iter().filter(|n| n.pad as usize == i).count();
        let name_color = if has { text() } else { faint() };
        clipped(p, pos2(left, row_y), Align2::LEFT_CENTER, &name, semibold(14.5), name_color, (right - left - 6.0).max(20.0));

        // The rest of the pad is the waveform, or an invitation.
        let wave = Rect::from_min_max(pos2(rect.left() + m, row_y + 19.0), pos2(rect.right() - m, rect.bottom() - m + 2.0));
        if let Some(s) = &sample {
            if self.settings.show_wave && wave.height() > 10.0 {
                let progress = if self.settings.show_progress && active { self.audio.progress(i) } else { None };
                waveform(p, wave, &s.peaks, alpha(color, 0.62 + 0.38 * glow.min(1.0)), progress);
            }
        } else if self.loading[i] || (has && self.factory_installing) {
            p.text(wave.center(), Align2::CENTER_CENTER, l("Загрузка…", "Loading…"), FontId::proportional(12.5), dim());
        } else if self.errors[i].is_some() {
            p.text(wave.center(), Align2::CENTER_CENTER, l("Файл не найден", "File not found"), FontId::proportional(12.5), DANGER);
        } else if !has && wave.height() > 16.0 {
            p.text(wave.center(), Align2::CENTER_CENTER, l("+  перетащите звук", "+  drop a sound"), FontId::proportional(12.0), faint());
        }

        let tip = match &cfg.sample {
            Some(path) => format!("{}\n{}", path.display(), l("Клик — играть · правая кнопка — меню", "Click to play · right-click for options")),
            None => l("Клик — выбрать · перетащите сюда аудиофайл", "Click to select · drop an audio file here").to_string(),
        };
        let resp = resp.on_hover_text(tip);
        let bound = cfg.binding.is_some();
        resp.context_menu(|ui| {
            if ui.button(l("Выбрать звук из библиотеки", "Pick a sound from the library")).clicked() {
                self.actions.push(Action::ChooseSample(i));
                ui.close();
            }
            if ui.button(l("Загрузить из файла…", "Load from a file…")).clicked() {
                self.actions.push(Action::Browse(i));
                ui.close();
            }
            if ui.button(l("Назначить кнопку геймпада…", "Assign a controller button…")).clicked() {
                self.actions.push(Action::Learn(Learn::Pad(i)));
                ui.close();
            }
            if bound && ui.button(l("Снять кнопку", "Remove the button")).clicked() {
                self.actions.push(Action::Bind(i, None));
                ui.close();
            }
            if notes > 0 && ui.button(l("Убрать из лупа", "Remove from the loop")).clicked() {
                self.actions.push(Action::ClearPadNotes(i));
                ui.close();
            }
            if has && ui.button(RichText::new(l("Очистить пэд", "Clear the pad")).color(DANGER)).clicked() {
                self.actions.push(Action::Clear(i));
                ui.close();
            }
        });
        if resp.secondary_clicked() {
            self.selected = i;
        }
    }
}
