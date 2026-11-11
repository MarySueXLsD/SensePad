//! Application state and everything that changes it. Drawing lives in `ui/`.

use crate::audio::{self, AudioEngine, AudioHandle, PadSlot, SampleData, SeqEvent};
use crate::controller_skin::SkinTextures;
use crate::dualsense::{self, HidCmd, HidEvent, PadSnapshot};
use crate::factory;
use crate::i18n::{self, input_label, l, lf};
use crate::input::Input;
use crate::kit::{
    self, Func, Functions, FxKind, Kit, LightbarMode, PlayMode, ProfileDef, Settings, StickMode, StickSettings, FX_COUNT,
    PAD_COUNT, PROFILE_COLORS,
};
use crate::library::{self, Library};
use crate::naming;
use crate::seq::{Note, Pattern};
use crate::theme::{self, rgb, scaled, DANGER, OK, WARN};
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Color32, Key, Rect, TextureHandle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

pub const PAD_KEYS: [Key; PAD_COUNT] = [
    Key::Num1, Key::Num2, Key::Num3, Key::Num4,
    Key::Q, Key::W, Key::E, Key::R,
    Key::A, Key::S, Key::D, Key::F,
    Key::Z, Key::X, Key::C, Key::V,
];
pub const KEY_LABELS: [&str; PAD_COUNT] = ["1", "2", "3", "4", "Q", "W", "E", "R", "A", "S", "D", "F", "Z", "X", "C", "V"];

/// What the next controller button press will be assigned to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Learn {
    Pad(usize),
    Func(Func),
}

pub enum UiMsg {
    Learned(Learn, Input),
    LearnCancelled,
    Loaded { pad: usize, path: PathBuf, result: Result<Arc<SampleData>, String> },
    Preview(PathBuf, Result<Arc<SampleData>, String>),
    FactoryInstalled(Result<(), String>),
    Controller(bool),
    Func(Func),
}

/// The little the controller thread needs to know about a pad.
#[derive(Clone, Copy)]
pub struct PadMeta {
    pub binding: Option<Input>,
    pub rumble: bool,
    pub loaded: bool,
}

/// State shared with the controller thread, so buttons and sticks reach the sound
/// engine without waiting for the next frame of the interface.
pub struct Shared {
    pads: RwLock<Vec<PadMeta>>,
    learning: Mutex<Option<Learn>>,
    sticks: RwLock<StickSettings>,
    functions: RwLock<Functions>,
    rumble: AtomicU32,
    knobs: Mutex<[f32; FX_COUNT]>,
    touch: Mutex<Option<(f32, f32)>>,
    pub audio: AudioHandle,
    pub hid: Sender<HidCmd>,
    ui: Sender<UiMsg>,
    egui: egui::Context,
}

impl Shared {
    /// A pad was hit by hand.
    pub fn fire(&self, pad: usize) {
        self.audio.hit(pad);
        let meta = self.pads.read().unwrap().get(pad).copied();
        if let Some(m) = meta {
            let k = f32::from_bits(self.rumble.load(Ordering::Relaxed));
            if m.loaded && m.rumble && k > 0.01 {
                let _ = self.hid.send(HidCmd::Rumble { strong: 0.85 * k, weak: 0.35 * k, ms: 70 });
            }
        }
        self.egui.request_repaint();
    }

    pub fn release(&self, pad: usize) {
        self.audio.release(pad);
    }

    fn bound_to(&self, input: Input) -> Vec<usize> {
        let pads = self.pads.read().unwrap();
        pads.iter().enumerate().filter(|(_, p)| p.binding == Some(input)).map(|(i, _)| i).collect()
    }

    /// Turn stick (and touchpad) positions into DJ effect values.
    fn apply_sticks(&self, axes: [f32; 4], dt: f32) {
        let cfg = *self.sticks.read().unwrap();
        let dz = cfg.deadzone.clamp(0.0, 0.9);
        let shape = |v: f32| if v.abs() < dz { 0.0 } else { (v - v.signum() * dz) / (1.0 - dz) };
        // Stick Y is "down = +" on the wire; flip it so pushing up is positive.
        let vals = [shape(axes[0]), shape(-axes[1]), shape(axes[2]), shape(-axes[3])];
        let mut out = [0.0f32; FX_COUNT];
        match cfg.mode {
            StickMode::Spring => {
                for (k, v) in cfg.axes().into_iter().zip(vals) {
                    if k == FxKind::None {
                        continue;
                    }
                    let v = if k.bipolar() { v } else { v.abs() };
                    if v.abs() > out[k.index()].abs() {
                        out[k.index()] = v;
                    }
                }
            }
            StickMode::Knob => {
                let mut knobs = self.knobs.lock().unwrap();
                for (k, v) in cfg.axes().into_iter().zip(vals) {
                    if k == FxKind::None {
                        continue;
                    }
                    let lo = if k.bipolar() { -1.0 } else { 0.0 };
                    knobs[k.index()] = (knobs[k.index()] + v * dt * 0.9).clamp(lo, 1.0);
                }
                out = *knobs;
            }
        }
        if cfg.touch_fx {
            if let Some((x, y)) = *self.touch.lock().unwrap() {
                for (k, v) in [(cfg.touch_x, x), (cfg.touch_y, 1.0 - y)] {
                    if k != FxKind::None {
                        out[k.index()] = if k.bipolar() { shape(v * 2.0 - 1.0) } else { v.clamp(0.0, 1.0) };
                    }
                }
            }
        }
        for k in FxKind::ALL.into_iter().skip(1) {
            self.audio.fx.set(k, out[k.index()]);
        }
    }

    pub fn set_knob(&self, k: FxKind, v: f32) {
        self.knobs.lock().unwrap()[k.index()] = v;
        self.audio.fx.set(k, v);
    }

    pub fn reset_fx(&self) {
        *self.knobs.lock().unwrap() = [0.0; FX_COUNT];
        self.audio.fx.reset();
    }

    fn on_hid(&self, ev: HidEvent) {
        match ev {
            HidEvent::Pressed(input) => {
                if let Some(learn) = self.learning.lock().unwrap().take() {
                    let msg = if input == Input::Ps { UiMsg::LearnCancelled } else { UiMsg::Learned(learn, input) };
                    let _ = self.ui.send(msg);
                    self.egui.request_repaint();
                    return;
                }
                let func = self.functions.read().unwrap().of(input);
                if let Some(f) = func {
                    match f {
                        Func::Play => {
                            if self.audio.playing() {
                                self.audio.stop();
                            } else {
                                self.audio.play();
                            }
                        }
                        Func::Record => {
                            let on = !self.audio.recording();
                            if on && !self.audio.playing() {
                                self.audio.play();
                            }
                            self.audio.set_recording(on);
                        }
                        Func::StopAll => {
                            self.audio.stop();
                            self.audio.stop_all();
                        }
                        Func::Tap => {}
                    }
                    let _ = self.ui.send(UiMsg::Func(f));
                    self.egui.request_repaint();
                    return;
                }
                for pad in self.bound_to(input) {
                    self.fire(pad);
                }
            }
            HidEvent::Released(input) => {
                for pad in self.bound_to(input) {
                    self.release(pad);
                }
            }
            HidEvent::Sticks(axes, dt) => self.apply_sticks(axes, dt),
            HidEvent::Touch(t) => *self.touch.lock().unwrap() = t,
            HidEvent::Connected | HidEvent::Disconnected => {
                if matches!(ev, HidEvent::Disconnected) {
                    *self.touch.lock().unwrap() = None;
                    if self.sticks.read().unwrap().mode == StickMode::Spring {
                        self.audio.fx.reset();
                    }
                }
                let _ = self.ui.send(UiMsg::Controller(matches!(ev, HidEvent::Connected)));
                self.egui.request_repaint();
            }
        }
    }
}

pub struct Toast {
    pub text: String,
    pub color: Color32,
    pub born: Instant,
}

/// Full-window pages shown over the instrument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    None,
    Settings,
    Help,
    Welcome,
}

/// Changes requested while drawing; applied after the frame is laid out.
pub enum Action {
    Browse(usize),
    Clear(usize),
    Learn(Learn),
    CancelLearn,
    Bind(usize, Option<Input>),
    SetFunc(Func, Option<Input>),
    Preview(usize),
    ChooseSample(usize),
    SelectPad(usize),
    LibRoot(usize),
    LibOpen(PathBuf),
    LibUp,
    LibPreview(PathBuf),
    LibAssign(PathBuf),
    LibLoadFolder(PathBuf),
    InstallFactory,
    AddFolder,
    RemoveFolder(usize),
    UseProfile(usize),
    DeleteProfile(usize),
    RenameProfile,
    CreateProfile,
    Open(Page),
    OpenSettings(usize),
    ClosePage,
    FinishWelcome,
    ResetFx,
    LoadFactoryKit(String),
    LoadUserKit(PathBuf),
    SaveKitAs,
    ImportKit,
    ExportKit,
    ResetKit,
    OpenKitsFolder,
    Play,
    Record,
    StopAll,
    Tap,
    Demo,
    ClearPattern,
    ClearPadNotes(usize),
    ToggleStep(usize, usize),
    SetBars(u8),
    RefreshDevices,
    ReopenAudio,
}

/// What to start besides the window (tests switch both off).
pub struct Boot {
    pub audio: bool,
    pub hid: bool,
}

pub struct SensePadApp {
    pub(crate) shared: Arc<Shared>,
    pub(crate) engine: AudioEngine,
    pub(crate) audio: AudioHandle,
    pub(crate) audio_wanted: bool,
    pub(crate) audio_error: Option<String>,
    audio_retry: Instant,
    seq_rx: Receiver<SeqEvent>,
    pub(crate) snapshot: Arc<Mutex<PadSnapshot>>,
    rx: Receiver<UiMsg>,
    pub(crate) profiles: Vec<ProfileDef>,
    pub(crate) profile: usize,
    pub(crate) kit: Kit,
    pub(crate) settings: Settings,
    pub(crate) samples: Vec<Option<Arc<SampleData>>>,
    pub(crate) selected: usize,
    pub(crate) loading: [bool; PAD_COUNT],
    pub(crate) errors: Vec<Option<String>>,
    pub(crate) learning: Option<Learn>,
    pub(crate) toasts: Vec<Toast>,
    kit_dirty: Option<Instant>,
    pub(crate) settings_dirty: Option<Instant>,
    profiles_dirty: Option<Instant>,
    last_save_error: Option<(String, Instant)>,
    pub(crate) meter: f32,
    pub(crate) pad_rects: [Rect; PAD_COUNT],
    pub(crate) central_rect: Rect,
    pub(crate) mouse_down: Option<usize>,
    pub(crate) actions: Vec<Action>,
    pub(crate) inspector_tab: usize,
    pub(crate) library: Library,
    preview_cache: Option<(PathBuf, Arc<SampleData>)>,
    pub(crate) factory_installing: bool,
    pub(crate) factory_progress: Arc<AtomicU32>,
    pub(crate) page: Page,
    pub(crate) settings_tab: usize,
    pub(crate) help_topic: usize,
    pub(crate) welcome_step: usize,
    pub(crate) rename_buf: String,
    pub(crate) new_name: String,
    pub(crate) new_tagline: String,
    pub(crate) new_color: [u8; 3],
    pub(crate) profile_error: Option<String>,
    pub(crate) delete_armed: Option<usize>,
    pub(crate) art: Option<TextureHandle>,
    pub(crate) skin_tex: Option<SkinTextures>,
    hit_seen: [u32; PAD_COUNT],
    hit_at: [Option<Instant>; PAD_COUNT],
    last_step: Option<usize>,
    taps: Vec<Instant>,
    demo_pending: bool,
    last_rec: bool,
    pub(crate) user_kits: Vec<(String, PathBuf)>,
    pub(crate) devices: Vec<String>,
    pub(crate) started: Instant,
}

impl SensePadApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::boot(cc.egui_ctx.clone(), Boot { audio: true, hid: true })
    }

    pub fn boot(ctx: egui::Context, boot: Boot) -> Self {
        theme::install_fonts(&ctx);
        let settings = kit::load_settings();
        i18n::set_lang(settings.language);
        let profiles = kit::load_profiles();
        let profile = profiles.iter().position(|p| p.name == settings.profile).unwrap_or(0);

        theme::set_round(settings.round);
        theme::set_density(settings.density.scale());
        theme::set_colored_glyphs(settings.colored_glyphs);
        theme::set_custom_skin(settings.custom_skin);
        let accent = rgb(settings.accent_override.unwrap_or(profiles[profile].accent));
        theme::apply(&ctx, settings.theme, settings.custom_bg, accent);
        theme::init_textures(&ctx);
        ctx.set_zoom_factor(settings.ui_scale);

        let (ui_tx, rx) = crossbeam_channel::unbounded();
        let (hid_tx, hid_rx) = crossbeam_channel::unbounded();
        let (mut engine, seq_rx) = AudioEngine::new(settings.master);
        let audio = engine.handle.clone();
        let audio_error = if boot.audio { engine.open(settings.audio_device.as_deref(), settings.buffer).err() } else { None };
        if let Some(e) = &audio_error {
            kit::log(&format!("audio: {e}"));
        }
        audio.set_metronome(settings.metronome);
        audio.set_launch_quant(settings.launch_quant);
        audio.set_fx_sync(settings.sticks.sync);
        audio.fx.set_pitch_range(settings.sticks.pitch_range);

        let (kit, fresh) = kit::load_profile_kit(&profiles[profile].name);
        let shared = Arc::new(Shared {
            pads: RwLock::new(Vec::new()),
            learning: Mutex::new(None),
            sticks: RwLock::new(settings.sticks),
            functions: RwLock::new(settings.functions),
            rumble: AtomicU32::new(settings.rumble.to_bits()),
            knobs: Mutex::new([0.0; FX_COUNT]),
            touch: Mutex::new(None),
            audio: audio.clone(),
            hid: hid_tx,
            ui: ui_tx,
            egui: ctx.clone(),
        });

        let snapshot = Arc::new(Mutex::new(PadSnapshot::default()));
        if boot.hid {
            let sh = shared.clone();
            dualsense::spawn(snapshot.clone(), hid_rx, move |ev| sh.on_hid(ev));
        }

        let library = Library::new(&settings.library_folders);
        let rename_buf = profiles[profile].name.clone();
        let onboarded = settings.onboarded;
        let mut app = Self {
            shared,
            engine,
            audio,
            audio_wanted: boot.audio,
            audio_error,
            audio_retry: Instant::now(),
            seq_rx,
            snapshot,
            rx,
            profiles,
            profile,
            kit,
            settings,
            samples: vec![None; PAD_COUNT],
            selected: 0,
            loading: [false; PAD_COUNT],
            errors: vec![None; PAD_COUNT],
            learning: None,
            toasts: Vec::new(),
            kit_dirty: None,
            settings_dirty: None,
            profiles_dirty: None,
            last_save_error: None,
            meter: 0.0,
            pad_rects: [Rect::NOTHING; PAD_COUNT],
            central_rect: Rect::NOTHING,
            mouse_down: None,
            actions: Vec::new(),
            inspector_tab: 0,
            library,
            preview_cache: None,
            factory_installing: false,
            factory_progress: Arc::new(AtomicU32::new(0)),
            page: if onboarded { Page::None } else { Page::Welcome },
            settings_tab: 0,
            help_topic: 0,
            welcome_step: 0,
            rename_buf,
            new_name: String::new(),
            new_tagline: String::new(),
            new_color: PROFILE_COLORS[2],
            profile_error: None,
            delete_armed: None,
            art: crate::controller_art::load_texture(&ctx),
            skin_tex: SkinTextures::load(&ctx),
            hit_seen: [0; PAD_COUNT],
            hit_at: [None; PAD_COUNT],
            last_step: None,
            taps: Vec::new(),
            demo_pending: false,
            last_rec: false,
            user_kits: kit::user_kits(),
            devices: Vec::new(),
            started: Instant::now(),
        };
        if app.repair_kit() {
            app.kit_dirty = Some(Instant::now());
        }
        app.push_kit_to_engine();
        app.push_outputs();
        let _ = app.shared.hid.send(HidCmd::TriggerPoint(app.settings.trigger_point));
        app.reload_all_samples();

        let factory_ok = kit::factory_dir().join(factory::CATEGORIES[0]).is_dir();
        if app.settings.factory_version < factory::FACTORY_VERSION || !factory_ok {
            app.start_factory_install();
        } else if fresh {
            app.load_default_factory_kit();
        }
        if let Some(e) = app.audio_error.clone() {
            app.toast(lf("Звук недоступен: {}", "Sound is unavailable: {}", &[&e]), DANGER);
        }
        app
    }

    // ----- small accessors -------------------------------------------------

    pub(crate) fn prof(&self) -> &ProfileDef {
        &self.profiles[self.profile]
    }

    pub(crate) fn accent(&self) -> Color32 {
        rgb(self.settings.accent_override.unwrap_or(self.prof().accent))
    }

    pub(crate) fn pad_name(&self, i: usize) -> String {
        let n = &self.kit.pads[i].name;
        if n.is_empty() { lf("Пэд {}", "Pad {}", &[&(i + 1)]) } else { n.clone() }
    }

    pub(crate) fn toast(&mut self, text: impl Into<String>, color: Color32) {
        self.toasts.push(Toast { text: text.into(), color, born: Instant::now() });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    /// Seconds since a pad last sounded (by hand or from the looper).
    pub(crate) fn since_hit(&self, pad: usize) -> f32 {
        self.hit_at[pad].map(|t| t.elapsed().as_secs_f32()).unwrap_or(f32::INFINITY)
    }

    pub(crate) fn apply_theme(&self) {
        theme::set_round(self.settings.round);
        theme::set_density(self.settings.density.scale());
        theme::apply(&self.shared.egui, self.settings.theme, self.settings.custom_bg, self.accent());
    }

    fn set_title(&self) {
        self.shared.egui.send_viewport_cmd(egui::ViewportCommand::Title(format!("SensePad — {}", self.prof().name)));
    }

    // ----- engine sync -----------------------------------------------------

    fn push_pad(&self, i: usize) {
        let cfg = &self.kit.pads[i];
        {
            let mut pads = self.shared.pads.write().unwrap();
            pads.resize(PAD_COUNT, PadMeta { binding: None, rumble: false, loaded: false });
            pads[i] = PadMeta { binding: cfg.binding, rumble: cfg.rumble, loaded: self.samples[i].is_some() };
        }
        self.audio.set_pad(i, PadSlot::new(cfg, self.samples[i].clone()));
    }

    pub(crate) fn sync_pad(&mut self, i: usize) {
        self.push_pad(i);
        self.kit_dirty = Some(Instant::now());
    }

    fn push_kit_to_engine(&self) {
        self.audio.set_tempo(self.kit.bpm);
        self.audio.set_swing(self.kit.swing);
        self.audio.set_pattern(&self.kit.pattern);
        for i in 0..PAD_COUNT {
            self.push_pad(i);
        }
    }

    pub(crate) fn mark_kit_changed(&mut self) {
        self.kit_dirty = Some(Instant::now());
    }

    pub(crate) fn pattern_changed(&mut self) {
        self.audio.set_pattern(&self.kit.pattern);
        self.kit_dirty = Some(Instant::now());
    }

    /// Light bar, player LEDs and mute LED for the current profile and settings.
    pub(crate) fn push_outputs(&self) {
        let p = self.prof();
        let base = match self.settings.lightbar {
            LightbarMode::Off => [0, 0, 0],
            _ => scaled(self.settings.accent_override.unwrap_or(p.accent), self.settings.lightbar_brightness),
        };
        let _ = self.shared.hid.send(HidCmd::BaseColor(base));
        let _ = self.shared.hid.send(HidCmd::PlayerLeds(p.player_leds));
        let _ = self.shared.hid.send(HidCmd::MuteLed(self.learning.is_some() || self.audio.recording()));
    }

    // ----- samples ---------------------------------------------------------

    fn spawn_load(&mut self, i: usize) {
        let Some(path) = self.kit.pads[i].sample.clone() else { return };
        self.loading[i] = true;
        let tx = self.shared.ui.clone();
        let ctx = self.shared.egui.clone();
        std::thread::spawn(move || {
            let result = audio::load_sample(&path).map(Arc::new);
            let _ = tx.send(UiMsg::Loaded { pad: i, path, result });
            ctx.request_repaint();
        });
    }

    fn reload_all_samples(&mut self) {
        for i in 0..PAD_COUNT {
            self.spawn_load(i);
        }
    }

    /// Fix names from older versions and point moved factory and pack sounds at their new files.
    fn repair_kit(&mut self) -> bool {
        let root = kit::factory_dir();
        let mut changed = false;
        for (i, pad) in self.kit.pads.iter_mut().enumerate() {
            let Some(s) = pad.sample.clone() else { continue };
            if let Some(new) = factory::relocate(&s, &root).or_else(|| crate::library::relocate_pack_sound(&s)) {
                if naming::is_auto_name(&pad.name, Some(&s), pad.pitch, i) {
                    pad.name = naming::auto_name(&new, pad.pitch);
                }
                pad.sample = Some(new);
                self.loading[i] = false;
                self.samples[i] = None;
                changed = true;
            } else if pad.name != naming::clean_name(&s) && naming::is_auto_name(&pad.name, Some(&s), pad.pitch, i) && pad.pitch == 0.0 && !pad.name.is_empty() {
                pad.name = naming::clean_name(&s);
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn assign_sample(&mut self, i: usize, path: PathBuf) {
        let (auto, pitch) = {
            let pad = &self.kit.pads[i];
            (naming::is_auto_name(&pad.name, pad.sample.as_deref(), pad.pitch, i), pad.pitch)
        };
        let clean = naming::clean_name(&path);
        let folder = path.parent().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
        let low = clean.to_lowercase();
        let loopish = folder == "breaks" || folder == "loops" || low.contains("loop") || low.contains("break");
        let pad = &mut self.kit.pads[i];
        if auto {
            pad.name = naming::auto_name(&path, pitch);
        }
        match naming::bpm_in_name(&clean).filter(|_| loopish) {
            Some(bpm) => {
                pad.sync_bpm = bpm;
                if !low.contains("roll") {
                    pad.mode = PlayMode::Loop;
                }
            }
            None => {
                if pad.sync_bpm > 0.0 && pad.mode == PlayMode::Loop {
                    pad.mode = PlayMode::OneShot;
                }
                pad.sync_bpm = 0.0;
            }
        }
        if let Some(cat) = path.parent().and_then(|p| p.file_name()).and_then(|f| f.to_str()) {
            if factory::CATEGORIES.contains(&cat) && path.starts_with(kit::factory_dir()) {
                pad.color = factory::pad_color(cat);
            }
        }
        pad.sample = Some(path);
        self.errors[i] = None;
        self.samples[i] = None;
        self.kit.factory = None;
        self.sync_pad(i);
        self.spawn_load(i);
    }

    fn clear_sample(&mut self, i: usize) {
        self.audio.stop_pad(i);
        self.kit.pads[i].sample = None;
        self.kit.pads[i].name = String::new();
        self.kit.pads[i].sync_bpm = 0.0;
        self.samples[i] = None;
        self.loading[i] = false;
        self.errors[i] = None;
        self.sync_pad(i);
    }

    fn browse(&mut self, i: usize) {
        let title = lf("Сэмпл для пэда {}", "Sample for pad {}", &[&(i + 1)]);
        if let Some(path) = rfd::FileDialog::new().set_title(title).add_filter("Audio", &library::AUDIO_EXT).pick_file() {
            self.assign_sample(i, path);
        }
    }

    fn start_factory_install(&mut self) {
        if self.factory_installing {
            return;
        }
        self.factory_installing = true;
        let tx = self.shared.ui.clone();
        let ctx = self.shared.egui.clone();
        let progress = self.factory_progress.clone();
        std::thread::spawn(move || {
            let result = factory::install(&kit::factory_dir(), &progress);
            let _ = tx.send(UiMsg::FactoryInstalled(result));
            ctx.request_repaint();
        });
    }

    fn preview_file(&mut self, path: PathBuf) {
        self.library.selected = Some(path.clone());
        if let Some((p, s)) = &self.preview_cache {
            if *p == path {
                self.audio.preview(s.clone());
                return;
            }
        }
        let tx = self.shared.ui.clone();
        let ctx = self.shared.egui.clone();
        std::thread::spawn(move || {
            let result = audio::load_sample(&path).map(Arc::new);
            let _ = tx.send(UiMsg::Preview(path, result));
            ctx.request_repaint();
        });
    }

    // ----- kits ------------------------------------------------------------

    /// Replace the kit being played.
    fn set_kit(&mut self, kit: Kit) {
        self.audio.stop();
        self.audio.stop_all();
        self.kit = kit.normalized();
        self.samples = vec![None; PAD_COUNT];
        self.loading = [false; PAD_COUNT];
        self.errors = vec![None; PAD_COUNT];
        self.repair_kit();
        self.push_kit_to_engine();
        self.reload_all_samples();
        self.kit_dirty = Some(Instant::now());
    }

    /// Load a factory kit, keeping the buttons you have assigned to each pad.
    fn load_factory_kit(&mut self, id: &str) {
        let Some(spec) = factory::find_kit(id) else { return };
        let mut kit = spec.build(&self.prof().name, &kit::factory_dir());
        for (new, old) in kit.pads.iter_mut().zip(&self.kit.pads) {
            new.binding = old.binding;
        }
        let title = kit.name.clone();
        self.set_kit(kit);
        let accent = self.accent();
        self.toast(lf("Кит «{}» загружен", "Kit “{}” loaded", &[&title]), accent);
    }

    pub(crate) fn load_default_factory_kit(&mut self) {
        let all = factory::kits();
        let id = all[self.profile % all.len()].id;
        if kit::factory_dir().join(factory::CATEGORIES[0]).is_dir() {
            self.load_factory_kit(id);
        }
    }

    fn load_user_kit(&mut self, path: &Path) {
        match kit::read_json::<Kit>(path) {
            Some(mut k) => {
                k.owner = self.prof().name.clone();
                let title = k.name.clone();
                self.set_kit(k);
                self.toast(lf("Кит «{}» загружен", "Kit “{}” loaded", &[&title]), OK);
            }
            None => self.toast(l("Это не файл кита SensePad", "That file isn't a SensePad kit"), DANGER),
        }
    }

    fn save_kit_as(&mut self) {
        let name = if self.kit.name.trim().is_empty() { self.prof().name.clone() } else { self.kit.name.trim().to_string() };
        match kit::save_user_kit(&self.kit, &name) {
            Ok(_) => {
                self.user_kits = kit::user_kits();
                self.toast(lf("Кит «{}» сохранён в «Мои киты»", "Kit “{}” saved to My kits", &[&name]), OK);
            }
            Err(e) => self.toast(lf("Не удалось сохранить кит: {}", "Couldn't save the kit: {}", &[&e]), DANGER),
        }
    }

    fn export_kit(&mut self) {
        let file: String = self.kit.name.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        if let Some(path) = rfd::FileDialog::new()
            .set_title(l("Экспорт кита", "Export kit"))
            .add_filter("SensePad kit", &["json"])
            .set_file_name(format!("{file}.sensepad.json"))
            .save_file()
        {
            match kit::write_json(&path, &self.kit) {
                Ok(()) => self.toast(l("Кит экспортирован", "Kit exported"), OK),
                Err(e) => self.toast(lf("Ошибка экспорта: {}", "Export failed: {}", &[&e]), DANGER),
            }
        }
    }

    fn import_kit(&mut self) {
        let picked = rfd::FileDialog::new().set_title(l("Импорт кита", "Import kit")).add_filter("SensePad kit", &["json"]).pick_file();
        if let Some(path) = picked {
            self.load_user_kit(&path);
        }
    }

    fn reset_kit(&mut self) {
        let kit = Kit::new_for(&self.prof().name);
        self.set_kit(kit);
        self.toast(l("Пустой кит — кнопки назначены по умолчанию", "Empty kit — default buttons restored"), WARN);
    }

    fn save_kit(&mut self) {
        let path = kit::profile_kit_path(&self.prof().name);
        let res = kit::write_json(&path, &self.kit);
        self.kit_dirty = None;
        self.report_save(res);
    }

    fn report_save(&mut self, res: Result<(), String>) {
        if let Err(e) = res {
            let recent = self.last_save_error.as_ref().is_some_and(|(m, t)| *m == e && t.elapsed() < Duration::from_secs(20));
            if !recent {
                self.toast(lf("Не удалось сохранить: {}", "Couldn't save: {}", &[&e]), DANGER);
                self.last_save_error = Some((e, Instant::now()));
            }
        }
    }

    // ----- bindings --------------------------------------------------------

    fn set_binding(&mut self, pad: usize, input: Option<Input>) {
        let mut note = String::new();
        if let Some(inp) = input {
            if let Some(f) = self.settings.functions.of(inp) {
                self.settings.functions.clear(inp);
                *self.shared.functions.write().unwrap() = self.settings.functions;
                self.settings_dirty = Some(Instant::now());
                note = lf("  (кнопка больше не «{}»)", "  (no longer “{}”)", &[&func_name(f)]);
            }
            for j in 0..PAD_COUNT {
                if j != pad && self.kit.pads[j].binding == Some(inp) {
                    self.kit.pads[j].binding = None;
                    self.sync_pad(j);
                    note = lf("  (перенесено с «{}»)", "  (moved from “{}”)", &[&self.pad_name(j)]);
                }
            }
        }
        self.kit.pads[pad].binding = input;
        self.sync_pad(pad);
        let name = self.pad_name(pad);
        let mut msg = match input {
            Some(inp) => format!("{} → {}", input_label(inp), name),
            None => lf("«{}»: кнопка снята", "“{}” is no longer bound", &[&name]),
        };
        msg.push_str(&note);
        let accent = self.accent();
        self.toast(msg, accent);
    }

    fn set_function(&mut self, f: Func, input: Option<Input>) {
        if let Some(inp) = input {
            for j in 0..PAD_COUNT {
                if self.kit.pads[j].binding == Some(inp) {
                    self.kit.pads[j].binding = None;
                    self.sync_pad(j);
                }
            }
        }
        self.settings.functions.set(f, input);
        *self.shared.functions.write().unwrap() = self.settings.functions;
        self.settings_dirty = Some(Instant::now());
        let msg = match input {
            Some(inp) => format!("{} → {}", input_label(inp), func_name(f)),
            None => lf("«{}»: кнопка снята", "“{}” has no button now", &[&func_name(f)]),
        };
        let accent = self.accent();
        self.toast(msg, accent);
    }

    fn start_learning(&mut self, what: Learn) {
        self.learning = Some(what);
        *self.shared.learning.lock().unwrap() = Some(what);
        self.push_outputs();
    }

    pub(crate) fn cancel_learning(&mut self) {
        self.learning = None;
        *self.shared.learning.lock().unwrap() = None;
        self.push_outputs();
    }

    // ----- looper ----------------------------------------------------------

    fn toggle_play(&mut self) {
        if self.audio.playing() {
            self.audio.stop();
        } else if self.audio.is_online() {
            self.audio.play();
        } else {
            self.toast(l("Нет звукового устройства", "No sound device"), WARN);
        }
    }

    fn toggle_record(&mut self) {
        let on = !self.audio.recording();
        if on && !self.audio.playing() {
            self.audio.play();
        }
        self.audio.set_recording(on);
    }

    fn stop_everything(&mut self) {
        self.audio.stop();
        self.audio.stop_all();
    }

    pub(crate) fn set_bpm(&mut self, bpm: f32) {
        let bpm = bpm.clamp(60.0, 220.0).round();
        if (bpm - self.kit.bpm).abs() > 0.01 {
            self.kit.bpm = bpm;
            self.audio.set_tempo(bpm);
            self.kit_dirty = Some(Instant::now());
        }
    }

    pub(crate) fn set_swing(&mut self, swing: f32) {
        self.kit.swing = swing.clamp(0.0, 0.6);
        self.audio.set_swing(self.kit.swing);
        self.kit_dirty = Some(Instant::now());
    }

    /// Tap tempo: the average of the last few taps.
    fn tap(&mut self) {
        let now = Instant::now();
        if self.taps.last().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(2)) {
            self.taps.clear();
        }
        self.taps.push(now);
        if self.taps.len() > 6 {
            self.taps.remove(0);
        }
        if self.taps.len() >= 3 {
            let span = now.duration_since(self.taps[0]).as_secs_f32();
            let bpm = 60.0 * (self.taps.len() - 1) as f32 / span.max(0.05);
            self.set_bpm(bpm);
        }
    }

    /// Put the factory kit's demo beat back and play it.
    fn play_demo(&mut self) {
        let spec = self.kit.factory.as_deref().and_then(factory::find_kit);
        match spec {
            Some(spec) => {
                self.kit.pattern = Pattern::from_rows(spec.bars, &spec.demo);
                self.kit.bpm = spec.bpm;
                self.audio.set_tempo(spec.bpm);
                self.pattern_changed();
                self.audio.stop();
                self.audio.play();
            }
            None => self.toast(
                l("Демо есть у готовых китов: выберите один в меню «Кит»", "Factory kits have demos: pick one in the Kit menu"),
                WARN,
            ),
        }
    }

    fn toggle_step(&mut self, pad: usize, step: usize) {
        let (s, p) = (step as u8, pad as u8);
        if !self.kit.pattern.remove(s, p) {
            let len = match self.kit.pads[pad].mode {
                PlayMode::OneShot => 1,
                PlayMode::Hold => 2,
                PlayMode::Loop => (self.kit.pattern.steps() - step).min(255) as u8,
            };
            self.kit.pattern.insert(Note { step: s, pad: p, len, vel: 100 });
            if !self.audio.playing() && self.kit.pads[pad].mode == PlayMode::OneShot {
                self.shared.fire(pad);
            }
        }
        self.pattern_changed();
    }

    // ----- profiles --------------------------------------------------------

    fn switch_profile(&mut self, idx: usize) {
        if idx == self.profile || idx >= self.profiles.len() {
            return;
        }
        self.cancel_learning();
        self.save_kit();
        self.profile = idx;
        self.settings.profile = self.prof().name.clone();
        self.settings_dirty = Some(Instant::now());
        let (kit, fresh) = kit::load_profile_kit(&self.prof().name);
        self.selected = 0;
        self.set_kit(kit);
        if fresh {
            self.load_default_factory_kit();
        }
        self.apply_theme();
        self.push_outputs();
        self.set_title();
        self.rename_buf = self.prof().name.clone();
        self.delete_armed = None;
        let hello = lf("Привет, {}!", "Hello, {}!", &[&self.prof().name]);
        let accent = self.accent();
        self.toast(hello, accent);
    }

    fn create_profile(&mut self) {
        let name = self.new_name.trim().to_string();
        if name.is_empty() {
            self.profile_error = Some(l("Введите имя профиля", "Give the profile a name").into());
            return;
        }
        if self.profiles.iter().any(|p| p.name.to_lowercase() == name.to_lowercase()) {
            self.profile_error = Some(l("Такое имя уже есть", "That name is already used").into());
            return;
        }
        let tagline = if self.new_tagline.trim().is_empty() { l("Готов играть.", "Ready to play.").to_string() } else { self.new_tagline.trim().to_string() };
        self.profiles.push(ProfileDef { name, tagline, accent: self.new_color, player_leds: kit::player_leds_for(self.profiles.len()) });
        self.profiles_dirty = Some(Instant::now());
        self.new_name.clear();
        self.new_tagline.clear();
        self.profile_error = None;
        self.switch_profile(self.profiles.len() - 1);
    }

    fn rename_profile(&mut self) {
        let new = self.rename_buf.trim().to_string();
        let old = self.prof().name.clone();
        if new == old {
            return;
        }
        if new.is_empty() {
            self.profile_error = Some(l("Введите имя профиля", "Give the profile a name").into());
            return;
        }
        if self.profiles.iter().enumerate().any(|(i, p)| i != self.profile && p.name.to_lowercase() == new.to_lowercase()) {
            self.profile_error = Some(l("Такое имя уже есть", "That name is already used").into());
            return;
        }
        self.profiles[self.profile].name = new.clone();
        self.kit.owner = new.clone();
        self.settings.profile = new;
        self.profiles_dirty = Some(Instant::now());
        self.settings_dirty = Some(Instant::now());
        self.save_kit();
        self.set_title();
        self.profile_error = None;
    }

    fn delete_profile(&mut self, i: usize) {
        if self.profiles.len() <= 1 || i >= self.profiles.len() {
            return;
        }
        if i == self.profile {
            self.switch_profile(if i == 0 { 1 } else { 0 });
        }
        self.profiles.remove(i);
        if self.profile > i {
            self.profile -= 1;
        }
        self.profiles_dirty = Some(Instant::now());
        self.delete_armed = None;
    }

    pub(crate) fn profiles_changed(&mut self) {
        self.profiles_dirty = Some(Instant::now());
    }

    // ----- per-frame plumbing ----------------------------------------------

    fn process_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                UiMsg::Learned(what, input) => {
                    self.learning = None;
                    self.push_outputs();
                    match what {
                        Learn::Pad(pad) => self.set_binding(pad, Some(input)),
                        Learn::Func(f) => self.set_function(f, Some(input)),
                    }
                }
                UiMsg::LearnCancelled => {
                    self.learning = None;
                    self.push_outputs();
                    self.toast(l("Назначение отменено", "Binding cancelled"), theme::dim());
                }
                UiMsg::Loaded { pad, path, result } => {
                    if self.kit.pads[pad].sample.as_ref() != Some(&path) {
                        continue; // replaced while loading
                    }
                    self.loading[pad] = false;
                    match result {
                        Ok(s) => {
                            self.samples[pad] = Some(s);
                            self.errors[pad] = None;
                            self.push_pad(pad);
                        }
                        Err(e) => {
                            // Factory sounds are being generated; the kit is repaired when that finishes.
                            let quiet = self.factory_installing && path.starts_with(kit::factory_dir());
                            if !quiet {
                                let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                                self.toast(lf("Не удалось загрузить {}: {}", "Couldn't load {}: {}", &[&file, &e]), DANGER);
                            }
                            self.errors[pad] = Some(e);
                        }
                    }
                }
                UiMsg::Preview(path, result) => match result {
                    Ok(s) => {
                        self.audio.preview(s.clone());
                        self.preview_cache = Some((path, s));
                    }
                    Err(e) => {
                        let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                        self.toast(lf("Не удалось загрузить {}: {}", "Couldn't load {}: {}", &[&file, &e]), DANGER);
                    }
                },
                UiMsg::FactoryInstalled(result) => {
                    self.factory_installing = false;
                    match result {
                        Ok(()) => {
                            self.settings.factory_version = factory::FACTORY_VERSION;
                            self.settings_dirty = Some(Instant::now());
                            self.library.rescan();
                            if self.repair_kit() {
                                self.kit_dirty = Some(Instant::now());
                            }
                            let root = kit::factory_dir();
                            let lost = self.kit.pads.iter().filter(|p| p.sample.as_ref().is_some_and(|s| s.starts_with(&root) && !s.exists())).count();
                            if self.kit.is_empty() || lost >= 8 {
                                self.load_default_factory_kit();
                            } else {
                                for i in 0..PAD_COUNT {
                                    if self.samples[i].is_none() && self.kit.pads[i].sample.is_some() {
                                        self.push_pad(i);
                                        self.spawn_load(i);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            kit::log(&format!("factory install: {e}"));
                            self.toast(lf("Не удалось подготовить звуки: {}", "Couldn't prepare the sounds: {}", &[&e]), DANGER);
                        }
                    }
                }
                UiMsg::Controller(true) => {
                    self.push_outputs();
                    let _ = self.shared.hid.send(HidCmd::TriggerPoint(self.settings.trigger_point));
                    self.toast(l("DualSense подключён", "DualSense connected"), OK);
                }
                UiMsg::Controller(false) => self.toast(l("DualSense отключён", "DualSense disconnected"), WARN),
                UiMsg::Func(f) => {
                    if f == Func::Tap {
                        self.tap();
                    }
                    self.push_outputs();
                }
            }
        }
        // Notes recorded by the engine while you played along.
        let mut recorded = false;
        while let Ok(ev) = self.seq_rx.try_recv() {
            match ev {
                SeqEvent::Recorded(n) => recorded |= self.kit.pattern.insert(n),
                SeqEvent::Len { step, pad, len } => recorded |= self.kit.pattern.set_len(step, pad, len),
            }
        }
        if recorded {
            self.kit_dirty = Some(Instant::now());
        }
    }

    /// Pad glow and light-bar flashes follow what the engine actually played.
    fn track_hits(&mut self) {
        let flash = self.settings.lightbar == LightbarMode::PadFlash;
        for i in 0..PAD_COUNT {
            let c = self.audio.hit_count(i);
            if c != self.hit_seen[i] {
                self.hit_seen[i] = c;
                self.hit_at[i] = Some(Instant::now());
                if flash {
                    let _ = self.shared.hid.send(HidCmd::Flash(scaled(self.kit.pads[i].color, self.settings.lightbar_brightness)));
                }
            }
        }
        let step = self.audio.step();
        if step != self.last_step {
            self.last_step = step;
            if self.settings.lightbar == LightbarMode::Beat && step.is_some_and(|s| s % 4 == 0) {
                let _ = self.shared.hid.send(HidCmd::Flash([255, 255, 255]));
            }
        }
    }

    fn watch_audio(&mut self) {
        if !self.audio_wanted || self.audio_retry.elapsed() < Duration::from_secs(2) {
            return;
        }
        if self.audio.failed() || !self.engine.is_open() {
            self.audio_retry = Instant::now();
            self.reopen_audio(false);
        }
    }

    fn reopen_audio(&mut self, announce: bool) {
        let was_broken = self.audio_error.is_some() || self.audio.failed();
        match self.engine.open(self.settings.audio_device.as_deref(), self.settings.buffer) {
            Ok(()) => {
                self.audio_error = None;
                if was_broken || announce {
                    self.toast(lf("Звук: {}", "Sound: {}", &[&self.engine.device_name]), OK);
                }
            }
            Err(e) => {
                if announce || self.audio_error.as_deref() != Some(&e) {
                    self.toast(lf("Звук недоступен: {}", "Sound is unavailable: {}", &[&e]), DANGER);
                }
                self.audio_error = Some(e);
            }
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let (dropped, pointer) = ctx.input(|i| {
            (i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect::<Vec<_>>(), i.pointer.hover_pos())
        });
        if dropped.is_empty() {
            return;
        }
        let files: Vec<PathBuf> = dropped.into_iter().filter(|p| library::is_audio(p)).collect();
        if files.is_empty() {
            self.toast(l("Перетащите аудиофайлы (wav, mp3, flac, ogg, aiff, m4a)", "Drop audio files (wav, mp3, flac, ogg, aiff, m4a)"), WARN);
            return;
        }
        let target = pointer.and_then(|p| self.pad_rects.iter().position(|r| r.contains(p))).unwrap_or(self.selected);
        let count = files.len();
        let mut files = files.into_iter();
        if let Some(first) = files.next() {
            self.assign_sample(target, first);
        }
        let mut slot = target;
        for f in files {
            let next = (1..PAD_COUNT).map(|k| (slot + k) % PAD_COUNT).find(|&j| self.kit.pads[j].sample.is_none());
            let Some(j) = next else { break };
            self.assign_sample(j, f);
            slot = j;
        }
        self.selected = target;
        let accent = self.accent();
        let msg = if count == 1 { l("Сэмпл загружен", "Sample loaded").to_string() } else { lf("Загружено сэмплов: {}", "{} samples loaded", &[&count]) };
        self.toast(msg, accent);
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let events: Vec<(Key, bool)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed, repeat: false, .. } => Some((*key, *pressed)),
                    _ => None,
                })
                .collect()
        });
        for (key, pressed) in events {
            if key == Key::Escape && pressed {
                if self.learning.is_some() {
                    self.cancel_learning();
                    self.toast(l("Назначение отменено", "Binding cancelled"), theme::dim());
                } else if matches!(self.page, Page::Settings | Page::Help) {
                    self.page = Page::None;
                }
                continue;
            }
            if key == Key::F1 && pressed && self.page != Page::Welcome {
                self.page = if self.page == Page::Help { Page::None } else { Page::Help };
                continue;
            }
            if self.page != Page::None || ctx.egui_wants_keyboard_input() {
                continue;
            }
            match key {
                Key::Space if pressed => self.toggle_play(),
                Key::Enter if pressed => self.toggle_record(),
                Key::Backspace if pressed => self.stop_everything(),
                _ => {
                    if let Some(i) = PAD_KEYS.iter().position(|k| *k == key) {
                        if pressed {
                            self.shared.fire(i);
                        } else {
                            self.shared.release(i);
                        }
                    }
                }
            }
        }
    }

    fn autosave(&mut self) {
        let due = |t: Option<Instant>| t.is_some_and(|t| t.elapsed() > Duration::from_millis(800));
        if due(self.kit_dirty) {
            self.save_kit();
        }
        if due(self.settings_dirty) {
            self.settings_dirty = None;
            let res = kit::save_settings(&self.settings);
            self.report_save(res);
        }
        if due(self.profiles_dirty) {
            self.profiles_dirty = None;
            let res = kit::save_profiles(&self.profiles);
            self.report_save(res);
        }
    }

    /// Everything that has to happen once per frame before drawing.
    pub(crate) fn tick(&mut self, ctx: &egui::Context) {
        self.process_messages();
        self.handle_drops(ctx);
        self.handle_keyboard(ctx);
        self.track_hits();
        self.watch_audio();
        if self.demo_pending && self.loading.iter().all(|l| !*l) && !self.factory_installing {
            self.demo_pending = false;
            self.play_demo();
        }
        let rec = self.audio.recording();
        if rec != self.last_rec {
            self.last_rec = rec;
            self.push_outputs();
        }
        let peak = self.audio.take_peak();
        self.meter = (self.meter * 0.86).max(peak);
    }

    /// Everything that has to happen once per frame after drawing.
    pub(crate) fn after_frame(&mut self) {
        self.run_actions();
        self.autosave();
    }

    fn run_actions(&mut self) {
        for action in std::mem::take(&mut self.actions) {
            match action {
                Action::Browse(i) => self.browse(i),
                Action::Clear(i) => self.clear_sample(i),
                Action::Learn(what) => self.start_learning(what),
                Action::CancelLearn => self.cancel_learning(),
                Action::Bind(i, input) => self.set_binding(i, input),
                Action::SetFunc(f, input) => self.set_function(f, input),
                Action::Preview(i) => self.shared.fire(i),
                Action::ChooseSample(i) => {
                    self.selected = i;
                    self.inspector_tab = 1;
                    if let Some(cat) = library::guess_category(&self.pad_name(i)) {
                        self.library.open_factory_category(cat);
                    }
                }
                Action::SelectPad(i) => {
                    self.selected = i;
                    self.inspector_tab = 0;
                }
                Action::LibRoot(i) => self.library.open_root(i),
                Action::LibOpen(p) => self.library.open_dir(p),
                Action::LibUp => self.library.up(),
                Action::LibPreview(p) => self.preview_file(p),
                Action::LibAssign(p) => {
                    let i = self.selected;
                    let sample = naming::clean_name(&p);
                    self.assign_sample(i, p);
                    let accent = self.accent();
                    self.toast(lf("{} → пэд {}", "{} → pad {}", &[&sample, &(i + 1)]), accent);
                }
                Action::LibLoadFolder(p) => self.load_folder_onto_pads(&p),
                Action::InstallFactory => self.start_factory_install(),
                Action::AddFolder => {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        if !self.settings.library_folders.contains(&dir) {
                            self.settings.library_folders.push(dir);
                            self.library.refresh_roots(&self.settings.library_folders);
                            self.library.open_root(self.library.roots.len() - 1);
                            self.settings_dirty = Some(Instant::now());
                        }
                    }
                }
                Action::RemoveFolder(i) => {
                    if i < self.settings.library_folders.len() {
                        self.settings.library_folders.remove(i);
                        self.library.refresh_roots(&self.settings.library_folders);
                        self.library.open_root(0);
                        self.settings_dirty = Some(Instant::now());
                    }
                }
                Action::UseProfile(i) => self.switch_profile(i),
                Action::DeleteProfile(i) => self.delete_profile(i),
                Action::RenameProfile => self.rename_profile(),
                Action::CreateProfile => self.create_profile(),
                Action::Open(page) => self.page = page,
                Action::OpenSettings(tab) => {
                    self.page = Page::Settings;
                    self.settings_tab = tab;
                    if self.audio_wanted && self.devices.is_empty() {
                        self.devices = AudioEngine::devices();
                    }
                }
                Action::ClosePage => {
                    self.page = Page::None;
                    self.delete_armed = None;
                    self.profile_error = None;
                }
                Action::FinishWelcome => {
                    self.page = Page::None;
                    self.settings.onboarded = true;
                    self.settings_dirty = Some(Instant::now());
                    let hello = lf("Привет, {}! Жми кнопки — это твой кит.", "Hello, {}! Press the buttons — this is your kit.", &[&self.prof().name]);
                    let accent = self.accent();
                    self.toast(hello, accent);
                }
                Action::ResetFx => {
                    self.shared.reset_fx();
                    self.toast(l("Эффекты сброшены", "Effects reset"), theme::dim());
                }
                Action::LoadFactoryKit(id) => self.load_factory_kit(&id),
                Action::LoadUserKit(path) => self.load_user_kit(&path),
                Action::SaveKitAs => self.save_kit_as(),
                Action::ImportKit => self.import_kit(),
                Action::ExportKit => self.export_kit(),
                Action::ResetKit => self.reset_kit(),
                Action::OpenKitsFolder => {
                    let dir = kit::kits_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer").arg(dir).spawn();
                }
                Action::Play => self.toggle_play(),
                Action::Record => self.toggle_record(),
                Action::StopAll => self.stop_everything(),
                Action::Tap => self.tap(),
                Action::Demo => self.demo_pending = true,
                Action::ClearPattern => {
                    self.kit.pattern.notes.clear();
                    self.pattern_changed();
                    self.toast(l("Луп очищен", "Loop cleared"), theme::dim());
                }
                Action::ClearPadNotes(pad) => {
                    self.kit.pattern.clear_pad(pad as u8);
                    self.pattern_changed();
                }
                Action::ToggleStep(pad, step) => self.toggle_step(pad, step),
                Action::SetBars(bars) => {
                    self.kit.pattern.set_bars(bars);
                    self.pattern_changed();
                }
                Action::RefreshDevices => self.devices = AudioEngine::devices(),
                Action::ReopenAudio => self.reopen_audio(true),
            }
        }
    }

    /// Put every sound of a folder onto the pads (in name order).
    fn load_folder_onto_pads(&mut self, dir: &Path) {
        let files = library::audio_files_in(dir);
        if files.is_empty() {
            return;
        }
        self.audio.stop_all();
        for (i, f) in files.into_iter().take(PAD_COUNT).enumerate() {
            self.kit.pads[i].name = String::new();
            self.kit.pads[i].mode = PlayMode::OneShot;
            self.kit.pads[i].choke = 0;
            self.kit.pads[i].pitch = 0.0;
            self.assign_sample(i, f);
        }
        let label = dir.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        let accent = self.accent();
        self.toast(lf("Папка «{}» разложена по пэдам", "Folder “{}” is on the pads", &[&label]), accent);
    }

    /// React to changes made in Settings (compares with the state before the edit).
    pub(crate) fn on_settings_changed(&mut self, before: &Settings) {
        self.settings_dirty = Some(Instant::now());
        let s = self.settings.clone();
        let ctx = self.shared.egui.clone();
        if before.language != s.language {
            i18n::set_lang(s.language);
        }
        if before.theme != s.theme
            || before.custom_bg != s.custom_bg
            || before.accent_override != s.accent_override
            || before.round != s.round
            || before.density != s.density
        {
            self.apply_theme();
        }
        if before.colored_glyphs != s.colored_glyphs {
            theme::set_colored_glyphs(s.colored_glyphs);
        }
        if before.custom_skin != s.custom_skin {
            theme::set_custom_skin(s.custom_skin);
        }
        if before.ui_scale != s.ui_scale {
            ctx.set_zoom_factor(s.ui_scale);
        }
        if before.master != s.master {
            self.audio.set_master(s.master);
        }
        if before.metronome != s.metronome {
            self.audio.set_metronome(s.metronome);
        }
        if before.launch_quant != s.launch_quant {
            self.audio.set_launch_quant(s.launch_quant);
        }
        if before.sticks != s.sticks {
            *self.shared.sticks.write().unwrap() = s.sticks;
            self.audio.fx.set_pitch_range(s.sticks.pitch_range);
            self.audio.set_fx_sync(s.sticks.sync);
            if before.sticks.mode != s.sticks.mode || before.sticks.axes() != s.sticks.axes() || before.sticks.touch_fx != s.sticks.touch_fx {
                self.shared.reset_fx();
            }
        }
        if before.functions != s.functions {
            *self.shared.functions.write().unwrap() = s.functions;
        }
        if before.rumble != s.rumble {
            self.shared.rumble.store(s.rumble.to_bits(), Ordering::Relaxed);
        }
        if before.trigger_point != s.trigger_point {
            let _ = self.shared.hid.send(HidCmd::TriggerPoint(s.trigger_point));
        }
        if before.lightbar != s.lightbar || before.lightbar_brightness != s.lightbar_brightness || before.accent_override != s.accent_override {
            self.push_outputs();
        }
        if before.audio_device != s.audio_device || before.buffer != s.buffer {
            self.reopen_audio(true);
        }
    }

    pub(crate) fn save_all(&mut self) {
        if self.kit_dirty.is_some() {
            self.save_kit();
        }
        let _ = kit::save_settings(&self.settings);
        let _ = kit::save_profiles(&self.profiles);
    }
}

impl Drop for SensePadApp {
    fn drop(&mut self) {
        self.save_all();
        let _ = self.shared.hid.send(HidCmd::MuteLed(false));
    }
}

pub fn func_name(f: Func) -> &'static str {
    match f {
        Func::Play => l("Старт / стоп лупа", "Play / stop the loop"),
        Func::Record => l("Запись", "Record"),
        Func::StopAll => l("Стоп всё", "Stop everything"),
        Func::Tap => l("Тап-темп", "Tap tempo"),
    }
}

pub fn fx_name(k: FxKind) -> &'static str {
    match k {
        FxKind::None => l("Выкл", "Off"),
        FxKind::Filter => l("Фильтр", "Filter"),
        FxKind::Echo => l("Эхо", "Echo"),
        FxKind::Pitch => l("Питч", "Pitch bend"),
        FxKind::Volume => l("Приглушение", "Volume cut"),
        FxKind::Crush => l("Биткраш", "Bitcrush"),
        FxKind::Stutter => l("Статтер", "Stutter"),
        FxKind::Pan => l("Панорама", "Pan"),
    }
}
