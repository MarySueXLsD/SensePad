//! Settings: one page with a list of sections on the left.

use super::widgets::*;
use crate::app::{func_name, fx_name, Action, Learn, Page, SensePadApp};
use crate::controller_skin;
use crate::factory;
use crate::i18n::{input_label, l, lf, Lang};
use crate::icons::{self, Icon};
use crate::kit::{self, BufferPref, ControllerStyle, Density, Func, FxKind, LightbarMode, PadLayout, StickMode, PROFILE_COLORS};
use crate::theme::*;
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontId, Frame, Id, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder,
};
use std::sync::atomic::Ordering;

/// Frame shared by the full-window pages (settings, help, welcome).
pub(super) fn page_frame() -> Frame {
    Frame::new().fill(surface()).corner_radius(cr(18.0)).inner_margin(Margin::same(spi(22.0))).stroke(Stroke::new(1.0, border()))
}

/// Size of a page for the current window.
pub(super) fn page_size(ctx: &egui::Context, max_w: f32, max_h: f32) -> egui::Vec2 {
    let screen = ctx.content_rect().size();
    vec2((screen.x - 80.0).clamp(520.0, max_w), (screen.y - 150.0).clamp(360.0, max_h))
}

/// Split a page into the list of sections and the content area.
pub(super) fn split_page(area: Rect, nav_w: f32) -> (Rect, Rect) {
    let nav = Rect::from_min_size(area.min, vec2(nav_w, area.height()));
    let body = Rect::from_min_max(pos2(area.min.x + nav_w + sp(12.0), area.min.y), area.max);
    (nav, body)
}

/// A row in the list of sections on the left of a page.
pub(super) fn nav_item(ui: &mut Ui, icon: Icon, label: &str, selected: bool, accent: Color32) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), sp(36.0).max(30.0)), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(r, cr(9.0), alpha(accent, 0.2));
        p.rect_filled(Rect::from_min_size(r.min + vec2(0.0, 8.0), vec2(3.0, r.height() - 16.0)), egui::CornerRadius::same(2), accent);
    } else if resp.hovered() {
        p.rect_filled(r, cr(9.0), card_hi());
    }
    let col = if selected { text() } else { dim() };
    icons::paint(p, Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(16.0, 16.0)), icon, col);
    p.text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, label, if selected { semibold(14.0) } else { FontId::proportional(14.0) }, col);
    resp.clicked()
}

impl SensePadApp {
    pub(crate) fn settings_page(&mut self, ctx: &egui::Context) {
        let modal = egui::Modal::new(Id::unique("sensepad-settings")).frame(page_frame()).backdrop_color(Color32::from_black_alpha(150));
        let resp = modal.show(ctx, |ui| self.settings_body(ui));
        if resp.inner || resp.should_close() {
            self.actions.push(Action::ClosePage);
        }
    }

    fn settings_body(&mut self, ui: &mut Ui) -> bool {
        let accent = self.accent();
        let size = page_size(ui.ctx(), 940.0, 640.0);
        let mut close = false;
        ui.set_width(size.x);
        ui.horizontal(|ui| {
            ui.label(RichText::new(l("Настройки", "Settings")).font(display(26.0)).color(text()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_btn(ui, Icon::Close, 30.0, l("Закрыть · Esc", "Close · Esc")).clicked() {
                    close = true;
                }
            });
        });
        ui.add_space(sp(8.0));
        let tabs = [
            (Icon::Gear, l("Общие", "General")),
            (Icon::Palette, l("Оформление", "Look")),
            (Icon::Grid, l("Пэды", "Pads")),
            (Icon::Gamepad, l("Геймпад", "Controller")),
            (Icon::Bolt, l("Стики и эффекты", "Sticks & effects")),
            (Icon::Volume, l("Звук", "Sound")),
            (Icon::User, l("Профили", "Profiles")),
            (Icon::Folder, l("Библиотека", "Library")),
        ];
        let before = self.settings.clone();
        let (area, _) = ui.allocate_exact_size(size, Sense::hover());
        let (nav_rect, body_rect) = split_page(area, 190.0);
        {
            ui.scope_builder(UiBuilder::new().max_rect(nav_rect).layout(Layout::top_down(Align::Min)), |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                for (i, (icon, label)) in tabs.iter().enumerate() {
                    if nav_item(ui, *icon, label, self.settings_tab == i, accent) {
                        self.settings_tab = i;
                    }
                }
            });
            ui.scope_builder(UiBuilder::new().max_rect(body_rect).layout(Layout::top_down(Align::Min)), |ui| {
                egui::ScrollArea::vertical().max_height(size.y).auto_shrink([false, false]).show(ui, |ui| {
                    Frame::new().inner_margin(Margin { left: 2, right: 14, top: 0, bottom: 8 }).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = sp(10.0);
                        match self.settings_tab {
                            0 => self.tab_general(ui),
                            1 => self.tab_look(ui),
                            2 => self.tab_pads(ui),
                            3 => self.tab_controller(ui),
                            4 => self.tab_sticks(ui),
                            5 => self.tab_sound(ui),
                            6 => self.tab_profiles(ui),
                            _ => self.tab_library(ui),
                        }
                    });
                });
            });
        }
        if self.settings != before {
            self.on_settings_changed(&before);
        }
        close
    }

    fn tab_general(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        section(ui, l("Язык", "Language"), |ui| {
            segmented(ui, &mut self.settings.language, &[(Lang::Ru, "Русский"), (Lang::En, "English")], accent);
        });
        section(ui, l("Помощь", "Help"), |ui| {
            toggle_row(
                ui,
                l("Подсказки под геймпадом", "Tips under the controller"),
                l("Короткие советы, которые сменяются каждые несколько секунд.", "Short tips that change every few seconds."),
                &mut self.settings.show_hints,
                accent,
            );
            ui.horizontal_wrapped(|ui| {
                if icon_label_btn(ui, Icon::Book, l("Открыть инструкцию", "Open the guide"), None).clicked() {
                    self.actions.push(Action::Open(Page::Help));
                }
                if icon_label_btn(ui, Icon::Sparkle, l("Пройти знакомство заново", "Replay the welcome tour"), None).clicked() {
                    self.welcome_step = 0;
                    self.actions.push(Action::Open(Page::Welcome));
                }
            });
        });
        section(ui, l("Данные", "Data"), |ui| {
            hint(ui, l("Настройки, профили, киты и звуки хранятся здесь:", "Settings, profiles, kits and sounds are kept here:"));
            ui.label(RichText::new(kit::app_dir().display().to_string()).size(12.5).color(dim()));
            if btn(ui, l("Открыть папку", "Open the folder")).clicked() {
                let _ = std::process::Command::new("explorer").arg(kit::app_dir()).spawn();
            }
        });
        hint(ui, &lf("SensePad {} · автор {}", "SensePad {} · by {}", &[&env!("CARGO_PKG_VERSION"), &kit::AUTHOR]));
    }

    fn tab_look(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        let profile_accent = self.prof().accent;
        section(ui, l("Тема", "Theme"), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                for kind in ThemeKind::ALL {
                    let (r, resp) = ui.allocate_exact_size(vec2(108.0, 92.0), Sense::click());
                    let sel = self.settings.theme == kind;
                    let pal = kind.palette(self.settings.custom_bg);
                    let p = ui.painter();
                    let pic = Rect::from_min_max(r.min, pos2(r.max.x, r.max.y - 24.0));
                    p.rect(pic, cr(10.0), pal.bg, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { accent } else { border() }), StrokeKind::Inside);
                    p.rect_filled(Rect::from_min_size(pic.min + vec2(6.0, 6.0), vec2(pic.width() - 12.0, 9.0)), egui::CornerRadius::same(3), pal.surface);
                    for k in 0..4 {
                        let cell = Rect::from_min_size(pic.min + vec2(6.0 + k as f32 * 24.5, 21.0), vec2(21.0, 17.0));
                        p.rect_filled(cell, egui::CornerRadius::same(4), if k == 1 { accent } else { pal.card_hi });
                    }
                    p.rect_filled(Rect::from_min_size(pic.min + vec2(6.0, 44.0), vec2(pic.width() - 12.0, 16.0)), egui::CornerRadius::same(4), pal.card);
                    p.text(pos2(r.center().x, r.bottom() - 10.0), Align2::CENTER_CENTER, kind.name(), FontId::proportional(12.5), if sel { text() } else { dim() });
                    if resp.clicked() {
                        self.settings.theme = kind;
                    }
                }
            });
            if self.settings.theme == ThemeKind::Custom {
                setting_row(ui, l("Цвет фона", "Background colour"), |ui| {
                    egui::color_picker::color_edit_button_srgb(ui, &mut self.settings.custom_bg);
                });
                hint(ui, l("Остальные цвета темы подберутся сами. Светлый фон даёт светлую тему.", "The other theme colours follow. A light background gives a light theme."));
            }
        });
        section(ui, l("Акцентный цвет", "Accent colour"), |ui| {
            let mut own = self.settings.accent_override.is_some();
            if segmented(ui, &mut own, &[(false, l("Цвет профиля", "Profile colour")), (true, l("Свой", "My own"))], accent) {
                self.settings.accent_override = own.then_some(profile_accent);
            }
            if let Some(c) = self.settings.accent_override.as_mut() {
                color_row(ui, &PROFILE_COLORS, c, l("Любой цвет", "Any colour"));
            }
        });
        section(ui, l("Форма и размер", "Shape and size"), |ui| {
            value_slider(ui, l("Скругление углов", "Corner rounding"), &mut self.settings.round, 0.0..=1.6, 1.0, None, |v| format!("{:.0}%", v * 100.0));
            field_label(ui, l("Плотность", "Density"));
            segmented(
                ui,
                &mut self.settings.density,
                &[(Density::Compact, l("Компактно", "Compact")), (Density::Cozy, l("Обычно", "Regular")), (Density::Roomy, l("Просторно", "Roomy"))],
                accent,
            );
            field_label(ui, l("Масштаб интерфейса", "Interface scale"));
            segmented(ui, &mut self.settings.ui_scale, &[(0.85, "85%"), (1.0, "100%"), (1.15, "115%"), (1.3, "130%"), (1.5, "150%")], accent);
        });
        section(ui, l("Движение", "Motion"), |ui| {
            value_slider(ui, l("Свечение пэдов при ударе", "Pad glow on a hit"), &mut self.settings.glow, 0.0..=1.5, 1.0, None, |v| format!("{:.0}%", v * 100.0));
            toggle_row(ui, l("Анимации", "Animations"), l("Плавное затухание свечения и нажатие пэдов.", "Glow fades smoothly and pads press in."), &mut self.settings.animations, accent);
        });
    }

    fn tab_pads(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        let s = &mut self.settings;
        section(ui, l("Раскладка", "Layout"), |ui| {
            segmented(ui, &mut s.pad_layout, &[(PadLayout::Grid4x4, l("Сетка 4 × 4", "4 × 4 grid")), (PadLayout::Wide8x2, l("Два ряда по 8", "Two rows of 8"))], accent);
        });
        section(ui, l("Что видно на пэде", "What a pad shows"), |ui| {
            toggle_row(ui, l("Форма волны", "Waveform"), "", &mut s.show_wave, accent);
            toggle_row(ui, l("Ход воспроизведения", "Playback position"), l("Сыгранная часть волны подсвечивается.", "The part already played lights up."), &mut s.show_progress, accent);
            toggle_row(ui, l("Клавиша клавиатуры", "Keyboard key"), "", &mut s.show_keys, accent);
        });
        section(ui, l("Вокруг пэдов", "Around the pads"), |ui| {
            toggle_row(ui, l("Шаги лупа под пэдами", "Loop steps under the pads"), l("Полоса, где видно и можно править удары выбранного пэда.", "A strip where you see and edit the hits of the selected pad."), &mut s.show_steps, accent);
            toggle_row(ui, l("Геймпад внизу окна", "Controller at the bottom"), "", &mut s.show_controller, accent);
            toggle_row(ui, l("Колонки привязок у геймпада", "Mapping columns by the controller"), "", &mut s.show_mapping, accent);
            toggle_row(ui, l("Панель эффектов", "Effects panel"), "", &mut s.show_fx, accent);
        });
    }

    fn tab_controller(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        section(ui, l("Геймпад на экране", "Controller on screen"), |ui| {
            segmented(
                ui,
                &mut self.settings.controller_style,
                &[(ControllerStyle::Colour, l("Цветной", "Colour")), (ControllerStyle::LineArt, l("Контурный", "Line art")), (ControllerStyle::Drawn, l("Рисунок", "Drawing"))],
                accent,
            );
            if self.settings.controller_style != ControllerStyle::LineArt {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    for skin in Skin::ALL {
                        let (r, resp) = ui.allocate_exact_size(vec2(112.0, 96.0), Sense::click());
                        let sel = self.settings.skin == skin;
                        let p = ui.painter();
                        p.rect(r, cr(12.0), if resp.hovered() { card_hi() } else { bg() }, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { accent } else { border() }), StrokeKind::Inside);
                        let pic = Rect::from_min_max(r.min + vec2(8.0, 6.0), pos2(r.max.x - 8.0, r.max.y - 24.0));
                        match &self.skin_tex {
                            Some(tex) => controller_skin::preview(p, pic, tex, skin.tints(), accent),
                            None => {
                                p.rect_filled(pic.shrink(8.0), cr(10.0), skin.tints().shell);
                            }
                        }
                        p.text(pos2(r.center().x, r.bottom() - 12.0), Align2::CENTER_CENTER, skin.name(), FontId::proportional(11.5), if sel { text() } else { dim() });
                        if resp.clicked() {
                            self.settings.skin = skin;
                        }
                    }
                });
                if self.settings.skin == Skin::Custom {
                    ui.horizontal_wrapped(|ui| {
                        for (k, label) in [l("Корпус", "Shell"), l("Тачпад", "Touchpad"), l("Центр", "Centre")].into_iter().enumerate() {
                            ui.label(RichText::new(label).color(text()));
                            egui::color_picker::color_edit_button_srgb(ui, &mut self.settings.custom_skin[k]);
                            ui.add_space(12.0);
                        }
                    });
                }
            }
            field_label(ui, l("Символы кнопок", "Button symbols"));
            segmented(ui, &mut self.settings.colored_glyphs, &[(false, l("Серые, как на DualSense", "Grey, as on the DualSense")), (true, l("Цветные, классические", "Classic colours"))], accent);
        });

        section(ui, l("Служебные кнопки", "Function buttons"), |ui| {
            hint(ui, l(
                "Этим кнопкам не хватило пэдов, поэтому они управляют лупом. Любую можно переназначить или отдать пэду.",
                "These buttons are left over after the pads, so they run the looper. Reassign any of them or give it to a pad.",
            ));
            for f in Func::ALL {
                let learning = self.learning == Some(Learn::Func(f));
                let current = self.settings.functions.get(f);
                ui.horizontal(|ui| {
                    let (lr, _) = ui.allocate_exact_size(vec2(190.0, 28.0), Sense::hover());
                    ui.painter().text(lr.left_center(), Align2::LEFT_CENTER, func_name(f), FontId::proportional(14.0), text());
                    let label = if learning {
                        l("Нажмите кнопку…", "Press a button…").to_string()
                    } else {
                        current.map(|i| input_label(i).to_string()).unwrap_or_else(|| l("нет кнопки", "no button").to_string())
                    };
                    ui.label(RichText::new(label).font(semibold(14.0)).color(if learning { accent } else if current.is_some() { text() } else { faint() }));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if current.is_some() && btn(ui, l("Снять", "Remove")).clicked() {
                            self.actions.push(Action::SetFunc(f, None));
                        }
                        let b = if learning { l("Отмена", "Cancel") } else { l("Назначить", "Assign") };
                        if btn(ui, b).clicked() {
                            self.actions.push(if learning { Action::CancelLearn } else { Action::Learn(Learn::Func(f)) });
                        }
                    });
                });
            }
        });

        let s = &mut self.settings;
        section(ui, l("Световая панель", "Light bar"), |ui| {
            segmented(
                ui,
                &mut s.lightbar,
                &[
                    (LightbarMode::PadFlash, l("Вспышки пэдов", "Pad flashes")),
                    (LightbarMode::Profile, l("Цвет профиля", "Profile colour")),
                    (LightbarMode::Beat, l("В такт", "On the beat")),
                    (LightbarMode::Off, l("Выкл", "Off")),
                ],
                accent,
            );
            hint(ui, match s.lightbar {
                LightbarMode::PadFlash => l("Горит цветом профиля и вспыхивает цветом каждого сыгранного пэда.", "Glows in the profile colour and flashes in the colour of every pad you play."),
                LightbarMode::Profile => l("Всегда цвет профиля.", "Always the profile colour."),
                LightbarMode::Beat => l("Мигает на каждую долю, пока играет луп.", "Blinks on every beat while the loop plays."),
                LightbarMode::Off => l("Погашена.", "Dark."),
            });
            value_slider(ui, l("Яркость", "Brightness"), &mut s.lightbar_brightness, 0.05..=1.0, 1.0, None, |v| format!("{:.0}%", v * 100.0));
        });
        section(ui, l("Отдача", "Feel"), |ui| {
            value_slider(ui, l("Сила вибрации", "Rumble strength"), &mut s.rumble, 0.0..=1.0, 1.0, None, |v| if v < 0.01 { l("выкл", "off").to_string() } else { format!("{:.0}%", v * 100.0) });
            value_slider(ui, l("Ход курков L2 / R2 до удара", "L2 / R2 travel before a hit"), &mut s.trigger_point, 0.05..=0.9, 0.3, None, |v| format!("{:.0}%", v * 100.0));
            hint(ui, l("Меньше — курок срабатывает от лёгкого касания, больше — нужно дожать.", "Lower fires on a light touch, higher needs a full squeeze."));
        });
        hint(ui, l(
            "Цветной геймпад — PS5-скин для GamepadViewer, автор CrazyGrape (через проект 8bitdev0x8/ps5-controller-visualizer).",
            "Colour controller art: the PS5 skin for GamepadViewer by CrazyGrape (via 8bitdev0x8/ps5-controller-visualizer).",
        ));
    }

    fn tab_sticks(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        let mut reset = false;
        let s = &mut self.settings.sticks;
        section(ui, l("Что делает каждый стик", "What each stick does"), |ui| {
            hint(ui, l("Стики работают как ручки диджейского пульта и меняют весь звук сразу.", "The sticks work like the knobs of a DJ mixer and shape the whole sound at once."));
            let rows: [(&str, &mut FxKind); 4] = [
                (l("Левый стик ↔", "Left stick ↔"), &mut s.lx),
                (l("Левый стик ↕", "Left stick ↕"), &mut s.ly),
                (l("Правый стик ↔", "Right stick ↔"), &mut s.rx),
                (l("Правый стик ↕", "Right stick ↕"), &mut s.ry),
            ];
            for (n, (label, field)) in rows.into_iter().enumerate() {
                setting_row(ui, label, |ui| {
                    egui::ComboBox::from_id_salt(("fx-axis", n)).selected_text(fx_name(*field)).width(220.0).show_ui(ui, |ui| {
                        for k in FxKind::ALL {
                            ui.selectable_value(field, k, fx_name(k));
                        }
                    });
                });
            }
        });
        section(ui, l("Поведение", "Behaviour"), |ui| {
            segmented(ui, &mut s.mode, &[(StickMode::Spring, l("Пружина", "Spring")), (StickMode::Knob, l("Ручка", "Knob"))], accent);
            hint(ui, match s.mode {
                StickMode::Spring => l("Эффект следует за стиком и исчезает, когда вы его отпускаете.", "The effect follows the stick and goes away when you let go."),
                StickMode::Knob => l("Стик крутит ручку: отклонили — значение поползло, отпустили — осталось.", "The stick turns a knob: push to change the value, let go and it stays."),
            });
            toggle_row(ui, l("Эхо и статтер в такт", "Echo and stutter in time"), l("Длина повторов привязана к темпу.", "Repeat lengths follow the tempo."), &mut s.sync, accent);
            value_slider(ui, l("Мёртвая зона", "Dead zone"), &mut s.deadzone, 0.0..=0.4, 0.12, None, |v| format!("{:.0}%", v * 100.0));
            field_label(ui, l("Диапазон питча, полутонов", "Pitch range, semitones"));
            segmented(ui, &mut s.pitch_range, &[(2.0, "±2"), (7.0, "±7"), (12.0, "±12"), (24.0, "±24")], accent);
            if btn(ui, l("Сбросить эффекты", "Reset the effects")).clicked() {
                reset = true;
            }
        });
        section(ui, l("Тачпад как XY-пад", "Touchpad as an XY pad"), |ui| {
            toggle_row(
                ui,
                l("Водить пальцем — менять эффекты", "Slide a finger to play effects"),
                l("Как Kaoss Pad: влево-вправо один эффект, вверх-вниз другой. Убрали палец — эффект ушёл.", "Like a Kaoss Pad: one effect left to right, another up and down. Lift the finger and it's gone."),
                &mut s.touch_fx,
                accent,
            );
            if s.touch_fx {
                for (n, (label, field)) in [(l("Тачпад ↔", "Touchpad ↔"), &mut s.touch_x), (l("Тачпад ↕", "Touchpad ↕"), &mut s.touch_y)].into_iter().enumerate() {
                    setting_row(ui, label, |ui| {
                        egui::ComboBox::from_id_salt(("fx-touch", n)).selected_text(fx_name(*field)).width(220.0).show_ui(ui, |ui| {
                            for k in FxKind::ALL {
                                ui.selectable_value(field, k, fx_name(k));
                            }
                        });
                    });
                }
                hint(ui, l("Совет: снимите пэды с нажатия тачпада, чтобы они не срабатывали случайно.", "Tip: take the pads off the touchpad click so they don't fire by accident."));
            }
        });
        if reset {
            self.actions.push(Action::ResetFx);
        }
    }

    fn tab_sound(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        let now = if self.engine.is_open() {
            lf("Сейчас: {} · {} кГц", "Now: {} · {} kHz", &[&self.engine.device_name, &format!("{:.1}", self.engine.sample_rate as f32 / 1000.0)])
        } else {
            self.audio_error.clone().unwrap_or_else(|| l("Звук выключен", "Sound is off").to_string())
        };
        let ok = self.engine.is_open();
        section(ui, l("Куда играть", "Output"), |ui| {
            let current = self.settings.audio_device.clone().unwrap_or_else(|| l("Как в Windows (по умолчанию)", "Same as Windows (default)").to_string());
            egui::ComboBox::from_id_salt("audio-device").selected_text(current).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.settings.audio_device, None, l("Как в Windows (по умолчанию)", "Same as Windows (default)"));
                for d in &self.devices {
                    ui.selectable_value(&mut self.settings.audio_device, Some(d.clone()), d);
                }
            });
            ui.label(RichText::new(now).size(12.5).color(if ok { dim() } else { DANGER }));
            ui.horizontal(|ui| {
                if btn(ui, l("Обновить список", "Refresh the list")).clicked() {
                    self.actions.push(Action::RefreshDevices);
                }
                if btn(ui, l("Переподключить звук", "Reconnect the sound")).clicked() {
                    self.actions.push(Action::ReopenAudio);
                }
            });
        });
        section(ui, l("Задержка", "Latency"), |ui| {
            segmented(
                ui,
                &mut self.settings.buffer,
                &[(BufferPref::Auto, l("Авто", "Auto")), (BufferPref::Low, l("Быстрый отклик", "Fast response")), (BufferPref::Safe, l("Без треска", "No crackle"))],
                accent,
            );
            hint(ui, l("Если звук запаздывает — «Быстрый отклик». Если трещит — «Без треска».", "If the sound lags, pick Fast response. If it crackles, pick No crackle."));
        });
        section(ui, l("Громкость и луп", "Volume and loop"), |ui| {
            value_slider(ui, l("Общая громкость", "Master volume"), &mut self.settings.master, 0.0..=1.0, 0.85, None, |v| format!("{:.0}%", v * 100.0));
            toggle_row(ui, l("Метроном", "Metronome"), l("Щелчок на каждую долю, пока играет луп.", "A click on every beat while the loop plays."), &mut self.settings.metronome, accent);
            field_label(ui, l("Пэды-петли вступают", "Loop pads come in"));
            segmented(ui, &mut self.settings.launch_quant, &[(0, l("сразу", "at once")), (4, l("в долю", "on the beat")), (16, l("в такт", "on the bar"))], accent);
        });
    }

    fn tab_profiles(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        hint(ui, l("У каждого профиля свой кит, цвет и огоньки на геймпаде.", "Each profile has its own kit, colour and controller lights."));
        let mut actions = Vec::new();
        let count = self.profiles.len();
        let mut arm = None;
        for (i, p) in self.profiles.iter().enumerate() {
            card_frame().inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 14.0, rgb(p.accent));
                    let initial: String = p.name.chars().take(1).collect();
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, initial, semibold(14.0), on_color(rgb(p.accent)));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(RichText::new(&p.name).font(semibold(15.0)).color(text()));
                        ui.label(RichText::new(&p.tagline).size(12.0).color(dim()));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if count > 1 {
                            let armed = self.delete_armed == Some(i);
                            if btn_danger(ui, if armed { l("Точно удалить?", "Really delete?") } else { l("Удалить", "Delete") }).clicked() {
                                if armed {
                                    actions.push(Action::DeleteProfile(i));
                                } else {
                                    arm = Some(i);
                                }
                            }
                        }
                        if i == self.profile {
                            ui.label(RichText::new(l("играет сейчас", "playing now")).font(semibold(12.5)).color(accent));
                        } else if btn(ui, l("Выбрать", "Use")).clicked() {
                            actions.push(Action::UseProfile(i));
                        }
                    });
                });
            });
        }
        if arm.is_some() {
            self.delete_armed = arm;
        }

        let mut changed = false;
        let mut new_accent = None;
        section(ui, l("Профиль, который играет", "The profile that is playing"), |ui| {
            setting_row(ui, l("Имя", "Name"), |ui| {
                if btn(ui, l("Переименовать", "Rename")).clicked() {
                    actions.push(Action::RenameProfile);
                }
                ui.add(egui::TextEdit::singleline(&mut self.rename_buf).desired_width(240.0));
            });
            let active = &mut self.profiles[self.profile];
            setting_row(ui, l("Девиз", "Tagline"), |ui| {
                if ui.add(egui::TextEdit::singleline(&mut active.tagline).desired_width(340.0)).changed() {
                    changed = true;
                }
            });
            field_label(ui, l("Цвет", "Colour"));
            let mut c = active.accent;
            if color_row(ui, &PROFILE_COLORS, &mut c, l("Любой цвет", "Any colour")) {
                new_accent = Some(c);
            }
        });
        if let Some(c) = new_accent {
            self.profiles[self.profile].accent = c;
            changed = true;
            self.apply_theme();
            self.push_outputs();
        }
        if changed {
            self.profiles_changed();
        }

        section(ui, l("Новый профиль", "New profile"), |ui| {
            setting_row(ui, l("Имя", "Name"), |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.new_name).hint_text(l("Например, Гость", "For example, Guest")).desired_width(240.0));
            });
            setting_row(ui, l("Девиз", "Tagline"), |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.new_tagline).hint_text(l("Готов играть.", "Ready to play.")).desired_width(340.0));
            });
            field_label(ui, l("Цвет", "Colour"));
            color_row(ui, &PROFILE_COLORS, &mut self.new_color, l("Любой цвет", "Any colour"));
            ui.horizontal(|ui| {
                if btn_primary(ui, l("Создать профиль", "Create the profile"), rgb(self.new_color)).clicked() {
                    actions.push(Action::CreateProfile);
                }
                if let Some(e) = &self.profile_error {
                    ui.label(RichText::new(e).color(DANGER));
                }
            });
        });
        hint(ui, l("Удалённый профиль исчезает из списка, а файл его кита остаётся на диске.", "A deleted profile leaves the list; its kit file stays on disk."));
        self.actions.extend(actions);
    }

    fn tab_library(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        let mut actions = Vec::new();
        let installing = self.factory_installing;
        let progress = self.factory_progress.load(Ordering::Relaxed);
        section(ui, l("Звуки SensePad", "SensePad sounds"), |ui| {
            hint(ui, &lf(
                "{} готовых китов и около 140 звуков по категориям. Они создаются на вашем компьютере при первом запуске — это синтез, а не чужие записи.",
                "{} ready-made kits and about 140 sounds in categories. They are created on your computer on first run — synthesis, not someone else's recordings.",
                &[&factory::kits().len()],
            ));
            ui.horizontal(|ui| {
                if installing {
                    ui.spinner();
                    ui.label(RichText::new(lf("Готовлю звуки… {}%", "Preparing the sounds… {}%", &[&progress])).color(dim()));
                } else if btn(ui, l("Создать звуки заново", "Create the sounds again")).clicked() {
                    actions.push(Action::InstallFactory);
                }
            });
        });
        section(ui, "FL Studio", |ui| {
            if self.library.fl_found.is_empty() {
                hint(ui, l(
                    "FL Studio не найдена. Если она установлена, её папка Packs появится в библиотеке сама.",
                    "FL Studio wasn't found. If it is installed, its Packs folder shows up in the library by itself.",
                ));
            } else {
                ui.label(RichText::new(l("Паки FL Studio подключены:", "FL Studio packs are connected:")).color(OK));
                for p in &self.library.fl_found {
                    ui.label(RichText::new(p.display().to_string()).size(12.0).color(dim()));
                }
            }
        });
        section(ui, l("Свои папки со звуками", "Your own sound folders"), |ui| {
            if self.settings.library_folders.is_empty() {
                hint(ui, l("Добавьте папку с сэмплами — она появится в библиотеке рядом со звуками SensePad.", "Add a folder of samples and it appears in the library next to the SensePad sounds."));
            }
            for (i, f) in self.settings.library_folders.iter().enumerate() {
                setting_row(ui, &f.display().to_string(), |ui| {
                    if btn(ui, l("Убрать", "Remove")).clicked() {
                        actions.push(Action::RemoveFolder(i));
                    }
                });
            }
            if icon_label_btn(ui, Icon::Plus, l("Добавить папку", "Add a folder"), Some(accent)).clicked() {
                actions.push(Action::AddFolder);
            }
        });
        self.actions.extend(actions);
    }
}
