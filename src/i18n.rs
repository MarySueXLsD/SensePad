//! Interface text in Russian and English. Strings live next to the code that shows them:
//! `l("Громкость", "Volume")` returns the one for the current language.

use crate::input::Input;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    /// The language a fresh install starts in: Russian when Windows is shown in Russian,
    /// English for everyone else. It can be changed in the welcome tour and in Settings.
    pub fn system() -> Lang {
        #[cfg(windows)]
        {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetUserDefaultUILanguage() -> u16;
            }
            // The low ten bits name the language; 0x19 is Russian.
            if unsafe { GetUserDefaultUILanguage() } & 0x3ff == 0x19 {
                return Lang::Ru;
            }
        }
        Lang::En
    }
}

impl Default for Lang {
    fn default() -> Self {
        Lang::system()
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 0 { Lang::Ru } else { Lang::En }
}

/// Pick the text for the current language: Russian is written first, English second.
pub fn l(ru: &'static str, en: &'static str) -> &'static str {
    match lang() {
        Lang::Ru => ru,
        Lang::En => en,
    }
}

/// Replace `{}` placeholders in order.
pub fn fill(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut parts = template.split("{}");
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    for (i, part) in parts.enumerate() {
        if let Some(a) = args.get(i) {
            out.push_str(&a.to_string());
        }
        out.push_str(part);
    }
    out
}

/// `l` + `fill` in one call.
pub fn lf(ru: &'static str, en: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    fill(l(ru, en), args)
}

pub fn input_label(input: Input) -> &'static str {
    match input {
        Input::Cross => l("Крест", "Cross"),
        Input::Circle => l("Круг", "Circle"),
        Input::Square => l("Квадрат", "Square"),
        Input::Triangle => l("Треугольник", "Triangle"),
        Input::Up => l("Крестовина ↑", "D-pad Up"),
        Input::Down => l("Крестовина ↓", "D-pad Down"),
        Input::Left => l("Крестовина ←", "D-pad Left"),
        Input::Right => l("Крестовина →", "D-pad Right"),
        Input::L1 => "L1",
        Input::R1 => "R1",
        Input::L2 => "L2",
        Input::R2 => "R2",
        Input::L3 => l("L3 (левый стик)", "L3 (left stick)"),
        Input::R3 => l("R3 (правый стик)", "R3 (right stick)"),
        Input::Create => "Create",
        Input::Options => "Options",
        Input::Ps => "PS",
        Input::Mute => l("Кнопка микрофона", "Mute button"),
        Input::TouchLeft => l("Тачпад слева", "Touchpad left"),
        Input::TouchRight => l("Тачпад справа", "Touchpad right"),
    }
}

/// Display name for a library category folder.
pub fn category_label(folder: &str) -> String {
    let ru = match folder {
        "Kits" => "Киты",
        "Kicks" => "Бочки",
        "Snares" => "Снейры",
        "Claps" => "Хлопки",
        "Hi-Hats" => "Хэты",
        "Cymbals" => "Тарелки",
        "Percussion" => "Перкуссия",
        "Breaks" => "Брейки",
        "Loops" => "Лупы",
        "Bass" => "Бас",
        "Stabs" => "Стабы",
        "Keys" => "Клавиши",
        "FX" => "Эффекты",
        "Vox" => "Вокал",
        _ => "",
    };
    if lang() == Lang::Ru && !ru.is_empty() { ru.to_string() } else { folder.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_in_order() {
        assert_eq!(fill("{} → {}", &[&"A", &2]), "A → 2");
        assert_eq!(fill("no holes", &[&1]), "no holes");
    }
}
