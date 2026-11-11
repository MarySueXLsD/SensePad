//! Sample library browser: factory sounds, sample packs, FL Studio packs (if installed)
//! and the user's folders.

use crate::kit;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const AUDIO_EXT: [&str; 9] = ["wav", "mp3", "flac", "ogg", "oga", "aif", "aiff", "m4a", "aac"];

/// Preferred order of the factory categories (anything else sorts alphabetically after them).
const ORDER: [&str; 12] =
    ["Kits", "Kicks", "Snares", "Claps", "Hi-Hats", "Cymbals", "Percussion", "Breaks", "Bass", "Stabs", "FX", "Vox"];

pub fn is_audio(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| AUDIO_EXT.contains(&e.to_lowercase().as_str()))
}

fn rank(name: &str) -> usize {
    ORDER.iter().position(|o| o.eq_ignore_ascii_case(name)).unwrap_or(ORDER.len())
}

/// Audio files directly inside `dir`, sorted by name.
pub fn audio_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_audio(p)).collect())
        .unwrap_or_default();
    v.sort_by_key(|p| p.file_name().map(|f| f.to_string_lossy().to_lowercase()));
    v
}

pub fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> =
        std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
    v.sort_by(|a, b| {
        let (na, nb) = (name_of(a), name_of(b));
        rank(&na).cmp(&rank(&nb)).then_with(|| na.to_lowercase().cmp(&nb.to_lowercase()))
    });
    v
}

pub fn name_of(p: &Path) -> String {
    p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()
}

/// FL Studio keeps its stock samples in `<install>\Data\Patches\Packs`.
pub fn find_fl_studio_packs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        let Ok(base) = std::env::var(var) else { continue };
        let Ok(rd) = std::fs::read_dir(Path::new(&base).join("Image-Line")) else { continue };
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with("FL Studio") {
                let packs = e.path().join("Data").join("Patches").join("Packs");
                if packs.is_dir() && !found.contains(&packs) {
                    found.push(packs);
                }
            }
        }
    }
    if let Some(docs) = dirs::document_dir() {
        let packs = docs.join("Image-Line").join("FL Studio").join("Data").join("Patches").join("Packs");
        if packs.is_dir() && !found.contains(&packs) {
            found.push(packs);
        }
    }
    found
}

/// Installed sample packs as (name, folder), looked up once per run. A pack is a folder of
/// category folders ("Kicks", "Snares"…) inside `packs` next to the program, or inside
/// `Packs` in the data folder.
pub fn packs() -> &'static [(String, PathBuf)] {
    static PACKS: OnceLock<Vec<(String, PathBuf)>> = OnceLock::new();
    PACKS.get_or_init(|| {
        let mut homes: Vec<PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            homes.extend(exe.parent().map(|d| d.join("packs")));
            // Running from a build tree (target/release, or target/release/deps for tests):
            // the packs sit in the project folder.
            homes.extend([3, 4].iter().filter_map(|&n| exe.ancestors().nth(n)).filter(|d| d.join("Cargo.toml").is_file()).map(|d| d.join("packs")));
        }
        homes.push(kit::app_dir().join("Packs"));
        let mut found: Vec<(String, PathBuf)> = Vec::new();
        for home in homes {
            for dir in subdirs(&home) {
                let name = name_of(&dir);
                let has_sounds = subdirs(&dir).iter().any(|c| !audio_files_in(c).is_empty());
                if has_sounds && !found.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
                    found.push((name, dir));
                }
            }
        }
        found
    })
}

pub fn pack_dir(name: &str) -> Option<&'static Path> {
    packs().iter().find(|(n, _)| n == name).map(|(_, p)| p.as_path())
}

/// A pack sound whose file is gone because the program folder was moved: the same sound
/// in the pack where it is installed now.
pub fn relocate_pack_sound(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return None;
    }
    let parts: Vec<String> = path.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    for (name, dir) in packs() {
        if let Some(i) = parts.iter().rposition(|p| p == name) {
            let new = parts[i + 1..].iter().fold(dir.clone(), |p, part| p.join(part));
            if new.is_file() {
                return Some(new);
            }
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    Factory,
    Pack,
    FlStudio,
    User,
}

pub struct Root {
    pub kind: RootKind,
    pub label: String,
    pub path: PathBuf,
}

pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct Library {
    pub roots: Vec<Root>,
    pub root: usize,
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    /// Top-level folders of the current source, shown as category chips.
    pub categories: Vec<PathBuf>,
    pub search: String,
    pub selected: Option<PathBuf>,
    pub fl_found: Vec<PathBuf>,
}

impl Library {
    pub fn new(user_folders: &[PathBuf]) -> Self {
        let mut lib = Self {
            roots: Vec::new(),
            root: 0,
            dir: kit::factory_dir(),
            entries: Vec::new(),
            categories: Vec::new(),
            search: String::new(),
            selected: None,
            fl_found: find_fl_studio_packs(),
        };
        lib.refresh_roots(user_folders);
        lib.open_root(0);
        lib
    }

    pub fn refresh_roots(&mut self, user_folders: &[PathBuf]) {
        self.roots.clear();
        self.roots.push(Root { kind: RootKind::Factory, label: String::new(), path: kit::factory_dir() });
        for (name, dir) in packs() {
            self.roots.push(Root { kind: RootKind::Pack, label: name.clone(), path: dir.clone() });
        }
        for p in &self.fl_found {
            self.roots.push(Root { kind: RootKind::FlStudio, label: String::new(), path: p.clone() });
        }
        for p in user_folders {
            let label = name_of(p);
            self.roots.push(Root { kind: RootKind::User, label: if label.is_empty() { p.display().to_string() } else { label }, path: p.clone() });
        }
        if self.root >= self.roots.len() {
            self.root = 0;
        }
    }

    pub fn root_path(&self) -> Option<&Path> {
        self.roots.get(self.root).map(|r| r.path.as_path())
    }

    pub fn open_root(&mut self, i: usize) {
        if let Some(r) = self.roots.get(i) {
            self.root = i;
            self.dir = r.path.clone();
            self.search.clear();
            self.rescan();
        }
    }

    pub fn open_dir(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.search.clear();
        self.rescan();
    }

    /// Jump to a top-level category of the factory library (e.g. "Kicks").
    pub fn open_factory_category(&mut self, category: &str) {
        self.open_root(0);
        let dir = kit::factory_dir().join(category);
        if dir.is_dir() {
            self.open_dir(dir);
        }
    }

    pub fn can_go_up(&self) -> bool {
        self.root_path().is_some_and(|r| self.dir != r && self.dir.starts_with(r))
    }

    pub fn up(&mut self) {
        if self.can_go_up() {
            if let Some(parent) = self.dir.parent() {
                let p = parent.to_path_buf();
                self.open_dir(p);
            }
        }
    }

    /// The top-level category the current folder lives in.
    pub fn current_category(&self) -> Option<&PathBuf> {
        self.categories.iter().find(|c| self.dir.starts_with(c))
    }

    pub fn rescan(&mut self) {
        self.categories = self.root_path().map(subdirs).unwrap_or_default();
        let mut entries: Vec<Entry> = subdirs(&self.dir)
            .into_iter()
            .map(|p| Entry { name: name_of(&p), path: p, is_dir: true })
            .collect();
        entries.extend(audio_files_in(&self.dir).into_iter().map(|p| Entry { name: name_of(&p), path: p, is_dir: false }));
        self.entries = entries;
    }
}

/// Guess a factory category from a pad name ("Kick 2" → "Kicks").
pub fn guess_category(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    let table: [(&[&str], &str); 11] = [
        (&["break", "amen", "loop"], "Breaks"),
        (&["kick", "бочк"], "Kicks"),
        (&["snare", "rim", "снейр"], "Snares"),
        (&["clap", "snap", "хлоп"], "Claps"),
        (&["hat", "хэт"], "Hi-Hats"),
        (&["ride", "crash", "cymbal", "splash", "china", "тарел"], "Cymbals"),
        (&["tom", "conga", "bongo", "perc", "cowbell", "wood", "clave", "shaker", "tamb"], "Percussion"),
        (&["bass", "reese", "808", "sub", "hoover", "growl", "wobble", "бас"], "Bass"),
        (&["stab", "chord", "organ", "piano", "pad", "bell", "pluck", "brass"], "Stabs"),
        (&["fx", "siren", "horn", "riser", "rewind", "zap", "impact", "sweep", "lifter", "gun", "vinyl", "tape"], "FX"),
        (&["vox", "vocal", "вокал"], "Vox"),
    ];
    table.iter().find(|(keys, _)| keys.iter().any(|k| n.contains(k))).map(|(_, c)| *c)
}
