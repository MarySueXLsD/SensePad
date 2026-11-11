//! The guide: how to play, explained in short sections.

use super::settings::{nav_item, page_frame, page_size, split_page};
use super::widgets::*;
use crate::app::{func_name, Action, Page, SensePadApp};
use crate::i18n::{input_label, l};
use crate::icons::Icon;
use crate::kit::Func;
use crate::theme::*;
use eframe::egui::{self, vec2, Align, Color32, Frame, Id, Layout, Margin, RichText, Sense, Ui, UiBuilder};

fn h(ui: &mut Ui, s: &str) {
    ui.add_space(sp(4.0));
    ui.label(RichText::new(s).font(display(19.0)).color(text()));
}

fn par(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).size(14.0).color(text()));
}

fn note(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).size(13.0).color(dim()));
}

fn bullet(ui: &mut Ui, s: &str) {
    ui.horizontal_top(|ui| {
        ui.label(RichText::new("•").size(14.0).color(faint()));
        ui.label(RichText::new(s).size(14.0).color(text()));
    });
}

fn keys(ui: &mut Ui, caps: &[&str], what: &str) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(150.0, 24.0), Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for c in caps {
                keycap(ui, c);
            }
        });
        ui.label(RichText::new(what).size(14.0).color(text()));
    });
}

impl SensePadApp {
    pub(crate) fn help_page(&mut self, ctx: &egui::Context) {
        let modal = egui::Modal::new(Id::unique("sensepad-help")).frame(page_frame()).backdrop_color(Color32::from_black_alpha(150));
        let resp = modal.show(ctx, |ui| self.help_body(ui));
        if resp.inner || resp.should_close() {
            self.actions.push(Action::ClosePage);
        }
    }

    fn help_body(&mut self, ui: &mut Ui) -> bool {
        let accent = self.accent();
        let size = page_size(ui.ctx(), 900.0, 620.0);
        let mut close = false;
        ui.set_width(size.x);
        ui.horizontal(|ui| {
            ui.label(RichText::new(l("Инструкция", "How to play")).font(display(26.0)).color(text()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_btn(ui, Icon::Close, 30.0, l("Закрыть · Esc", "Close · Esc")).clicked() {
                    close = true;
                }
                if btn(ui, l("Знакомство заново", "Replay the tour")).clicked() {
                    self.welcome_step = 0;
                    self.actions.push(Action::Open(Page::Welcome));
                }
            });
        });
        ui.add_space(sp(8.0));
        let topics = [
            (Icon::Sparkle, l("Быстрый старт", "Quick start")),
            (Icon::Grid, l("Пэды и звуки", "Pads and sounds")),
            (Icon::Gamepad, l("Кнопки геймпада", "Controller buttons")),
            (Icon::Record, l("Луп и запись", "Looper and recording")),
            (Icon::Bolt, l("Стики и эффекты", "Sticks and effects")),
            (Icon::Folder, l("Библиотека", "Library")),
            (Icon::User, l("Киты и профили", "Kits and profiles")),
            (Icon::Target, l("Клавиатура", "Keyboard")),
            (Icon::Help, l("Если что-то не так", "Troubleshooting")),
        ];
        let (area, _) = ui.allocate_exact_size(size, Sense::hover());
        let (nav_rect, body_rect) = split_page(area, 200.0);
        {
            ui.scope_builder(UiBuilder::new().max_rect(nav_rect).layout(Layout::top_down(Align::Min)), |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                for (i, (icon, label)) in topics.iter().enumerate() {
                    if nav_item(ui, *icon, label, self.help_topic == i, accent) {
                        self.help_topic = i;
                    }
                }
            });
            ui.scope_builder(UiBuilder::new().max_rect(body_rect).layout(Layout::top_down(Align::Min)), |ui| {
                egui::ScrollArea::vertical().max_height(size.y).auto_shrink([false, false]).show(ui, |ui| {
                    Frame::new().inner_margin(Margin { left: 2, right: 16, top: 0, bottom: 10 }).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = sp(8.0);
                        self.help_topic_body(ui);
                    });
                });
            });
        }
        close
    }

    fn help_topic_body(&mut self, ui: &mut Ui) {
        let f = self.settings.functions;
        let button = |func: Func| f.get(func).map(input_label).unwrap_or_else(|| l("не назначена", "not assigned"));
        match self.help_topic {
            0 => {
                h(ui, l("Сыграть первый бит за минуту", "Play your first beat in a minute"));
                bullet(ui, l("Подключите DualSense кабелем или по Bluetooth. Вверху появится зелёная метка.", "Connect the DualSense by cable or Bluetooth. A green tag appears at the top."));
                bullet(ui, l("Выберите кит в меню с названием кита вверху. Нажмите «Демо», чтобы услышать, на что он способен.", "Pick a kit in the menu with the kit's name at the top. Press Demo to hear what it can do."));
                bullet(ui, l("Жмите кнопки геймпада: каждая играет свой пэд. Пэд на экране вспыхивает, когда звучит.", "Press the controller buttons: each plays its own pad. The pad on screen flashes when it sounds."));
                bullet(ui, l("Покачайте стики — звук фильтруется, дробится и уезжает по высоте.", "Move the sticks — the sound gets filtered, chopped and bent."));
                bullet(ui, l("Нажмите запись и сыграйте — ваш бит зациклится. Добавляйте слои.", "Press record and play — your beat loops. Keep adding layers."));
                note(ui, l("Нет геймпада под рукой? Всё то же самое работает с клавиатуры и мыши.", "No controller at hand? All of it works from the keyboard and mouse."));
            }
            1 => {
                h(ui, l("Шестнадцать пэдов", "Sixteen pads"));
                par(ui, l("Пэд — это один звук и кнопка, которая его играет. Кликните по пэду, чтобы выбрать его: справа откроются все его настройки.", "A pad is one sound and the button that plays it. Click a pad to select it: all its settings open on the right."));
                h(ui, l("Как положить звук на пэд", "Putting a sound on a pad"));
                bullet(ui, l("«Выбрать звук» открывает библиотеку сразу в нужной категории.", "Pick a sound opens the library in the right category."));
                bullet(ui, l("Перетащите аудиофайл из Проводника прямо на пэд. Несколько файлов займут пустые пэды.", "Drag an audio file from Explorer straight onto a pad. Several files fill the empty pads."));
                bullet(ui, l("Правый клик по пэду — быстрое меню.", "Right-click a pad for a quick menu."));
                h(ui, l("Как пэд играет", "How a pad plays"));
                bullet(ui, l("Удар — звук играет до конца. Для барабанов.", "One-shot — the sound plays to the end. For drums."));
                bullet(ui, l("Пока держу — звучит, пока кнопка зажата. Для баса и пэдов.", "While held — sounds as long as you hold the button. For bass and pads."));
                bullet(ui, l("Петля — нажали, играет по кругу; нажали ещё раз, замолчал. Для брейков.", "Loop — press to loop, press again to stop. For breaks."));
                par(ui, l("Группа глушения нужна, чтобы один звук обрывал другой: закрытый хэт гасит открытый, новая нота баса — предыдущую.", "A choke group makes one sound cut another: a closed hat silences the open one, a new bass note cuts the last."));
                par(ui, l("«Следовать темпу» растягивает брейк или луп под текущий BPM. У звуков с темпом в названии это включается само.", "Follow the tempo stretches a break or loop to the current BPM. Sounds with a tempo in their name switch it on by themselves."));
            }
            2 => {
                h(ui, l("Назначить кнопку", "Assigning a button"));
                bullet(ui, l("Выберите пэд, кликните по карточке «Кнопка геймпада» и нажмите любую кнопку.", "Select a pad, click the Controller button card and press any button."));
                bullet(ui, l("Или кликните по кнопке прямо на картинке геймпада внизу.", "Or click the button right on the controller picture at the bottom."));
                bullet(ui, l("Колонки по бокам картинки показывают, что играет каждая кнопка. Shift+клик отдаёт кнопку выбранному пэду.", "The columns beside the picture show what every button plays. Shift+click gives the button to the selected pad."));
                par(ui, l("Одна кнопка — один пэд: если кнопка уже занята, она переедет на новый пэд.", "One button, one pad: a button that is already taken moves to the new pad."));
                h(ui, l("Служебные кнопки", "Function buttons"));
                bullet(ui, &format!("{} — {}", button(Func::Play), func_name(Func::Play)));
                bullet(ui, &format!("{} — {}", button(Func::Record), func_name(Func::Record)));
                bullet(ui, &format!("{} — {}", button(Func::StopAll), func_name(Func::StopAll)));
                bullet(ui, &format!("{} — {}", button(Func::Tap), func_name(Func::Tap)));
                note(ui, l("Меняются в Настройках → Геймпад. Кнопку PS занимают Windows и Steam, поэтому она не используется.", "Change them in Settings → Controller. Windows and Steam keep the PS button, so it isn't used."));
                par(ui, l("Курки L2 и R2 срабатывают как кнопки. Насколько их нужно нажать, настраивается там же.", "The L2 and R2 triggers act as buttons. How far they must travel is set in the same place."));
            }
            3 => {
                h(ui, l("Луп — ваш барабанщик", "The looper is your drummer"));
                par(ui, l("Под верхней панелью живёт луп: темп, старт, запись. Он повторяет бит по кругу, пока вы играете поверх.", "Under the top bar lives the looper: tempo, play, record. It repeats a beat while you play over it."));
                bullet(ui, l("▶ запускает и останавливает. Четыре огонька показывают долю такта.", "▶ starts and stops. Four lamps show the beat of the bar."));
                bullet(ui, l("● включает запись. Сыгранные удары встают на ближайшую шестнадцатую и остаются в лупе.", "● turns recording on. The hits you play snap to the nearest 16th and stay in the loop."));
                bullet(ui, l("Зажатые ноты (бас) запоминаются вместе с длиной.", "Held notes (bass) are remembered with their length."));
                bullet(ui, l("«Такты» задают длину: 1, 2 или 4. При удлинении бит повторяется.", "Bars sets the length: 1, 2 or 4. Making it longer repeats the beat."));
                bullet(ui, l("«Свинг» оттягивает слабые шестнадцатые — бит начинает качать.", "Swing delays the off 16ths — the beat starts to groove."));
                bullet(ui, l("Корзина очищает луп. «Демо» возвращает демо-бит кита.", "The bin clears the loop. Demo brings back the kit's demo beat."));
                h(ui, l("Шаги", "Steps"));
                par(ui, l("Полоса под пэдами показывает шаги выбранного пэда. Клик по клетке ставит или убирает удар. Чёрточки сверху — удары остальных пэдов.", "The strip under the pads shows the steps of the selected pad. Click a cell to add or remove a hit. The ticks above are the other pads' hits."));
                h(ui, l("Петли вступают вовремя", "Loops come in on time"));
                par(ui, l("Пока луп играет, пэд в режиме «Петля» может подождать следующей доли или такта. Пэд мигает, пока ждёт.", "While the looper plays, a Loop pad can wait for the next beat or bar. The pad blinks while it waits."));
            }
            4 => {
                h(ui, l("Стики — ручки диджейского пульта", "The sticks are DJ mixer knobs"));
                par(ui, l("Эффекты действуют на весь звук сразу. По умолчанию:", "Effects act on the whole sound at once. By default:"));
                bullet(ui, l("Левый стик ↔ — фильтр: влево глушит верха, вправо срезает низы.", "Left stick ↔ — filter: left dulls the highs, right cuts the lows."));
                bullet(ui, l("Левый стик ↕ — эхо.", "Left stick ↕ — echo."));
                bullet(ui, l("Правый стик ↔ — питч: звук уезжает вверх или вниз.", "Right stick ↔ — pitch: the sound bends up or down."));
                bullet(ui, l("Правый стик ↕ — статтер: зацикливает последний кусочек звука.", "Right stick ↕ — stutter: loops the last little piece of sound."));
                par(ui, l("Ещё есть биткраш, приглушение и панорама. Что на каком стике — решаете вы: Настройки → Стики и эффекты.", "There are also bitcrush, volume cut and pan. You decide what goes on which stick: Settings → Sticks & effects."));
                h(ui, l("Пружина и ручка", "Spring and knob"));
                bullet(ui, l("Пружина: эффект есть, пока стик отклонён.", "Spring: the effect lasts while the stick is pushed."));
                bullet(ui, l("Ручка: стик двигает значение, и оно остаётся. Полосы эффектов можно тянуть мышью.", "Knob: the stick moves a value and it stays. The effect bars can be dragged with the mouse."));
                h(ui, l("Тачпад", "Touchpad"));
                par(ui, l("Тачпад можно превратить в XY-пад: водите пальцем — меняются два эффекта сразу. Включается там же, в настройках стиков.", "The touchpad can become an XY pad: slide a finger and two effects change at once. It is switched on in the same stick settings."));
            }
            5 => {
                h(ui, l("Звуки SensePad", "SensePad sounds"));
                par(ui, l("Во вкладке «Библиотека» лежат готовые киты и около 140 звуков по категориям: бочки, снейры, хэты, брейки, лупы, басы, стэбы, клавиши, эффекты, вокал.", "The Library tab holds ready-made kits and about 140 sounds in categories: kicks, snares, hats, breaks, loops, basses, stabs, keys, effects, vox."));
                bullet(ui, l("Клик по звуку — прослушать.", "Click a sound to hear it."));
                bullet(ui, l("Двойной клик или кнопка внизу — поставить на выбранный пэд.", "Double-click, or the button below, puts it on the selected pad."));
                bullet(ui, l("«Вся папка → на пэды» раскладывает до 16 звуков по порядку.", "Whole folder → pads lays out up to 16 sounds in order."));
                note(ui, l("Эти звуки синтезированы самой программой. Знаменитые брейки воссозданы по рисунку, это не оригинальные записи.", "These sounds are synthesised by the app. The famous breaks are recreated from their patterns; they are not the original recordings."));
                h(ui, l("Свои звуки", "Your own sounds"));
                bullet(ui, l("Плюс рядом со списком добавляет вашу папку с сэмплами.", "The plus next to the list adds your own folder of samples."));
                bullet(ui, l("Если установлена FL Studio, её паки появятся в списке сами.", "If FL Studio is installed, its packs show up in the list by themselves."));
                if !crate::library::packs().is_empty() {
                    bullet(ui, l(
                        "В списке источников есть паки живых барабанов и перкуссии. Они лежат в папке packs рядом с программой; свои паки кладите туда же.",
                        "The list of sources has packs of live drums and percussion. They live in the packs folder next to the program; put your own packs there too.",
                    ));
                }
                bullet(ui, l("Подходят WAV, MP3, FLAC, OGG, AIFF и M4A.", "WAV, MP3, FLAC, OGG, AIFF and M4A work."));
            }
            6 => {
                h(ui, l("Киты", "Kits"));
                par(ui, l("Кит — это 16 пэдов, темп и луп. Меню с названием кита вверху переключает готовые киты и ваши сохранённые.", "A kit is 16 pads, a tempo and a loop. The menu with the kit's name at the top switches between factory kits and your saved ones."));
                bullet(ui, l("Всё, что вы меняете, сохраняется само.", "Everything you change is saved automatically."));
                bullet(ui, l("«Сохранить в Мои киты» кладёт копию в список, чтобы к ней вернуться.", "Save to My kits puts a copy in the list so you can come back to it."));
                bullet(ui, l("Экспорт и импорт — чтобы поделиться китом файлом.", "Export and import let you share a kit as a file."));
                bullet(ui, l("При смене готового кита ваши кнопки остаются на своих пэдах.", "When you switch factory kits, your buttons stay on their pads."));
                h(ui, l("Профили", "Profiles"));
                par(ui, l("У каждого игрока свой профиль: свой кит, цвет и огоньки на геймпаде. Переключаются вверху слева, добавляются плюсом.", "Each player has a profile: their own kit, colour and controller lights. Switch them at the top left; the plus adds one."));
            }
            7 => {
                h(ui, l("Играть с клавиатуры", "Playing from the keyboard"));
                keys(ui, &["1", "2", "3", "4"], l("пэды 1–4", "pads 1–4"));
                keys(ui, &["Q", "W", "E", "R"], l("пэды 5–8", "pads 5–8"));
                keys(ui, &["A", "S", "D", "F"], l("пэды 9–12", "pads 9–12"));
                keys(ui, &["Z", "X", "C", "V"], l("пэды 13–16", "pads 13–16"));
                h(ui, l("Луп и окно", "Looper and window"));
                keys(ui, &[l("Пробел", "Space")], l("старт и стоп лупа", "play and stop the loop"));
                keys(ui, &["Enter"], l("запись", "record"));
                keys(ui, &["Backspace"], l("остановить всё", "stop everything"));
                keys(ui, &["F1"], l("эта инструкция", "this guide"));
                keys(ui, &["Esc"], l("отмена и закрытие окон", "cancel and close pages"));
                note(ui, l("Мышью тоже можно: клик по пэду играет его.", "The mouse works too: clicking a pad plays it."));
            }
            _ => {
                h(ui, l("Геймпад не находится", "The controller isn't found"));
                bullet(ui, l("Закройте Steam, DS4Windows и DualSenseX. Они забирают геймпад и прячут его от других программ.", "Close Steam, DS4Windows and DualSenseX. They take the controller and hide it from other apps."));
                bullet(ui, l("По Bluetooth: зажмите Create и PS, пока световая панель не замигает, и добавьте устройство в Windows.", "Over Bluetooth: hold Create and PS until the light bar blinks, then add the device in Windows."));
                bullet(ui, l("Попробуйте другой кабель: некоторые умеют только заряжать.", "Try another cable: some can only charge."));
                h(ui, l("Нет звука", "No sound"));
                bullet(ui, l("Настройки → Звук: выберите устройство и нажмите «Переподключить звук».", "Settings → Sound: choose the device and press Reconnect the sound."));
                bullet(ui, l("Проверьте общую громкость вверху справа.", "Check the master volume at the top right."));
                bullet(ui, l("Правый стик мог остаться в режиме «Ручка» на приглушении — сбросьте эффекты.", "A stick in Knob mode may have left the volume cut on — reset the effects."));
                h(ui, l("Звук запаздывает или трещит", "The sound lags or crackles"));
                bullet(ui, l("Настройки → Звук → Задержка. «Быстрый отклик» убирает запаздывание, «Без треска» — щелчки.", "Settings → Sound → Latency. Fast response removes lag, No crackle removes clicks."));
                bullet(ui, l("Наушники по Bluetooth всегда запаздывают. С проводными играть приятнее.", "Bluetooth headphones always lag. Wired ones feel better."));
                h(ui, l("Пэд пишет «Файл не найден»", "A pad says File not found"));
                bullet(ui, l("Файл переместили или удалили. Выберите звук заново.", "The file was moved or deleted. Pick the sound again."));
                bullet(ui, l("Для звуков SensePad: Настройки → Библиотека → «Создать звуки заново».", "For SensePad sounds: Settings → Library → Create the sounds again."));
            }
        }
    }
}
