//! Data model: profiles, pads, kits and settings, plus saving them to disk.

use crate::i18n::Lang;
use crate::input::Input;
use crate::seq::Pattern;
use crate::theme::{Skin, ThemeKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub const PAD_COUNT: usize = 16;
pub const AUTHOR: &str = "MarySue";

// ----- profiles --------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileDef {
    pub name: String,
    pub tagline: String,
    pub accent: [u8; 3],
    /// DualSense player-indicator LEDs (5 bits).
    pub player_leds: u8,
}

impl Default for ProfileDef {
    fn default() -> Self {
        default_profiles().remove(0)
    }
}

/// What the first profile is called until the player types a name, in Russian and English.
pub const DEFAULT_PROFILE_NAMES: (&str, &str) = ("Игрок 1", "Player 1");

pub fn default_profile_name() -> &'static str {
    crate::i18n::l(DEFAULT_PROFILE_NAMES.0, DEFAULT_PROFILE_NAMES.1)
}

/// A fresh install starts with one profile. The welcome tour asks for a name, and more
/// profiles can be added in Settings → Profiles.
pub fn default_profiles() -> Vec<ProfileDef> {
    vec![ProfileDef {
        name: default_profile_name().into(),
        tagline: crate::i18n::l("Готов играть.", "Ready to play.").into(),
        accent: [94, 234, 212],
        player_leds: 0x04,
    }]
}

pub const PROFILE_COLORS: [[u8; 3]; 10] = [
    [94, 234, 212],
    [251, 113, 133],
    [167, 139, 250],
    [96, 165, 250],
    [250, 204, 21],
    [74, 222, 128],
    [251, 146, 60],
    [244, 114, 182],
    [255, 122, 69],
    [226, 232, 240],
];

pub fn player_leds_for(index: usize) -> u8 {
    [0x04, 0x0A, 0x15, 0x1B, 0x1F][index % 5]
}

// ----- pads & kits -----------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlayMode {
    /// Plays to the end on every press.
    #[default]
    OneShot,
    /// Plays while the button is held.
    Hold,
    /// Press to start looping, press again to stop.
    Loop,
}

pub const PAD_COLORS: [[u8; 3]; 10] = [
    [255, 92, 92],
    [255, 153, 72],
    [250, 210, 80],
    [132, 226, 96],
    [72, 222, 170],
    [72, 196, 246],
    [112, 144, 255],
    [170, 128, 255],
    [240, 112, 200],
    [214, 220, 232],
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PadConfig {
    pub name: String,
    pub sample: Option<PathBuf>,
    pub binding: Option<Input>,
    pub volume: f32,
    pub pitch: f32,
    pub pan: f32,
    pub mode: PlayMode,
    pub choke: u8,
    pub overlap: bool,
    pub rumble: bool,
    pub color: [u8; 3],
    /// Original tempo of a loop. When above zero the pad follows the project tempo.
    pub sync_bpm: f32,
}

impl Default for PadConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            sample: None,
            binding: None,
            volume: 0.9,
            pitch: 0.0,
            pan: 0.0,
            mode: PlayMode::OneShot,
            choke: 0,
            overlap: false,
            rumble: true,
            color: PAD_COLORS[0],
            sync_bpm: 0.0,
        }
    }
}

/// Default layout: face buttons, d-pad, shoulders, triggers, stick clicks, touchpad halves.
pub const DEFAULT_BINDINGS: [Input; PAD_COUNT] = [
    Input::Cross,
    Input::Circle,
    Input::Square,
    Input::Triangle,
    Input::Up,
    Input::Down,
    Input::Left,
    Input::Right,
    Input::L1,
    Input::R1,
    Input::L2,
    Input::R2,
    Input::L3,
    Input::R3,
    Input::TouchLeft,
    Input::TouchRight,
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kit {
    pub name: String,
    pub owner: String,
    pub author: String,
    pub pads: Vec<PadConfig>,
    pub bpm: f32,
    pub swing: f32,
    pub pattern: Pattern,
    /// Id of the factory kit this one started from, if any.
    pub factory: Option<String>,
}

impl Default for Kit {
    fn default() -> Self {
        Kit::new_for(default_profile_name())
    }
}

impl Kit {
    pub fn new_for(owner: &str) -> Self {
        let pads = (0..PAD_COUNT)
            .map(|i| PadConfig {
                binding: Some(DEFAULT_BINDINGS[i]),
                color: PAD_COLORS[(i / 2) % PAD_COLORS.len()],
                ..Default::default()
            })
            .collect();
        Self {
            name: String::new(),
            owner: owner.to_string(),
            author: AUTHOR.to_string(),
            pads,
            bpm: 174.0,
            swing: 0.0,
            pattern: Pattern::default(),
            factory: None,
        }
    }

    /// Make sure a loaded kit always has exactly PAD_COUNT pads and sane numbers.
    pub fn normalized(mut self) -> Self {
        let fresh = Kit::new_for(&self.owner);
        while self.pads.len() < PAD_COUNT {
            self.pads.push(fresh.pads[self.pads.len()].clone());
        }
        self.pads.truncate(PAD_COUNT);
        if !self.bpm.is_finite() || self.bpm < 40.0 || self.bpm > 240.0 {
            self.bpm = 174.0;
        }
        self.swing = if self.swing.is_finite() { self.swing.clamp(0.0, 0.6) } else { 0.0 };
        self.pattern.bars = self.pattern.bars.clamp(1, crate::seq::MAX_BARS);
        let steps = self.pattern.steps();
        self.pattern.notes.retain(|n| (n.step as usize) < steps && (n.pad as usize) < PAD_COUNT);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.pads.iter().all(|p| p.sample.is_none())
    }
}

// ----- sticks & DJ effects ---------------------------------------------------

pub const FX_COUNT: usize = 8;

/// DJ effects a stick direction can drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FxKind {
    #[default]
    None,
    /// Low-pass to the left, high-pass to the right.
    Filter,
    Echo,
    Pitch,
    /// Volume cut (0 = full volume).
    Volume,
    Crush,
    Stutter,
    Pan,
}

impl FxKind {
    pub const ALL: [FxKind; FX_COUNT] = [
        FxKind::None,
        FxKind::Filter,
        FxKind::Echo,
        FxKind::Pitch,
        FxKind::Volume,
        FxKind::Crush,
        FxKind::Stutter,
        FxKind::Pan,
    ];

    pub fn bipolar(self) -> bool {
        matches!(self, FxKind::Filter | FxKind::Pitch | FxKind::Pan)
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StickMode {
    /// The effect follows the stick and snaps back when released.
    #[default]
    Spring,
    /// The stick turns a virtual knob; the value stays where you leave it.
    Knob,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StickSettings {
    pub lx: FxKind,
    pub ly: FxKind,
    pub rx: FxKind,
    pub ry: FxKind,
    pub mode: StickMode,
    pub deadzone: f32,
    /// Pitch-bend range in semitones.
    pub pitch_range: f32,
    /// Echo and stutter follow the project tempo.
    pub sync: bool,
    /// Use the touchpad surface as an XY effect pad.
    pub touch_fx: bool,
    pub touch_x: FxKind,
    pub touch_y: FxKind,
}

impl Default for StickSettings {
    fn default() -> Self {
        Self {
            lx: FxKind::Filter,
            ly: FxKind::Echo,
            rx: FxKind::Pitch,
            ry: FxKind::Stutter,
            mode: StickMode::Spring,
            deadzone: 0.12,
            pitch_range: 12.0,
            sync: true,
            touch_fx: false,
            touch_x: FxKind::Filter,
            touch_y: FxKind::Crush,
        }
    }
}

impl StickSettings {
    pub fn axes(&self) -> [FxKind; 4] {
        [self.lx, self.ly, self.rx, self.ry]
    }
}

// ----- controller functions --------------------------------------------------

/// Things a spare controller button can do besides playing a pad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Play,
    Record,
    StopAll,
    Tap,
}

impl Func {
    pub const ALL: [Func; 4] = [Func::Play, Func::Record, Func::StopAll, Func::Tap];
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Functions {
    pub play: Option<Input>,
    pub record: Option<Input>,
    pub stop_all: Option<Input>,
    pub tap: Option<Input>,
}

impl Default for Functions {
    fn default() -> Self {
        Self { play: Some(Input::Create), record: Some(Input::Mute), stop_all: Some(Input::Options), tap: None }
    }
}

impl Functions {
    pub fn get(&self, f: Func) -> Option<Input> {
        match f {
            Func::Play => self.play,
            Func::Record => self.record,
            Func::StopAll => self.stop_all,
            Func::Tap => self.tap,
        }
    }

    pub fn set(&mut self, f: Func, input: Option<Input>) {
        if let Some(i) = input {
            self.clear(i);
        }
        match f {
            Func::Play => self.play = input,
            Func::Record => self.record = input,
            Func::StopAll => self.stop_all = input,
            Func::Tap => self.tap = input,
        }
    }

    pub fn of(&self, input: Input) -> Option<Func> {
        Func::ALL.into_iter().find(|f| self.get(*f) == Some(input))
    }

    /// Free a button from whatever function uses it.
    pub fn clear(&mut self, input: Input) {
        for f in Func::ALL {
            if self.get(f) == Some(input) {
                match f {
                    Func::Play => self.play = None,
                    Func::Record => self.record = None,
                    Func::StopAll => self.stop_all = None,
                    Func::Tap => self.tap = None,
                }
            }
        }
    }
}

// ----- settings --------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ControllerStyle {
    /// Full-colour DualSense with moving sticks and colourways.
    #[default]
    Colour,
    /// The DualSense line-art picture.
    LineArt,
    /// The painted drawing.
    Drawn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Density {
    Compact,
    #[default]
    Cozy,
    Roomy,
}

impl Density {
    pub fn scale(self) -> f32 {
        match self {
            Density::Compact => 0.8,
            Density::Cozy => 1.0,
            Density::Roomy => 1.25,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LightbarMode {
    /// Profile colour, flashing in the colour of each pad you hit.
    #[default]
    PadFlash,
    /// Always the profile colour.
    Profile,
    /// Pulses on every beat while the loop plays.
    Beat,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BufferPref {
    #[default]
    Auto,
    Low,
    Safe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PadLayout {
    #[default]
    Grid4x4,
    Wide8x2,
}

pub const SETTINGS_VERSION: u32 = 4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub settings_version: u32,
    /// The welcome tour has been finished (or skipped).
    pub onboarded: bool,
    pub profile: String,
    pub master: f32,
    pub language: Lang,

    // Look
    pub theme: ThemeKind,
    pub custom_bg: [u8; 3],
    /// Use this accent instead of the profile colour.
    pub accent_override: Option<[u8; 3]>,
    pub round: f32,
    pub density: Density,
    pub ui_scale: f32,
    pub glow: f32,
    pub animations: bool,

    // Pads
    pub pad_layout: PadLayout,
    pub show_wave: bool,
    pub show_keys: bool,
    pub show_progress: bool,

    // Controller
    pub controller_style: ControllerStyle,
    pub skin: Skin,
    /// Shell, touchpad and centre colours for the Custom colourway.
    pub custom_skin: [[u8; 3]; 3],
    pub colored_glyphs: bool,
    pub lightbar: LightbarMode,
    pub lightbar_brightness: f32,
    pub rumble: f32,
    /// How far L2/R2 must be pressed to count as a hit.
    pub trigger_point: f32,
    pub functions: Functions,
    pub sticks: StickSettings,

    // Layout
    pub show_controller: bool,
    pub show_mapping: bool,
    pub show_fx: bool,
    pub show_steps: bool,
    pub show_hints: bool,
    pub sidebar_width: f32,
    pub bottom_height: f32,

    // Sound
    pub audio_device: Option<String>,
    pub buffer: BufferPref,
    pub metronome: bool,
    /// 0 = loops start at once, 4 = on the next beat, 16 = on the next bar.
    pub launch_quant: u8,

    // Library
    pub library_folders: Vec<PathBuf>,
    pub factory_version: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            settings_version: SETTINGS_VERSION,
            onboarded: false,
            profile: String::new(),
            master: 0.85,
            language: Lang::system(),
            theme: ThemeKind::Studio,
            custom_bg: [30, 27, 46],
            accent_override: None,
            round: 1.0,
            density: Density::Cozy,
            ui_scale: 1.0,
            glow: 1.0,
            animations: true,
            pad_layout: PadLayout::Grid4x4,
            show_wave: true,
            show_keys: true,
            show_progress: true,
            controller_style: ControllerStyle::Colour,
            skin: Skin::White,
            custom_skin: [[236, 238, 244], [236, 238, 244], [48, 48, 48]],
            colored_glyphs: false,
            lightbar: LightbarMode::PadFlash,
            lightbar_brightness: 1.0,
            rumble: 1.0,
            trigger_point: 0.3,
            functions: Functions::default(),
            sticks: StickSettings::default(),
            show_controller: true,
            show_mapping: true,
            show_fx: true,
            show_steps: true,
            show_hints: true,
            sidebar_width: 400.0,
            bottom_height: 276.0,
            audio_device: None,
            buffer: BufferPref::Auto,
            metronome: false,
            launch_quant: 16,
            library_folders: Vec::new(),
            factory_version: 0,
        }
    }
}

impl Settings {
    /// Clamp anything a hand-edited or damaged file could have put out of range.
    fn sanitized(mut self) -> Self {
        let fix = |v: &mut f32, lo: f32, hi: f32, def: f32| {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { def };
        };
        fix(&mut self.master, 0.0, 1.0, 0.85);
        fix(&mut self.round, 0.0, 1.6, 1.0);
        fix(&mut self.ui_scale, 0.75, 1.6, 1.0);
        fix(&mut self.glow, 0.0, 1.5, 1.0);
        fix(&mut self.lightbar_brightness, 0.0, 1.0, 1.0);
        fix(&mut self.rumble, 0.0, 1.0, 1.0);
        fix(&mut self.trigger_point, 0.05, 0.9, 0.3);
        fix(&mut self.sidebar_width, 320.0, 760.0, 400.0);
        fix(&mut self.bottom_height, 200.0, 640.0, 276.0);
        fix(&mut self.sticks.deadzone, 0.0, 0.5, 0.12);
        fix(&mut self.sticks.pitch_range, 1.0, 24.0, 12.0);
        if ![0, 4, 16].contains(&self.launch_quant) {
            self.launch_quant = 16;
        }
        self.settings_version = SETTINGS_VERSION;
        self
    }
}

// ----- persistence -----------------------------------------------------------

static DATA_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Keep all data somewhere else (used by tests; `SENSEPAD_DATA` does the same for users).
#[cfg(test)]
pub fn set_data_dir(dir: PathBuf) {
    *DATA_DIR.write().unwrap() = Some(dir);
}

pub fn app_dir() -> PathBuf {
    if let Some(d) = DATA_DIR.read().unwrap().clone() {
        return d;
    }
    if let Some(d) = std::env::var_os("SENSEPAD_DATA").filter(|d| !d.is_empty()) {
        return PathBuf::from(d);
    }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("SensePad")
}

pub fn factory_dir() -> PathBuf {
    app_dir().join("Library").join("Factory")
}

pub fn kits_dir() -> PathBuf {
    app_dir().join("Kits")
}

fn folder_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' })
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() { "Profile".into() } else { s }
}

pub fn profile_dir(profile: &str) -> PathBuf {
    app_dir().join("profiles").join(folder_name(profile))
}

/// The kit a profile is playing right now.
pub fn profile_kit_path(profile: &str) -> PathBuf {
    profile_dir(profile).join("current.json")
}

fn settings_path() -> PathBuf {
    app_dir().join("config.json")
}

fn profiles_path() -> PathBuf {
    app_dir().join("profiles.json")
}

/// Append a line to `sensepad.log` (kept small). Never fails.
pub fn log(msg: &str) {
    use std::io::Write;
    if cfg!(test) && DATA_DIR.read().unwrap().is_none() {
        return;
    }
    let path = app_dir().join("sensepad.log");
    if std::fs::metadata(&path).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    let _ = std::fs::create_dir_all(app_dir());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "[{secs}] {msg}");
    }
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            log(&format!("can't read {}: {e}", path.display()));
            None
        }
    }
}

/// Write through a temporary file so a crash never leaves half a file, and replace
/// the old file even when it can't be overwritten in place.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let attempt = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &text)?;
        if std::fs::rename(&tmp, path).is_ok() {
            return Ok(());
        }
        let _ = std::fs::remove_file(path);
        match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(_) => {
                let r = std::fs::write(path, &text);
                let _ = std::fs::remove_file(&tmp);
                r
            }
        }
    };
    attempt().map_err(|e| {
        let msg = format!("{}: {e}", path.display());
        log(&format!("save failed: {msg}"));
        msg
    })
}

/// Returns the kit and whether it is brand new (nothing saved for this profile yet).
pub fn load_profile_kit(profile: &str) -> (Kit, bool) {
    let current = profile_kit_path(profile);
    let legacy = profile_dir(profile).join("kit.json");
    let found = read_json::<Kit>(&current).or_else(|| read_json::<Kit>(&legacy));
    match found {
        Some(k) => {
            let mut k = k.normalized();
            k.owner = profile.to_string();
            let fresh = k.is_empty();
            (k, fresh)
        }
        None => (Kit::new_for(profile), true),
    }
}

pub fn load_settings() -> Settings {
    let current = settings_path();
    let s: Settings = if current.exists() {
        read_json(&current).unwrap_or_default()
    } else {
        // Before version 4 the file was called settings.json and only these two fields are worth keeping.
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Old {
            profile: String,
            master: f32,
        }
        let mut s = Settings::default();
        if let Some(old) = read_json::<Old>(&app_dir().join("settings.json")) {
            if !old.profile.is_empty() {
                s.profile = old.profile;
            }
            if old.master > 0.0 {
                s.master = old.master;
            }
        }
        s
    };
    s.sanitized()
}

pub fn save_settings(s: &Settings) -> Result<(), String> {
    write_json(&settings_path(), s)
}

pub fn load_profiles() -> Vec<ProfileDef> {
    match read_json::<Vec<ProfileDef>>(&profiles_path()) {
        Some(p) if !p.is_empty() => p,
        _ => default_profiles(),
    }
}

pub fn save_profiles(p: &[ProfileDef]) -> Result<(), String> {
    write_json(&profiles_path(), &p)
}

/// Kits saved with "Save kit as…", newest first.
pub fn user_kits() -> Vec<(String, PathBuf)> {
    let mut v: Vec<(std::time::SystemTime, String, PathBuf)> = std::fs::read_dir(kits_dir())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .map(|p| {
                    let t = std::fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                    let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    (t, name, p)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, n, p)| (n, p)).collect()
}

pub fn save_user_kit(kit: &Kit, name: &str) -> Result<PathBuf, String> {
    let path = kits_dir().join(format!("{}.json", folder_name(name)));
    write_json(&path, kit)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_a_round_trip() {
        let mut s = Settings::default();
        s.language = Lang::En;
        s.theme = ThemeKind::Custom;
        s.functions.set(Func::Tap, Some(Input::Options));
        let text = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(s, back);
        assert_eq!(back.functions.stop_all, None, "a button serves one function at a time");
    }

    #[test]
    fn old_and_partial_files_still_load() {
        let s: Settings = serde_json::from_str(r#"{"profile":"Someone","master":0.5}"#).unwrap();
        assert_eq!(s.language, Lang::system());
        assert!(!s.onboarded);
        let k: Kit = serde_json::from_str(r#"{"name":"x","pads":[{"name":"Kick"}]}"#).unwrap();
        let k = k.normalized();
        assert_eq!(k.pads.len(), PAD_COUNT);
        assert_eq!(k.bpm, 174.0);
    }

    #[test]
    fn write_json_replaces_existing_files() {
        let dir = std::env::temp_dir().join("sensepad_kit_test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("a").join("file.json");
        write_json(&path, &vec![1, 2, 3]).unwrap();
        write_json(&path, &vec![4]).unwrap();
        assert_eq!(read_json::<Vec<i32>>(&path).unwrap(), vec![4]);
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn damaged_numbers_are_repaired() {
        let mut s = Settings::default();
        s.ui_scale = f32::NAN;
        s.sidebar_width = 9999.0;
        s.launch_quant = 7;
        let s = s.sanitized();
        assert_eq!(s.ui_scale, 1.0);
        assert_eq!(s.sidebar_width, 760.0);
        assert_eq!(s.launch_quant, 16);
    }
}
