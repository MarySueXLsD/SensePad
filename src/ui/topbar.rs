//! Top bar: who is playing, which kit, controller status, volume and the main buttons.

use super::widgets::*;
use crate::app::{Action, Page, SensePadApp};
use crate::dualsense::{Link, PadSnapshot};
use crate::factory;
use crate::i18n::{l, lf};
use crate::icons::{self, Icon};
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use std::time::Instant;

impl SensePadApp {
    pub(crate) fn top_bar(&mut self, ui: &mut Ui, snap: &PadSnapshot) {
        let accent = self.accent();
        let mut actions: Vec<Action> = Vec::new();
        let roomy = ui.available_width() > 1330.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = sp(9.0);
            let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
            logo(ui.painter(), r, accent);
            if roomy {
                ui.label(RichText::new("SensePad").font(display(23.0)).color(text()));
                ui.add_space(sp(12.0));
            }

            // Profiles: chips while there are few, a menu once there are many.
            if self.profiles.len() <= 3 {
                for (i, p) in self.profiles.iter().enumerate() {
                    let tip = lf("Играет {}", "{} is playing", &[&p.name]);
                    if profile_chip(ui, &p.name, rgb(p.accent), i == self.profile).on_hover_text(tip).clicked() {
                        actions.push(Action::UseProfile(i));
                    }
                }
            } else {
                let p = self.prof();
                let chip = profile_chip(ui, &format!("{}  ▾", p.name), rgb(p.accent), true);
                egui::Popup::menu(&chip).show(|ui| {
                    for (i, p) in self.profiles.iter().enumerate() {
                        if ui.selectable_label(i == self.profile, &p.name).clicked() {
                            actions.push(Action::UseProfile(i));
                        }
                    }
                });
            }
            if icon_btn(ui, Icon::Plus, 34.0, l("Новый профиль", "New profile")).clicked() {
                actions.push(Action::OpenSettings(6));
            }
            ui.add_space(sp(8.0));
            self.kit_menu(ui, &mut actions);

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = sp(8.0);
                if icon_label_btn(ui, Icon::Stop, l("Стоп", "Stop"), None)
                    .on_hover_text(l("Остановить луп и все звуки · Backspace", "Stop the loop and every sound · Backspace"))
                    .clicked()
                {
                    actions.push(Action::StopAll);
                }
                if icon_btn(ui, Icon::Gear, 34.0, l("Настройки", "Settings")).clicked() {
                    actions.push(Action::OpenSettings(self.settings_tab));
                }
                if icon_btn(ui, Icon::Help, 34.0, l("Инструкция · F1", "How to play · F1")).clicked() {
                    actions.push(Action::Open(Page::Help));
                }
                self.view_menu(ui);
                ui.add_space(sp(6.0));

                // Master volume with a level meter.
                let (mr, _) = ui.allocate_exact_size(vec2(6.0, 26.0), Sense::hover());
                ui.painter().rect_filled(mr, egui::CornerRadius::same(3), card_hi());
                let lvl = self.meter.clamp(0.0, 1.0);
                let fillr = Rect::from_min_max(pos2(mr.left(), mr.bottom() - mr.height() * lvl), mr.max);
                ui.painter().rect_filled(fillr, egui::CornerRadius::same(3), if lvl > 0.95 { DANGER } else { accent });
                ui.spacing_mut().slider_width = 100.0;
                let resp = ui
                    .add(egui::Slider::new(&mut self.settings.master, 0.0..=1.0).show_value(false))
                    .on_hover_text(lf("Общая громкость: {}%", "Master volume: {}%", &[&((self.settings.master * 100.0).round() as i32)]));
                if resp.changed() {
                    self.audio.set_master(self.settings.master);
                    self.settings_dirty = Some(Instant::now());
                }
                let (ir, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                icons::paint(ui.painter(), ir, Icon::Volume, dim());
                ui.add_space(sp(6.0));

                if let Some(e) = &self.audio_error {
                    pill(ui, DANGER, l("Нет звука", "No sound")).on_hover_text(e);
                }
                match &snap.conn {
                    Some(c) => {
                        let link = if c.link == Link::Usb { "USB" } else { "Bluetooth" };
                        let mut txt = if roomy { format!("{} · {link}", c.model) } else { c.model.to_string() };
                        if let Some(b) = snap.battery {
                            txt.push_str(&format!(" · {b}%{}", if snap.charging { " ⚡" } else { "" }));
                        }
                        pill(ui, OK, &txt);
                    }
                    None => {
                        pill(ui, WARN, l("Геймпад не найден", "No controller")).on_hover_text(l(
                            "Подключите DualSense по USB-C или Bluetooth.\nНе появляется? Закройте Steam, DS4Windows и DualSenseX — они прячут геймпад.\nБез геймпада играйте с клавиатуры: 1-4, Q-R, A-F, Z-V.",
                            "Connect a DualSense over USB-C or Bluetooth.\nNot showing up? Close Steam, DS4Windows and DualSenseX — they hide the controller.\nWithout one, play from the keyboard: 1-4, Q-R, A-F, Z-V.",
                        ));
                    }
                }
            });
        });
        self.actions.extend(actions);
    }

    /// The kit picker: factory kits, your saved kits, and file actions.
    fn kit_menu(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let title = if self.kit.name.trim().is_empty() { l("Без названия", "Untitled").to_string() } else { self.kit.name.clone() };
        let short: String = if title.chars().count() > 22 { title.chars().take(21).chain(['…']).collect() } else { title };
        let resp = icon_label_btn(ui, Icon::Grid, &format!("{short}  ▾"), None).on_hover_text(l("Сменить кит", "Switch kit"));
        let current = self.kit.factory.clone();
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(330.0);
            ui.label(RichText::new(l("Готовые киты", "Factory kits")).font(display(14.0)).color(dim()));
            for spec in factory::kits() {
                let sel = current.as_deref() == Some(spec.id);
                let (r, row) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
                let p = ui.painter();
                if sel || row.hovered() {
                    p.rect_filled(r, cr(8.0), if sel { alpha(rgb(spec.accent), 0.18) } else { card_hi() });
                }
                p.circle_filled(pos2(r.left() + 14.0, r.center().y), 5.0, rgb(spec.accent));
                p.text(pos2(r.left() + 28.0, r.center().y - 8.0), Align2::LEFT_CENTER, spec.title(), semibold(14.0), text());
                clipped(p, pos2(r.left() + 28.0, r.center().y + 9.0), Align2::LEFT_CENTER, spec.about(), FontId::proportional(11.5), dim(), r.width() - 100.0);
                p.text(pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, format!("{} BPM", spec.bpm as i32), display(13.0), faint());
                if row.clicked() {
                    actions.push(Action::LoadFactoryKit(spec.id.to_string()));
                    ui.close();
                }
            }
            if !self.user_kits.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(l("Мои киты", "My kits")).font(display(14.0)).color(dim()));
                for (name, path) in self.user_kits.iter().take(12) {
                    if ui.button(name).clicked() {
                        actions.push(Action::LoadUserKit(path.clone()));
                        ui.close();
                    }
                }
            }
            ui.separator();
            let items = [
                (l("Сохранить в «Мои киты»", "Save to My kits"), 0),
                (l("Импорт из файла…", "Import from a file…"), 1),
                (l("Экспорт в файл…", "Export to a file…"), 2),
                (l("Открыть папку китов", "Open the kits folder"), 3),
            ];
            for (label, id) in items {
                if ui.button(label).clicked() {
                    actions.push(match id {
                        0 => Action::SaveKitAs,
                        1 => Action::ImportKit,
                        2 => Action::ExportKit,
                        _ => Action::OpenKitsFolder,
                    });
                    ui.close();
                }
            }
            if ui.button(RichText::new(l("Очистить все пэды", "Clear every pad")).color(DANGER)).clicked() {
                actions.push(Action::ResetKit);
                ui.close();
            }
        });
    }

    /// Show or hide parts of the window.
    fn view_menu(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        let resp = icon_btn(ui, Icon::Eye, 34.0, l("Что показывать", "What to show"));
        let before = self.settings.clone();
        let s = &mut self.settings;
        egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_min_width(280.0);
            toggle_row(ui, l("Геймпад внизу", "Controller bar"), "", &mut s.show_controller, accent);
            toggle_row(ui, l("Колонки привязок", "Mapping columns"), "", &mut s.show_mapping, accent);
            toggle_row(ui, l("Панель эффектов", "Effects panel"), "", &mut s.show_fx, accent);
            toggle_row(ui, l("Шаги лупа под пэдами", "Loop steps under the pads"), "", &mut s.show_steps, accent);
            toggle_row(ui, l("Подсказки", "Tips"), "", &mut s.show_hints, accent);
        });
        if self.settings != before {
            self.settings_dirty = Some(Instant::now());
        }
    }
}

fn profile_chip(ui: &mut Ui, name: &str, color: Color32, selected: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(name.to_string(), semibold(14.0), if selected { text() } else { dim() });
    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 50.0, 34.0), Sense::click());
    let p = ui.painter();
    let fill_c = if selected { mix(surface(), color, 0.18) } else if resp.hovered() { card_hi() } else { card() };
    let stroke = if selected { Stroke::new(1.5, color) } else { Stroke::new(1.0, border()) };
    p.rect(r, egui::CornerRadius::same(17), fill_c, stroke, StrokeKind::Inside);
    let c = pos2(r.left() + 18.0, r.center().y);
    let dot = if selected { color } else { mix(card_hi(), color, 0.5) };
    p.circle_filled(c, 11.0, dot);
    let initial: String = name.chars().take(1).collect();
    p.text(c, Align2::CENTER_CENTER, initial, semibold(12.5), on_color(dot));
    p.galley(pos2(r.left() + 36.0, r.center().y - galley.size().y / 2.0), galley, text());
    resp
}
