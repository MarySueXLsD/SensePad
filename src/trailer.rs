//! Test-only: records the README preview and the trailer without opening a window.
//!
//! The app is driven by a script on the headless stage (`preview.rs`): the sound engine
//! runs offline in step with the pictures, a pretend controller presses the buttons the
//! loop plays, and title cards and captions are painted on top. Frames and the sound
//! land in `target/trailer`; `tools/trailer/build.py` turns them into the GIF and video.
//!
//! ```text
//! cargo test --release render_trailer -- --ignored --nocapture
//! python tools/trailer/build.py
//! ```

use crate::app::{Action, Page};
use crate::dualsense::HidEvent;
use crate::input::Input;
use crate::kit::PlayMode;
use crate::library::RootKind;
use crate::preview::{fake_controller, Stage};
use crate::theme::{self, ThemeKind};
use eframe::egui::{self, pos2, vec2, Align2, Color32, Painter, Rect};
use std::path::{Path, PathBuf};

/// Video frames per second.
const FPS: f64 = 30.0;
/// The engine runs at 48 kHz, so one video frame is this many audio frames.
const AUDIO_PER_FRAME: usize = 1600;

const REPO: &str = "github.com/MarySueXLsD/SensePad";
const AUTHOR: &str = "MarySue";
const AUTHOR_LINKS: &str = "github.com/MarySueXLsD    ·    marysue.itch.io";

struct Film {
    s: Stage,
    dir: PathBuf,
    count: usize,
    audio: Vec<f32>,
    keep_audio: bool,
    last_step: Option<usize>,
    /// Buttons the pretend controller is holding, with the frames left.
    held: Vec<(Input, u32)>,
    /// Colour and frames left of the light-bar flash.
    glow: Option<([u8; 3], u32)>,
    /// Stick axes: left x, left y, right x, right y.
    sticks: [f32; 4],
}

impl Film {
    fn new(s: Stage) -> Self {
        s.app.engine.run_without_device();
        Self { s, dir: PathBuf::new(), count: 0, audio: Vec::new(), keep_audio: false, last_step: None, held: Vec::new(), glow: None, sticks: [0.0; 4] }
    }

    /// Start a new numbered run of frames in `dir`.
    fn reel(&mut self, dir: PathBuf, keep_audio: bool) {
        std::fs::create_dir_all(&dir).unwrap();
        self.dir = dir;
        self.count = 0;
        self.keep_audio = keep_audio;
    }

    fn frames_for_bars(&self, bars: f64) -> usize {
        (FPS * 240.0 / self.s.app.kit.bpm as f64 * bars).round() as usize
    }

    /// Start the loop from its first step.
    fn restart_loop(&mut self) {
        self.s.app.audio.stop();
        self.s.app.audio.stop_all();
        self.s.app.audio.play();
        self.last_step = None;
        self.held.clear();
    }

    /// The pretend controller presses whatever the loop plays on this step.
    fn follow_the_loop(&mut self) {
        let step = self.s.app.audio.step();
        if step == self.last_step {
            return;
        }
        self.last_step = step;
        let Some(step) = step else { return };
        let kit = &self.s.app.kit;
        let frames_per_step = (FPS * 15.0 / kit.bpm as f64).ceil() as u32;
        for note in kit.pattern.notes.iter().filter(|n| n.step as usize == step) {
            let pad = &kit.pads[note.pad as usize];
            let Some(input) = pad.binding else { continue };
            let hold = if pad.mode == PlayMode::OneShot { 3 } else { (note.len as u32 * frames_per_step).max(3) };
            self.held.retain(|(i, _)| *i != input);
            self.held.push((input, hold));
            self.glow = Some((pad.color, 6));
        }
    }

    /// One frame of video: move the controller, run the engine for 1/30 s, draw, save.
    fn frame(&mut self, show_app: bool, over: impl FnOnce(&Painter, Rect)) {
        self.follow_the_loop();
        let mut snap = fake_controller();
        snap.buttons = 0;
        snap.touches = [None, None];
        (snap.l2, snap.r2) = (0.0, 0.0);
        for (input, left) in self.held.iter_mut() {
            snap.buttons |= input.bit();
            match input {
                Input::L2 => snap.l2 = 1.0,
                Input::R2 => snap.r2 = 1.0,
                _ => {}
            }
            *left -= 1;
        }
        self.held.retain(|(_, left)| *left > 0);
        [snap.lx, snap.ly, snap.rx, snap.ry] = self.sticks;
        if let Some((color, left)) = self.glow {
            let k = left as f32 / 6.0;
            snap.lightbar = std::array::from_fn(|i| (snap.lightbar[i] as f32 * (1.0 - k) + color[i] as f32 * k) as u8);
            self.glow = (left > 1).then_some((color, left - 1));
        }
        *self.s.app.snapshot.lock().unwrap() = snap;
        self.s.app.shared.on_hid(HidEvent::Sticks(self.sticks, 1.0 / FPS as f32));

        let chunk = self.s.app.engine.render_offline(AUDIO_PER_FRAME);
        if self.keep_audio {
            self.audio.extend_from_slice(&chunk);
        }

        let img = self.s.picture(1.0 / FPS, show_app, over);
        save_fast(&img, &self.dir.join(format!("{:05}.png", self.count)));
        self.count += 1;
    }

    /// `frames` frames of the app with a caption along the bottom.
    fn scene(&mut self, frames: usize, text: &str) {
        let accent = self.s.app.accent();
        for i in 0..frames {
            let a = fade(i, frames);
            self.frame(true, |p, screen| caption(p, screen, text, accent, a));
        }
    }
}

/// Fade in over the first frames and out over the last.
fn fade(i: usize, frames: usize) -> f32 {
    let edge = 8.0;
    ((i as f32 + 1.0) / edge).min((frames - i) as f32 / edge).clamp(0.0, 1.0)
}

fn save_fast(img: &image::RgbaImage, path: &Path) {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    PngEncoder::new_with_quality(file, CompressionType::Fast, FilterType::Sub)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .unwrap();
}

fn save_wav(path: &Path, stereo: &[f32]) {
    let data_len = (stereo.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&(48_000u32 * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in stereo {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

/// A caption on a dark band along the bottom of the picture.
fn caption(p: &Painter, screen: Rect, text: &str, accent: Color32, a: f32) {
    let band = Rect::from_min_max(pos2(screen.left(), screen.bottom() - 92.0), screen.max);
    p.rect_filled(band, egui::CornerRadius::ZERO, Color32::from_black_alpha((235.0 * a) as u8));
    p.rect_filled(Rect::from_min_size(band.min, vec2(band.width(), 3.0)), egui::CornerRadius::ZERO, accent.gamma_multiply(a));
    p.text(band.center(), Align2::CENTER_CENTER, text, theme::display(40.0), Color32::WHITE.gamma_multiply(a));
}

fn backdrop(p: &Painter, screen: Rect) {
    p.rect_filled(screen, egui::CornerRadius::ZERO, Color32::from_rgb(14, 15, 20));
}

fn title_card(p: &Painter, screen: Rect, accent: Color32, a: f32) {
    backdrop(p, screen);
    let c = screen.center();
    theme::logo(p, Rect::from_center_size(pos2(c.x, c.y - 170.0), vec2(150.0, 150.0)), accent.gamma_multiply(a));
    p.text(pos2(c.x, c.y + 10.0), Align2::CENTER_CENTER, "SensePad", theme::display(128.0), Color32::WHITE.gamma_multiply(a));
    p.text(pos2(c.x, c.y + 112.0), Align2::CENTER_CENTER, "Turn a DualSense controller into a drum machine", theme::display(40.0), Color32::from_gray(185).gamma_multiply(a));
}

fn end_card(p: &Painter, screen: Rect, accent: Color32, a: f32) {
    backdrop(p, screen);
    let c = screen.center() + vec2(0.0, 55.0);
    let grey = Color32::from_gray(170).gamma_multiply(a);
    theme::logo(p, Rect::from_center_size(pos2(c.x, c.y - 300.0), vec2(96.0, 96.0)), accent.gamma_multiply(a));
    p.text(pos2(c.x, c.y - 190.0), Align2::CENTER_CENTER, "SensePad", theme::display(84.0), Color32::WHITE.gamma_multiply(a));
    p.text(pos2(c.x, c.y - 118.0), Align2::CENTER_CENTER, "Free for Windows. Open source.", theme::display(34.0), grey);
    p.text(pos2(c.x, c.y - 10.0), Align2::CENTER_CENTER, REPO, theme::display(52.0), accent.gamma_multiply(a));
    p.rect_filled(Rect::from_center_size(pos2(c.x, c.y + 78.0), vec2(120.0, 3.0)), egui::CornerRadius::ZERO, Color32::from_gray(70).gamma_multiply(a));
    p.text(pos2(c.x, c.y + 150.0), Align2::CENTER_CENTER, format!("by {AUTHOR}"), theme::display(46.0), Color32::WHITE.gamma_multiply(a));
    p.text(pos2(c.x, c.y + 214.0), Align2::CENTER_CENTER, AUTHOR_LINKS, theme::display(32.0), grey);
}

#[test]
#[ignore = "records the README preview and the trailer frames; run on request"]
fn render_trailer() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("trailer");
    let mut f = Film::new(Stage::fresh("sensepad_trailer_data", root.clone()));
    f.s.app.toasts.clear();
    let accent = f.s.app.accent();
    // Leave headroom so the recording never hits full scale.
    f.s.app.settings.master = 0.7;
    f.s.app.audio.set_master(0.7);

    // ---- README preview: four bars of the first kit, a filter sweep on the left stick in the third
    f.reel(root.join("preview"), false);
    f.restart_loop();
    let total = f.frames_for_bars(4.0);
    for i in 0..total {
        let bar = i as f32 * 4.0 / total as f32;
        f.sticks[0] = if (2.0..3.0).contains(&bar) { -((bar - 2.0) * std::f32::consts::PI).sin() * 0.9 } else { 0.0 };
        f.frame(true, |_, _| {});
    }
    println!("preview: {} frames", f.count);

    // ---- trailer
    f.reel(root.join("frames"), true);
    f.sticks = [0.0; 4];
    f.restart_loop();
    let two_bars = f.frames_for_bars(2.0);
    for i in 0..two_bars {
        let a = ((i as f32 + 1.0) / 10.0).min(1.0);
        f.frame(false, |p, screen| title_card(p, screen, accent, a));
    }
    f.scene(two_bars, "16 pads. Every button plays a sound.");
    for i in 0..two_bars {
        let t = i as f32 / two_bars as f32;
        // Left stick: a filter sweep. Right stick: a stutter at the end of the phrase.
        f.sticks[0] = -(t * std::f32::consts::PI).sin() * 0.95;
        f.sticks[3] = if t > 0.78 { 0.9 } else { 0.0 };
        let a = fade(i, two_bars);
        f.frame(true, |p, screen| caption(p, screen, "The sticks run DJ effects.", accent, a));
    }
    f.sticks = [0.0; 4];

    // A kit made of recorded drums, when the packs are installed.
    let kit = if crate::factory::find_kit("Live Breaks").is_some_and(|k| crate::factory::kits().iter().any(|x| x.id == k.id)) { "Live Breaks" } else { "Jungle Massive" };
    f.s.app.audio.stop();
    f.s.app.audio.stop_all();
    f.s.app.actions.push(Action::LoadFactoryKit(kit.into()));
    f.s.frame();
    f.s.wait_until_ready();
    f.s.app.toasts.clear();
    assert_eq!(f.s.app.kit.factory.as_deref(), Some(kit));
    f.restart_loop();
    let four_bars = f.frames_for_bars(4.0);
    f.scene(four_bars, "10 kits. Real recorded drums and breaks.");

    // A quick tour while the loop keeps playing.
    let cut = four_bars / 3;
    f.s.app.inspector_tab = 1;
    if let Some(root_i) = f.s.app.library.roots.iter().position(|r| r.kind == RootKind::Pack) {
        f.s.app.library.open_root(root_i);
        if let Some(dir) = f.s.app.library.categories.iter().find(|c| c.ends_with("Snares")).cloned() {
            f.s.app.library.open_dir(dir);
        }
    } else {
        f.s.app.library.open_factory_category("Kicks");
    }
    f.scene(cut, "A sound library sorted by category.");
    f.s.app.inspector_tab = 0;
    f.s.app.page = Page::Settings;
    f.s.app.settings_tab = 3;
    f.scene(cut, "Your controller, your colours.");
    f.s.app.page = Page::None;
    f.s.set(|c| c.theme = ThemeKind::Light);
    f.scene(four_bars - 2 * cut, "Seven themes. English and Russian.");
    f.s.set(|c| c.theme = ThemeKind::Studio);

    let end = (FPS * 4.5) as usize;
    for i in 0..end {
        let a = ((i as f32 + 1.0) / 10.0).min(1.0);
        f.frame(false, |p, screen| end_card(p, screen, accent, a));
    }
    save_wav(&root.join("audio.wav"), &f.audio);
    let peak = f.audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05, "the engine should have played something (peak {peak})");
    println!("trailer: {} frames, {:.1} s, sound peak {peak:.2}", f.count, f.count as f64 / FPS);
}
