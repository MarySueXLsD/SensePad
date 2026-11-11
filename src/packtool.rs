//! Builds the sample packs in `packs/` from CC0 drum recordings.
//!
//! The recordings come as one file per microphone, so each pack sound is a small mix:
//! the microphones are summed, silence is trimmed, the tail is faded and the level is
//! normalised. The break loops are the library's own patterns played with those hits.
//!
//! `tools/packs/plan.py` writes the recipe (`manifest.json`), `tools/packs/fetch.py`
//! downloads the recordings, then:
//!
//! ```text
//! SENSEPAD_PACK_SRC=<download dir> cargo test --release build_packs -- --ignored --nocapture
//! ```

use crate::audio::load_sample;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SR: u32 = 44_100;
/// Peak level of every finished sound.
const PEAK: f32 = 0.89;

#[derive(Deserialize)]
struct Manifest {
    packs: Vec<Pack>,
}

#[derive(Deserialize)]
struct Pack {
    name: String,
    by: String,
    url: String,
    sounds: Vec<Sound>,
    breaks: Vec<Break>,
}

#[derive(Deserialize)]
struct Sound {
    category: String,
    name: String,
    seconds: f32,
    sources: Vec<Source>,
}

#[derive(Deserialize)]
struct Source {
    repo: String,
    path: String,
    gain: f32,
    pan: f32,
}

#[derive(Deserialize)]
struct Break {
    name: String,
    bpm: f32,
    swing: f32,
    /// Pattern voice ("k", "s", "h"…) → "Category/Sound name".
    voices: HashMap<String, String>,
    tracks: Vec<(String, String)>,
}

#[derive(Clone, Default)]
struct Stereo {
    l: Vec<f32>,
    r: Vec<f32>,
}

impl Stereo {
    fn len(&self) -> usize {
        self.l.len()
    }

    fn peak(&self) -> f32 {
        self.l.iter().chain(&self.r).fold(0.0f32, |m, s| m.max(s.abs()))
    }

    fn is_mono(&self) -> bool {
        self.l.iter().zip(&self.r).all(|(a, b)| (a - b).abs() < 1e-5)
    }

    fn scale(&mut self, gain: f32) {
        for s in self.l.iter_mut().chain(self.r.iter_mut()) {
            *s *= gain;
        }
    }

    fn normalise(&mut self) {
        let peak = self.peak();
        if peak > 1e-6 {
            self.scale(PEAK / peak);
        }
    }

    /// Fade the last `n` samples to silence.
    fn fade_out(&mut self, n: usize) {
        let len = self.len();
        let n = n.min(len);
        for i in 0..n {
            let g = (i as f32 / n as f32).powi(2);
            self.l[len - 1 - i] *= g;
            self.r[len - 1 - i] *= g;
        }
    }
}

/// Band-limited resampling (Hann-windowed sinc).
fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return x.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let cutoff = ratio.min(1.0) * 0.96;
    let half = (20.0 / cutoff).ceil() as isize;
    let n = (x.len() as f64 * ratio).round() as usize;
    (0..n)
        .map(|i| {
            let pos = i as f64 / ratio;
            let centre = pos.floor() as isize;
            let mut acc = 0.0f64;
            for k in (centre - half + 1)..=(centre + half) {
                if k < 0 || k as usize >= x.len() {
                    continue;
                }
                let t = pos - k as f64;
                let window = 0.5 + 0.5 * (std::f64::consts::PI * t / half as f64).cos();
                let a = std::f64::consts::PI * t * cutoff;
                let sinc = if a.abs() < 1e-9 { 1.0 } else { a.sin() / a };
                acc += x[k as usize] as f64 * sinc * window * cutoff;
            }
            acc as f32
        })
        .collect()
}

fn load(path: &Path) -> Stereo {
    let s = load_sample(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let l: Vec<f32> = s.frames.iter().step_by(2).copied().collect();
    let r: Vec<f32> = s.frames.iter().skip(1).step_by(2).copied().collect();
    Stereo { l: resample(&l, s.sample_rate, SR), r: resample(&r, s.sample_rate, SR) }
}

/// Sum the microphones of one hit. A microphone that would cancel the mix so far
/// (a bottom snare mic, for example) is flipped.
fn mix(sources: &[Source], src_dir: &Path) -> Stereo {
    let mut out = Stereo::default();
    for s in sources {
        let x = load(&src_dir.join(&s.repo).join(&s.path));
        let (gl, gr) = (s.gain * (1.0 - s.pan).min(1.0), s.gain * (1.0 + s.pan).min(1.0));
        if x.len() > out.len() {
            out.l.resize(x.len(), 0.0);
            out.r.resize(x.len(), 0.0);
        }
        let dot: f64 = (0..x.len()).map(|i| ((out.l[i] + out.r[i]) * (x.l[i] * gl + x.r[i] * gr)) as f64).sum();
        let sign = if dot < 0.0 { -1.0 } else { 1.0 };
        for i in 0..x.len() {
            out.l[i] += x.l[i] * gl * sign;
            out.r[i] += x.r[i] * gr * sign;
        }
    }
    out
}

/// Cut the silence before the hit, end the sound once it has rung out (or after
/// `seconds`), and smooth both ends.
fn trim(x: &mut Stereo, seconds: f32) {
    let peak = x.peak().max(1e-9);
    let loud = |x: &Stereo, i: usize, level: f32| x.l[i].abs() > level || x.r[i].abs() > level;
    // A pad has to answer at once, so quiet noise ahead of the hit (a pedal moving, a stick
    // being lifted) goes too: the sound starts where it first reaches 2% of its peak.
    let first = (0..x.len()).find(|&i| loud(x, i, peak * 0.02)).unwrap_or(0);
    let start = first.saturating_sub(SR as usize / 500);
    x.l.drain(..start);
    x.r.drain(..start);
    let attack = (SR as usize / 2000).min(x.len());
    for i in 0..attack {
        let g = i as f32 / attack as f32;
        x.l[i] *= g;
        x.r[i] *= g;
    }
    let last = (0..x.len()).rev().find(|&i| loud(x, i, peak * 0.001)).unwrap_or(0);
    let natural = (last + SR as usize / 20).min(x.len());
    let cap = (seconds * SR as f32) as usize;
    if natural > cap {
        x.l.truncate(cap);
        x.r.truncate(cap);
        x.fade_out(cap / 3);
    } else {
        x.l.truncate(natural);
        x.r.truncate(natural);
        x.fade_out(SR as usize / 20);
    }
}

/// How loud each pattern voice sits in a break.
fn voice_level(voice: &str) -> f32 {
    match voice {
        "k" => 1.0,
        "s" => 0.9,
        "h" | "oh" => 0.5,
        "r" => 0.4,
        "c" => 0.55,
        _ => 0.6,
    }
}

/// Play a 16th-step pattern with real hits into a seamless loop. Patterns use
/// `x` hit, `X` accent, `o` ghost note, `.` rest. A hi-hat hit cuts the previous one,
/// the way a real hi-hat behaves.
fn render_break(b: &Break, hits: &HashMap<String, Stereo>) -> Stereo {
    let step = 60.0 / b.bpm / 4.0 * SR as f32;
    let steps = b.tracks.iter().map(|(_, p)| p.chars().filter(|c| !c.is_whitespace()).count()).max().unwrap_or(16);
    let n = (step * steps as f32).round() as usize;
    let mut out = Stereo { l: vec![0.0; n], r: vec![0.0; n] };
    // (start, sound, gain, is hi-hat)
    let mut events: Vec<(usize, &Stereo, f32, bool)> = Vec::new();
    for (voice, pattern) in &b.tracks {
        let key = b.voices.get(voice).unwrap_or_else(|| panic!("{}: no sound for voice {voice}", b.name));
        let hit = hits.get(key).unwrap_or_else(|| panic!("{}: missing sound {key}", b.name));
        for (i, ch) in pattern.chars().filter(|c| !c.is_whitespace()).enumerate() {
            let vel = match ch {
                'X' => 1.15,
                'x' => 1.0,
                'o' => 0.35,
                _ => continue,
            };
            let at = ((i as f32 + if i % 2 == 1 { b.swing } else { 0.0 }) * step).round() as usize;
            events.push((at.min(n - 1), hit, vel * voice_level(voice), voice == "h" || voice == "oh"));
        }
    }
    let mut hat_starts: Vec<usize> = events.iter().filter(|e| e.3).map(|e| e.0).collect();
    hat_starts.sort_unstable();
    for (at, hit, gain, is_hat) in events {
        // A hit never lasts longer than the loop, so its tail wraps round at most once.
        let mut len = hit.len().min(n);
        if is_hat {
            let next = hat_starts.iter().copied().find(|&s| s > at).unwrap_or(hat_starts[0] + n);
            len = len.min(next - at);
        }
        let fade = (SR as usize / 150).min(len);
        for j in 0..len {
            let tail = len - j;
            let g = gain * if tail <= fade { tail as f32 / fade as f32 } else { 1.0 };
            let k = (at + j) % n;
            out.l[k] += hit.l[j] * g;
            out.r[k] += hit.r[j] * g;
        }
    }
    // A little saturation glues the hits together like a drum bus would.
    let drive = 1.0 / out.peak().max(1e-6);
    for s in out.l.iter_mut().chain(out.r.iter_mut()) {
        *s = (*s * drive * 1.4).tanh();
    }
    out
}

fn write_wav(path: &Path, x: &Stereo) {
    let mono = x.is_mono();
    let channels: u16 = if mono { 1 } else { 2 };
    let data_len = (x.len() * channels as usize * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&SR.to_le_bytes());
    b.extend_from_slice(&(SR * channels as u32 * 2).to_le_bytes());
    b.extend_from_slice(&(channels * 2).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    let pcm = |s: f32| ((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes();
    for i in 0..x.len() {
        b.extend_from_slice(&pcm(x.l[i]));
        if !mono {
            b.extend_from_slice(&pcm(x.r[i]));
        }
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b).unwrap();
}

fn credits(pack: &Pack) -> String {
    let lines = [
        pack.name.clone(),
        String::new(),
        format!("Recorded by {} and released into the public domain (CC0 1.0).", pack.by),
        format!("Source: {}", pack.url),
        String::new(),
        "For SensePad the microphone recordings of each hit were mixed into one file,".to_string(),
        "trimmed and normalised. The loops in Breaks are new patterns played with these hits.".to_string(),
        String::new(),
    ];
    lines.join("\r\n")
}

fn project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
#[ignore = "needs the downloaded recordings: set SENSEPAD_PACK_SRC"]
fn build_packs() {
    let src = PathBuf::from(std::env::var("SENSEPAD_PACK_SRC").expect("set SENSEPAD_PACK_SRC to the download folder"));
    let text = std::fs::read_to_string(project_dir().join("tools/packs/manifest.json")).expect("manifest.json");
    let manifest: Manifest = serde_json::from_str(&text).expect("manifest.json");
    let out = project_dir().join("packs");
    let (mut files, mut bytes) = (0usize, 0u64);
    for pack in &manifest.packs {
        let dir = out.join(&pack.name);
        let _ = std::fs::remove_dir_all(&dir);
        let mut hits: HashMap<String, Stereo> = HashMap::new();
        for sound in &pack.sounds {
            let mut x = mix(&sound.sources, &src);
            trim(&mut x, sound.seconds);
            x.normalise();
            let secs = x.len() as f32 / SR as f32;
            assert!(secs > 0.03, "{}: only {secs:.3} s long", sound.name);
            assert!(x.l.iter().chain(&x.r).all(|s| s.is_finite()), "{}: bad samples", sound.name);
            let path = dir.join(&sound.category).join(format!("{}.wav", sound.name));
            write_wav(&path, &x);
            println!("{:<18} {:<11} {:<30} {:>5.2} s {}", pack.name, sound.category, sound.name, secs, if x.is_mono() { "mono" } else { "stereo" });
            hits.insert(format!("{}/{}", sound.category, sound.name), x);
        }
        for b in &pack.breaks {
            let mut x = render_break(b, &hits);
            x.normalise();
            write_wav(&dir.join("Breaks").join(format!("{}.wav", b.name)), &x);
            println!("{:<18} {:<11} {:<30} {:>5.2} s loop", pack.name, "Breaks", b.name, x.len() as f32 / SR as f32);
        }
        std::fs::write(dir.join("CREDITS.txt"), credits(pack)).unwrap();
        for cat in crate::library::subdirs(&dir) {
            for f in crate::library::audio_files_in(&cat) {
                // Every file must load back through the app's own decoder at the right level.
                let s = load_sample(&f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
                let peak = s.frames.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                assert!((peak - PEAK).abs() < 0.01, "{}: peak {peak}", f.display());
                files += 1;
                bytes += std::fs::metadata(&f).unwrap().len();
            }
        }
    }
    println!("{files} files, {:.1} MB in {}", bytes as f64 / 1_048_576.0, out.display());
}

#[test]
fn resampling_keeps_pitch_and_level() {
    // One second of 1 kHz at 48 kHz becomes one second of 1 kHz at 44.1 kHz.
    let x: Vec<f32> = (0..48_000).map(|i| (i as f32 * 1000.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5).collect();
    let y = resample(&x, 48_000, SR);
    assert_eq!(y.len(), SR as usize);
    let mid = &y[2_000..42_000];
    let peak = mid.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!((peak - 0.5).abs() < 0.01, "peak {peak}");
    let crossings = mid.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
    assert!((905..=909).contains(&crossings), "{crossings} cycles in 40000 samples");
}

#[test]
fn trimming_removes_silence_and_caps_the_length() {
    let mut x = Stereo { l: vec![0.0; SR as usize * 3], r: vec![0.0; SR as usize * 3] };
    for i in 0..SR as usize * 2 {
        let v = (i as f32 * 0.05).sin() * 0.8;
        x.l[SR as usize / 2 + i] = v;
        x.r[SR as usize / 2 + i] = v;
    }
    trim(&mut x, 1.0);
    assert_eq!(x.len(), SR as usize);
    assert!(x.l[0].abs() < 1e-6 && x.l[x.len() - 1].abs() < 1e-3);
    assert!(x.l[200].abs() > 0.1, "the hit should start right away");
}

#[test]
fn a_break_is_exactly_as_long_as_its_bars() {
    let click = Stereo { l: vec![0.5; 400], r: vec![0.5; 400] };
    let hits: HashMap<String, Stereo> = [("Kicks/K".to_string(), click.clone()), ("Hi-Hats/H".to_string(), click)].into();
    let b = Break {
        name: "Test 120".into(),
        bpm: 120.0,
        swing: 0.0,
        voices: [("k".to_string(), "Kicks/K".to_string()), ("h".to_string(), "Hi-Hats/H".to_string())].into(),
        tracks: vec![("k".into(), "x...x...x...x...".into()), ("h".into(), "x.x.x.x.x.x.x.x.".into())],
    };
    let x = render_break(&b, &hits);
    assert_eq!(x.len(), SR as usize * 2, "one bar at 120 BPM is two seconds");
    assert!(x.peak() <= 1.0);
}
