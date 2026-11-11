//! The sidebar's "Library" tab: factory kits and sounds, sample packs, FL Studio packs, your folders.

use super::widgets::*;
use crate::app::{Action, SensePadApp};
use crate::factory;
use crate::i18n::{category_label, l, lf};
use crate::icons::{self, Icon};
use crate::kit;
use crate::library::{self, RootKind};
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, RichText, Sense, Ui};
use std::sync::atomic::Ordering;

impl SensePadApp {
    pub(crate) fn library_panel(&mut self, ui: &mut Ui) {
        let accent = self.accent();
        let installing = self.factory_installing;
        let progress = self.factory_progress.load(Ordering::Relaxed);
        let pad_no = self.selected + 1;
        let current_kit = self.kit.factory.clone();
        let mut actions = Vec::new();
        let lib = &mut self.library;
        let mut fl_count = 0;
        let labels: Vec<String> = lib
            .roots
            .iter()
            .map(|r| match r.kind {
                RootKind::Factory => l("Звуки SensePad", "SensePad sounds").to_string(),
                RootKind::Pack => format!("{} · {}", l("Пак", "Pack"), r.label),
                RootKind::FlStudio => {
                    fl_count += 1;
                    if fl_count > 1 {
                        format!("FL Studio {fl_count}")
                    } else {
                        l("Паки FL Studio", "FL Studio packs").to_string()
                    }
                }
                RootKind::User => r.label.clone(),
            })
            .collect();
        let current = labels.get(lib.root).cloned().unwrap_or_default();
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("lib-root").selected_text(current).width(ui.available_width() - 46.0).show_ui(ui, |ui| {
                for (i, label) in labels.iter().enumerate() {
                    if ui.selectable_label(i == lib.root, label).clicked() {
                        actions.push(Action::LibRoot(i));
                    }
                }
            });
            if icon_btn(ui, Icon::Plus, 30.0, l("Добавить свою папку со звуками", "Add your own folder of sounds")).clicked() {
                actions.push(Action::AddFolder);
            }
        });

        let at_factory = lib.roots.get(lib.root).is_some_and(|r| r.kind == RootKind::Factory);
        let at_root = lib.root_path().is_some_and(|r| r == lib.dir);
        if at_factory && installing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(lf("Готовлю звуки… {}%", "Preparing the sounds… {}%", &[&progress])).color(dim()));
            });
        } else if at_factory && lib.entries.is_empty() && lib.dir == kit::factory_dir() {
            if btn_primary(ui, l("Создать звуки заново", "Create the sounds again"), accent).clicked() {
                actions.push(Action::InstallFactory);
            }
        }

        if !lib.categories.is_empty() {
            let current_cat = lib.current_category().cloned();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                if at_factory && chip(ui, l("Киты", "Kits"), at_root, accent).clicked() {
                    actions.push(Action::LibRoot(lib.root));
                }
                for c in &lib.categories {
                    let sel = current_cat.as_ref() == Some(c);
                    if chip(ui, &category_label(&library::name_of(c)), sel, accent).clicked() {
                        actions.push(Action::LibOpen(c.clone()));
                    }
                }
            });
        }

        let footer_h = 78.0;
        if at_factory && at_root && !installing {
            // The front page of the factory library: ready-made kits.
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(4.0);
                hint(ui, l("Кит — это 16 звуков, темп и демо-бит. Кнопки геймпада при смене кита остаются вашими.", "A kit is 16 sounds, a tempo and a demo beat. Your controller buttons stay as you set them."));
                for spec in factory::kits() {
                    let sel = current_kit.as_deref() == Some(spec.id);
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 10.0, 62.0), Sense::click());
                    let p = ui.painter();
                    let col = rgb(spec.accent);
                    let fill = if sel { mix(card(), col, 0.2) } else if resp.hovered() { card_hi() } else { card() };
                    p.rect(r, cr(12.0), fill, egui::Stroke::new(if sel { 1.5 } else { 1.0 }, if sel { col } else { border() }), egui::StrokeKind::Inside);
                    p.rect_filled(Rect::from_min_size(r.min + vec2(0.0, 12.0), vec2(4.0, r.height() - 24.0)), egui::CornerRadius::same(2), col);
                    p.text(pos2(r.left() + 16.0, r.top() + 19.0), Align2::LEFT_CENTER, spec.title(), display(17.0), text());
                    p.text(pos2(r.right() - 12.0, r.top() + 19.0), Align2::RIGHT_CENTER, format!("{} BPM", spec.bpm as i32), display(13.0), dim());
                    clipped(p, pos2(r.left() + 16.0, r.top() + 43.0), Align2::LEFT_CENTER, spec.about(), FontId::proportional(12.0), dim(), r.width() - 28.0);
                    if resp.on_hover_text(spec.about()).clicked() {
                        actions.push(Action::LoadFactoryKit(spec.id.to_string()));
                    }
                }
            });
            self.actions.extend(actions);
            return;
        }

        if at_root && lib.roots.get(lib.root).is_some_and(|r| r.kind == RootKind::Pack) {
            hint(ui, l(
                "Живые записи барабанов и перкуссии. Общественное достояние (CC0); кто записал — в файле CREDITS.txt в папке пака.",
                "Live drum and percussion recordings. Public domain (CC0); who recorded them is in CREDITS.txt in the pack folder.",
            ));
        }
        ui.horizontal(|ui| {
            if lib.can_go_up() && icon_btn(ui, Icon::ChevronLeft, 26.0, l("Назад", "Back")).clicked() {
                actions.push(Action::LibUp);
            }
            let here = category_label(&library::name_of(&lib.dir));
            ui.label(RichText::new(here).font(display(17.0)).color(text()));
        });
        ui.add(egui::TextEdit::singleline(&mut lib.search).hint_text(l("Поиск по названию…", "Search by name…")).desired_width(f32::INFINITY));

        let q = lib.search.to_lowercase();
        let list_h = (ui.available_height() - footer_h).max(120.0);
        egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width() - 10.0;
            let mut shown = 0;
            for e in lib.entries.iter().filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q)) {
                shown += 1;
                let (r, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
                let selected = lib.selected.as_ref() == Some(&e.path);
                let p = ui.painter();
                if selected {
                    p.rect_filled(r, cr(8.0), alpha(accent, 0.2));
                } else if resp.hovered() {
                    p.rect_filled(r, cr(8.0), card_hi());
                }
                let icon_r = Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(14.0, 14.0));
                icons::paint(p, icon_r, if e.is_dir { Icon::Folder } else { Icon::Wave }, if e.is_dir { accent } else { dim() });
                let display_name = if e.is_dir {
                    category_label(&e.name)
                } else {
                    e.path.file_stem().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| e.name.clone())
                };
                clipped(p, pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, &display_name, FontId::proportional(13.5), text(), r.width() - 56.0);
                if e.is_dir {
                    icons::paint(p, Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(10.0, 10.0)), Icon::ChevronRight, faint());
                    if resp.clicked() {
                        actions.push(Action::LibOpen(e.path.clone()));
                    }
                } else if resp.double_clicked() {
                    actions.push(Action::LibAssign(e.path.clone()));
                } else if resp.clicked() {
                    actions.push(Action::LibPreview(e.path.clone()));
                }
            }
            if shown == 0 && !installing {
                ui.add_space(8.0);
                hint(ui, if q.is_empty() { l("В этой папке нет звуков.", "No sounds in this folder.") } else { l("Ничего не нашлось.", "Nothing found.") });
            }
        });

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            if let Some(sel) = lib.selected.clone().filter(|s| s.parent() == Some(lib.dir.as_path())) {
                let stem = sel.file_stem().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                let short: String = if stem.chars().count() > 18 { stem.chars().take(17).chain(['…']).collect() } else { stem };
                if btn_primary(ui, &lf("«{}» → пэд {}", "“{}” → pad {}", &[&short, &pad_no]), accent).clicked() {
                    actions.push(Action::LibAssign(sel));
                }
            }
            if lib.entries.iter().any(|e| !e.is_dir) && btn(ui, l("Вся папка → на пэды", "Whole folder → pads")).clicked() {
                actions.push(Action::LibLoadFolder(lib.dir.clone()));
            }
        });
        hint(ui, l("Клик — прослушать. Двойной клик — поставить на выбранный пэд.", "Click to listen. Double-click to put it on the selected pad."));
        self.actions.extend(actions);
    }
}
