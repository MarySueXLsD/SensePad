//! Factory sounds and kits. Everything is synthesised on first run, so SensePad ships
//! with playable material and no third-party sample licences.
//!
//! Sounds live in `<factory>/<Category>/<Sound>.wav`. Kits are presets that point at
//! those sounds and carry a tempo and a demo beat.

use crate::kit::{Kit, PadConfig, PlayMode, DEFAULT_BINDINGS, PAD_COLORS};
use crate::seq::Pattern;
use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Bump when the factory sounds change so they get regenerated.
pub const FACTORY_VERSION: u32 = 3;
pub const CATEGORIES: [&str; 13] = [
    "Kicks", "Snares", "Claps", "Hi-Hats", "Cymbals", "Percussion", "Breaks", "Loops", "Bass", "Stabs", "Keys", "FX", "Vox",
];
/// Roughly how many sounds `install` writes (for the progress bar).
const EXPECTED: u32 = 144;
const SR: f32 = 44_100.0;

// ----- building blocks -------------------------------------------------------

struct Rng(u32);
impl Rng {
    fn noise(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn n(seconds: f32) -> usize {
    (seconds * SR) as usize
}
fn env(t: f32, decay: f32) -> f32 {
    (-t / decay).exp()
}
fn hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}
fn saw(phase: f32) -> f32 {
    2.0 * phase.fract() - 1.0
}
fn square(phase: f32) -> f32 {
    if phase.fract() < 0.5 { 1.0 } else { -1.0 }
}

/// Zero-delay state-variable filter. `run` returns (low, band, high).
#[derive(Default, Clone, Copy)]
struct Svf {
    ic1: f32,
    ic2: f32,
}
impl Svf {
    fn run(&mut self, x: f32, fc: f32, q: f32) -> (f32, f32, f32) {
        let g = (PI * fc.clamp(10.0, SR * 0.45) / SR).tan();
        let k = 1.0 / q.max(0.1);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - k * v1 - v2)
    }
}

/// Render `seconds` of sound from a function of time, with a tiny fade at the end.
fn gen(seconds: f32, mut f: impl FnMut(f32) -> f32) -> Vec<f32> {
    let mut v: Vec<f32> = (0..n(seconds)).map(|i| f(i as f32 / SR)).collect();
    let fade = 96.min(v.len() / 2);
    let len = v.len();
    for i in 0..fade {
        v[len - 1 - i] *= i as f32 / fade as f32;
    }
    v
}

/// Mix `src` into `dst` at `at`. Past the end it either wraps around (loops) or is dropped.
fn mix_at(dst: &mut [f32], src: &[f32], at: usize, gain: f32, wrap: bool) {
    let len = dst.len();
    if len == 0 {
        return;
    }
    for (i, s) in src.iter().enumerate() {
        let j = at + i;
        if j < len {
            dst[j] += s * gain;
        } else if wrap {
            dst[j % len] += s * gain;
        } else {
            break;
        }
    }
}

/// Layer sounds on top of each other; the result is as long as the longest.
fn layer(parts: &[(&[f32], usize, f32)]) -> Vec<f32> {
    let len = parts.iter().map(|(s, at, _)| at + s.len()).max().unwrap_or(0);
    let mut out = vec![0.0f32; len];
    for (s, at, g) in parts {
        mix_at(&mut out, s, *at, *g, false);
    }
    out
}

/// Small Schroeder reverb, appends a tail.
fn reverb(dry: &[f32], wet: f32, size: f32) -> Vec<f32> {
    let tail = n(1.2 * size);
    let mut x: Vec<f32> = dry.to_vec();
    x.resize(dry.len() + tail, 0.0);
    let mut out = vec![0.0f32; x.len()];
    for d in [1557.0f32, 1617.0, 1491.0, 1422.0] {
        let bl = ((d * size) as usize).max(1);
        let mut buf = vec![0.0f32; bl];
        let mut lp = 0.0;
        for i in 0..x.len() {
            let y = buf[i % bl];
            lp = y * 0.6 + lp * 0.4;
            buf[i % bl] = x[i] + lp * 0.8;
            out[i] += y * 0.25;
        }
    }
    for d in [225usize, 556] {
        let mut buf = vec![0.0f32; d];
        for i in 0..out.len() {
            let b = buf[i % d];
            let y = -out[i] + b;
            buf[i % d] = out[i] + b * 0.5;
            out[i] = y;
        }
    }
    x.iter().zip(out).map(|(d, w)| d + w * wet).collect()
}

/// Play backwards, with a short fade so the swell doesn't end in a click.
fn reverse(v: Vec<f32>) -> Vec<f32> {
    let mut out: Vec<f32> = v.into_iter().rev().collect();
    let fade = n(0.012).min(out.len() / 2);
    let len = out.len();
    for i in 0..fade {
        out[len - 1 - i] *= i as f32 / fade as f32;
    }
    out
}

/// A looping line on a 16th-note grid; tails wrap so the loop is seamless.
fn line(bpm: f32, steps: usize, events: &[(usize, &[f32], f32)]) -> Vec<f32> {
    let step = 60.0 / bpm / 4.0 * SR;
    let mut out = vec![0.0f32; (step * steps as f32).round() as usize];
    for (s, snd, gain) in events {
        mix_at(&mut out, snd, (*s as f32 * step).round() as usize, *gain, true);
    }
    out
}

/// Repeat a hit `hits` times, `per` steps apart, getting louder.
fn roll(src: &[f32], bpm: f32, hits: usize, per: f32) -> Vec<f32> {
    let step = 60.0 / bpm / 4.0 * SR * per;
    let mut out = vec![0.0f32; (step * hits as f32).round() as usize];
    for i in 0..hits {
        let g = 0.3 + 0.7 * i as f32 / (hits.max(2) - 1) as f32;
        mix_at(&mut out, src, (i as f32 * step).round() as usize, g, false);
    }
    out
}

// ----- drums -----------------------------------------------------------------

struct Kick {
    f0: f32,
    f1: f32,
    sweep: f32,
    decay: f32,
    drive: f32,
    click: f32,
    len: f32,
    lp: f32,
}

fn kick(r: &mut Rng, k: Kick) -> Vec<f32> {
    let (mut ph, mut f) = (0.0f32, Svf::default());
    gen(k.len, |t| {
        ph += TAU * (k.f1 + (k.f0 - k.f1) * env(t, k.sweep)) / SR;
        let body = (k.drive * ph.sin()).tanh() * env(t, k.decay);
        let click = r.noise() * env(t, 0.0025) * k.click;
        f.run(body + click, k.lp, 0.7).0
    })
}

struct Snare {
    tone: f32,
    tone_decay: f32,
    noise_decay: f32,
    bp: f32,
    body: f32,
    noise: f32,
    drive: f32,
    len: f32,
}

fn snare(r: &mut Rng, s: Snare) -> Vec<f32> {
    let (mut ph, mut f) = (0.0f32, Svf::default());
    gen(s.len, |t| {
        ph += TAU * s.tone * (1.0 + 0.3 * env(t, 0.012)) / SR;
        let tone = ph.sin() * env(t, s.tone_decay) * s.body;
        let (_, b, h) = f.run(r.noise(), s.bp, 0.8);
        let nz = (b * 0.7 + h * 0.6) * env(t, s.noise_decay) * s.noise;
        ((tone + nz) * s.drive).tanh()
    })
}

fn rimshot(r: &mut Rng) -> Vec<f32> {
    gen(0.14, |t| {
        (TAU * 1750.0 * t).sin() * env(t, 0.012) + 0.5 * (TAU * 480.0 * t).sin() * env(t, 0.02) + r.noise() * env(t, 0.004) * 0.6
    })
}

fn clap(r: &mut Rng, spread: f32, tail: f32, bp: f32, len: f32) -> Vec<f32> {
    let mut f = Svf::default();
    gen(len, |t| {
        let mut e: f32 = 0.0;
        for k in 0..4 {
            let start = k as f32 * spread;
            if t >= start {
                e = e.max(env(t - start, if k == 3 { tail } else { 0.006 }));
            }
        }
        f.run(r.noise(), bp, 1.3).1 * e * 2.5
    })
}

const HAT_FREQS: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];

fn metal(r: &mut Rng, decay: f32, len: f32, hp: f32, metal_mix: f32, pitch: f32) -> Vec<f32> {
    let (mut f, mut f2) = (Svf::default(), Svf::default());
    gen(len, |t| {
        let sq: f32 = HAT_FREQS.iter().map(|fr| square(t * fr * pitch)).sum::<f32>() / 6.0;
        let src = sq * metal_mix + r.noise() * (1.0 - metal_mix);
        let (_, b, _) = f.run(src, 9_000.0 * pitch.min(1.3), 0.9);
        f2.run(b, hp, 0.7).2 * env(t, decay)
    })
}

fn ride(r: &mut Rng, bell: bool) -> Vec<f32> {
    let mut v = metal(r, 0.45, 1.6, 4_000.0, 0.75, 1.4);
    if bell {
        for (i, s) in v.iter_mut().enumerate() {
            let t = i as f32 / SR;
            *s += 0.5 * ((TAU * 2_350.0 * t).sin() + 0.6 * (TAU * 3_520.0 * t).sin()) * env(t, 0.35);
        }
    }
    v
}

fn tom(r: &mut Rng, f0: f32, f1: f32, decay: f32) -> Vec<f32> {
    let mut ph = 0.0f32;
    gen(decay * 5.0, |t| {
        ph += TAU * (f1 + (f0 - f1) * env(t, 0.05)) / SR;
        ph.sin() * env(t, decay) + r.noise() * env(t, 0.003) * 0.2
    })
}

/// 808-style cowbell: two detuned squares through a band-pass. Also the phonk lead.
fn cowbell(note: f32, len: f32) -> Vec<f32> {
    let f = hz(note);
    let mut fl = Svf::default();
    gen(len, |t| {
        let s = square(t * f) + 0.8 * square(t * f * 1.48);
        let y = fl.run(s, (f * 3.4).min(9_000.0), 2.2).1;
        (y * 2.0).tanh() * (0.7 * env(t, 0.03) + 0.3 * env(t, len * 0.35))
    })
}

fn shaker(r: &mut Rng, len: f32) -> Vec<f32> {
    let mut f = Svf::default();
    gen(len, |t| f.run(r.noise(), 7_000.0, 0.7).2 * (t / 0.02).min(1.0) * env(t, len * 0.25))
}

fn tambourine(r: &mut Rng) -> Vec<f32> {
    let mut f = Svf::default();
    gen(0.35, |t| {
        let jingle = 0.6 + 0.4 * (TAU * 28.0 * t).sin();
        f.run(r.noise(), 6_500.0, 2.5).1 * jingle * env(t, 0.09)
    })
}

// ----- breaks ----------------------------------------------------------------

struct BreakKit {
    kick: Vec<f32>,
    snare: Vec<f32>,
    hat: Vec<f32>,
    open: Vec<f32>,
    ride: Vec<f32>,
    crash: Vec<f32>,
    bongo: Vec<f32>,
    conga: Vec<f32>,
}

fn break_kit(r: &mut Rng) -> BreakKit {
    BreakKit {
        kick: kick(r, Kick { f0: 130.0, f1: 56.0, sweep: 0.03, decay: 0.16, drive: 1.4, click: 0.35, len: 0.4, lp: 5_000.0 }),
        snare: snare(r, Snare { tone: 200.0, tone_decay: 0.06, noise_decay: 0.13, bp: 2_400.0, body: 0.7, noise: 1.0, drive: 1.3, len: 0.35 }),
        hat: metal(r, 0.025, 0.09, 6_500.0, 0.4, 1.0),
        open: metal(r, 0.16, 0.45, 6_000.0, 0.45, 1.0),
        ride: metal(r, 0.4, 1.2, 4_500.0, 0.75, 1.4),
        crash: metal(r, 0.7, 2.0, 3_500.0, 0.5, 1.1),
        bongo: tom(r, 560.0, 430.0, 0.07),
        conga: tom(r, 360.0, 290.0, 0.11),
    }
}

/// Render a seamless drum loop. Patterns use 16th steps: `x` hit, `X` accent, `o` ghost, `.` rest.
fn render_break(r: &mut Rng, k: &BreakKit, bpm: f32, swing: f32, tracks: &[(&str, &str)]) -> Vec<f32> {
    let step = 60.0 / bpm / 4.0 * SR;
    let steps = tracks.iter().map(|(_, p)| p.chars().filter(|c| !c.is_whitespace()).count()).max().unwrap_or(16);
    let mut out = vec![0.0f32; (step * steps as f32).round() as usize];
    for (voice, pattern) in tracks {
        let src = match *voice {
            "k" => &k.kick,
            "s" => &k.snare,
            "h" => &k.hat,
            "oh" => &k.open,
            "r" => &k.ride,
            "c" => &k.crash,
            "b" => &k.bongo,
            _ => &k.conga,
        };
        for (i, ch) in pattern.chars().filter(|c| !c.is_whitespace()).enumerate() {
            let vel = match ch {
                'X' => 1.15,
                'x' => 1.0,
                'o' => 0.35,
                _ => continue,
            };
            let at = (i as f32 + if i % 2 == 1 { swing } else { 0.0 }) * step;
            mix_at(&mut out, src, at.round() as usize, vel, true);
        }
    }
    // Vintage colour: saturation, gentle low-pass, a touch of hiss.
    let mut lp = 0.0f32;
    for s in out.iter_mut() {
        let x = (*s * 0.9).tanh();
        lp += 0.55 * (x - lp);
        *s = lp + r.noise() * 0.002;
    }
    out
}

/// Play a loop backwards while speeding up: the DJ "rewind".
fn rewind(src: &[f32], len: f32) -> Vec<f32> {
    let mut pos = src.len() as f32 - 1.0;
    let mut f = Svf::default();
    gen(len, |t| {
        let rate = 1.0 + 7.0 * (t / len).powi(2);
        pos -= rate;
        if pos < 1.0 {
            pos += src.len() as f32 - 2.0;
        }
        let i = pos as usize;
        let fr = pos - i as f32;
        let s = src[i] + (src[(i + 1).min(src.len() - 1)] - src[i]) * fr;
        f.run(s, 9_000.0 - 6_000.0 * t / len, 0.7).0 * (1.0 - (t / len).powi(3))
    })
}

/// Slow a loop down to a halt.
fn tape_stop(src: &[f32], len: f32) -> Vec<f32> {
    let mut pos = 0.0f32;
    gen(len, |t| {
        pos += (1.0 - t / len).max(0.0).powf(1.5);
        src[(pos as usize).min(src.len() - 2)] * (1.0 - t / len).sqrt()
    })
}

// ----- bass ------------------------------------------------------------------

fn reese(note: f32, len: f32, detune: f32, lfo: f32) -> Vec<f32> {
    let f = hz(note);
    let (mut p1, mut p2, mut p3, mut fl) = (0.0f32, 0.31f32, 0.0f32, Svf::default());
    gen(len, |t| {
        p1 += f * (1.0 + detune) / SR;
        p2 += f * (1.0 - detune) / SR;
        p3 += TAU * f * 0.5 / SR;
        let cutoff = 260.0 + 700.0 * (0.5 + 0.5 * (TAU * lfo * t).sin());
        let s = fl.run(saw(p1) + saw(p2), cutoff, 1.2).0;
        ((1.8 * s).tanh() * 0.6 + 0.55 * p3.sin()) * (t / 0.01).min(1.0)
    })
}

fn hoover(note: f32, len: f32) -> Vec<f32> {
    let f = hz(note);
    let mut ph = [0.0f32, 0.13, 0.27, 0.41, 0.6];
    let det = [-0.02, -0.008, 0.0, 0.009, 0.021];
    let mut fl = Svf::default();
    gen(len, |t| {
        let glide = 2f32.powf(2.0 * env(t, 0.06) / 12.0);
        let mut s = 0.0;
        for k in 0..5 {
            ph[k] += f * glide * (1.0 + det[k]) / SR;
            s += saw(ph[k]) - 0.6 * saw(ph[k] + 0.37 + 0.1 * (TAU * 0.8 * t).sin());
        }
        (1.4 * fl.run(s / 3.0, 3_200.0, 0.8).0).tanh() * (t / 0.015).min(1.0)
    })
}

fn growl(note: f32, len: f32) -> Vec<f32> {
    let f = hz(note);
    let (mut c, mut m, mut fl) = (0.0f32, 0.0f32, Svf::default());
    gen(len, |t| {
        let lfo = 0.5 + 0.5 * (TAU * 4.0 * t).sin();
        m += TAU * f * 2.0 / SR;
        c += TAU * f / SR;
        let s = (c + (1.0 + 4.0 * lfo) * m.sin()).sin();
        (3.0 * fl.run(s, 300.0 + 2_500.0 * lfo, 3.0).0).tanh() * 0.8 * (t / 0.01).min(1.0)
    })
}

fn wobble(note: f32, len: f32, rate: f32) -> Vec<f32> {
    let f = hz(note);
    let (mut p, mut fl) = (0.0f32, Svf::default());
    gen(len, |t| {
        p += f / SR;
        let lfo = 0.5 - 0.5 * (TAU * rate * t).cos();
        (2.0 * fl.run(saw(p) + 0.5 * square(p * 0.5), 120.0 + 1_800.0 * lfo, 4.0).0).tanh() * 0.8
    })
}

fn sub(note: f32, len: f32) -> Vec<f32> {
    let f = hz(note);
    gen(len, |t| (TAU * f * t).sin() * (t / 0.004).min(1.0) * env(t, len * 0.7))
}

fn sub_drop() -> Vec<f32> {
    let mut ph = 0.0f32;
    gen(1.8, |t| {
        ph += TAU * (32.0 + 70.0 * env(t, 0.35)) / SR;
        (1.4 * ph.sin()).tanh() * env(t, 0.8)
    })
}

fn bass808(note: f32, len: f32, drive: f32) -> Vec<f32> {
    let f = hz(note);
    let (mut ph, mut fl) = (0.0f32, Svf::default());
    gen(len, |t| {
        ph += TAU * f * (1.0 + env(t, 0.03)) / SR;
        let s = (drive * ph.sin()).tanh();
        fl.run(s, 2_400.0, 0.8).0 * env(t, len * 0.45) * (t / 0.002).min(1.0)
    })
}

fn pluck_bass(note: f32) -> Vec<f32> {
    let f = hz(note);
    let (mut p, mut fl) = (0.0f32, Svf::default());
    gen(0.5, |t| {
        p += f / SR;
        let y = fl.run(saw(p), 200.0 + 3_000.0 * env(t, 0.05), 1.5).0;
        (y * 1.5 + 0.5 * (TAU * f * t).sin()) * env(t, 0.22) * (t / 0.002).min(1.0)
    })
}

/// The bouncy FM "donk" of hardbass.
fn donk(note: f32) -> Vec<f32> {
    let f = hz(note);
    gen(0.3, |t| {
        let idx = 7.0 * env(t, 0.035) + 0.4;
        let s = (TAU * f * t + idx * (TAU * f * 2.0 * t).sin()).sin();
        (1.6 * s).tanh() * env(t, 0.11) * (t / 0.001).min(1.0)
    })
}

// ----- stabs & keys ----------------------------------------------------------

fn rave_stab(notes: &[f32], bright: f32, len: f32) -> Vec<f32> {
    let mut ph: Vec<f32> = vec![0.0; notes.len() * 2];
    let mut fl = Svf::default();
    gen(len, |t| {
        let mut s = 0.0;
        for (k, nt) in notes.iter().enumerate() {
            let f = hz(*nt);
            ph[k * 2] += f * 1.004 / SR;
            ph[k * 2 + 1] += f * 0.996 / SR;
            s += saw(ph[k * 2]) + saw(ph[k * 2 + 1]);
        }
        let cutoff = 300.0 + bright * 7_000.0 * env(t, 0.09);
        fl.run(s / notes.len() as f32, cutoff, 1.4).0 * env(t, len * 0.35) * (t / 0.002).min(1.0)
    })
}

fn organ(notes: &[f32]) -> Vec<f32> {
    gen(0.7, |t| {
        let s: f32 = notes
            .iter()
            .map(|nt| {
                let f = hz(*nt);
                (TAU * f * t).sin() + 0.6 * (TAU * 2.0 * f * t).sin() + 0.4 * (TAU * 3.0 * f * t).sin() + 0.25 * (TAU * 4.0 * f * t).sin()
            })
            .sum();
        s / notes.len() as f32 * env(t, 0.28) * (t / 0.003).min(1.0)
    })
}

fn piano(notes: &[f32]) -> Vec<f32> {
    gen(2.0, |t| {
        let mut s = 0.0;
        for nt in notes {
            let f = hz(*nt);
            for k in 1..7 {
                let kf = k as f32;
                s += (TAU * f * kf * (1.0 + 0.0004 * kf * kf) * t).sin() / kf * env(t, 1.4 / kf);
            }
        }
        s / notes.len() as f32 * (t / 0.002).min(1.0)
    })
}

/// FM electric piano with a little tremolo.
fn rhodes(notes: &[f32]) -> Vec<f32> {
    gen(2.4, |t| {
        let mut s = 0.0;
        for nt in notes {
            let f = hz(*nt);
            let idx = 1.6 * env(t, 0.5) + 0.25;
            s += (TAU * f * t + idx * (TAU * f * t).sin()).sin() * env(t, 1.5);
            s += 0.12 * (TAU * f * 7.0 * t).sin() * env(t, 0.06);
        }
        s / notes.len() as f32 * (1.0 + 0.06 * (TAU * 4.5 * t).sin()) * (t / 0.003).min(1.0)
    })
}

fn pad(notes: &[f32]) -> Vec<f32> {
    let mut ph: Vec<f32> = vec![0.0; notes.len() * 2];
    let mut fl = Svf::default();
    let len = 3.2;
    gen(len, |t| {
        let mut s = 0.0;
        for (k, nt) in notes.iter().enumerate() {
            let f = hz(*nt);
            ph[k * 2] += f * 1.006 / SR;
            ph[k * 2 + 1] += f * 0.994 / SR;
            s += saw(ph[k * 2]) + saw(ph[k * 2 + 1]);
        }
        let amp = (t / 0.35).min(1.0) * ((len - t) / 0.8).clamp(0.0, 1.0);
        fl.run(s / notes.len() as f32, 900.0 + 400.0 * (TAU * 0.3 * t).sin(), 0.9).0 * amp
    })
}

fn brass(notes: &[f32]) -> Vec<f32> {
    let mut ph: Vec<f32> = vec![0.0; notes.len()];
    let mut fl = Svf::default();
    gen(0.6, |t| {
        let mut s = 0.0;
        for (k, nt) in notes.iter().enumerate() {
            ph[k] += hz(*nt) / SR;
            s += saw(ph[k]);
        }
        let open = (t / 0.04).min(1.0) * env(t, 0.25);
        fl.run(s / notes.len() as f32, 400.0 + 4_000.0 * open, 1.0).0 * open
    })
}

/// Plucked string (Karplus-Strong).
fn pluck(r: &mut Rng, note: f32, len: f32) -> Vec<f32> {
    let period = (SR / hz(note)).max(2.0) as usize;
    let mut buf: Vec<f32> = (0..period).map(|_| r.noise()).collect();
    let mut v: Vec<f32> = (0..n(len))
        .map(|i| {
            let s = 0.498 * (buf[i % period] + buf[(i + 1) % period]);
            buf[i % period] = s;
            s
        })
        .collect();
    let fade = 400.min(v.len());
    let l = v.len();
    for i in 0..fade {
        v[l - 1 - i] *= i as f32 / fade as f32;
    }
    v
}

fn pluck_chord(r: &mut Rng, notes: &[f32]) -> Vec<f32> {
    let parts: Vec<Vec<f32>> = notes.iter().map(|nt| pluck(r, *nt, 1.2)).collect();
    let refs: Vec<(&[f32], usize, f32)> = parts.iter().enumerate().map(|(k, p)| (p.as_slice(), k * 220, 1.0)).collect();
    layer(&refs)
}

fn bell(note: f32) -> Vec<f32> {
    let f = hz(note);
    gen(1.6, |t| (TAU * f * t + 2.2 * env(t, 0.3) * (TAU * 3.5 * f * t).sin()).sin() * env(t, 0.5))
}

fn whistle() -> Vec<f32> {
    let mut ph = 0.0f32;
    let len = 0.5;
    gen(len, |t| {
        ph += TAU * (1_900.0 + 500.0 * t / len + 70.0 * (TAU * 26.0 * t).sin()) / SR;
        ph.sin() * (t / 0.01).min(1.0) * ((len - t) / 0.05).clamp(0.0, 1.0) * 0.8
    })
}

// ----- FX --------------------------------------------------------------------

fn air_horn() -> Vec<f32> {
    let notes = [70.0f32, 74.0, 77.0];
    let mut ph = [0.0f32; 3];
    let mut fl = Svf::default();
    let blasts = [(0.0f32, 0.11f32), (0.15, 0.11), (0.3, 0.11), (0.45, 0.75)];
    gen(1.3, |t| {
        let gate = blasts
            .iter()
            .map(|(s, l)| if t >= *s && t < s + l { ((t - s) / 0.01).min(1.0) * ((s + l - t) / 0.02).min(1.0) } else { 0.0 })
            .fold(0.0, f32::max);
        let vib = 1.0 + 0.012 * (TAU * 9.0 * t).sin();
        let mut s = 0.0;
        for k in 0..3 {
            ph[k] += hz(notes[k]) * vib / SR;
            s += saw(ph[k]);
        }
        (2.5 * fl.run(s / 3.0, 2_400.0, 1.8).1).tanh() * gate
    })
}

fn dub_siren() -> Vec<f32> {
    let mut ph = 0.0f32;
    let dry = gen(2.0, |t| {
        ph += (500.0 + 900.0 * (t * 2.0).fract()) / SR;
        square(ph) * 0.4 * (t / 0.01).min(1.0)
    });
    let echoes: Vec<(&[f32], usize, f32)> = (0..5).map(|k| (dry.as_slice(), n(0.3 * k as f32), 0.5f32.powi(k))).collect();
    layer(&echoes)
}

fn police_siren() -> Vec<f32> {
    let mut ph = 0.0f32;
    gen(2.0, |t| {
        ph += if (t * 2.0) as usize % 2 == 0 { 960.0 } else { 770.0 } / SR;
        (saw(ph) * 0.6 + (TAU * ph).sin() * 0.4) * (t / 0.02).min(1.0)
    })
}

fn riser(r: &mut Rng, len: f32) -> Vec<f32> {
    let (mut ph, mut fl) = (0.0f32, Svf::default());
    gen(len, |t| {
        let x = t / len;
        ph += (200.0 + 1_800.0 * x * x) / SR;
        let nz = fl.run(r.noise(), 400.0 + 9_000.0 * x * x, 1.5).1;
        (saw(ph) * 0.3 + nz * 0.9) * x * x
    })
}

fn downlifter(r: &mut Rng) -> Vec<f32> {
    let (mut ph, mut fl) = (0.0f32, Svf::default());
    gen(2.0, |t| {
        let x = t / 2.0;
        ph += TAU * (900.0 * (1.0 - x) + 60.0) / SR;
        let nz = fl.run(r.noise(), 8_000.0 * (1.0 - x) + 200.0, 1.0).0;
        (nz * 0.8 + ph.sin() * 0.4) * (1.0 - x)
    })
}

fn impact(r: &mut Rng) -> Vec<f32> {
    let mut ph = 0.0f32;
    let dry = gen(1.6, |t| {
        ph += TAU * (35.0 + 40.0 * env(t, 0.15)) / SR;
        (1.5 * ph.sin()).tanh() * env(t, 0.6) + r.noise() * env(t, 0.05) * 0.6
    });
    reverb(&dry, 0.35, 1.2)
}

fn laser() -> Vec<f32> {
    let mut ph = 0.0f32;
    gen(0.3, |t| {
        ph += TAU * (100.0 + 3_000.0 * env(t, 0.05)) / SR;
        ph.sin() * env(t, 0.1)
    })
}

fn gunshot(r: &mut Rng) -> Vec<f32> {
    let (mut ph, mut fl) = (0.0f32, Svf::default());
    let dry = gen(0.5, |t| {
        ph += TAU * (70.0 + 80.0 * env(t, 0.02)) / SR;
        fl.run(r.noise(), 3_000.0, 0.7).0 * env(t, 0.03) * 1.4 + ph.sin() * env(t, 0.12)
    });
    reverb(&dry, 0.5, 1.4)
}

fn noise_sweep(r: &mut Rng) -> Vec<f32> {
    let mut fl = Svf::default();
    gen(2.0, |t| {
        let x = t / 2.0;
        fl.run(r.noise(), 200.0 + 10_000.0 * (PI * x).sin().powi(2), 2.5).1 * (PI * x).sin()
    })
}

fn vinyl(r: &mut Rng) -> Vec<f32> {
    let mut fl = Svf::default();
    gen(4.0, |_| {
        let hiss = fl.run(r.noise(), 3_000.0, 0.7).0 * 0.08;
        let click = if r.noise() > 0.9993 { r.noise() * 0.9 } else { 0.0 };
        hiss + click
    })
}

// ----- vox (formant synthesis) -----------------------------------------------

fn vox(formants: [(f32, f32); 3], len: f32, f0: f32) -> Vec<f32> {
    let mut ph = 0.0f32;
    let mut fl = [Svf::default(); 3];
    gen(len, |t| {
        ph += f0 * (1.0 + 0.01 * (TAU * 5.5 * t).sin()) / SR;
        let src = saw(ph);
        let mut s = 0.0;
        for (k, (fc, g)) in formants.iter().enumerate() {
            s += fl[k].run(src, *fc, 8.0).1 * g;
        }
        s * (t / 0.03).min(1.0) * ((len - t) / 0.08).clamp(0.0, 1.0)
    })
}

/// A short shout: an open vowel sliding closed while the pitch drops.
fn vox_hey() -> Vec<f32> {
    let mut ph = 0.0f32;
    let mut fl = [Svf::default(); 3];
    let len = 0.32;
    gen(len, |t| {
        let x = t / len;
        ph += (235.0 - 60.0 * x) / SR;
        let src = saw(ph);
        let s = fl[0].run(src, 640.0 - 260.0 * x, 7.0).1 + 0.7 * fl[1].run(src, 1_750.0 + 450.0 * x, 9.0).1 + 0.3 * fl[2].run(src, 2_900.0, 9.0).1;
        s * (t / 0.012).min(1.0) * ((len - t) / 0.06).clamp(0.0, 1.0)
    })
}

// ----- library ---------------------------------------------------------------

fn write_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let peak = samples.iter().fold(1e-9f32, |m, s| m.max(s.abs()));
    let gain = 0.89 / peak;
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + samples.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&(SR as u32).to_le_bytes());
    b.extend_from_slice(&((SR as u32) * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&(((s * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b).map_err(|e| format!("{}: {e}", path.display()))
}

struct Writer<'a> {
    dir: &'a Path,
    count: u32,
    err: Option<String>,
    progress: &'a AtomicU32,
}

impl Writer<'_> {
    fn add(&mut self, cat: &str, name: &str, v: Vec<f32>) {
        if self.err.is_some() {
            return;
        }
        let dir = self.dir.join(cat);
        let res = std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| write_wav(&dir.join(format!("{name}.wav")), &v));
        if let Err(e) = res {
            self.err = Some(e);
        }
        self.count += 1;
        self.progress.store((self.count * 100 / EXPECTED).min(99), Ordering::Relaxed);
    }
}

fn build(w: &mut Writer) {
    let mut rng = Rng(0x5EED_BEEF);
    let r = &mut rng;

    // Kicks
    let kicks: [(&str, Kick); 14] = [
        ("DnB Punch", Kick { f0: 180.0, f1: 50.0, sweep: 0.03, decay: 0.15, drive: 2.4, click: 0.45, len: 0.45, lp: 9_000.0 }),
        ("DnB Tight", Kick { f0: 200.0, f1: 58.0, sweep: 0.02, decay: 0.09, drive: 2.0, click: 0.5, len: 0.3, lp: 10_000.0 }),
        ("Liquid Soft", Kick { f0: 140.0, f1: 48.0, sweep: 0.04, decay: 0.2, drive: 1.2, click: 0.15, len: 0.5, lp: 3_000.0 }),
        ("Neuro Distorted", Kick { f0: 220.0, f1: 46.0, sweep: 0.03, decay: 0.17, drive: 6.0, click: 0.5, len: 0.45, lp: 8_000.0 }),
        ("Jungle Boom", Kick { f0: 110.0, f1: 42.0, sweep: 0.06, decay: 0.35, drive: 1.5, click: 0.2, len: 0.8, lp: 2_500.0 }),
        ("Techstep Click", Kick { f0: 260.0, f1: 55.0, sweep: 0.015, decay: 0.12, drive: 3.0, click: 0.9, len: 0.35, lp: 12_000.0 }),
        ("Sub Kick", Kick { f0: 90.0, f1: 38.0, sweep: 0.05, decay: 0.45, drive: 1.1, click: 0.05, len: 0.9, lp: 1_200.0 }),
        ("909 Style", Kick { f0: 170.0, f1: 52.0, sweep: 0.025, decay: 0.22, drive: 2.6, click: 0.6, len: 0.5, lp: 7_000.0 }),
        ("808 Style", Kick { f0: 120.0, f1: 45.0, sweep: 0.05, decay: 0.6, drive: 1.3, click: 0.1, len: 1.3, lp: 2_000.0 }),
        ("Halftime Thump", Kick { f0: 150.0, f1: 44.0, sweep: 0.04, decay: 0.28, drive: 2.0, click: 0.3, len: 0.6, lp: 5_000.0 }),
        ("Rolling Kick", Kick { f0: 190.0, f1: 54.0, sweep: 0.02, decay: 0.11, drive: 2.2, click: 0.4, len: 0.35, lp: 9_000.0 }),
        ("Acoustic Break Kick", Kick { f0: 130.0, f1: 60.0, sweep: 0.03, decay: 0.14, drive: 1.3, click: 0.35, len: 0.35, lp: 4_500.0 }),
        ("Phonk Kick", Kick { f0: 160.0, f1: 47.0, sweep: 0.04, decay: 0.3, drive: 5.0, click: 0.4, len: 0.6, lp: 5_000.0 }),
        ("Hard Kick", Kick { f0: 230.0, f1: 55.0, sweep: 0.022, decay: 0.2, drive: 3.5, click: 0.7, len: 0.42, lp: 9_000.0 }),
    ];
    for (name, k) in kicks {
        w.add("Kicks", name, kick(r, k));
    }

    // Snares
    let snares: [(&str, Snare); 10] = [
        ("DnB Crack", Snare { tone: 190.0, tone_decay: 0.05, noise_decay: 0.11, bp: 3_000.0, body: 0.6, noise: 1.0, drive: 1.8, len: 0.32 }),
        ("Liquid Snare", Snare { tone: 210.0, tone_decay: 0.07, noise_decay: 0.18, bp: 2_200.0, body: 0.5, noise: 0.8, drive: 1.1, len: 0.45 }),
        ("Neuro Snare", Snare { tone: 230.0, tone_decay: 0.04, noise_decay: 0.09, bp: 3_800.0, body: 0.8, noise: 1.1, drive: 4.0, len: 0.3 }),
        ("Jungle Tuned", Snare { tone: 260.0, tone_decay: 0.12, noise_decay: 0.12, bp: 2_600.0, body: 1.0, noise: 0.7, drive: 1.5, len: 0.4 }),
        ("Tight Snare", Snare { tone: 240.0, tone_decay: 0.03, noise_decay: 0.06, bp: 4_000.0, body: 0.6, noise: 1.0, drive: 1.6, len: 0.2 }),
        ("Fat Snare", Snare { tone: 170.0, tone_decay: 0.08, noise_decay: 0.2, bp: 1_800.0, body: 0.9, noise: 1.0, drive: 2.2, len: 0.5 }),
        ("Ghost Snare", Snare { tone: 200.0, tone_decay: 0.03, noise_decay: 0.05, bp: 3_000.0, body: 0.4, noise: 0.6, drive: 1.0, len: 0.15 }),
        ("Halftime Snare", Snare { tone: 180.0, tone_decay: 0.1, noise_decay: 0.3, bp: 2_000.0, body: 0.7, noise: 1.0, drive: 1.5, len: 0.7 }),
        ("Acoustic Break Snare", Snare { tone: 200.0, tone_decay: 0.06, noise_decay: 0.13, bp: 2_400.0, body: 0.7, noise: 1.0, drive: 1.3, len: 0.35 }),
        ("Electro Snare", Snare { tone: 300.0, tone_decay: 0.05, noise_decay: 0.08, bp: 5_000.0, body: 0.5, noise: 1.0, drive: 2.5, len: 0.25 }),
    ];
    for (name, s) in snares {
        w.add("Snares", name, snare(r, s));
    }
    w.add("Snares", "Rimshot", rimshot(r));
    let dry = snare(r, Snare { tone: 200.0, tone_decay: 0.05, noise_decay: 0.12, bp: 2_800.0, body: 0.6, noise: 1.0, drive: 1.6, len: 0.3 });
    w.add("Snares", "Reverb Snare", reverb(&dry, 0.45, 1.0));

    // Claps
    w.add("Claps", "Clap", clap(r, 0.011, 0.12, 1_200.0, 0.45));
    w.add("Claps", "Big Clap", clap(r, 0.014, 0.25, 1_000.0, 0.7));
    w.add("Claps", "Tight Clap", clap(r, 0.007, 0.06, 1_600.0, 0.25));
    w.add("Claps", "Snap", clap(r, 0.004, 0.03, 2_500.0, 0.15));
    let c = clap(r, 0.01, 0.1, 1_300.0, 0.4);
    w.add("Claps", "Reverb Clap", reverb(&c, 0.6, 1.2));
    let a = clap(r, 0.012, 0.14, 1_100.0, 0.45);
    let b = snare(r, Snare { tone: 220.0, tone_decay: 0.03, noise_decay: 0.07, bp: 3_500.0, body: 0.3, noise: 0.6, drive: 1.2, len: 0.3 });
    w.add("Claps", "Layered Clap", layer(&[(&a, 0, 1.0), (&b, 0, 0.6)]));

    // Hi-hats
    let closed = metal(r, 0.018, 0.07, 7_000.0, 0.5, 1.0);
    w.add("Hi-Hats", "Closed Tight", closed.clone());
    w.add("Hi-Hats", "Closed Short", metal(r, 0.03, 0.1, 6_500.0, 0.55, 1.0));
    w.add("Hi-Hats", "Closed Bright", metal(r, 0.025, 0.09, 9_000.0, 0.3, 1.2));
    w.add("Hi-Hats", "Closed Dusty", metal(r, 0.035, 0.12, 5_000.0, 0.3, 0.9));
    w.add("Hi-Hats", "Pedal Hat", metal(r, 0.05, 0.15, 5_500.0, 0.6, 0.95));
    w.add("Hi-Hats", "Open Short", metal(r, 0.1, 0.3, 6_000.0, 0.5, 1.0));
    w.add("Hi-Hats", "Open Hat", metal(r, 0.16, 0.45, 6_000.0, 0.5, 1.0));
    w.add("Hi-Hats", "Open Long", metal(r, 0.3, 0.9, 6_000.0, 0.5, 1.0));
    w.add("Hi-Hats", "Shaker Hat", shaker(r, 0.12));
    w.add("Hi-Hats", "Metallic Tick", metal(r, 0.012, 0.05, 8_000.0, 0.9, 1.4));

    // Cymbals
    w.add("Cymbals", "Ride", ride(r, false));
    w.add("Cymbals", "Ride Bell", ride(r, true));
    let crash = metal(r, 0.7, 2.2, 3_500.0, 0.5, 1.1);
    w.add("Cymbals", "Crash", crash.clone());
    w.add("Cymbals", "Splash", metal(r, 0.3, 1.0, 5_000.0, 0.5, 1.3));
    w.add("Cymbals", "China", metal(r, 0.5, 1.6, 2_500.0, 0.8, 0.8));
    w.add("Cymbals", "Reverse Crash", reverse(crash));

    // Percussion
    w.add("Percussion", "Tom High", tom(r, 300.0, 210.0, 0.12));
    w.add("Percussion", "Tom Mid", tom(r, 220.0, 150.0, 0.14));
    w.add("Percussion", "Tom Low", tom(r, 160.0, 100.0, 0.16));
    w.add("Percussion", "Conga High", tom(r, 400.0, 330.0, 0.1));
    w.add("Percussion", "Conga Low", tom(r, 300.0, 240.0, 0.12));
    w.add("Percussion", "Bongo", tom(r, 560.0, 430.0, 0.07));
    w.add("Percussion", "Cowbell", cowbell(73.0, 0.4));
    w.add("Percussion", "Woodblock", gen(0.12, |t| (TAU * 1_100.0 * t).sin() * env(t, 0.02)));
    w.add("Percussion", "Clave", gen(0.1, |t| (TAU * 2_500.0 * t).sin() * env(t, 0.025)));
    w.add("Percussion", "Shaker", shaker(r, 0.18));
    w.add("Percussion", "Tambourine", tambourine(r));

    // Breaks: the classic patterns replayed with synthesised drums, not the original recordings.
    let bk = break_kit(r);
    let amen = render_break(r, &bk, 170.0, 0.0, &[
        ("k", "x.x.......xx.... x.x.......xx.... x.x.......x..... ..xx......x....."),
        ("s", "....x..o.o..x..o ....x..o.o..x..o ....x..o.o....x. .o..x..o.o....x."),
        ("r", "x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x. x.x.x.x.x......."),
        ("c", "................ ................ ................ ..........x....."),
    ]);
    w.add("Breaks", "Amen-style 170", amen.clone());
    w.add("Breaks", "Amen-style Chopped 174", render_break(r, &bk, 174.0, 0.0, &[
        ("k", "x.x.......xx.... ..xx......x..... x.x.......xx.... x.x...x...x....."),
        ("s", "....x..o.o..x..o .o..x..o.o....x. ....x..o.o..x.xx ....x..o.oxoxoxx"),
        ("r", "x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x. x.x.x.x........."),
    ]));
    w.add("Breaks", "Think-style 170", render_break(r, &bk, 170.0, 0.08, &[
        ("k", "x.....x.x.x..x.. x.x...x...x....."),
        ("s", "....x.......x... ....x..o....x..o"),
        ("h", "x.x.x.x.x.x.x... x.x.x.x.x.x.x.x."),
        ("oh", "..............x. ................"),
    ]));
    w.add("Breaks", "Apache-style 172", render_break(r, &bk, 172.0, 0.05, &[
        ("k", "x.....x...x..x.. x.....x...x....."),
        ("s", "....x.......x... ....x.......x..x"),
        ("h", "x.xxx.xxx.xxx.xx x.xxx.xxx.xxx.xx"),
        ("b", "..x...x...x.x... ..x...x...x.x.x."),
        ("cg", "x.......x....... x.......x.x....."),
    ]));
    w.add("Breaks", "Two-Step 174", render_break(r, &bk, 174.0, 0.0, &[
        ("k", "x.........x..... x.........x....."),
        ("s", "....x.......x... ....x.......x..o"),
        ("h", "o.x.o.x.o.x.o.x. o.x.o.x.o.x.o.x."),
    ]));
    w.add("Breaks", "Jungle Roller 165", render_break(r, &bk, 165.0, 0.0, &[
        ("k", "x..x......x..x.. x..x......x....."),
        ("s", "....x.o..o.ox..o ....x.o..o..x.ox"),
        ("h", "x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x."),
    ]));
    w.add("Breaks", "Halftime 170", render_break(r, &bk, 170.0, 0.0, &[
        ("k", "x.....x...x..... x.....x...x...x."),
        ("s", "........x....... ........x......."),
        ("h", "x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x."),
    ]));
    w.add("Breaks", "Funky-style 100", render_break(r, &bk, 100.0, 0.1, &[
        ("k", "x.x...x...x..x.. x.x...x...x..x.."),
        ("s", "....x..o.o..x..o ....x..o.o..x..o"),
        ("h", "xxxxxxx.xxxxxxxx xxxxxxx.xxxxxxxx"),
        ("oh", ".......x........ .......x........"),
    ]));
    w.add("Breaks", "Liquid Shuffle 174", render_break(r, &bk, 174.0, 0.12, &[
        ("k", "x.........x..... x.........x....."),
        ("s", "....x.......x... ....x.......x..."),
        ("h", "x.ox.ox.x.ox.ox. x.ox.ox.x.ox.ox."),
        ("r", "x...x...x...x... x...x...x...x..."),
    ]));

    // Loops: short melodic lines that follow the tempo.
    let arp = |r: &mut Rng, bpm: f32, notes: [f32; 32]| -> Vec<f32> {
        let sounds: Vec<Vec<f32>> = notes.iter().map(|nt| pluck(r, *nt, 0.45)).collect();
        let ev: Vec<(usize, &[f32], f32)> = sounds.iter().enumerate().map(|(i, s)| (i, s.as_slice(), if i % 4 == 0 { 1.0 } else { 0.7 })).collect();
        line(bpm, 32, &ev)
    };
    w.add("Loops", "Arp Am 172", arp(r, 172.0, [
        57.0, 60.0, 64.0, 69.0, 64.0, 60.0, 57.0, 60.0, 64.0, 69.0, 72.0, 69.0, 64.0, 60.0, 64.0, 60.0,
        53.0, 57.0, 60.0, 65.0, 60.0, 57.0, 53.0, 57.0, 55.0, 59.0, 62.0, 67.0, 62.0, 59.0, 55.0, 59.0,
    ]));
    w.add("Loops", "Arp Fm 174", arp(r, 174.0, [
        53.0, 56.0, 60.0, 65.0, 60.0, 56.0, 53.0, 56.0, 60.0, 65.0, 68.0, 65.0, 60.0, 56.0, 60.0, 56.0,
        49.0, 53.0, 56.0, 61.0, 56.0, 53.0, 49.0, 53.0, 51.0, 55.0, 58.0, 63.0, 58.0, 55.0, 51.0, 55.0,
    ]));
    let cb: Vec<Vec<f32>> = [74.0, 77.0, 79.0, 81.0].iter().map(|nt| cowbell(*nt, 0.45)).collect();
    w.add("Loops", "Cowbell Line Dm 130", line(130.0, 32, &[
        (0, &cb[0], 1.0), (2, &cb[0], 0.8), (4, &cb[1], 1.0), (6, &cb[0], 0.8), (8, &cb[2], 1.0), (10, &cb[1], 0.8), (12, &cb[0], 1.0),
        (16, &cb[0], 1.0), (18, &cb[0], 0.8), (20, &cb[1], 1.0), (22, &cb[0], 0.8), (24, &cb[2], 1.0), (26, &cb[2], 0.8), (28, &cb[3], 1.0), (30, &cb[1], 0.9),
    ]));
    let dk: Vec<Vec<f32>> = [40.0, 43.0, 45.0, 47.0].iter().map(|nt| donk(*nt)).collect();
    w.add("Loops", "Donk Line E 150", line(150.0, 32, &[
        (2, &dk[0], 1.0), (6, &dk[0], 1.0), (10, &dk[0], 1.0), (14, &dk[0], 1.0),
        (18, &dk[1], 1.0), (22, &dk[1], 1.0), (26, &dk[2], 1.0), (30, &dk[3], 1.0),
    ]));
    w.add("Loops", "Hat Roll 140", roll(&closed, 140.0, 8, 0.5));
    let ghost = snare(r, Snare { tone: 210.0, tone_decay: 0.03, noise_decay: 0.06, bp: 3_200.0, body: 0.5, noise: 0.9, drive: 1.4, len: 0.14 });
    w.add("Loops", "Snare Roll 174", roll(&ghost, 174.0, 16, 1.0));

    // Bass
    for (name, note) in [("Reese F", 41.0), ("Reese G#", 44.0), ("Reese C", 36.0), ("Reese A", 45.0)] {
        w.add("Bass", name, reese(note, 1.6, 0.006, 1.5));
    }
    w.add("Bass", "Dark Reese F", reese(41.0, 2.0, 0.011, 0.7));
    for (name, note) in [("Hoover A", 45.0), ("Hoover E", 40.0), ("Hoover F", 41.0)] {
        w.add("Bass", name, hoover(note, 1.4));
    }
    w.add("Bass", "Neuro Growl F", growl(41.0, 1.2));
    w.add("Bass", "Neuro Growl G#", growl(44.0, 1.2));
    w.add("Bass", "Wobble F", wobble(41.0, 1.6, 3.0));
    for (name, note) in [("Sub F", 29.0), ("Sub G", 31.0), ("Sub A", 33.0)] {
        w.add("Bass", name, sub(note, 1.4));
    }
    w.add("Bass", "Sub Drop", sub_drop());
    for (name, note) in [("808 C", 36.0), ("808 D#", 39.0), ("808 F", 41.0), ("808 G", 43.0)] {
        w.add("Bass", name, bass808(note, 1.6, 1.6));
    }
    for (name, note) in [("808 Dist D", 38.0), ("808 Dist F", 41.0), ("808 Dist G", 43.0), ("808 Dist C", 36.0)] {
        w.add("Bass", name, bass808(note, 1.2, 5.0));
    }
    for (name, note) in [("Pluck Bass A", 33.0), ("Pluck Bass C", 36.0), ("Pluck Bass D", 38.0), ("Pluck Bass E", 40.0)] {
        w.add("Bass", name, pluck_bass(note));
    }
    for (name, i) in [("Donk E", 0), ("Donk G", 1), ("Donk A", 2), ("Donk B", 3)] {
        w.add("Bass", name, dk[i].clone());
    }

    // Stabs
    w.add("Stabs", "Rave Stab Am", rave_stab(&[57.0, 60.0, 64.0, 67.0], 1.0, 0.45));
    w.add("Stabs", "Rave Stab Fm", rave_stab(&[53.0, 56.0, 60.0, 63.0], 1.0, 0.45));
    w.add("Stabs", "Rave Stab Em", rave_stab(&[52.0, 55.0, 59.0, 64.0], 1.0, 0.45));
    w.add("Stabs", "Dark Stab", rave_stab(&[41.0, 44.0, 48.0, 51.0], 0.4, 0.5));
    w.add("Stabs", "Chord Stab House", rave_stab(&[57.0, 60.0, 64.0, 67.0], 0.6, 0.5));
    w.add("Stabs", "Brass Stab", brass(&[58.0, 62.0, 65.0]));
    w.add("Stabs", "Organ Stab Am", organ(&[57.0, 60.0, 64.0]));
    w.add("Stabs", "Liquid Pad Am", pad(&[57.0, 60.0, 64.0, 69.0]));
    w.add("Stabs", "Liquid Pad F", pad(&[53.0, 57.0, 60.0, 65.0]));

    // Keys
    w.add("Keys", "Piano Chord Am", piano(&[57.0, 60.0, 64.0, 67.0]));
    w.add("Keys", "Piano Chord F", piano(&[53.0, 57.0, 60.0, 64.0]));
    w.add("Keys", "Rhodes Am7", rhodes(&[57.0, 60.0, 64.0, 67.0]));
    w.add("Keys", "Rhodes Fmaj7", rhodes(&[53.0, 57.0, 60.0, 64.0]));
    w.add("Keys", "Pluck Chord Cm", pluck_chord(r, &[60.0, 63.0, 67.0, 70.0]));
    w.add("Keys", "Bell", bell(76.0));
    w.add("Keys", "Bell Low", bell(72.0));
    for (name, i) in [("Cowbell D", 0), ("Cowbell F", 1), ("Cowbell G", 2), ("Cowbell A", 3)] {
        w.add("Keys", name, cb[i].clone());
    }
    w.add("Keys", "Whistle", whistle());

    // FX
    w.add("FX", "Air Horn", air_horn());
    w.add("FX", "Dub Siren", dub_siren());
    w.add("FX", "Police Siren", police_siren());
    w.add("FX", "Rewind", rewind(&amen, 1.4));
    w.add("FX", "Tape Stop", tape_stop(&amen, 1.2));
    w.add("FX", "Riser", riser(r, 4.0));
    w.add("FX", "Short Riser", riser(r, 2.0));
    w.add("FX", "Downlifter", downlifter(r));
    w.add("FX", "Impact Boom", impact(r));
    w.add("FX", "Laser Zap", laser());
    w.add("FX", "Gunshot", gunshot(r));
    w.add("FX", "Noise Sweep", noise_sweep(r));
    w.add("FX", "Vinyl Crackle", vinyl(r));

    // Vox
    let ah = vox([(800.0, 1.0), (1_150.0, 0.6), (2_900.0, 0.25)], 0.6, 150.0);
    w.add("Vox", "Vox Ah", ah.clone());
    w.add("Vox", "Vox Oh", vox([(450.0, 1.0), (800.0, 0.5), (2_830.0, 0.2)], 0.6, 140.0));
    w.add("Vox", "Vox Ee", vox([(270.0, 1.0), (2_290.0, 0.5), (3_010.0, 0.3)], 0.6, 170.0));
    w.add("Vox", "Vox Hey", vox_hey());
    let chop: Vec<f32> = ah.iter().take(n(60.0 / 170.0 / 4.0 * 0.9)).copied().collect();
    let hits: Vec<(usize, &[f32], f32)> =
        "x.x..x.x..x.xx.x".chars().enumerate().filter(|(_, c)| *c == 'x').map(|(i, _)| (i, chop.as_slice(), 1.0)).collect();
    w.add("Vox", "Vox Chop 170", line(170.0, 16, &hits));
}

/// Generate the whole library into `dir`. `progress` goes from 0 to 100.
pub fn install(dir: &Path, progress: &AtomicU32) -> Result<(), String> {
    progress.store(0, Ordering::Relaxed);
    for cat in CATEGORIES.iter().chain(["Kits"].iter()) {
        let old = dir.join(cat);
        if old.is_dir() {
            let _ = std::fs::remove_dir_all(old);
        }
    }
    let mut w = Writer { dir, count: 0, err: None, progress };
    build(&mut w);
    progress.store(100, Ordering::Relaxed);
    match w.err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// A sample that used to live somewhere else in the factory library (older versions kept
/// copies in `Kits/<kit>/NN Name.wav` and a few sounds were renamed). Returns its new home.
pub fn relocate(path: &Path, root: &Path) -> Option<PathBuf> {
    if path.exists() || !path.starts_with(root) {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let digits = stem.chars().take_while(|c| c.is_ascii_digit()).count();
    let stem = if (1..=2).contains(&digits) && stem[digits..].starts_with(' ') { stem[digits..].trim_start() } else { stem };
    let stem = match stem {
        "Hoover" => "Hoover A",
        "Neuro Growl" => "Neuro Growl F",
        "Wobble" => "Wobble F",
        "Dark Reese D" => "Dark Reese F",
        "Piano Chord" => "Piano Chord Am",
        "Organ Stab" => "Organ Stab Am",
        "Pluck Chord" => "Pluck Chord Cm",
        "Liquid Pad" => "Liquid Pad Am",
        other => other,
    };
    CATEGORIES.iter().map(|c| root.join(c).join(format!("{stem}.wav"))).find(|p| p.exists())
}

// ----- kits ------------------------------------------------------------------

pub struct PadSpec {
    /// Sample pack the sound comes from; empty for the built-in library.
    pub pack: &'static str,
    pub cat: &'static str,
    pub sound: &'static str,
    pub mode: PlayMode,
    pub choke: u8,
    pub sync: f32,
}

fn p(cat: &'static str, sound: &'static str) -> PadSpec {
    PadSpec { pack: "", cat, sound, mode: PlayMode::OneShot, choke: 0, sync: 0.0 }
}

/// A sound from a sample pack.
fn pk(pack: &'static str, cat: &'static str, sound: &'static str) -> PadSpec {
    PadSpec { pack, ..p(cat, sound) }
}

impl PadSpec {
    /// Cut by (and cuts) other pads in the same group.
    fn ch(mut self, group: u8) -> Self {
        self.choke = group;
        self
    }
    /// Plays while held.
    fn hold(mut self) -> Self {
        self.mode = PlayMode::Hold;
        self
    }
    /// A loop recorded at `bpm` that follows the project tempo.
    fn lp(mut self, bpm: f32) -> Self {
        self.mode = PlayMode::Loop;
        self.sync = bpm;
        self
    }
    /// A one-shot recorded at `bpm` that follows the project tempo.
    fn sync(mut self, bpm: f32) -> Self {
        self.sync = bpm;
        self
    }
}

pub struct KitSpec {
    pub id: &'static str,
    pub name_ru: &'static str,
    pub about_ru: &'static str,
    pub about_en: &'static str,
    pub bpm: f32,
    pub swing: f32,
    pub accent: [u8; 3],
    pub pads: Vec<PadSpec>,
    pub bars: u8,
    /// Demo beat: one text row per pad (see `Pattern::from_rows`).
    pub demo: Vec<(u8, &'static str)>,
}

impl KitSpec {
    pub fn title(&self) -> &'static str {
        crate::i18n::l(self.name_ru, self.id)
    }
    pub fn about(&self) -> &'static str {
        crate::i18n::l(self.about_ru, self.about_en)
    }
}

pub fn pad_color(cat: &str) -> [u8; 3] {
    let i = match cat {
        "Kicks" => 0,
        "Snares" | "Claps" => 1,
        "Hi-Hats" | "Cymbals" => 2,
        "Percussion" => 3,
        "Breaks" => 4,
        "Loops" => 5,
        "Bass" => 6,
        "Stabs" | "Keys" => 7,
        "FX" => 8,
        _ => 9,
    };
    PAD_COLORS[i]
}

/// Every kit that can be played right now: the built-in ones, then the kits made of
/// sample-pack sounds whose packs are installed.
pub fn kits() -> Vec<KitSpec> {
    let installed = |k: &KitSpec| k.pads.iter().all(|p| p.pack.is_empty() || crate::library::pack_dir(p.pack).is_some());
    let mut kits = builtin_kits();
    kits.extend(pack_kits().into_iter().filter(installed));
    kits
}

const UNRULY: &str = "Unruly Drums";
const SWIRLY: &str = "Swirly Drums";
const FRANKEN: &str = "Frankensnare";
const VIRTUOSITY: &str = "Virtuosity Drums";

fn pack_kits() -> Vec<KitSpec> {
    vec![
        KitSpec {
            id: "Live Breaks",
            name_ru: "Живые брейки",
            about_ru: "Настоящая установка: записанные бочка, снейр, тарелки и брейки поверх риз-баса. Фа минор.",
            about_en: "A real drum kit: recorded kick, snare, cymbals and breaks over a Reese bass. F minor.",
            bpm: 172.0,
            swing: 0.0,
            accent: [240, 150, 70],
            pads: vec![
                pk(UNRULY, "Kicks", "Unruly Kick Dirty"),
                pk(UNRULY, "Snares", "Unruly Snare 14"),
                pk(UNRULY, "Hi-Hats", "Unruly Hat Closed").ch(1),
                pk(UNRULY, "Hi-Hats", "Unruly Hat Open").ch(1),
                pk(UNRULY, "Breaks", "Unruly Amen-style 170").lp(170.0),
                pk(UNRULY, "Breaks", "Unruly Think-style 170").lp(170.0),
                pk(UNRULY, "Cymbals", "Unruly Ride"),
                pk(UNRULY, "Cymbals", "Unruly Crash"),
                p("Bass", "Reese F").hold().ch(2),
                p("Bass", "Reese G#").hold().ch(2),
                p("Bass", "Reese C").hold().ch(2),
                p("Bass", "Sub Drop"),
                pk(UNRULY, "Snares", "Unruly Snare 14 Rimshot"),
                pk(UNRULY, "Breaks", "Unruly Jungle Roller 165").lp(165.0),
                pk(FRANKEN, "Claps", "Franken Clap Stack 1"),
                pk(UNRULY, "Breaks", "Unruly Halftime 170").lp(170.0),
            ],
            bars: 4,
            demo: vec![
                (0, "x.........x..... x.........x..... x.........x..... x.........x..x.."),
                (1, "....x.......x... ....x.......x... ....x.......x... ....x.......x..."),
                (2, "x.o.x.o.x.o.x.o. x.o.x.o.x.o.x.o. x.o.x.o.x.o.x.o. x.o.x.o.x.o.x..."),
                (3, "................ ..............x. ................ ..............x."),
                (6, "................ ................ x...x...x...x... x...x...x......."),
                (7, "x............... ................ ................ ................"),
                (8, "x-----------.... ................ x-----------.... ................"),
                (9, "................ x-------........ ................ x-------........"),
                (10, "................ ........x-----.. ................ ........x-----.."),
                (12, "................ .......o.o...... ................ .......o.o...ox."),
                (14, "................ ................ ....x.......x... ....x.......x..."),
            ],
        },
        KitSpec {
            id: "Percussion Jam",
            name_ru: "Перкуссия",
            about_ru: "Живая перкуссия: кахон, дарбука, джембе, бонго, конги, клаве, шейкер и ковбелл.",
            about_en: "Live hand percussion: cajon, darbouka, djembe, bongos, congas, claves, shaker and cowbell.",
            bpm: 108.0,
            swing: 0.0,
            accent: [235, 190, 80],
            pads: vec![
                pk(SWIRLY, "Kicks", "Swirly Cajon Kick"),
                pk(SWIRLY, "Percussion", "Swirly Darbouka"),
                pk(VIRTUOSITY, "Percussion", "Shaker Down"),
                pk(VIRTUOSITY, "Percussion", "Shaker Up"),
                pk(SWIRLY, "Percussion", "Swirly Djembe"),
                pk(SWIRLY, "Percussion", "Swirly Bongo High"),
                pk(SWIRLY, "Percussion", "Swirly Bongo Low"),
                pk(VIRTUOSITY, "Percussion", "Conga High"),
                pk(VIRTUOSITY, "Percussion", "Conga"),
                pk(VIRTUOSITY, "Percussion", "Tumba"),
                pk(VIRTUOSITY, "Percussion", "Claves"),
                pk(SWIRLY, "Percussion", "Swirly Cowbell"),
                pk(VIRTUOSITY, "Percussion", "Agogo High"),
                pk(VIRTUOSITY, "Percussion", "Agogo Low"),
                pk(VIRTUOSITY, "Percussion", "Triangle"),
                pk(VIRTUOSITY, "Percussion", "Vibraslap"),
            ],
            bars: 2,
            demo: vec![
                (0, "x.....x...x..... x.....x...x.x..."),
                (1, "....x..x....x... ....x..x....x.x."),
                (2, "x.x.x.x.x.x.x.x. x.x.x.x.x.x.x.x."),
                (3, ".o.o.o.o.o.o.o.o .o.o.o.o.o.o.o.o"),
                (5, "..x...x......x.. ..x...x......x.x"),
                (6, "........x.x..... ........x.x....."),
                (8, "......x.x....... ......x.x.....x."),
                (10, "x..x..x...x.x... x..x..x...x.x..."),
                (11, "x...x...x...x... x...x...x...x..."),
            ],
        },
    ]
}

fn builtin_kits() -> Vec<KitSpec> {
    vec![
        KitSpec {
            id: "DnB Essentials",
            name_ru: "Драм-н-бейс",
            about_ru: "Классика жанра: амен-брейк, риз-бас и рейв-стэбы. Фа минор.",
            about_en: "The classics: an Amen-style break, Reese bass and rave stabs. F minor.",
            bpm: 174.0,
            swing: 0.0,
            accent: [72, 222, 170],
            pads: vec![
                p("Kicks", "DnB Punch"),
                p("Snares", "DnB Crack"),
                p("Hi-Hats", "Closed Tight").ch(1),
                p("Hi-Hats", "Open Hat").ch(1),
                p("Breaks", "Amen-style 170").lp(170.0),
                p("Breaks", "Think-style 170").lp(170.0),
                p("Cymbals", "Ride"),
                p("Cymbals", "Crash"),
                p("Bass", "Reese F").hold().ch(2),
                p("Bass", "Reese G#").hold().ch(2),
                p("Bass", "Reese C").hold().ch(2),
                p("Bass", "Sub Drop"),
                p("Stabs", "Rave Stab Fm"),
                p("FX", "Air Horn"),
                p("FX", "Dub Siren"),
                p("FX", "Rewind"),
            ],
            bars: 4,
            demo: vec![
                (0, "x.........x..... x.........x..... x.........x..... x.........x..x.."),
                (1, "....x.......x... ....x.......x... ....x.......x... ....x.......x.x."),
                (2, "x.o.x.o.x.o.x.o. x.o.x.o.x.o.x.o. x.o.x.o.x.o.x.o. x.o.x.o.x.o.x..."),
                (3, "................ ..............x. ................ ..............x."),
                (7, "x..............."),
                (8, "x-----------.... ................ x-----------.... ................"),
                (9, "................ x-------........ ................ x-------........"),
                (10, "................ ........x-----.. ................ ........x-----.."),
                (12, "................ ................ ......x......... ......x...x....."),
            ],
        },
        KitSpec {
            id: "Jungle Massive",
            name_ru: "Джангл",
            about_ru: "Олдскул: рубленые брейки, глубокий саб, рейв-пиано и сирены. Ля минор.",
            about_en: "Old school: chopped breaks, deep sub, rave piano and sirens. A minor.",
            bpm: 165.0,
            swing: 0.0,
            accent: [250, 210, 80],
            pads: vec![
                p("Kicks", "Jungle Boom"),
                p("Snares", "Jungle Tuned"),
                p("Hi-Hats", "Closed Dusty").ch(1),
                p("Hi-Hats", "Open Short").ch(1),
                p("Breaks", "Amen-style Chopped 174").lp(174.0),
                p("Breaks", "Jungle Roller 165").lp(165.0),
                p("Breaks", "Apache-style 172").lp(172.0),
                p("Cymbals", "Crash"),
                p("Bass", "Sub A").hold().ch(2),
                p("Bass", "Sub G").hold().ch(2),
                p("Bass", "Hoover A").hold().ch(2),
                p("Bass", "Sub Drop"),
                p("Stabs", "Rave Stab Am"),
                p("Keys", "Piano Chord Am"),
                p("FX", "Dub Siren"),
                p("FX", "Gunshot"),
            ],
            bars: 4,
            demo: vec![
                (5, "x--------------- ---------------- ---------------- ----------------"),
                (7, "x..............."),
                (8, "x-----..x---.... ................ x-----..x---.... ................"),
                (9, "................ x-----....x--... ................ x---------......"),
                (13, "x.....x.....x... ................ x.....x.....x... ................"),
                (12, "................ ..........x..x.. ................ ..........x..x.."),
            ],
        },
        KitSpec {
            id: "Neurofunk",
            name_ru: "Нейрофанк",
            about_ru: "Тёмный и злой: рычащие басы, жёсткие барабаны, импакты. Фа минор.",
            about_en: "Dark and mean: growling basses, hard drums, impacts. F minor.",
            bpm: 174.0,
            swing: 0.0,
            accent: [170, 128, 255],
            pads: vec![
                p("Kicks", "Neuro Distorted"),
                p("Snares", "Neuro Snare"),
                p("Hi-Hats", "Closed Bright").ch(1),
                p("Hi-Hats", "Metallic Tick"),
                p("Breaks", "Two-Step 174").lp(174.0),
                p("Breaks", "Halftime 170").lp(170.0),
                p("Cymbals", "Ride Bell"),
                p("Cymbals", "China"),
                p("Bass", "Neuro Growl F").hold().ch(2),
                p("Bass", "Neuro Growl G#").hold().ch(2),
                p("Bass", "Wobble F").hold().ch(2),
                p("Bass", "Dark Reese F").hold().ch(2),
                p("Stabs", "Dark Stab"),
                p("FX", "Impact Boom"),
                p("FX", "Riser"),
                p("FX", "Downlifter"),
            ],
            bars: 4,
            demo: vec![
                (0, "x.........x..... x.....x...x..... x.........x..... x.....x...x..x.."),
                (1, "....x.......x... ....x.......x... ....x.......x... ....x.......x..."),
                (2, "x.oxx.oxx.oxx.ox x.oxx.oxx.oxx.ox x.oxx.oxx.oxx.ox x.oxx.oxx.oxx.ox"),
                (8, "x-..x-....x-x-.. ................ x-..x-....x-x-.. ................"),
                (9, "................ x-..x-....x-.... ................ ........x-x-x-.."),
                (10, "................ ............x--- ................ x-------........"),
                (13, "x..............."),
            ],
        },
        KitSpec {
            id: "Liquid",
            name_ru: "Ликвид",
            about_ru: "Мягкий мелодичный драм-н-бейс: родес, пэды, арпеджио и тёплый саб. Ля минор.",
            about_en: "Soft, melodic drum & bass: Rhodes, pads, an arpeggio and warm sub. A minor.",
            bpm: 172.0,
            swing: 0.0,
            accent: [72, 196, 246],
            pads: vec![
                p("Kicks", "Liquid Soft"),
                p("Snares", "Liquid Snare"),
                p("Hi-Hats", "Closed Dusty").ch(1),
                p("Hi-Hats", "Shaker Hat"),
                p("Breaks", "Liquid Shuffle 174").lp(174.0),
                p("Loops", "Arp Am 172").lp(172.0),
                p("Cymbals", "Ride"),
                p("Cymbals", "Reverse Crash"),
                p("Bass", "Sub A").hold().ch(2),
                p("Bass", "Sub F").hold().ch(2),
                p("Bass", "Sub G").hold().ch(2),
                p("Stabs", "Liquid Pad Am").hold(),
                p("Keys", "Rhodes Am7"),
                p("Keys", "Rhodes Fmaj7"),
                p("Vox", "Vox Ah"),
                p("Keys", "Bell"),
            ],
            bars: 4,
            demo: vec![
                (0, "x.........x..... x.........x..... x.........x..... x.........x....."),
                (1, "....x.......x... ....x.......x... ....x.......x... ....x.......x..."),
                (2, "..x...x...x...x. ..x...x...x...x. ..x...x...x...x. ..x...x...x...x."),
                (3, "o.o.o.o.o.o.o.o. o.o.o.o.o.o.o.o. o.o.o.o.o.o.o.o. o.o.o.o.o.o.o.o."),
                (5, "x--------------- ---------------- ---------------- ----------------"),
                (8, "x-------------.. ................ x-------------.. ................"),
                (9, "................ x-------........ ................ x-------........"),
                (10, "................ ........x------. ................ ........x------."),
                (12, "x............... ................ x............... ................"),
                (13, "................ x............... ................ x..............."),
                (15, "..........x..... ................ ..........x...x. ................"),
            ],
        },
        KitSpec {
            id: "Phonk",
            name_ru: "Фонк",
            about_ru: "Дрифт-фонк: мелодии на ковбеллах и перегруженные 808-е. Ре минор.",
            about_en: "Drift phonk: cowbell melodies and overdriven 808s. D minor.",
            bpm: 130.0,
            swing: 0.0,
            accent: [255, 92, 92],
            pads: vec![
                p("Kicks", "Phonk Kick"),
                p("Claps", "Clap"),
                p("Hi-Hats", "Closed Short").ch(1),
                p("Hi-Hats", "Open Short").ch(1),
                p("Keys", "Cowbell D"),
                p("Keys", "Cowbell F"),
                p("Keys", "Cowbell G"),
                p("Keys", "Cowbell A"),
                p("Bass", "808 Dist D").ch(2),
                p("Bass", "808 Dist F").ch(2),
                p("Bass", "808 Dist G").ch(2),
                p("Bass", "808 Dist C").ch(2),
                p("Snares", "Tight Snare"),
                p("Vox", "Vox Hey"),
                p("Loops", "Cowbell Line Dm 130").lp(130.0),
                p("FX", "Impact Boom"),
            ],
            bars: 4,
            demo: vec![
                (0, "x...x...x...x... x...x...x...x... x...x...x...x... x...x...x...x..."),
                (1, "....x.......x... ....x.......x... ....x.......x... ....x.......x..."),
                (2, "x.x.x.xxx.x.x.x. x.x.x.x.x.x.xxxx x.x.x.xxx.x.x.x. x.x.x.x.x.x.x.x."),
                (4, "x.x...x.....x... x.x...x......... x.x...x.....x... x.x...x........."),
                (5, "....x.....x..... ....x........... ....x.....x..... ....x..........."),
                (6, "........x....... ........x.x..... ........x....... ........x......."),
                (7, "................ ............x... ................ ..........x.x..."),
                (8, "x.......x..x.... ................ x.......x..x.... ................"),
                (9, "................ x.......x....... ................ ................"),
                (10, "................ ...........x.... ................ x.......x......."),
                (11, "................ ................ ................ ...........x..x."),
            ],
        },
        KitSpec {
            id: "Hardbass",
            name_ru: "Хардбасс",
            about_ru: "Прямая бочка, донк-бас в слабую долю и рейв-свисток. Ми минор.",
            about_en: "Four-on-the-floor kick, off-beat donk bass and a rave whistle. E minor.",
            bpm: 150.0,
            swing: 0.0,
            accent: [255, 153, 72],
            pads: vec![
                p("Kicks", "Hard Kick"),
                p("Claps", "Big Clap"),
                p("Hi-Hats", "Closed Tight").ch(1),
                p("Hi-Hats", "Open Hat").ch(1),
                p("Bass", "Donk E").ch(2),
                p("Bass", "Donk G").ch(2),
                p("Bass", "Donk A").ch(2),
                p("Bass", "Donk B").ch(2),
                p("Loops", "Donk Line E 150").lp(150.0),
                p("Stabs", "Rave Stab Em"),
                p("Bass", "Hoover E").hold(),
                p("Keys", "Whistle"),
                p("FX", "Air Horn"),
                p("Vox", "Vox Hey"),
                p("FX", "Short Riser"),
                p("Cymbals", "Crash"),
            ],
            bars: 4,
            demo: vec![
                (0, "x...x...x...x... x...x...x...x... x...x...x...x... x...x...x...x..."),
                (1, "....x.......x... ....x.......x... ....x.......x... ....x.......x..."),
                (3, "..x...x...x...x. ..x...x...x...x. ..x...x...x...x. ..x...x...x...x."),
                (4, "..x...x...x...x. ................ ..x...x...x...x. ..............x."),
                (5, "................ ..x...x......... ................ ................"),
                (6, "................ ..........x...x. ................ ................"),
                (7, "................ ................ ................ ..x...x...x....."),
                (9, "x............... ................ x.....x......... ................"),
                (11, "................ ............x... ................ ............x.x."),
                (15, "x..............."),
            ],
        },
        KitSpec {
            id: "808 Trap",
            name_ru: "Трэп 808",
            about_ru: "Долгие 808-басы, дробные хэты и колокольчики. До минор.",
            about_en: "Long 808 basses, rolling hats and bells. C minor.",
            bpm: 140.0,
            swing: 0.0,
            accent: [240, 112, 200],
            pads: vec![
                p("Kicks", "808 Style"),
                p("Claps", "Clap"),
                p("Hi-Hats", "Closed Short").ch(1),
                p("Hi-Hats", "Open Short").ch(1),
                p("Snares", "Tight Snare"),
                p("Snares", "Rimshot"),
                p("Claps", "Snap"),
                p("Loops", "Hat Roll 140").sync(140.0),
                p("Bass", "808 C").ch(2),
                p("Bass", "808 D#").ch(2),
                p("Bass", "808 F").ch(2),
                p("Bass", "808 G").ch(2),
                p("Keys", "Bell Low"),
                p("Keys", "Pluck Chord Cm"),
                p("FX", "Laser Zap"),
                p("FX", "Downlifter"),
            ],
            bars: 4,
            demo: vec![
                (0, "x.....x...x..... x.....x......x.. x.....x...x..... x..x..x...x....."),
                (1, "........x....... ........x....... ........x....... ........x......."),
                (2, "x.x.x.x.x.x.x.x. x.x.x.x.x.x..... x.x.x.x.x.x.x.x. x.xxx.x.x.x.x.x."),
                (7, "................ ............x... ................ ................"),
                (8, "x.....x......... ................ x.....x......... ................"),
                (9, "..........x..... ................ ..........x..... ................"),
                (10, "................ x.....x......... ................ x..............."),
                (11, "................ ..............x. ................ ......x...x....."),
                (12, "x.......x...x... ........x....... x.......x...x... ................"),
            ],
        },
        KitSpec {
            id: "House Party",
            name_ru: "Хаус",
            about_ru: "Прямая бочка, органные стэбы, рояль и упругий бас. Ля минор.",
            about_en: "Four-on-the-floor kick, organ stabs, piano and a bouncy bass. A minor.",
            bpm: 124.0,
            swing: 0.0,
            accent: [112, 144, 255],
            pads: vec![
                p("Kicks", "909 Style"),
                p("Claps", "Big Clap"),
                p("Hi-Hats", "Closed Tight").ch(1),
                p("Hi-Hats", "Open Hat").ch(1),
                p("Percussion", "Shaker"),
                p("Percussion", "Conga High"),
                p("Percussion", "Tom Low"),
                p("Cymbals", "Crash"),
                p("Bass", "Pluck Bass A").ch(2),
                p("Bass", "Pluck Bass C").ch(2),
                p("Bass", "Pluck Bass D").ch(2),
                p("Bass", "Pluck Bass E").ch(2),
                p("Stabs", "Organ Stab Am"),
                p("Keys", "Piano Chord Am"),
                p("Vox", "Vox Ah"),
                p("FX", "Riser"),
            ],
            bars: 4,
            demo: vec![
                (0, "x...x...x...x... x...x...x...x... x...x...x...x... x...x...x...x..."),
                (1, "....x.......x... ....x.......x... ....x.......x... ....x.......x..."),
                (3, "..x...x...x...x. ..x...x...x...x. ..x...x...x...x. ..x...x...x...x."),
                (4, "o.o.o.o.o.o.o.o. o.o.o.o.o.o.o.o. o.o.o.o.o.o.o.o. o.o.o.o.o.o.o.o."),
                (8, "..x..x....x..x.. ................ ..x..x....x..x.. ................"),
                (9, "................ ..x..x.......... ................ ..x..x.........."),
                (10, "................ ..........x..x.. ................ ................"),
                (11, "................ ................ ................ ..........x..x.."),
                (12, "x.....x......... ................ x.....x......... ................"),
                (13, "................ x.....x.....x... ................ x.....x.....x..."),
            ],
        },
    ]
}

pub fn find_kit(id: &str) -> Option<KitSpec> {
    kits().into_iter().find(|k| k.id == id)
}

impl KitSpec {
    /// Turn the preset into a playable kit. Built-in sounds are taken from `root`,
    /// pack sounds from their pack.
    pub fn build(&self, owner: &str, root: &Path) -> Kit {
        let pads = self
            .pads
            .iter()
            .enumerate()
            .map(|(i, s)| PadConfig {
                name: s.sound.to_string(),
                sample: Some(crate::library::pack_dir(s.pack).unwrap_or(root).join(s.cat).join(format!("{}.wav", s.sound))),
                binding: Some(DEFAULT_BINDINGS[i]),
                mode: s.mode,
                choke: s.choke,
                sync_bpm: s.sync,
                color: pad_color(s.cat),
                ..Default::default()
            })
            .collect();
        Kit {
            name: self.title().to_string(),
            owner: owner.to_string(),
            author: crate::kit::AUTHOR.to_string(),
            pads,
            bpm: self.bpm,
            swing: self.swing,
            pattern: Pattern::from_rows(self.bars, &self.demo),
            factory: Some(self.id.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed() -> PathBuf {
        use std::sync::OnceLock;
        static DIR: OnceLock<PathBuf> = OnceLock::new();
        DIR.get_or_init(|| {
            let dir = std::env::temp_dir().join("sensepad_factory_test");
            let _ = std::fs::remove_dir_all(&dir);
            let progress = AtomicU32::new(0);
            install(&dir, &progress).expect("install");
            assert_eq!(progress.load(Ordering::Relaxed), 100);
            dir
        })
        .clone()
    }

    #[test]
    fn every_category_has_sounds() {
        let dir = installed();
        let mut total = 0;
        for cat in CATEGORIES {
            let count = std::fs::read_dir(dir.join(cat)).unwrap().count();
            assert!(count >= 4, "{cat} has only {count} sounds");
            total += count as u32;
        }
        assert!((total as i64 - EXPECTED as i64).abs() <= 6, "EXPECTED is {EXPECTED} but {total} sounds were written");
    }

    #[test]
    fn kits_point_at_real_sounds_and_have_sane_demos() {
        let dir = installed();
        let all = kits();
        assert!(all.len() >= 8);
        for spec in &all {
            assert_eq!(spec.pads.len(), 16, "{}", spec.id);
            let kit = spec.build("Player", &dir);
            for (i, pad) in kit.pads.iter().enumerate() {
                let path = pad.sample.as_ref().unwrap();
                assert!(path.exists(), "{}: pad {} points at a missing file {}", spec.id, i + 1, path.display());
            }
            assert!(!kit.pattern.is_empty(), "{} has no demo", spec.id);
            for n in &kit.pattern.notes {
                assert!((n.step as usize) < kit.pattern.steps() && n.pad < 16);
                let mode = kit.pads[n.pad as usize].mode;
                assert!(n.len == 1 || mode != PlayMode::OneShot, "{}: long note on one-shot pad {}", spec.id, n.pad + 1);
            }
        }
    }

    #[test]
    fn sounds_decode_cleanly_and_loops_fit_their_bars() {
        let dir = installed();
        for f in ["Kicks/DnB Punch.wav", "FX/Rewind.wav", "Bass/Hoover A.wav", "Keys/Cowbell D.wav", "Bass/Donk E.wav", "Vox/Vox Hey.wav"] {
            let s = crate::audio::load_sample(&dir.join(f)).unwrap();
            assert!(s.duration > 0.1 && s.frames.iter().all(|v| v.is_finite()), "{f}");
            let peak = s.frames.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.5, "{f} is too quiet ({peak})");
        }
        let bars = |file: &str, bpm: f32| -> f32 {
            let s = crate::audio::load_sample(&dir.join(file)).unwrap();
            s.duration / (4.0 * 60.0 / bpm)
        };
        assert!((bars("Breaks/Amen-style 170.wav", 170.0) - 4.0).abs() < 0.001);
        assert!((bars("Loops/Arp Am 172.wav", 172.0) - 2.0).abs() < 0.001);
        assert!((bars("Loops/Donk Line E 150.wav", 150.0) - 2.0).abs() < 0.001);
        assert!((bars("Loops/Hat Roll 140.wav", 140.0) - 0.25).abs() < 0.001);
    }

    #[test]
    fn every_sound_is_audible_and_clean() {
        let dir = installed();
        let mut checked = 0;
        let mut problems: Vec<String> = Vec::new();
        for cat in CATEGORIES {
            for entry in std::fs::read_dir(dir.join(cat)).unwrap().flatten() {
                let path = entry.path();
                let name = format!("{cat}/{}", path.file_name().unwrap().to_string_lossy());
                let s = match crate::audio::load_sample(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        problems.push(format!("{name}: {e}"));
                        continue;
                    }
                };
                let mono: Vec<f32> = s.frames.chunks(2).map(|f| f[0]).collect();
                let count = mono.len() as f32;
                let peak = mono.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                let rms = (mono.iter().map(|v| v * v).sum::<f32>() / count).sqrt();
                let dc = mono.iter().sum::<f32>() / count;
                let last = mono[mono.len() - 1].abs();
                let looped = cat == "Breaks" || cat == "Loops" || name.contains("Chop");
                if !mono.iter().all(|v| v.is_finite()) {
                    problems.push(format!("{name}: not finite"));
                }
                if !(0.04..=12.0).contains(&s.duration) {
                    problems.push(format!("{name}: odd length {}", s.duration));
                }
                if !(peak > 0.8 && peak <= 1.0) {
                    problems.push(format!("{name}: peak {peak}"));
                }
                if rms < 0.02 {
                    problems.push(format!("{name}: nearly silent (rms {rms})"));
                }
                if dc.abs() > 0.12 {
                    problems.push(format!("{name}: DC offset {dc}"));
                }
                if last > 0.05 && !looped {
                    problems.push(format!("{name}: ends with a click ({last})"));
                }
                checked += 1;
            }
        }
        assert!(problems.is_empty(), "{} problem(s):
{}", problems.len(), problems.join("
"));
        assert!(checked >= 140);
    }

    #[test]
    fn old_paths_find_their_new_home() {
        let dir = installed();
        let old = dir.join("Kits").join("DnB Essentials").join("01 DnB Punch.wav");
        assert_eq!(relocate(&old, &dir), Some(dir.join("Kicks").join("DnB Punch.wav")));
        let renamed = dir.join("Bass").join("Hoover.wav");
        assert_eq!(relocate(&renamed, &dir), Some(dir.join("Bass").join("Hoover A.wav")));
        assert_eq!(relocate(&dir.join("Kicks").join("DnB Punch.wav"), &dir), None);
        assert_eq!(relocate(Path::new("C:/elsewhere/Kick.wav"), &dir), None);
    }
}
