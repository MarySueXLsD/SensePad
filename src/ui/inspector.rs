//! The sidebar's "Pad" tab: everything about the selected pad.

use super::widgets::*;
use crate::app::{Action, Learn, SensePadApp, KEY_LABELS};
use crate::i18n::{input_label, l, lf};
use crate::icons::Icon;
use crate::input::Input;
use crate::kit::{PlayMode, PAD_COLORS};
use crate::naming::{self, NOTES};
use crate::theme::*;
use eframe::egui::{self, vec2, Align, Align2, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui};

impl SensePadApp {
    pub(crate) fn inspector(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        segmented(ui, &mut self.inspector_tab, &[(0, l("Пэд", "Pad")), (1, l("Библиотека", "Library"))], accent);
        ui.add_space(sp(8.0));
        if self.inspector_tab == 1 {
            self.library_panel(ui);
        } else {
            self.pad_panel(ui);
        }
    }

    fn pad_panel(&mut self, ui: &mut Ui) {
        let i = self.selected;
        let accent = self.accent();
        let before = self.kit.pads[i].clone();
        let sample = self.samples[i].clone();
        let learning = self.learning == Some(Learn::Pad(i));
        let loading = self.loading[i];
        let error = self.errors[i].clone();
        let time = ui.input(|inp| inp.time) as f32;
        let placeholder = lf("Пэд {}", "Pad {}", &[&(i + 1)]);
        let project_bpm = self.kit.bpm;
        let notes = self.kit.pattern.notes.iter().filter(|n| n.pad as usize == i).count();
        let func_buttons: Vec<Input> = crate::kit::Func::ALL.into_iter().filter_map(|f| self.settings.functions.get(f)).collect();
        let mut actions = Vec::new();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // Padding keeps text off the clipping edges and clear of the scroll bar.
            Frame::new().inner_margin(Margin { left: 2, right: 12, top: 2, bottom: 18 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = sp(10.0);
                let pad = &mut self.kit.pads[i];

                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 6.0, rgb(pad.color));
                    ui.label(RichText::new(lf("Пэд {}", "Pad {}", &[&format!("{:02}", i + 1)])).font(display(15.0)).color(dim()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        keycap(ui, KEY_LABELS[i]);
                        ui.label(RichText::new(l("клавиша", "key")).color(faint()).size(12.0));
                    });
                });
                ui.add(
                    egui::TextEdit::singleline(&mut pad.name)
                        .hint_text(placeholder)
                        .font(display(22.0))
                        .desired_width(f32::INFINITY)
                        .margin(Margin::symmetric(12, 8)),
                );

                section(ui, l("Звук", "Sound"), |ui| {
                    if let Some(s) = &sample {
                        let file = pad.sample.as_ref().and_then(|p| p.file_stem()).map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                        ui.label(RichText::new(file).font(semibold(15.0)).color(text()));
                        let ch = if s.channels > 1 { l("стерео", "stereo") } else { l("моно", "mono") };
                        ui.label(
                            RichText::new(format!("{:.2} {} · {:.1} {} · {ch}", s.duration, l("с", "s"), s.sample_rate as f32 / 1000.0, l("кГц", "kHz")))
                                .size(12.0)
                                .color(dim()),
                        );
                        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::hover());
                        ui.painter().rect_filled(r, cr(8.0), bg());
                        waveform(ui.painter(), r.shrink2(vec2(10.0, 6.0)), &s.peaks, rgb(pad.color), None);
                    } else if loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(l("Загрузка…", "Loading…")).color(dim()));
                        });
                    } else {
                        ui.label(RichText::new(l("Здесь пока пусто", "Nothing here yet")).font(semibold(15.0)).color(text()));
                        hint(ui, l(
                            "Выберите звук из библиотеки или перетащите аудиофайл на пэд. Подойдут WAV, MP3, FLAC, OGG, AIFF и M4A.",
                            "Pick a sound from the library or drop an audio file on the pad. WAV, MP3, FLAC, OGG, AIFF and M4A work.",
                        ));
                    }
                    if let Some(e) = &error {
                        ui.label(RichText::new(lf("Не удалось открыть файл: {}", "Couldn't open the file: {}", &[e])).size(12.0).color(DANGER));
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                        ui.spacing_mut().button_padding.x = sp(10.0);
                        if icon_label_btn(ui, Icon::Wave, l("Выбрать звук", "Pick a sound"), Some(accent)).clicked() {
                            actions.push(Action::ChooseSample(i));
                        }
                        if btn(ui, l("Из файла…", "From a file…")).clicked() {
                            actions.push(Action::Browse(i));
                        }
                        if sample.is_some() {
                            if icon_btn_colored(ui, Icon::Play, 30.0, l("Прослушать", "Listen"), text(), Some(card_hi())).clicked() {
                                actions.push(Action::Preview(i));
                            }
                            if icon_btn_colored(ui, Icon::Trash, 30.0, l("Очистить пэд", "Clear the pad"), DANGER, Some(card_hi())).clicked() {
                                actions.push(Action::Clear(i));
                            }
                        }
                    });
                });

                section(ui, l("Кнопка геймпада", "Controller button"), |ui| {
                    let w = ui.available_width();
                    let (r, resp) = ui.allocate_exact_size(vec2(w, 58.0), Sense::click());
                    let p = ui.painter();
                    p.rect_filled(r, cr(12.0), if resp.hovered() { card_hi() } else { bg() });
                    if learning {
                        let pulse = 0.5 + 0.5 * (time * 5.0).sin();
                        p.rect_stroke(r, cr(12.0), Stroke::new(1.5 + pulse, accent), StrokeKind::Inside);
                        p.text(r.left_center() + vec2(16.0, -9.0), Align2::LEFT_CENTER, l("Нажмите кнопку на геймпаде…", "Press a controller button…"), semibold(14.0), text());
                        p.text(r.left_center() + vec2(16.0, 11.0), Align2::LEFT_CENTER, l("Esc — отмена", "Esc cancels"), FontId::proportional(12.0), dim());
                    } else {
                        p.rect_stroke(r, cr(12.0), Stroke::new(1.0, border()), StrokeKind::Inside);
                        match pad.binding {
                            Some(b) => {
                                badge(p, Rect::from_center_size(r.left_center() + vec2(34.0, 0.0), vec2(40.0, 34.0)), b, card_hi());
                                p.text(r.left_center() + vec2(66.0, -9.0), Align2::LEFT_CENTER, input_label(b), semibold(15.0), text());
                                p.text(r.left_center() + vec2(66.0, 11.0), Align2::LEFT_CENTER, l("Кликните, чтобы сменить", "Click to change"), FontId::proportional(12.0), dim());
                            }
                            None => {
                                p.text(r.left_center() + vec2(16.0, -9.0), Align2::LEFT_CENTER, l("Кнопка не назначена", "No button yet"), semibold(15.0), text());
                                p.text(r.left_center() + vec2(16.0, 11.0), Align2::LEFT_CENTER, l("Кликните и нажмите любую кнопку", "Click, then press any button"), FontId::proportional(12.0), dim());
                            }
                        }
                    }
                    if resp.clicked() {
                        actions.push(if learning { Action::CancelLearn } else { Action::Learn(Learn::Pad(i)) });
                    }
                    ui.horizontal(|ui| {
                        let unbind_w = if pad.binding.is_some() { 110.0 } else { 0.0 };
                        let combo_w = (ui.available_width() - unbind_w).max(140.0);
                        egui::ComboBox::from_id_salt("bind-list").selected_text(l("Выбрать из списка", "Pick from a list")).width(combo_w).show_ui(ui, |ui| {
                            for input in Input::ALL.into_iter().filter(|x| x.bindable()) {
                                let mut label = input_label(input).to_string();
                                if func_buttons.contains(&input) {
                                    label.push_str(l("  (служебная)", "  (function)"));
                                }
                                if ui.selectable_label(pad.binding == Some(input), label).clicked() {
                                    actions.push(Action::Bind(i, Some(input)));
                                }
                            }
                        });
                        if pad.binding.is_some() && btn(ui, l("Снять", "Remove")).clicked() {
                            actions.push(Action::Bind(i, None));
                        }
                    });
                });

                section(ui, l("Как играет", "How it plays"), |ui| {
                    segmented(
                        ui,
                        &mut pad.mode,
                        &[(PlayMode::OneShot, l("Удар", "One-shot")), (PlayMode::Hold, l("Пока держу", "While held")), (PlayMode::Loop, l("Петля", "Loop"))],
                        accent,
                    );
                    hint(ui, match pad.mode {
                        PlayMode::OneShot => l("Каждое нажатие проигрывает звук до конца.", "Every press plays the sound to the end."),
                        PlayMode::Hold => l("Звучит, пока кнопка зажата. Хорошо для баса.", "Sounds while the button is held. Good for bass."),
                        PlayMode::Loop => l("Нажали — играет по кругу, нажали ещё раз — замолчал.", "Press to loop, press again to stop."),
                    });
                    let mut sync = pad.sync_bpm > 0.0;
                    if toggle_row(
                        ui,
                        l("Следовать темпу", "Follow the tempo"),
                        l("Для брейков и лупов: звук ускоряется и замедляется вместе с BPM.", "For breaks and loops: the sound speeds up and slows down with the BPM."),
                        &mut sync,
                        accent,
                    ) {
                        pad.sync_bpm = if sync {
                            pad.sample.as_deref().and_then(|s| naming::bpm_in_name(&naming::clean_name(s))).unwrap_or(project_bpm)
                        } else {
                            0.0
                        };
                    }
                    if pad.sync_bpm > 0.0 {
                        setting_row(ui, l("Родной темп звука", "The sound's own tempo"), |ui| {
                            ui.add(egui::DragValue::new(&mut pad.sync_bpm).range(60.0..=220.0).speed(0.25).fixed_decimals(0).suffix(" BPM"));
                        });
                    }
                    ui.separator();
                    field_label(ui, l("Группа глушения", "Choke group"));
                    segmented(ui, &mut pad.choke, &[(0, l("Нет", "None")), (1, "1"), (2, "2"), (3, "3"), (4, "4")], accent);
                    hint(ui, l("Пэды одной группы обрывают друг друга — как открытый и закрытый хэт.", "Pads in a group cut each other off, like open and closed hats."));
                    ui.separator();
                    toggle_row(ui, l("Наложение ударов", "Let hits overlap"), l("Новый удар не обрывает предыдущий.", "A new hit doesn't cut the previous one."), &mut pad.overlap, accent);
                    toggle_row(ui, l("Вибрация при ударе", "Rumble on hit"), "", &mut pad.rumble, accent);
                });

                section(ui, l("Звучание", "Shape"), |ui| {
                    value_slider(ui, l("Громкость", "Volume"), &mut pad.volume, 0.0..=1.5, 0.9, None, |v| format!("{:.0}%", v * 100.0));
                    let st = l("пт", "st");
                    let base_note = pad.sample.as_deref().map(naming::clean_name).and_then(|n| naming::note_of(&n));
                    value_slider(ui, l("Высота", "Pitch"), &mut pad.pitch, -24.0..=24.0, 0.0, Some(1.0), move |v| match base_note {
                        Some(n) => format!("{v:+.0} {st} · {}", NOTES[(n + v.round() as i32).rem_euclid(12) as usize]),
                        None => format!("{v:+.0} {st}"),
                    });
                    let (pc, pl, pr) = (l("центр", "centre"), l("лево", "left"), l("право", "right"));
                    value_slider(ui, l("Панорама", "Pan"), &mut pad.pan, -1.0..=1.0, 0.0, None, move |v| {
                        if v.abs() < 0.02 {
                            pc.to_string()
                        } else if v < 0.0 {
                            format!("{pl} {:.0}", -v * 100.0)
                        } else {
                            format!("{pr} {:.0}", v * 100.0)
                        }
                    });
                    hint(ui, l("Двойной клик по значению возвращает его к исходному.", "Double-click a value to reset it."));
                });

                section(ui, l("Цвет", "Colour"), |ui| {
                    color_row(ui, &PAD_COLORS, &mut pad.color, l("Любой цвет", "Any colour"));
                });

                if notes > 0 {
                    section(ui, l("В лупе", "In the loop"), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(lf("Нот этого пэда: {}", "Notes of this pad: {}", &[&notes])).color(text()));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if btn(ui, l("Убрать", "Remove")).clicked() {
                                    actions.push(Action::ClearPadNotes(i));
                                }
                            });
                        });
                    });
                }
            });
        });

        {
            let pad = &mut self.kit.pads[i];
            if pad.pitch != before.pitch {
                if let Some(s) = pad.sample.clone() {
                    if naming::is_auto_name(&pad.name, Some(&s), before.pitch, i) {
                        pad.name = naming::auto_name(&s, pad.pitch);
                    }
                }
            }
        }
        if self.kit.pads[i] != before {
            self.sync_pad(i);
        }
        self.actions.extend(actions);
    }
}
