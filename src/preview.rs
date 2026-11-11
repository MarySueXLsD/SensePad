//! Test-only: draws the interface into PNG files without opening a window, using a tiny
//! software rasteriser for egui's triangle meshes. Run with
//! `cargo test --release preview -- --ignored --nocapture`; pictures land in `target/preview`.

use crate::app::{Boot, Page, SensePadApp};
use crate::dualsense::{ConnInfo, Link, PadSnapshot};
use crate::i18n::Lang;
use crate::input::Input;
use crate::kit::{self, PadLayout};
use crate::theme::ThemeKind;
use eframe::egui::{self, epaint, Color32, Pos2, Rect, TextureId};
use std::collections::HashMap;
use std::path::PathBuf;

struct Tex {
    w: usize,
    h: usize,
    px: Vec<Color32>,
}

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

fn apply_delta(textures: &mut HashMap<TextureId, Tex>, delta: &epaint::textures::TexturesDelta) {
    for (id, deltas) in &delta.set {
        for d in deltas.iter() {
            let egui::ImageData::Color(img) = &d.image;
            let (w, h) = (img.size[0], img.size[1]);
            match d.pos {
                None => {
                    textures.insert(*id, Tex { w, h, px: img.pixels.clone() });
                }
                Some([x0, y0]) => {
                    if let Some(t) = textures.get_mut(id) {
                        for y in 0..h {
                            for x in 0..w {
                                if x0 + x < t.w && y0 + y < t.h {
                                    t.px[(y0 + y) * t.w + x0 + x] = img.pixels[y * w + x];
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn sample(tex: Option<&Tex>, u: f32, v: f32) -> [f32; 4] {
    let Some(t) = tex else { return [1.0; 4] };
    let x = (u * t.w as f32 - 0.5).clamp(0.0, t.w as f32 - 1.0);
    let y = (v * t.h as f32 - 0.5).clamp(0.0, t.h as f32 - 1.0);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(t.w - 1), (y0 + 1).min(t.h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let get = |xx: usize, yy: usize| t.px[yy * t.w + xx].to_array().map(|c| c as f32 / 255.0);
    let (a, b, c, d) = (get(x0, y0), get(x1, y0), get(x0, y1), get(x1, y1));
    std::array::from_fn(|i| (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy)
}

fn raster(canvas: &mut Canvas, prims: &[epaint::ClippedPrimitive], textures: &HashMap<TextureId, Tex>, ppp: f32) {
    for cp in prims {
        let epaint::Primitive::Mesh(mesh) = &cp.primitive else { continue };
        let tex = textures.get(&mesh.texture_id);
        let cx0 = (cp.clip_rect.min.x * ppp).floor().max(0.0) as i64;
        let cy0 = (cp.clip_rect.min.y * ppp).floor().max(0.0) as i64;
        let cx1 = ((cp.clip_rect.max.x * ppp).ceil() as i64).min(canvas.w as i64);
        let cy1 = ((cp.clip_rect.max.y * ppp).ceil() as i64).min(canvas.h as i64);
        for tri in mesh.indices.chunks_exact(3) {
            let v = [&mesh.vertices[tri[0] as usize], &mesh.vertices[tri[1] as usize], &mesh.vertices[tri[2] as usize]];
            let p: [(f32, f32); 3] = std::array::from_fn(|i| (v[i].pos.x * ppp, v[i].pos.y * ppp));
            let edge = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            let area = edge(p[0], p[1], p[2]);
            if area.abs() < 1e-6 {
                continue;
            }
            let minx = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor() as i64).max(cx0);
            let maxx = (p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil() as i64).min(cx1);
            let miny = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor() as i64).max(cy0);
            let maxy = (p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil() as i64).min(cy1);
            let col: [[f32; 4]; 3] = std::array::from_fn(|i| v[i].color.to_array().map(|c| c as f32 / 255.0));
            for y in miny..maxy {
                for x in minx..maxx {
                    let q = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = edge(p[1], p[2], q) / area;
                    let w1 = edge(p[2], p[0], q) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let u = w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x;
                    let vv = w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y;
                    let texel = sample(tex, u, vv);
                    let src: [f32; 4] = std::array::from_fn(|i| (w0 * col[0][i] + w1 * col[1][i] + w2 * col[2][i]) * texel[i]);
                    let dst = &mut canvas.px[y as usize * canvas.w + x as usize];
                    for i in 0..4 {
                        dst[i] = src[i] + dst[i] * (1.0 - src[3]);
                    }
                }
            }
        }
    }
}

pub(crate) struct Stage {
    pub(crate) ctx: egui::Context,
    pub(crate) app: SensePadApp,
    textures: HashMap<TextureId, Tex>,
    pub(crate) time: f64,
    pub(crate) size: (f32, f32),
    pub(crate) out: PathBuf,
}

impl Stage {
    /// A stage for scripted recordings: a fresh data folder, the English interface and
    /// the welcome tour already finished.
    pub(crate) fn fresh(data_name: &str, out: PathBuf) -> Stage {
        let data = std::env::temp_dir().join(data_name);
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("config.json"), r#"{"language":"En","onboarded":true}"#).unwrap();
        kit::set_data_dir(data);
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        let ctx = egui::Context::default();
        let app = SensePadApp::boot(ctx.clone(), Boot { audio: false, hid: false });
        let mut s = Stage { ctx, app, textures: HashMap::new(), time: 0.0, size: (1500.0, 980.0), out };
        s.wait_until_ready();
        s
    }

    /// Advance the clock by `dt` seconds, draw one frame and return it as a picture.
    /// `over` paints above everything (captions, title cards); with `show_app` off it is
    /// the only thing drawn.
    pub(crate) fn picture(&mut self, dt: f64, show_app: bool, over: impl FnOnce(&egui::Painter, Rect)) -> image::RgbaImage {
        self.time += dt;
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(self.size.0, self.size.1));
        let input = egui::RawInput { screen_rect: Some(screen), time: Some(self.time), ..Default::default() };
        let app = &mut self.app;
        let mut over = Some(over);
        let out = self.ctx.run_ui(input, |ui| {
            if show_app {
                app.frame(ui);
            }
            let top = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Debug, egui::Id::unique("stage-overlay")));
            if let Some(over) = over.take() {
                over(&top, screen);
            }
        });
        apply_delta(&mut self.textures, &out.textures_delta);
        let ppp = out.pixels_per_point;
        let prims = self.ctx.tessellate(out.shapes, ppp);
        let (w, h) = ((self.size.0 * ppp) as usize, (self.size.1 * ppp) as usize);
        let mut canvas = Canvas { w, h, px: vec![[0.0, 0.0, 0.0, 1.0]; w * h] };
        raster(&mut canvas, &prims, &self.textures, ppp);
        let bytes: Vec<u8> = canvas.px.iter().flat_map(|p| [p[0], p[1], p[2], 1.0].map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8)).collect();
        image::RgbaImage::from_raw(w as u32, h as u32, bytes).expect("image")
    }

    pub(crate) fn frame(&mut self) -> (Vec<epaint::ClippedShape>, f32) {
        self.time += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(self.size.0, self.size.1))),
            time: Some(self.time),
            ..Default::default()
        };
        let app = &mut self.app;
        let out = self.ctx.run_ui(input, |ui| app.frame(ui));
        apply_delta(&mut self.textures, &out.textures_delta);
        (out.shapes, out.pixels_per_point)
    }

    /// Let layout settle, then save a picture.
    fn shot(&mut self, name: &str) {
        for _ in 0..30 {
            self.frame();
        }
        let (shapes, ppp) = self.frame();
        let prims = self.ctx.tessellate(shapes, ppp);
        let (w, h) = ((self.size.0 * ppp) as usize, (self.size.1 * ppp) as usize);
        let mut canvas = Canvas { w, h, px: vec![[0.0, 0.0, 0.0, 1.0]; w * h] };
        raster(&mut canvas, &prims, &self.textures, ppp);
        let bytes: Vec<u8> = canvas.px.iter().flat_map(|p| [p[0], p[1], p[2], 1.0].map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8)).collect();
        let img = image::RgbaImage::from_raw(w as u32, h as u32, bytes).expect("image");
        let path = self.out.join(format!("{name}.png"));
        img.save(&path).expect("save png");
        println!("saved {}", path.display());
    }

    pub(crate) fn wait_until_ready(&mut self) {
        for _ in 0..1200 {
            self.frame();
            if !self.app.factory_installing && self.app.loading.iter().all(|l| !*l) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(!self.app.factory_installing, "factory sounds were never finished");
    }

    pub(crate) fn set(&mut self, change: impl FnOnce(&mut kit::Settings)) {
        let before = self.app.settings.clone();
        change(&mut self.app.settings);
        self.app.on_settings_changed(&before);
    }
}

pub(crate) fn fake_controller() -> PadSnapshot {
    PadSnapshot {
        conn: Some(ConnInfo { model: "DualSense", link: Link::Usb }),
        buttons: Input::Cross.bit() | Input::L1.bit(),
        lx: 0.6,
        ly: -0.3,
        rx: 0.0,
        ry: 0.0,
        l2: 0.0,
        r2: 0.7,
        touches: [Some((0.3, 0.4)), None],
        battery: Some(85),
        charging: false,
        lightbar: [94, 234, 212],
        error: None,
    }
}

#[test]
#[ignore = "renders pictures of the whole interface; run on request"]
fn preview_every_screen() {
    let data = std::env::temp_dir().join("sensepad_preview_data");
    let _ = std::fs::remove_dir_all(&data);
    // The pictures are taken in English whatever language this computer uses.
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("config.json"), r#"{"language":"En"}"#).unwrap();
    kit::set_data_dir(data);
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("preview");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();

    let ctx = egui::Context::default();
    let app = SensePadApp::boot(ctx.clone(), Boot { audio: false, hid: false });
    let mut s = Stage { ctx, app, textures: HashMap::new(), time: 0.0, size: (1500.0, 980.0), out };
    assert_eq!(s.app.page, Page::Welcome, "a fresh install starts with the welcome tour");
    s.wait_until_ready();
    assert_eq!(s.app.profiles.len(), 1, "a fresh install has one profile");
    assert_eq!(s.app.prof().name, "Player 1");
    assert!(!s.app.kit.is_empty(), "a fresh profile gets a factory kit");
    assert!(s.app.samples.iter().all(|x| x.is_some()), "every factory pad has its sound loaded");

    for step in 0..5 {
        s.app.welcome_step = step;
        if step == 1 {
            *s.app.snapshot.lock().unwrap() = fake_controller();
        }
        s.shot(&format!("welcome_{step}"));
    }

    s.app.page = Page::None;
    s.app.settings.onboarded = true;
    s.app.toasts.clear();
    s.shot("main");
    s.app.inspector_tab = 1;
    s.shot("library_kits");
    s.app.library.open_factory_category("Kicks");
    s.shot("library_kicks");
    s.app.inspector_tab = 0;

    // --- sample packs (only when the packs folder is installed) -------------
    use crate::app::Action;
    use crate::library::{self, RootKind};
    if let Some(root) = s.app.library.roots.iter().position(|r| r.kind == RootKind::Pack) {
        s.app.actions.push(Action::LoadFactoryKit("Live Breaks".into()));
        s.frame();
        s.wait_until_ready();
        s.app.toasts.clear();
        assert_eq!(s.app.kit.factory.as_deref(), Some("Live Breaks"));
        assert!(s.app.samples.iter().all(|x| x.is_some()), "every pack sound of the kit loads");
        assert_eq!(s.app.kit.pads[4].sync_bpm, 170.0, "break loops follow the tempo");
        s.shot("live_breaks");

        s.app.inspector_tab = 1;
        s.app.library.open_root(root);
        s.shot("pack_root");
        let snares = s.app.library.categories.iter().find(|c| c.ends_with("Snares")).cloned().expect("the pack has snares");
        s.app.library.open_dir(snares);
        s.shot("pack_snares");
        s.app.inspector_tab = 0;

        s.app.actions.push(Action::LoadFactoryKit("Percussion Jam".into()));
        s.frame();
        s.wait_until_ready();
        s.app.toasts.clear();
        assert!(s.app.samples.iter().all(|x| x.is_some()), "the percussion kit loads");
        s.shot("percussion");

        // A kit saved before the program folder was moved still finds its pack sounds.
        let (name, dir) = &library::packs()[0];
        let real = library::audio_files_in(&library::subdirs(dir)[0])[0].clone();
        let moved = std::path::Path::new("Z:/old place/packs").join(name).join(real.strip_prefix(dir).unwrap());
        assert_eq!(library::relocate_pack_sound(&moved), Some(real));

        s.app.actions.push(Action::LoadFactoryKit("DnB Essentials".into()));
        s.frame();
        s.wait_until_ready();
        s.app.toasts.clear();
    }

    for tab in 0..8 {
        s.app.page = Page::Settings;
        s.app.settings_tab = tab;
        s.shot(&format!("settings_{tab}"));
    }
    for topic in [0, 3, 7] {
        s.app.page = Page::Help;
        s.app.help_topic = topic;
        s.shot(&format!("help_{topic}"));
    }
    s.app.page = Page::None;

    s.set(|c| c.language = Lang::Ru);
    s.shot("main_ru");
    s.set(|c| c.language = Lang::En);
    for (theme, name) in [(ThemeKind::Light, "light"), (ThemeKind::Midnight, "midnight"), (ThemeKind::Sunset, "sunset")] {
        s.set(|c| c.theme = theme);
        s.shot(&format!("theme_{name}"));
    }
    s.set(|c| {
        c.theme = ThemeKind::Studio;
        c.pad_layout = PadLayout::Wide8x2;
        c.show_fx = false;
    });
    s.shot("layout_wide");
    s.set(|c| {
        c.pad_layout = PadLayout::Grid4x4;
        c.show_fx = true;
    });
    s.size = (1180.0, 760.0);
    s.shot("small_window");
    s.size = (1500.0, 980.0);

    // --- behaviour: the things a player does survive a restart -------------
    s.app.actions.push(Action::LoadFactoryKit("Phonk".into()));
    s.frame();
    s.wait_until_ready();
    assert_eq!(s.app.kit.factory.as_deref(), Some("Phonk"));
    assert_eq!(s.app.kit.bpm, 130.0);
    assert!(s.app.samples.iter().all(|x| x.is_some()), "the new kit is fully loaded");
    assert_eq!(s.app.kit.pads[0].binding, Some(Input::Cross), "buttons stay on their pads when the kit changes");
    s.app.toasts.clear();
    s.shot("phonk");

    // A function button given to a pad stops being a function.
    s.app.actions.push(Action::Bind(3, Some(Input::Options)));
    s.frame();
    assert_eq!(s.app.kit.pads[3].binding, Some(Input::Options));
    assert_eq!(s.app.settings.functions.stop_all, None);

    // Steps can be toggled by hand.
    let had = s.app.kit.pattern.find(1, 0).is_some();
    s.app.actions.push(Action::ToggleStep(0, 1));
    s.frame();
    assert_ne!(s.app.kit.pattern.find(1, 0).is_some(), had);

    s.set(|c| {
        c.language = Lang::Ru;
        c.theme = ThemeKind::Taiga;
        c.round = 0.5;
    });
    s.app.save_all();
    let saved = kit::load_settings();
    assert_eq!(saved.language, Lang::Ru);
    assert_eq!(saved.theme, ThemeKind::Taiga);
    assert!(saved.onboarded);
    assert_eq!(saved.functions.stop_all, None);
    assert!(kit::app_dir().join("config.json").exists());
    let (kit_back, fresh) = kit::load_profile_kit("Player 1");
    assert!(!fresh);
    assert_eq!(kit_back.factory.as_deref(), Some("Phonk"));
    assert_eq!(kit_back.pads[3].binding, Some(Input::Options));
    assert_eq!(kit_back.pattern, s.app.kit.pattern);

    // The name typed in the welcome tour becomes the profile's name and keeps its kit.
    s.app.rename_buf = "Alex".into();
    s.app.actions.push(Action::RenameProfile);
    s.frame();
    assert_eq!(s.app.prof().name, "Alex");
    s.app.save_all();
    assert_eq!(kit::load_settings().profile, "Alex");
    assert!(!kit::load_profile_kit("Alex").1, "the renamed profile keeps its kit");
    s.app.rename_buf = "Player 1".into();
    s.app.actions.push(Action::RenameProfile);
    s.frame();

    // Another player gets their own kit; coming back restores the first one.
    s.app.new_name = "Guest".into();
    s.app.actions.push(Action::CreateProfile);
    s.frame();
    s.wait_until_ready();
    assert_eq!(s.app.prof().name, "Guest");
    assert_eq!(s.app.kit.factory.as_deref(), Some("Jungle Massive"));
    s.app.actions.push(Action::UseProfile(0));
    s.frame();
    s.wait_until_ready();
    assert_eq!(s.app.kit.factory.as_deref(), Some("Phonk"));
}

#[test]
#[ignore = "opens the real sound device for a moment (silently); run on request"]
fn sound_device_opens_in_every_latency_mode() {
    use crate::audio::AudioEngine;
    use crate::kit::BufferPref;
    let (mut engine, _seq) = AudioEngine::new(0.8);
    println!("devices: {:?}", AudioEngine::devices());
    for pref in [BufferPref::Auto, BufferPref::Low, BufferPref::Safe] {
        engine.open(None, pref).expect("open the default output");
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert!(engine.is_open() && engine.handle.is_online() && !engine.handle.failed(), "{pref:?}");
        println!("{pref:?}: {} at {} Hz", engine.device_name, engine.sample_rate);
    }
}
