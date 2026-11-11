//! The welcome tour shown on first run: your name, the controller, a kit, how to play, the look.

use super::settings::{page_frame, page_size};
use super::widgets::*;
use crate::app::{Action, SensePadApp};
use crate::dualsense::{Link, PadSnapshot};
use crate::factory;
use crate::i18n::{l, lf, Lang};
use crate::icons::{self, Icon};
use crate::kit::PAD_COUNT;
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align, Align2, FontId, Id, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use std::sync::atomic::Ordering;

const STEPS: usize = 5;

impl SensePadApp {
    pub(crate) fn welcome_page(&mut self, ctx: &egui::Context, snap: &PadSnapshot) {
        let modal = egui::Modal::new(Id::unique("sensepad-welcome")).frame(page_frame()).backdrop_color(alpha(palette().bg2, 0.94));
        modal.show(ctx, |ui| self.welcome_body(ui, snap));
    }

    fn welcome_body(&mut self, ui: &mut Ui, snap: &PadSnapshot) {
        let accent = self.accent();
        let size = page_size(ui.ctx(), 820.0, 560.0);
        ui.set_width(size.x);
        let step = self.welcome_step.min(STEPS - 1);
        let before = self.settings.clone();

        // Progress: one bar per step.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for i in 0..STEPS {
                let (r, _) = ui.allocate_exact_size(vec2(34.0, 4.0), Sense::hover());
                ui.painter().rect_filled(r, egui::CornerRadius::same(2), if i <= step { accent } else { card_hi() });
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if step + 1 < STEPS && ui.add(egui::Button::new(RichText::new(l("Пропустить", "Skip")).color(dim())).frame(false)).clicked() {
                    self.actions.push(Action::FinishWelcome);
                }
            });
        });
        ui.add_space(sp(10.0));

        let body_h = size.y - 96.0;
        ui.allocate_ui_with_layout(vec2(size.x, body_h), Layout::top_down(Align::Min), |ui| {
            ui.set_min_height(body_h);
            ui.spacing_mut().item_spacing.y = sp(10.0);
            match step {
                0 => self.step_hello(ui),
                1 => self.step_controller(ui, snap),
                2 => self.step_kit(ui),
                3 => self.step_play(ui, snap),
                _ => self.step_look(ui, snap),
            }
        });

        ui.add_space(sp(8.0));
        ui.horizontal(|ui| {
            if step > 0 && btn(ui, l("Назад", "Back")).clicked() {
                self.welcome_step = step - 1;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if step + 1 < STEPS {
                    if btn_primary(ui, l("Дальше", "Next"), accent).clicked() {
                        self.welcome_step = step + 1;
                    }
                } else if btn_primary(ui, l("Начать играть", "Start playing"), accent).clicked() {
                    self.actions.push(Action::FinishWelcome);
                }
            });
        });
        if self.settings != before {
            self.on_settings_changed(&before);
        }
    }

    fn step_hello(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        ui.add_space(sp(18.0));
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
            logo(ui.painter(), r, accent);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("SensePad").font(display(40.0)).color(text()));
                ui.label(RichText::new(l("Геймпад DualSense становится драм-машиной.", "Your DualSense becomes a drum machine.")).size(16.0).color(dim()));
            });
        });
        ui.add_space(sp(14.0));
        ui.label(
            RichText::new(l(
                "Каждая кнопка играет свой звук, стики управляют эффектами, а луп запоминает то, что вы сыграли. Настроим всё за минуту.",
                "Every button plays its own sound, the sticks run the effects, and the looper remembers what you play. Setup takes a minute.",
            ))
            .size(15.0)
            .color(text()),
        );
        ui.add_space(sp(12.0));
        field_label(ui, l("Язык", "Language"));
        let lang_before = self.settings.language;
        ui.allocate_ui(vec2(320.0, 36.0), |ui| {
            segmented(ui, &mut self.settings.language, &[(Lang::Ru, "Русский"), (Lang::En, "English")], accent);
        });
        if self.settings.language != lang_before {
            // A profile nobody has named yet follows the language.
            let (ru, en) = crate::kit::DEFAULT_PROFILE_NAMES;
            let (old, new) = if self.settings.language == Lang::Ru { (en, ru) } else { (ru, en) };
            if self.prof().name == old && self.rename_buf.trim() == old {
                self.rename_buf = new.to_string();
                self.actions.push(Action::RenameProfile);
            }
        }
        ui.add_space(sp(8.0));
        field_label(ui, l("Как вас зовут?", "What's your name?"));
        let name = ui.add(egui::TextEdit::singleline(&mut self.rename_buf).font(FontId::proportional(16.0)).desired_width(320.0));
        if name.lost_focus() {
            if self.rename_buf.trim().is_empty() {
                self.rename_buf = self.prof().name.clone();
            } else {
                self.actions.push(Action::RenameProfile);
            }
        }
        if let Some(e) = &self.profile_error {
            ui.label(RichText::new(e).size(12.5).color(DANGER));
        }
        if self.profiles.len() > 1 {
            // Coming back to the tour with several profiles: pick who is playing.
            let mut pick = None;
            ui.horizontal_wrapped(|ui| {
                for (i, p) in self.profiles.iter().enumerate() {
                    if chip(ui, &p.name, i == self.profile, rgb(p.accent)).clicked() {
                        pick = Some(i);
                    }
                }
            });
            if let Some(i) = pick {
                self.actions.push(Action::UseProfile(i));
            }
        }
        hint(ui, l(
            "Имя видно в заголовке окна и в китах, которые вы сохраняете. У каждого профиля свой кит и цвет; другие профили добавляются в Настройках → Профили.",
            "The name shows in the window title and in the kits you save. Each profile has its own kit and colour; add more in Settings → Profiles.",
        ));
    }

    fn step_controller(&mut self, ui: &mut Ui, snap: &PadSnapshot) {
        ui.label(RichText::new(l("Подключите DualSense", "Connect your DualSense")).font(display(30.0)).color(text()));
        match &snap.conn {
            Some(c) => {
                let link = if c.link == Link::Usb { "USB" } else { "Bluetooth" };
                let mut line = lf("{} на связи · {}", "{} is connected · {}", &[&c.model, &link]);
                if let Some(b) = snap.battery {
                    line.push_str(&lf(" · заряд {}%", " · battery {}%", &[&b]));
                }
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 6.0, OK);
                    ui.label(RichText::new(line).size(15.0).color(text()));
                });
                hint(ui, l("Световая панель загорелась цветом профиля. Понажимайте кнопки — картинка отзывается.", "The light bar took the profile colour. Press some buttons — the picture responds."));
            }
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(l("Ищу геймпад…", "Looking for a controller…")).size(15.0).color(text()));
                });
                ui.label(RichText::new(l("Подключите его кабелем USB-C или по Bluetooth (зажмите Create и PS, пока панель не замигает).", "Plug it in with a USB-C cable or pair it over Bluetooth (hold Create and PS until the bar blinks).")).color(dim()));
                hint(ui, l(
                    "Не находится? Закройте Steam, DS4Windows и DualSenseX: они забирают геймпад себе. Без геймпада тоже можно — играйте с клавиатуры.",
                    "Not found? Close Steam, DS4Windows and DualSenseX: they take the controller for themselves. No controller is fine too — play from the keyboard.",
                ));
            }
        }
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, Sense::hover());
        self.live_controller(ui, area.shrink2(vec2(40.0, 6.0)), snap);
    }

    fn step_kit(&mut self, ui: &mut Ui) {
        let installing = self.factory_installing;
        let progress = self.factory_progress.load(Ordering::Relaxed);
        let current = self.kit.factory.clone();
        ui.label(RichText::new(l("Выберите звук", "Pick your sound")).font(display(30.0)).color(text()));
        if installing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(lf("Готовлю звуки… {}%", "Preparing the sounds… {}%", &[&progress])).color(dim()));
            });
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
            ui.painter().rect_filled(r, egui::CornerRadius::same(3), card_hi());
            let mut f = r;
            f.set_width(r.width() * progress as f32 / 100.0);
            ui.painter().rect_filled(f, egui::CornerRadius::same(3), self.accent());
            return;
        }
        hint(ui, l("Кликните по киту — он загрузится и сыграет демо. Сменить можно в любой момент.", "Click a kit — it loads and plays its demo. You can switch at any time."));
        let mut pick = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ((ui.available_width() - 22.0) / 2.0).max(200.0);
            let kits = factory::kits();
            for pair in kits.chunks(2) {
                ui.horizontal(|ui| {
                    for spec in pair {
                        let sel = current.as_deref() == Some(spec.id);
                        let col = rgb(spec.accent);
                        let (r, resp) = ui.allocate_exact_size(vec2(w, 66.0), Sense::click());
                        let p = ui.painter();
                        let fill = if sel { mix(card(), col, 0.22) } else if resp.hovered() { card_hi() } else { card() };
                        p.rect(r, cr(14.0), fill, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { col } else { border() }), StrokeKind::Inside);
                        p.rect_filled(Rect::from_min_size(r.min + vec2(0.0, 14.0), vec2(4.0, r.height() - 28.0)), egui::CornerRadius::same(2), col);
                        p.text(pos2(r.left() + 18.0, r.top() + 18.0), Align2::LEFT_CENTER, spec.title(), display(18.0), text());
                        p.text(pos2(r.right() - 14.0, r.top() + 18.0), Align2::RIGHT_CENTER, format!("{} BPM", spec.bpm as i32), display(13.0), dim());
                        let about: String = spec.about().to_string();
                        let galley = p.layout(about, FontId::proportional(12.0), dim(), r.width() - 34.0);
                        p.galley(pos2(r.left() + 18.0, r.top() + 31.0), galley, dim());
                        if sel {
                            icons::paint(p, Rect::from_center_size(pos2(r.right() - 20.0, r.bottom() - 14.0), vec2(12.0, 12.0)), Icon::Check, col);
                        }
                        if resp.clicked() {
                            pick = Some(spec.id);
                        }
                    }
                });
            }
        });
        if let Some(id) = pick {
            self.actions.push(Action::LoadFactoryKit(id.to_string()));
            self.actions.push(Action::Demo);
        }
    }

    fn step_play(&mut self, ui: &mut Ui, snap: &PadSnapshot) {
        ui.label(RichText::new(l("Как играть", "How to play")).font(display(30.0)).color(text()));
        let rows: [(Icon, &str, &str); 4] = [
            (Icon::Grid, l("Кнопки — это пэды", "Buttons are pads"), l("Крест, круг, крестовина, L1/R1, курки, стики и тачпад играют 16 звуков кита.", "Cross, circle, the d-pad, L1/R1, the triggers, sticks and touchpad play the kit's 16 sounds.")),
            (Icon::Bolt, l("Стики — эффекты", "Sticks are effects"), l("Левый: фильтр и эхо. Правый: питч и статтер. Отпустили — звук чистый.", "Left: filter and echo. Right: pitch and stutter. Let go and the sound is clean.")),
            (Icon::Record, l("Луп запоминает", "The looper remembers"), l("Create — старт и стоп. Кнопка микрофона — запись: сыграйте бит, и он зациклится.", "Create starts and stops. The mute button records: play a beat and it loops.")),
            (Icon::Stop, l("Options — тишина", "Options is silence"), l("Останавливает луп и все звуки разом.", "Stops the loop and every sound at once.")),
        ];
        for (icon, title, body) in rows {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                ui.painter().rect_filled(r, cr(9.0), card_hi());
                icons::paint(ui.painter(), r.shrink(9.0), icon, self.accent());
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(RichText::new(title).font(semibold(15.0)).color(text()));
                    ui.label(RichText::new(body).size(13.0).color(dim()));
                });
            });
        }
        let playing: Vec<String> = (0..PAD_COUNT).filter(|i| self.since_hit(*i) < 0.6).map(|i| self.pad_name(i)).collect();
        let line = if playing.is_empty() {
            l("Попробуйте прямо сейчас: нажмите любую кнопку геймпада или клавиши 1-4, Q-R.", "Try it now: press any controller button, or the keys 1-4, Q-R.").to_string()
        } else {
            lf("Звучит: {}", "Playing: {}", &[&playing.join(", ")])
        };
        ui.label(RichText::new(line).font(semibold(14.0)).color(self.accent()));
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, Sense::hover());
        self.live_controller(ui, area.shrink2(vec2(60.0, 2.0)), snap);
    }

    fn step_look(&mut self, ui: &mut Ui, snap: &PadSnapshot) {
        let accent = self.accent();
        ui.label(RichText::new(l("Оформление", "Make it yours")).font(display(30.0)).color(text()));
        field_label(ui, l("Тема", "Theme"));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for kind in ThemeKind::ALL {
                if chip(ui, kind.name(), self.settings.theme == kind, accent).clicked() {
                    self.settings.theme = kind;
                }
            }
        });
        field_label(ui, l("Цвет геймпада на экране", "Controller colour on screen"));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for skin in Skin::ALL.into_iter().filter(|s| *s != Skin::Custom) {
                if chip(ui, skin.name(), self.settings.skin == skin, accent).clicked() {
                    self.settings.skin = skin;
                }
            }
        });
        hint(ui, l(
            "Свои цвета, скругление, плотность, раскладка пэдов и многое другое — в Настройках (шестерёнка справа вверху). Инструкция открывается по F1.",
            "Your own colours, rounding, density, pad layout and much more live in Settings (the gear, top right). F1 opens the guide.",
        ));
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, Sense::hover());
        self.live_controller(ui, area.shrink2(vec2(90.0, 4.0)), snap);
    }

    /// The live controller picture, without binding clicks.
    fn live_controller(&mut self, ui: &mut Ui, area: Rect, snap: &PadSnapshot) {
        if area.height() < 60.0 {
            return;
        }
        let pad_names: Vec<String> = (0..PAD_COUNT).map(|i| self.pad_name(i)).collect();
        let _ = self.draw_controller(ui, area, snap, usize::MAX, &pad_names);
    }
}
