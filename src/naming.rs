//! Pad names derived from sample files: track numbers stripped, note names that follow
//! the pitch slider, and tempo read from loop names.

use std::path::Path;

pub const NOTES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Pad name from a file: strips a 1-2 digit track number only.
/// "09 808 C.wav" → "808 C", "01 - Kick.wav" → "Kick", "909 Style.wav" stays "909 Style".
pub fn clean_name(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Sample");
    let digits = stem.chars().take_while(|c| c.is_ascii_digit()).count();
    let rest = &stem[digits..];
    let name = if (1..=2).contains(&digits) && rest.starts_with([' ', '-', '_', '.']) {
        rest.trim_start_matches([' ', '-', '_', '.'])
    } else {
        stem
    };
    let name = if name.is_empty() { stem } else { name };
    name.chars().take(28).collect()
}

/// The first version's rule, which also ate numbers that belong to the name ("09 808 C" → "C").
fn legacy_clean_name(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Sample");
    let trimmed = stem.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, ' ' | '-' | '_' | '.'));
    let name = if trimmed.is_empty() { stem } else { trimmed };
    name.chars().take(22).collect()
}

/// Semitone (0-11) of a trailing note name: "808 C" → 0, "Reese G#" → 8, "Stab Am" → None.
pub fn note_of(name: &str) -> Option<i32> {
    let tok = name.rsplit(' ').next()?;
    let mut ch = tok.chars();
    let base: i32 = match ch.next()? {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let acc = match ch.next() {
        None => 0,
        Some('#') => 1,
        Some('b') => -1,
        _ => return None,
    };
    if ch.next().is_some() {
        return None;
    }
    Some((base + acc).rem_euclid(12))
}

/// Replace a trailing note name with the note `semis` semitones away ("808 C", +5 → "808 F").
pub fn transposed(name: &str, semis: f32) -> String {
    let st = semis.round() as i32;
    match note_of(name) {
        Some(n) if st != 0 => {
            let head = name.rsplit_once(' ').map(|(h, _)| format!("{h} ")).unwrap_or_default();
            format!("{head}{}", NOTES[(n + st).rem_euclid(12) as usize])
        }
        _ => name.to_string(),
    }
}

/// The name SensePad gives a pad for this sample and pitch.
pub fn auto_name(path: &Path, pitch: f32) -> String {
    transposed(&clean_name(path), pitch)
}

/// True when the pad still carries a name SensePad made (so it may follow sample and pitch
/// changes), false when the user typed their own.
pub fn is_auto_name(name: &str, sample: Option<&Path>, pitch: f32, pad: usize) -> bool {
    name.trim().is_empty()
        || name == format!("Pad {}", pad + 1)
        || name == format!("Пэд {}", pad + 1)
        || sample.is_some_and(|s| name == clean_name(s) || name == legacy_clean_name(s) || name == auto_name(s, pitch))
}

/// Tempo written at the end of a loop's name: "Amen-style 170" → 170.
pub fn bpm_in_name(name: &str) -> Option<f32> {
    let tok = name.rsplit(' ').next()?;
    let tok = tok.trim_end_matches(|c: char| c.is_ascii_alphabetic()); // "174bpm"
    let v: f32 = tok.parse().ok()?;
    (60.0..=220.0).contains(&v).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_name_keeps_numbers_that_belong_to_the_name() {
        assert_eq!(clean_name(Path::new("09 808 C.wav")), "808 C");
        assert_eq!(clean_name(Path::new("808 F.wav")), "808 F");
        assert_eq!(clean_name(Path::new("01 Kick.wav")), "Kick");
        assert_eq!(clean_name(Path::new("1 - Snare.wav")), "Snare");
        assert_eq!(clean_name(Path::new("909 Style.wav")), "909 Style");
        assert_eq!(clean_name(Path::new("70s Break.wav")), "70s Break");
    }

    #[test]
    fn names_follow_pitch_for_note_named_samples() {
        assert_eq!(note_of("808 C"), Some(0));
        assert_eq!(note_of("Reese G#"), Some(8));
        assert_eq!(note_of("Rave Stab Am"), None);
        assert_eq!(transposed("808 C", 5.0), "808 F");
        assert_eq!(transposed("808 C", -1.0), "808 B");
        assert_eq!(transposed("Kick", 3.0), "Kick");
        assert_eq!(auto_name(Path::new("09 808 C.wav"), 7.0), "808 G");
    }

    #[test]
    fn custom_names_are_left_alone() {
        let s = Path::new("Bass/808 C.wav");
        assert!(is_auto_name("808 C", Some(s), 0.0, 0));
        assert!(is_auto_name("808 F", Some(s), 5.0, 0));
        assert!(is_auto_name("C", Some(s), 0.0, 0), "names from the old rule count as automatic");
        assert!(is_auto_name("", None, 0.0, 3));
        assert!(!is_auto_name("My Wobble", Some(s), 0.0, 0));
    }

    #[test]
    fn tempo_is_read_from_loop_names() {
        assert_eq!(bpm_in_name("Amen-style 170"), Some(170.0));
        assert_eq!(bpm_in_name("Break 174bpm"), Some(174.0));
        assert_eq!(bpm_in_name("808 C"), None);
        assert_eq!(bpm_in_name("Kick 909"), None);
        assert_eq!(bpm_in_name("Tom 100"), Some(100.0));
    }
}
