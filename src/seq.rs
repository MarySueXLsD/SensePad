//! Step patterns for the looper: 16 steps per bar, up to 4 bars.

use serde::{Deserialize, Serialize};

pub const STEPS_PER_BAR: usize = 16;
pub const MAX_BARS: u8 = 4;
const MAX_NOTES: usize = 1024;

fn one() -> u8 {
    1
}
fn full() -> u8 {
    100
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub step: u8,
    pub pad: u8,
    /// Length in steps (used by Hold and Loop pads).
    #[serde(default = "one")]
    pub len: u8,
    /// Velocity in percent.
    #[serde(default = "full")]
    pub vel: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pattern {
    pub bars: u8,
    pub notes: Vec<Note>,
}

impl Default for Pattern {
    fn default() -> Self {
        Self { bars: 2, notes: Vec::new() }
    }
}

impl Pattern {
    pub fn steps(&self) -> usize {
        self.bars.clamp(1, MAX_BARS) as usize * STEPS_PER_BAR
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    pub fn find(&self, step: u8, pad: u8) -> Option<&Note> {
        self.notes.iter().find(|n| n.step == step && n.pad == pad)
    }

    /// Adds the note unless that pad already has one on that step.
    pub fn insert(&mut self, note: Note) -> bool {
        if self.find(note.step, note.pad).is_some() || self.notes.len() >= MAX_NOTES {
            return false;
        }
        self.notes.push(note);
        true
    }

    pub fn remove(&mut self, step: u8, pad: u8) -> bool {
        let before = self.notes.len();
        self.notes.retain(|n| !(n.step == step && n.pad == pad));
        self.notes.len() != before
    }

    pub fn set_len(&mut self, step: u8, pad: u8, len: u8) -> bool {
        match self.notes.iter_mut().find(|n| n.step == step && n.pad == pad) {
            Some(n) => {
                n.len = len.max(1);
                true
            }
            None => false,
        }
    }

    pub fn clear_pad(&mut self, pad: u8) {
        self.notes.retain(|n| n.pad != pad);
    }

    /// Change the length. Growing repeats what is already there; shrinking drops the rest.
    pub fn set_bars(&mut self, bars: u8) {
        let bars = bars.clamp(1, MAX_BARS);
        let old = self.steps();
        let new = bars as usize * STEPS_PER_BAR;
        if new > old {
            let base: Vec<Note> = self.notes.clone();
            let mut offset = old;
            while offset < new {
                for n in &base {
                    let step = n.step as usize + offset;
                    if step < new {
                        self.insert(Note { step: step as u8, ..*n });
                    }
                }
                offset += old;
            }
        } else {
            self.notes.retain(|n| (n.step as usize) < new);
        }
        self.bars = bars;
    }

    /// Build a pattern from text rows, one per pad:
    /// `x` hit, `o` ghost hit, `-` extends the previous hit, anything else is a rest.
    pub fn from_rows(bars: u8, rows: &[(u8, &str)]) -> Self {
        let mut p = Pattern { bars: bars.clamp(1, MAX_BARS), notes: Vec::new() };
        let total = p.steps();
        for (pad, row) in rows {
            let cells: Vec<char> = row.chars().filter(|c| !c.is_whitespace()).collect();
            let mut i = 0;
            while i < cells.len() && i < total {
                let vel = match cells[i] {
                    'x' | 'X' => 100,
                    'o' => 45,
                    _ => {
                        i += 1;
                        continue;
                    }
                };
                let mut len = 1;
                while i + len < cells.len() && cells[i + len] == '-' {
                    len += 1;
                }
                p.insert(Note { step: i as u8, pad: *pad, len: len.min(255) as u8, vel });
                i += len;
            }
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_parse_hits_ghosts_and_lengths() {
        let p = Pattern::from_rows(1, &[(0, "x...o...x---...."), (3, "....x...")]);
        assert_eq!(p.notes.len(), 4);
        assert_eq!(p.find(4, 0).unwrap().vel, 45);
        assert_eq!(p.find(8, 0).unwrap().len, 4);
        assert!(p.find(4, 3).is_some());
    }

    #[test]
    fn growing_repeats_and_shrinking_trims() {
        let mut p = Pattern::from_rows(1, &[(0, "x.......x.......")]);
        p.set_bars(2);
        assert_eq!(p.steps(), 32);
        assert!(p.find(16, 0).is_some() && p.find(24, 0).is_some());
        p.set_bars(1);
        assert_eq!(p.notes.len(), 2);
    }

    #[test]
    fn insert_is_idempotent() {
        let mut p = Pattern::default();
        let n = Note { step: 3, pad: 1, len: 1, vel: 100 };
        assert!(p.insert(n));
        assert!(!p.insert(n));
        assert!(p.remove(3, 1));
    }
}
