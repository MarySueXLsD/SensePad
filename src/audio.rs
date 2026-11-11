//! The sound engine: sample decoding, a 16-pad sampler, a tempo clock with a step
//! looper (record, overdub, swing, metronome, quantised loop launch) and DJ effects.

use crate::kit::{BufferPref, FxKind, PadConfig, PlayMode, FX_COUNT, PAD_COUNT};
use crate::seq::{Note, Pattern};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Decoded audio, always interleaved stereo f32.
pub struct SampleData {
    pub frames: Vec<f32>,
    pub len: usize,
    pub sample_rate: u32,
    pub channels: usize,
    pub duration: f32,
    /// Waveform overview: (min, max) per bucket.
    pub peaks: Vec<(f32, f32)>,
}

const MAX_SECONDS: usize = 180;
const PEAK_BUCKETS: usize = 160;
/// Voice slot used by the library preview.
pub const PREVIEW_PAD: usize = 31;

pub fn load_sample(path: &Path) -> Result<SampleData, String> {
    // Some WAV files carry a header the decoder refuses (an over-long `fmt ` chunk, for
    // example). Plain PCM is simple enough to read directly, so try that before giving up.
    decode(path).or_else(|e| read_plain_wav(path).ok_or(e))
}

/// Read uncompressed WAV data without the decoder.
fn read_plain_wav(path: &Path) -> Option<SampleData> {
    let b = std::fs::read(path).ok()?;
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let u16_at = |i: usize| b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
    let u32_at = |i: usize| b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    let (mut fmt, mut data) = (None, None);
    let mut at = 12;
    while at + 8 <= b.len() {
        let size = u32_at(at + 4)? as usize;
        let body = at + 8;
        match &b[at..at + 4] {
            b"fmt " => fmt = Some(body),
            b"data" => data = Some((body, size.min(b.len() - body))),
            _ => {}
        }
        at = body + size + (size & 1);
    }
    let (fmt, (start, size)) = (fmt?, data?);
    let (tag, channels, rate, bits) = (u16_at(fmt)?, u16_at(fmt + 2)? as usize, u32_at(fmt + 4)?, u16_at(fmt + 14)? as usize);
    // 1 is integer PCM, 3 is float; 0xFFFE ("extensible") names the real format further on.
    let tag = if tag == 0xFFFE { u16_at(fmt + 24)? } else { tag };
    if channels == 0 || rate == 0 || !matches!(tag, 1 | 3) || !matches!(bits, 8 | 16 | 24 | 32) {
        return None;
    }
    let bytes = bits / 8;
    let sample = |s: &[u8]| -> f32 {
        match (bits, tag) {
            (8, _) => (s[0] as f32 - 128.0) / 128.0,
            (16, _) => i16::from_le_bytes([s[0], s[1]]) as f32 / 32_768.0,
            (24, _) => (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32 / 8_388_608.0,
            (_, 3) => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
            _ => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0,
        }
    };
    let mut frames = Vec::with_capacity(size / (bytes * channels) * 2);
    for f in b[start..start + size].chunks_exact(bytes * channels).take(MAX_SECONDS * rate as usize) {
        let l = sample(&f[..bytes]);
        let r = if channels > 1 { sample(&f[bytes..bytes * 2]) } else { l };
        frames.push(l);
        frames.push(r);
    }
    finish(frames, rate, channels).ok()
}

fn decode(path: &Path) -> Result<SampleData, String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|e| format!("{e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("{e}"))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("no audio track")?;
    let track_id = track.id;
    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(44_100);
    let mut channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("{e}"))?;

    let mut frames: Vec<f32> = Vec::new();
    let mut buf: Option<(SampleBuffer<f32>, u64)> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(e) => {
                if frames.is_empty() {
                    return Err(format!("{e}"));
                }
                break;
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(Error::DecodeError(_)) => continue,
            Err(e) => {
                if frames.is_empty() {
                    return Err(format!("{e}"));
                }
                break;
            }
        };
        let spec = *decoded.spec();
        sample_rate = spec.rate;
        channels = spec.channels.count().max(1);
        let cap = decoded.capacity() as u64;
        if buf.as_ref().is_none_or(|(_, c)| *c < cap) {
            buf = Some((SampleBuffer::<f32>::new(cap, spec), cap));
        }
        let (sb, _) = buf.as_mut().unwrap();
        sb.copy_interleaved_ref(decoded);
        for f in sb.samples().chunks(channels) {
            let l = f[0];
            let r = if channels > 1 { f[1] } else { l };
            frames.push(l);
            frames.push(r);
        }
        if frames.len() / 2 > MAX_SECONDS * sample_rate as usize {
            break;
        }
    }
    finish(frames, sample_rate, channels)
}

/// Wrap decoded stereo frames up with their waveform overview.
fn finish(frames: Vec<f32>, sample_rate: u32, channels: usize) -> Result<SampleData, String> {
    let len = frames.len() / 2;
    if len == 0 {
        return Err("no audio".into());
    }

    let mut peaks = Vec::with_capacity(PEAK_BUCKETS);
    let per = (len / PEAK_BUCKETS).max(1);
    for b in 0..PEAK_BUCKETS {
        let start = b * per;
        if start >= len {
            break;
        }
        let end = ((b + 1) * per).min(len);
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        for i in start..end {
            let v = (frames[i * 2] + frames[i * 2 + 1]) * 0.5;
            lo = lo.min(v);
            hi = hi.max(v);
        }
        peaks.push((lo, hi));
    }

    Ok(SampleData { duration: len as f32 / sample_rate as f32, frames, len, sample_rate, channels, peaks })
}

// ----- shared state ----------------------------------------------------------

/// What the engine needs to know about one pad.
#[derive(Clone)]
pub struct PadSlot {
    pub sample: Option<Arc<SampleData>>,
    pub gain: f32,
    pub pan: f32,
    pub semis: f32,
    pub mode: PlayMode,
    pub choke: u8,
    pub overlap: bool,
    pub sync_bpm: f32,
}

impl PadSlot {
    pub fn new(cfg: &PadConfig, sample: Option<Arc<SampleData>>) -> Self {
        Self {
            sample,
            gain: cfg.volume,
            pan: cfg.pan,
            semis: cfg.pitch,
            mode: cfg.mode,
            choke: cfg.choke,
            overlap: cfg.overlap,
            sync_bpm: cfg.sync_bpm,
        }
    }

    fn empty() -> Self {
        Self::new(&PadConfig::default(), None)
    }
}

/// Lock-free effect parameters. Every effect is neutral at 0.
/// Bipolar effects (filter, pitch, pan) range -1..1, the rest 0..1.
pub struct FxParams {
    vals: [AtomicU32; FX_COUNT],
    pitch_range: AtomicU32,
}

impl FxParams {
    fn new() -> Self {
        Self { vals: std::array::from_fn(|_| AtomicU32::new(0)), pitch_range: AtomicU32::new(12f32.to_bits()) }
    }
    pub fn set(&self, k: FxKind, v: f32) {
        self.vals[k.index()].store(v.to_bits(), Ordering::Relaxed);
    }
    pub fn get(&self, k: FxKind) -> f32 {
        f32::from_bits(self.vals[k.index()].load(Ordering::Relaxed))
    }
    pub fn set_pitch_range(&self, semitones: f32) {
        self.pitch_range.store(semitones.to_bits(), Ordering::Relaxed);
    }
    fn pitch_range(&self) -> f32 {
        f32::from_bits(self.pitch_range.load(Ordering::Relaxed))
    }
    pub fn reset(&self) {
        for v in &self.vals {
            v.store(0, Ordering::Relaxed);
        }
    }
}

/// What the interface reads back from the engine.
pub struct EngineView {
    peak: AtomicU32,
    active: AtomicU32,
    queued: AtomicU32,
    playing: AtomicBool,
    recording: AtomicBool,
    step: AtomicU32,
    progress: [AtomicU32; PAD_COUNT],
    hits: [AtomicU32; PAD_COUNT],
    online: AtomicBool,
    failed: AtomicBool,
}

impl EngineView {
    fn new() -> Self {
        Self {
            peak: AtomicU32::new(0),
            active: AtomicU32::new(0),
            queued: AtomicU32::new(0),
            playing: AtomicBool::new(false),
            recording: AtomicBool::new(false),
            step: AtomicU32::new(u32::MAX),
            progress: std::array::from_fn(|_| AtomicU32::new((-1.0f32).to_bits())),
            hits: std::array::from_fn(|_| AtomicU32::new(0)),
            online: AtomicBool::new(false),
            failed: AtomicBool::new(false),
        }
    }
}

/// Notes the engine recorded while you played along.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SeqEvent {
    Recorded(Note),
    Len { step: u8, pad: u8, len: u8 },
}

enum Cmd {
    SetPad(usize, Box<PadSlot>),
    Hit(usize),
    Release(usize),
    Preview(Arc<SampleData>),
    StopPad(usize),
    StopAll,
    Master(f32),
    Play,
    Stop,
    Record(bool),
    Tempo(f32),
    Swing(f32),
    Metronome(bool),
    LaunchQuant(u8),
    FxSync(bool),
    SetPattern(Box<Pattern>),
}

/// Cheap, cloneable handle used by the interface and the controller thread.
#[derive(Clone)]
pub struct AudioHandle {
    tx: Sender<Cmd>,
    view: Arc<EngineView>,
    pub fx: Arc<FxParams>,
}

impl AudioHandle {
    fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
    fn online(&self) -> bool {
        self.view.online.load(Ordering::Relaxed)
    }

    pub fn set_pad(&self, i: usize, slot: PadSlot) {
        self.send(Cmd::SetPad(i, Box::new(slot)));
    }
    /// A pad was hit by hand (button, key or mouse).
    pub fn hit(&self, pad: usize) {
        if self.online() {
            self.send(Cmd::Hit(pad));
        }
    }
    pub fn release(&self, pad: usize) {
        if self.online() {
            self.send(Cmd::Release(pad));
        }
    }
    pub fn preview(&self, sample: Arc<SampleData>) {
        if self.online() {
            self.send(Cmd::Preview(sample));
        }
    }
    pub fn stop_pad(&self, pad: usize) {
        self.send(Cmd::StopPad(pad));
    }
    pub fn stop_all(&self) {
        self.send(Cmd::StopAll);
    }
    pub fn set_master(&self, v: f32) {
        self.send(Cmd::Master(v));
    }
    pub fn play(&self) {
        if self.online() {
            self.send(Cmd::Play);
        }
    }
    pub fn stop(&self) {
        self.send(Cmd::Stop);
    }
    pub fn set_recording(&self, on: bool) {
        self.send(Cmd::Record(on));
    }
    pub fn set_tempo(&self, bpm: f32) {
        self.send(Cmd::Tempo(bpm));
    }
    pub fn set_swing(&self, swing: f32) {
        self.send(Cmd::Swing(swing));
    }
    pub fn set_metronome(&self, on: bool) {
        self.send(Cmd::Metronome(on));
    }
    pub fn set_launch_quant(&self, steps: u8) {
        self.send(Cmd::LaunchQuant(steps));
    }
    pub fn set_fx_sync(&self, on: bool) {
        self.send(Cmd::FxSync(on));
    }
    pub fn set_pattern(&self, p: &Pattern) {
        self.send(Cmd::SetPattern(Box::new(p.clone())));
    }

    /// Bitmask of pads that are sounding.
    pub fn active_mask(&self) -> u32 {
        self.view.active.load(Ordering::Relaxed)
    }
    /// Bitmask of loop pads waiting for the next beat or bar.
    pub fn queued_mask(&self) -> u32 {
        self.view.queued.load(Ordering::Relaxed)
    }
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.view.peak.swap(0, Ordering::Relaxed))
    }
    pub fn playing(&self) -> bool {
        self.view.playing.load(Ordering::Relaxed)
    }
    pub fn recording(&self) -> bool {
        self.view.recording.load(Ordering::Relaxed)
    }
    /// The step the looper is on, while it plays.
    pub fn step(&self) -> Option<usize> {
        match self.view.step.load(Ordering::Relaxed) {
            u32::MAX => None,
            s => Some(s as usize),
        }
    }
    /// Playback position (0..1) of the newest voice of a pad.
    pub fn progress(&self, pad: usize) -> Option<f32> {
        let p = f32::from_bits(self.view.progress[pad].load(Ordering::Relaxed));
        (p >= 0.0).then_some(p)
    }
    /// How many times a pad has been triggered (by hand or by the looper).
    pub fn hit_count(&self, pad: usize) -> u32 {
        self.view.hits[pad].load(Ordering::Relaxed)
    }
    pub fn is_online(&self) -> bool {
        self.online()
    }
    pub fn failed(&self) -> bool {
        self.view.failed.load(Ordering::Relaxed)
    }
}

// ----- engine ----------------------------------------------------------------

pub struct AudioEngine {
    mixer: Arc<Mutex<Mixer>>,
    stream: Option<cpal::Stream>,
    pub handle: AudioHandle,
    pub device_name: String,
    pub sample_rate: u32,
}

impl AudioEngine {
    /// Run the engine without a sound device and return what it played: interleaved
    /// stereo at 48 kHz. The scripted trailer recording uses this.
    #[cfg(test)]
    pub fn run_without_device(&self) {
        // Play and pad hits are only accepted while the engine counts as running.
        self.handle.view.online.store(true, Ordering::Relaxed);
    }

    /// One slice of offline playback; see `run_without_device`.
    #[cfg(test)]
    pub fn render_offline(&self, frames: usize) -> Vec<f32> {
        let mut buf = vec![0.0f32; frames * 2];
        self.mixer.lock().unwrap().process(&mut buf, 2);
        buf
    }

    /// Create the engine without opening a device yet.
    pub fn new(master: f32) -> (Self, Receiver<SeqEvent>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (seq_tx, seq_rx) = crossbeam_channel::bounded(1024);
        let handle = AudioHandle { tx, view: Arc::new(EngineView::new()), fx: Arc::new(FxParams::new()) };
        let mixer = Mixer::new(rx, seq_tx, &handle, 48_000.0, master);
        (
            Self { mixer: Arc::new(Mutex::new(mixer)), stream: None, handle, device_name: String::new(), sample_rate: 0 },
            seq_rx,
        )
    }

    /// Names of the output devices, default first.
    pub fn devices() -> Vec<String> {
        let host = cpal::default_host();
        let name = |d: &cpal::Device| d.description().map(|x| x.name().to_string()).ok();
        let mut out: Vec<String> = Vec::new();
        if let Some(n) = host.default_output_device().as_ref().and_then(name) {
            out.push(n);
        }
        if let Ok(devs) = host.output_devices() {
            for n in devs.filter_map(|d| name(&d)) {
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        out
    }

    /// Open (or re-open) the output. `device` = None uses the system default.
    pub fn open(&mut self, device: Option<&str>, buffer: BufferPref) -> Result<(), String> {
        self.stream = None;
        self.handle.view.online.store(false, Ordering::Relaxed);
        let host = cpal::default_host();
        let wanted = device.and_then(|name| {
            host.output_devices().ok()?.find(|d| d.description().map(|x| x.name() == name).unwrap_or(false))
        });
        let dev = wanted.or_else(|| host.default_output_device()).ok_or("нет устройства вывода звука")?;
        let device_name = dev.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Audio".into());
        let supported = dev.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let mut config: cpal::StreamConfig = supported.config();
        let rate = config.sample_rate;
        if let Ok(mut m) = self.mixer.lock() {
            m.set_rate(rate as f64);
        }
        let frames = match buffer {
            BufferPref::Auto => None,
            BufferPref::Low => Some(256),
            BufferPref::Safe => Some(1024),
        };
        let mut stream = None;
        if let Some(n) = frames {
            config.buffer_size = cpal::BufferSize::Fixed(n);
            stream = self.build(&dev, &config, format).ok();
        }
        let stream = match stream {
            Some(s) => s,
            None => {
                config.buffer_size = cpal::BufferSize::Default;
                self.build(&dev, &config, format)?
            }
        };
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        self.device_name = device_name;
        self.sample_rate = rate;
        self.handle.view.failed.store(false, Ordering::Relaxed);
        self.handle.view.online.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn build(&self, dev: &cpal::Device, config: &cpal::StreamConfig, format: cpal::SampleFormat) -> Result<cpal::Stream, String> {
        match format {
            cpal::SampleFormat::F32 => self.build_typed::<f32>(dev, config),
            cpal::SampleFormat::I16 => self.build_typed::<i16>(dev, config),
            cpal::SampleFormat::U16 => self.build_typed::<u16>(dev, config),
            cpal::SampleFormat::I32 => self.build_typed::<i32>(dev, config),
            other => Err(format!("unsupported sample format {other:?}")),
        }
    }

    fn build_typed<T>(&self, dev: &cpal::Device, config: &cpal::StreamConfig) -> Result<cpal::Stream, String>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        let channels = config.channels as usize;
        let mixer = self.mixer.clone();
        let view = self.handle.view.clone();
        dev.build_output_stream(
            config.clone(),
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                if let Ok(mut m) = mixer.lock() {
                    m.process(data, channels);
                }
            },
            move |_| view.failed.store(true, Ordering::Relaxed),
            None,
        )
        .map_err(|e| e.to_string())
    }

    pub fn is_open(&self) -> bool {
        self.stream.is_some()
    }
}

const MAX_VOICES: usize = 64;
const RELEASE_SECONDS: f32 = 0.012;
const ATTACK_SECONDS: f32 = 0.0008;
const STUTTER_SECONDS: [f32; 4] = [0.25, 0.125, 0.0625, 0.03125];
const STUTTER_STEPS: [f64; 4] = [4.0, 2.0, 1.0, 0.5];

struct Voice {
    pad: usize,
    sample: Arc<SampleData>,
    pos: f64,
    base: f64,
    rate: f64,
    sync_bpm: f32,
    gl: f32,
    gr: f32,
    looping: bool,
    hold: bool,
    choke: u8,
    env: f32,
    releasing: bool,
    /// Steps left before a looper-started Hold or Loop voice is released.
    gate: Option<u32>,
    seq: bool,
}

#[derive(Clone, Copy)]
enum Src {
    Live,
    Seq(u8),
}

/// Zero-delay-feedback state-variable filter (one per channel).
#[derive(Default, Clone, Copy)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

struct Mixer {
    rx: Receiver<Cmd>,
    seq_tx: Sender<SeqEvent>,
    view: Arc<EngineView>,
    fx: Arc<FxParams>,
    out_rate: f64,
    master: f32,
    pads: Vec<PadSlot>,
    voices: Vec<Voice>,
    // clock and looper
    playing: bool,
    recording: bool,
    bpm: f32,
    swing: f32,
    metronome: bool,
    launch_q: usize,
    fx_sync: bool,
    pattern: Pattern,
    step: usize,
    phase: f64,
    step_len: f64,
    abs_step: u64,
    pending: Vec<usize>,
    skip_once: Vec<(u8, u8)>,
    open_notes: [Option<(u8, u64)>; PAD_COUNT],
    click_phase: f32,
    click_env: f32,
    click_freq: f32,
    // effects
    smooth: [f32; FX_COUNT],
    svf: [Svf; 2],
    crush_hold: [f32; 2],
    crush_count: usize,
    hist: Vec<[f32; 2]>,
    hist_w: usize,
    stutter: Option<(usize, usize, usize)>,
    stutter_idx: usize,
    delay: Vec<[f32; 2]>,
    delay_w: usize,
}

impl Mixer {
    fn new(rx: Receiver<Cmd>, seq_tx: Sender<SeqEvent>, h: &AudioHandle, out_rate: f64, master: f32) -> Self {
        let sr = out_rate as usize;
        Self {
            rx,
            seq_tx,
            view: h.view.clone(),
            fx: h.fx.clone(),
            out_rate,
            master,
            pads: (0..PAD_COUNT).map(|_| PadSlot::empty()).collect(),
            voices: Vec::with_capacity(MAX_VOICES),
            playing: false,
            recording: false,
            bpm: 174.0,
            swing: 0.0,
            metronome: false,
            launch_q: 0,
            fx_sync: true,
            pattern: Pattern::default(),
            step: 0,
            phase: 0.0,
            step_len: 0.0,
            abs_step: 0,
            pending: Vec::with_capacity(PAD_COUNT),
            skip_once: Vec::with_capacity(32),
            open_notes: [None; PAD_COUNT],
            click_phase: 0.0,
            click_env: 0.0,
            click_freq: 1000.0,
            smooth: [0.0; FX_COUNT],
            svf: [Svf::default(); 2],
            crush_hold: [0.0; 2],
            crush_count: 0,
            hist: vec![[0.0; 2]; sr * 2],
            hist_w: 0,
            stutter: None,
            stutter_idx: usize::MAX,
            delay: vec![[0.0; 2]; sr * 2],
            delay_w: 0,
        }
    }

    fn set_rate(&mut self, rate: f64) {
        if (rate - self.out_rate).abs() < 0.5 {
            return;
        }
        let ratio = self.out_rate / rate;
        for v in &mut self.voices {
            v.base *= ratio;
            v.rate *= ratio;
        }
        self.out_rate = rate;
        let n = rate as usize * 2;
        self.hist = vec![[0.0; 2]; n];
        self.delay = vec![[0.0; 2]; n];
        self.hist_w = 0;
        self.delay_w = 0;
        self.stutter = None;
        self.phase = 0.0;
        self.step_len = self.len_of(self.step);
    }

    /// Samples per 16th step at the current tempo.
    fn sps(&self) -> f64 {
        self.out_rate * 60.0 / self.bpm.clamp(40.0, 240.0) as f64 / 4.0
    }

    fn len_of(&self, step: usize) -> f64 {
        let s = self.swing.clamp(0.0, 0.6) as f64;
        self.sps() * if step % 2 == 0 { 1.0 + s } else { 1.0 - s }
    }

    fn release_where(&mut self, f: impl Fn(&Voice) -> bool) {
        for v in self.voices.iter_mut().filter(|v| f(v)) {
            v.releasing = true;
        }
    }

    fn sounding(&self, pad: usize) -> bool {
        self.voices.iter().any(|v| v.pad == pad && !v.releasing)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_voice(
        &mut self,
        pad: usize,
        sample: Arc<SampleData>,
        gain: f32,
        pan: f32,
        semis: f32,
        mode: PlayMode,
        choke: u8,
        sync_bpm: f32,
        gate: Option<u32>,
        seq: bool,
    ) {
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        let g = gain * std::f32::consts::SQRT_2;
        let base = sample.sample_rate as f64 / self.out_rate * 2f64.powf(semis as f64 / 12.0);
        let rate = base * if sync_bpm > 0.0 { (self.bpm / sync_bpm) as f64 } else { 1.0 };
        self.voices.push(Voice {
            pad,
            sample,
            pos: 0.0,
            base,
            rate,
            sync_bpm,
            gl: angle.cos() * g,
            gr: angle.sin() * g,
            looping: mode == PlayMode::Loop,
            hold: mode == PlayMode::Hold,
            choke,
            env: 0.0,
            releasing: false,
            gate,
            seq,
        });
        if pad < PAD_COUNT {
            self.view.hits[pad].fetch_add(1, Ordering::Relaxed);
        }
    }

    fn trigger(&mut self, pad: usize, vel: f32, src: Src) {
        let Some(slot) = self.pads.get(pad) else { return };
        let Some(sample) = slot.sample.clone() else { return };
        let (mode, choke, overlap) = (slot.mode, slot.choke, slot.overlap);
        let (gain, pan, semis, sync_bpm) = (slot.gain, slot.pan, slot.semis, slot.sync_bpm);
        if matches!(src, Src::Live) && mode == PlayMode::Loop && self.sounding(pad) {
            self.release_where(|v| v.pad == pad); // a second press stops the loop
            return;
        }
        if choke > 0 {
            self.release_where(|v| v.choke == choke);
        }
        if !overlap || mode != PlayMode::OneShot {
            self.release_where(|v| v.pad == pad);
        }
        let (gate, seq) = match src {
            Src::Seq(len) if mode != PlayMode::OneShot => (Some(len.max(1) as u32), true),
            Src::Seq(_) => (None, true),
            Src::Live => (None, false),
        };
        self.push_voice(pad, sample, gain * vel, pan, semis, mode, choke, sync_bpm, gate, seq);
    }

    fn frac(&self) -> f64 {
        if self.step_len > 0.0 { (self.phase / self.step_len).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// Write a hand-played hit into the pattern, snapped to the nearest step.
    fn record_note(&mut self, pad: usize, at_boundary: bool) {
        let total = self.pattern.steps();
        let future = !at_boundary && self.frac() >= 0.5;
        let qs = if future { (self.step + 1) % total } else { self.step % total };
        let mode = self.pads[pad].mode;
        let len = if mode == PlayMode::Loop { (total - qs).min(255) as u8 } else { 1 };
        let note = Note { step: qs as u8, pad: pad as u8, len, vel: 100 };
        if self.pattern.insert(note) {
            let _ = self.seq_tx.try_send(SeqEvent::Recorded(note));
        }
        if future || at_boundary {
            // It already sounded under the finger; don't fire it again when the step comes.
            self.skip_once.push((qs as u8, pad as u8));
        }
        self.open_notes[pad] = Some((qs as u8, self.abs_step + future as u64));
    }

    /// A held note or a loop ended: store how long it lasted.
    fn close_note(&mut self, pad: usize) {
        let Some((step, started)) = self.open_notes.get_mut(pad).and_then(|n| n.take()) else { return };
        if !(self.playing && self.recording) {
            return;
        }
        let now = self.abs_step + (self.frac() >= 0.5) as u64;
        let len = now.saturating_sub(started).clamp(1, self.pattern.steps() as u64) as u8;
        if self.pattern.set_len(step, pad as u8, len) {
            let _ = self.seq_tx.try_send(SeqEvent::Len { step, pad: pad as u8, len });
        }
    }

    fn hit(&mut self, pad: usize) {
        let Some(slot) = self.pads.get(pad) else { return };
        if slot.sample.is_none() {
            return;
        }
        let mode = slot.mode;
        let was_looping = mode == PlayMode::Loop && self.sounding(pad);
        if mode == PlayMode::Loop && self.playing && self.launch_q > 0 && !was_looping {
            // Wait for the next beat or bar so the loop lands in time.
            match self.pending.iter().position(|p| *p == pad) {
                Some(i) => {
                    self.pending.remove(i);
                }
                None => self.pending.push(pad),
            }
            return;
        }
        self.trigger(pad, 1.0, Src::Live);
        if self.playing && self.recording {
            if was_looping {
                self.close_note(pad);
            } else {
                self.record_note(pad, false);
            }
        }
    }

    fn stop_clock(&mut self) {
        self.playing = false;
        self.recording = false;
        self.pending.clear();
        self.skip_once.clear();
        self.open_notes = [None; PAD_COUNT];
        self.release_where(|v| v.seq);
        self.view.step.store(u32::MAX, Ordering::Relaxed);
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::SetPad(i, slot) => {
                if let Some(p) = self.pads.get_mut(i) {
                    *p = *slot;
                }
            }
            Cmd::Hit(pad) => self.hit(pad),
            Cmd::Release(pad) => {
                self.release_where(|v| v.pad == pad && v.hold && !v.seq);
                if self.pads.get(pad).is_some_and(|s| s.mode == PlayMode::Hold) {
                    self.close_note(pad);
                }
            }
            Cmd::Preview(sample) => {
                self.release_where(|v| v.pad == PREVIEW_PAD);
                self.push_voice(PREVIEW_PAD, sample, 0.9, 0.0, 0.0, PlayMode::OneShot, 0, 0.0, None, false);
            }
            Cmd::StopPad(pad) => self.release_where(|v| v.pad == pad),
            Cmd::StopAll => {
                self.release_where(|_| true);
                self.pending.clear();
            }
            Cmd::Master(m) => self.master = m,
            Cmd::Play => {
                if !self.playing {
                    self.playing = true;
                    self.step = self.pattern.steps() - 1;
                    self.phase = 0.0;
                    self.step_len = 0.0;
                    self.abs_step = 0;
                }
            }
            Cmd::Stop => self.stop_clock(),
            Cmd::Record(on) => {
                self.recording = on;
                if !on {
                    self.open_notes = [None; PAD_COUNT];
                }
            }
            Cmd::Tempo(bpm) => {
                if bpm.is_finite() {
                    self.bpm = bpm.clamp(40.0, 240.0);
                }
            }
            Cmd::Swing(s) => self.swing = if s.is_finite() { s.clamp(0.0, 0.6) } else { 0.0 },
            Cmd::Metronome(on) => self.metronome = on,
            Cmd::LaunchQuant(q) => self.launch_q = q as usize,
            Cmd::FxSync(on) => self.fx_sync = on,
            Cmd::SetPattern(p) => {
                self.pattern = *p;
                self.skip_once.clear();
            }
        }
    }

    /// The clock reached step `s`.
    fn on_step(&mut self, s: usize) {
        for v in self.voices.iter_mut() {
            if let Some(g) = v.gate.as_mut() {
                *g = g.saturating_sub(1);
                if *g == 0 {
                    v.releasing = true;
                    v.gate = None;
                }
            }
        }
        if self.launch_q > 0 && s % self.launch_q == 0 && !self.pending.is_empty() {
            let mut pend = std::mem::take(&mut self.pending);
            for &pad in &pend {
                self.trigger(pad, 1.0, Src::Live);
                if self.recording {
                    self.record_note(pad, true);
                }
            }
            pend.clear();
            self.pending = pend;
        }
        if self.metronome && s % 4 == 0 {
            self.click_env = 1.0;
            self.click_phase = 0.0;
            self.click_freq = if s % 16 == 0 { 1760.0 } else { 1175.0 };
        }
        let pat = std::mem::take(&mut self.pattern);
        for n in pat.notes.iter().filter(|n| n.step as usize == s) {
            if let Some(i) = self.skip_once.iter().position(|k| *k == (n.step, n.pad)) {
                self.skip_once.swap_remove(i);
                continue;
            }
            self.trigger(n.pad as usize, n.vel as f32 / 100.0, Src::Seq(n.len));
        }
        self.pattern = pat;
        self.skip_once.retain(|k| k.0 as usize != s);
        self.view.step.store(s as u32, Ordering::Relaxed);
    }

    /// Filter coefficients for the bipolar DJ filter value (None = bypass).
    fn filter_coeffs(&self, f: f32) -> Option<(bool, f32, f32, f32, f32)> {
        if f.abs() < 0.02 {
            return None;
        }
        let nyq = self.out_rate as f32 * 0.45;
        let (highpass, fc) = if f < 0.0 {
            (false, 20_000.0 * (120.0f32 / 20_000.0).powf(-f))
        } else {
            (true, 20.0 * (6_000.0f32 / 20.0).powf(f))
        };
        let g = (std::f32::consts::PI * fc.min(nyq) / self.out_rate as f32).tan();
        let k = 1.0 / 1.1; // gentle resonance
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        Some((highpass, k, a1, a2, a3))
    }

    fn process<T: cpal::SizedSample + cpal::FromSample<f32>>(&mut self, data: &mut [T], channels: usize) {
        while let Ok(cmd) = self.rx.try_recv() {
            self.handle(cmd);
        }
        let sr = self.out_rate as f32;
        let sps = self.sps();
        let attack = 1.0 / (ATTACK_SECONDS * sr);
        let release = 1.0 / (RELEASE_SECONDS * sr);
        let k_smooth = 1.0 - (-1.0 / (0.012 * sr)).exp();
        let click_decay = (-1.0 / (0.025 * sr)).exp();
        let targets: [f32; FX_COUNT] = std::array::from_fn(|i| self.fx.get(FxKind::ALL[i]));
        let range = self.fx.pitch_range();
        let coeffs = self.filter_coeffs(self.smooth[FxKind::Filter.index()]);
        let stutter_target = targets[FxKind::Stutter.index()];
        let hist_len = self.hist.len();
        let dly_len = self.delay.len();
        // Echo time: a dotted eighth when it follows the tempo, 300 ms otherwise.
        let echo = if self.fx_sync { sps * 3.0 } else { 0.3 * sr as f64 };
        let delay_len = (echo as usize).clamp(64, dly_len - 1);
        let bpm = self.bpm;
        for v in &mut self.voices {
            v.rate = v.base * if v.sync_bpm > 0.0 { (bpm / v.sync_bpm) as f64 } else { 1.0 };
        }
        let mut peak = 0.0f32;

        for frame in data.chunks_mut(channels.max(1)) {
            if self.playing {
                if self.phase >= self.step_len {
                    self.phase -= self.step_len;
                    self.step = (self.step + 1) % self.pattern.steps();
                    self.abs_step += 1;
                    self.step_len = self.len_of(self.step);
                    self.on_step(self.step);
                }
                self.phase += 1.0;
            }

            for i in 1..FX_COUNT {
                self.smooth[i] += (targets[i] - self.smooth[i]) * k_smooth;
            }
            let s = self.smooth;
            let bend = if s[FxKind::Pitch.index()].abs() > 0.001 {
                2f64.powf((s[FxKind::Pitch.index()] * range / 12.0) as f64)
            } else {
                1.0
            };

            // --- voices
            let (mut l, mut r) = (0.0f32, 0.0f32);
            for v in self.voices.iter_mut() {
                let len = v.sample.len;
                let mut i = v.pos as usize;
                if i >= len {
                    if v.looping {
                        v.pos %= len as f64;
                        i = v.pos as usize;
                    } else {
                        v.env = -1.0;
                        continue;
                    }
                }
                let frac = (v.pos - i as f64) as f32;
                let j = if i + 1 < len { i + 1 } else if v.looping { 0 } else { i };
                let f = &v.sample.frames;
                let sl = f[i * 2] + (f[j * 2] - f[i * 2]) * frac;
                let sr_ = f[i * 2 + 1] + (f[j * 2 + 1] - f[i * 2 + 1]) * frac;
                if v.releasing {
                    v.env -= release;
                    if v.env <= 0.0 {
                        v.env = -1.0;
                        continue;
                    }
                } else if v.env < 1.0 {
                    v.env = (v.env + attack).min(1.0);
                }
                l += sl * v.gl * v.env;
                r += sr_ * v.gr * v.env;
                v.pos += v.rate * bend;
            }

            // --- stutter (repeats the last moment)
            if stutter_target > 0.08 {
                let idx = (((stutter_target - 0.08) / 0.92) * 3.999) as usize;
                if self.stutter.is_none() || idx != self.stutter_idx {
                    let want = if self.fx_sync { sps * STUTTER_STEPS[idx] } else { (STUTTER_SECONDS[idx] * sr) as f64 };
                    let len = (want as usize).clamp(64, hist_len - 1);
                    let start = (self.hist_w + hist_len - len) % hist_len;
                    self.stutter = Some((start, len, 0));
                    self.stutter_idx = idx;
                }
                let (start, len, pos) = self.stutter.as_mut().map(|(a, b, c)| (*a, *b, c)).unwrap();
                let fade = 48.0f32;
                let g = ((*pos as f32) / fade).min(((len - *pos) as f32) / fade).min(1.0);
                let h = self.hist[(start + *pos) % hist_len];
                l = h[0] * g;
                r = h[1] * g;
                *pos = (*pos + 1) % len;
            } else {
                self.stutter = None;
                self.stutter_idx = usize::MAX;
                self.hist[self.hist_w] = [l, r];
                self.hist_w = (self.hist_w + 1) % hist_len;
            }

            // --- bitcrush
            let crush = s[FxKind::Crush.index()];
            if crush > 0.01 {
                let hold = 1 + (crush * crush * 24.0) as usize;
                if self.crush_count == 0 {
                    let levels = 2f32.powf(15.0 - crush * 12.0);
                    self.crush_hold = [(l * levels).round() / levels, (r * levels).round() / levels];
                }
                self.crush_count = (self.crush_count + 1) % hold;
                l = self.crush_hold[0];
                r = self.crush_hold[1];
            }

            // --- DJ filter
            if let Some((hp, k, a1, a2, a3)) = coeffs {
                let run = |x: f32, st: &mut Svf| {
                    let v3 = x - st.ic2;
                    let v1 = a1 * st.ic1 + a2 * v3;
                    let v2 = st.ic2 + a2 * st.ic1 + a3 * v3;
                    st.ic1 = 2.0 * v1 - st.ic1;
                    st.ic2 = 2.0 * v2 - st.ic2;
                    if hp { x - k * v1 - v2 } else { v2 }
                };
                l = run(l, &mut self.svf[0]);
                r = run(r, &mut self.svf[1]);
            }

            // --- echo (ping-pong; the tail keeps ringing after the stick returns)
            let send = s[FxKind::Echo.index()];
            let d = self.delay[(self.delay_w + dly_len - delay_len) % dly_len];
            self.delay[self.delay_w] = [l * send + d[1] * 0.5, r * send + d[0] * 0.5];
            self.delay_w = (self.delay_w + 1) % dly_len;
            l += d[0] * 0.75;
            r += d[1] * 0.75;

            // --- volume cut & pan
            let g = 1.0 - s[FxKind::Volume.index()].clamp(0.0, 1.0);
            let p = s[FxKind::Pan.index()];
            l *= g * (1.0 - p).min(1.0);
            r *= g * (1.0 + p).min(1.0);

            // --- metronome click (after the effects so it stays clean)
            if self.click_env > 0.001 {
                let c = self.click_phase.sin() * self.click_env * 0.28;
                self.click_phase += std::f32::consts::TAU * self.click_freq / sr;
                self.click_env *= click_decay;
                l += c;
                r += c;
            }

            let l = soft_clip(l * self.master);
            let r = soft_clip(r * self.master);
            peak = peak.max(l.abs()).max(r.abs());
            frame[0] = T::from_sample(l);
            if channels > 1 {
                frame[1] = T::from_sample(r);
            }
            for c in frame.iter_mut().skip(2) {
                *c = T::from_sample(0.0);
            }
        }
        self.voices.retain(|v| v.env >= 0.0);

        let mut mask = 0u32;
        let mut progress = [-1.0f32; PAD_COUNT];
        for v in &self.voices {
            if !v.releasing && v.pad < 32 {
                mask |= 1 << v.pad;
                if v.pad < PAD_COUNT {
                    progress[v.pad] = (v.pos / v.sample.len.max(1) as f64).clamp(0.0, 1.0) as f32;
                }
            }
        }
        for (i, p) in progress.iter().enumerate() {
            self.view.progress[i].store(p.to_bits(), Ordering::Relaxed);
        }
        self.view.active.store(mask, Ordering::Relaxed);
        self.view.queued.store(self.pending.iter().fold(0u32, |m, p| m | 1 << p), Ordering::Relaxed);
        self.view.playing.store(self.playing, Ordering::Relaxed);
        self.view.recording.store(self.recording, Ordering::Relaxed);
        let prev = f32::from_bits(self.view.peak.load(Ordering::Relaxed));
        if peak > prev {
            self.view.peak.store(peak.to_bits(), Ordering::Relaxed);
        }
    }
}

fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a < 0.8 {
        x
    } else {
        x.signum() * (0.8 + 0.2 * ((a - 0.8) / 0.2).tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_with_an_over_long_header_still_loads() {
        // A `fmt ` chunk of 20 bytes instead of 16, as some sample libraries ship.
        let mut b: Vec<u8> = Vec::new();
        let samples: [i16; 4] = [0, 16_384, -16_384, 32_767];
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(4 + 8 + 20 + 8 + 8u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&20u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&44_100u32.to_le_bytes());
        b.extend_from_slice(&88_200u32.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(&[0, 0, 0, 0]);
        b.extend_from_slice(b"data");
        b.extend_from_slice(&8u32.to_le_bytes());
        for v in samples {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let path = std::env::temp_dir().join("sensepad_long_fmt_test.wav");
        std::fs::write(&path, b).unwrap();
        let s = load_sample(&path).expect("plain PCM should load");
        let _ = std::fs::remove_file(&path);
        assert_eq!((s.len, s.sample_rate, s.channels), (4, 44_100, 1));
        assert!((s.frames[2] - 0.5).abs() < 1e-3 && (s.frames[3] - 0.5).abs() < 1e-3, "mono is copied to both sides");
        assert!((s.frames[4] + 0.5).abs() < 1e-3);
    }

    fn write_wav(path: &Path, rate: u32, channels: u16, frames: usize) {
        let data_len = (frames * channels as usize * 2) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        b.extend_from_slice(&(channels * 2).to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..frames {
            let v = ((i as f32 * 0.05).sin() * 16000.0) as i16;
            for _ in 0..channels {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        std::fs::write(path, b).unwrap();
    }

    struct Rig {
        m: Mixer,
        h: AudioHandle,
        seq: Receiver<SeqEvent>,
    }

    fn rig() -> Rig {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (seq_tx, seq) = crossbeam_channel::bounded(64);
        let h = AudioHandle { tx, view: Arc::new(EngineView::new()), fx: Arc::new(FxParams::new()) };
        h.view.online.store(true, Ordering::Relaxed);
        let mut m = Mixer::new(rx, seq_tx, &h, 48_000.0, 1.0);
        m.fx_sync = false;
        Rig { m, h, seq }
    }

    /// A constant 0.5 signal, `len` frames long at 48 kHz.
    fn dc(len: usize) -> Arc<SampleData> {
        Arc::new(SampleData {
            frames: vec![0.5; len * 2],
            len,
            sample_rate: 48_000,
            channels: 2,
            duration: len as f32 / 48_000.0,
            peaks: vec![],
        })
    }

    fn slot(mode: PlayMode, len: usize) -> PadSlot {
        PadSlot::new(&PadConfig { mode, volume: 1.0, ..Default::default() }, Some(dc(len)))
    }

    fn run(r: &mut Rig, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; frames * 2];
        r.m.process(&mut out, 2);
        out
    }

    #[test]
    fn decodes_mono_wav_to_stereo() {
        let path = std::env::temp_dir().join("sensepad_test_mono.wav");
        write_wav(&path, 22_050, 1, 22_050);
        let s = load_sample(&path).expect("decode");
        assert_eq!(s.sample_rate, 22_050);
        assert_eq!(s.len, 22_050);
        assert!((s.duration - 1.0).abs() < 0.01);
        assert_eq!(s.frames[200], s.frames[201]);
    }

    #[test]
    fn rejects_garbage() {
        let path = std::env::temp_dir().join("sensepad_test_garbage.wav");
        std::fs::write(&path, b"definitely not audio").unwrap();
        assert!(load_sample(&path).is_err());
    }

    #[test]
    fn loop_pad_toggles_and_reports_progress() {
        let mut r = rig();
        r.h.set_pad(3, slot(PlayMode::Loop, 1000));
        r.h.hit(3);
        let out = run(&mut r, 512);
        assert!(out.iter().any(|v| *v > 0.1));
        assert_eq!(r.h.active_mask(), 1 << 3);
        assert!(r.h.progress(3).is_some());
        assert_eq!(r.h.hit_count(3), 1);
        r.h.hit(3);
        for _ in 0..4 {
            run(&mut r, 512);
        }
        assert_eq!(r.h.active_mask(), 0);
        assert!(r.h.progress(3).is_none());
    }

    #[test]
    fn fx_chain_stays_finite_and_volume_cut_silences() {
        let mut r = rig();
        r.h.set_pad(0, slot(PlayMode::Loop, 1000));
        r.h.hit(0);
        for k in [FxKind::Filter, FxKind::Echo, FxKind::Pitch, FxKind::Crush, FxKind::Stutter, FxKind::Pan] {
            r.h.fx.set(k, if k.bipolar() { -0.7 } else { 0.7 });
        }
        for sync in [false, true] {
            r.h.set_fx_sync(sync);
            for _ in 0..20 {
                let out = run(&mut r, 1024);
                assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
            }
        }
        r.h.fx.reset();
        r.h.fx.set(FxKind::Volume, 1.0);
        let mut out = Vec::new();
        for _ in 0..80 {
            out = run(&mut r, 1024);
        }
        assert!(out.iter().all(|v| v.abs() < 1e-3), "full volume cut should silence output");
    }

    #[test]
    fn looper_fires_notes_on_the_grid() {
        let mut r = rig();
        r.h.set_pad(0, slot(PlayMode::OneShot, 100));
        r.h.set_tempo(120.0); // 6000 samples per step at 48 kHz
        r.h.set_pattern(&Pattern::from_rows(1, &[(0, "x...x...........")]));
        r.h.play();
        let out = run(&mut r, 30_000);
        let at = |frame: usize| out[frame * 2].abs();
        assert!(at(60) > 0.3, "step 0 sounds at once");
        assert!(at(12_000) < 1e-4, "silence between hits");
        assert!(at(24_060) > 0.3, "step 4 sounds 4 x 6000 samples later");
        assert_eq!(r.h.step(), Some(4));
        assert_eq!(r.h.hit_count(0), 2);
        r.h.stop();
        run(&mut r, 64);
        assert_eq!(r.h.step(), None);
        assert!(!r.h.playing());
    }

    #[test]
    fn recording_snaps_hits_to_the_nearest_step() {
        let mut r = rig();
        r.h.set_pad(2, slot(PlayMode::OneShot, 100));
        r.h.set_tempo(120.0);
        r.h.set_pattern(&Pattern { bars: 1, notes: vec![] });
        r.h.play();
        r.h.set_recording(true);
        run(&mut r, 6000 + 1000); // 1000 samples into step 1
        r.h.hit(2);
        run(&mut r, 4000); // now 5000 samples into step 1: closer to step 2
        r.h.hit(2);
        run(&mut r, 64);
        let got: Vec<SeqEvent> = r.seq.try_iter().collect();
        let steps: Vec<u8> = got.iter().filter_map(|e| if let SeqEvent::Recorded(n) = e { Some(n.step) } else { None }).collect();
        assert_eq!(steps, vec![1, 2]);
        // The early hit already sounded; it must not fire again when step 2 arrives.
        let before = r.h.hit_count(2);
        run(&mut r, 3000);
        assert_eq!(r.h.hit_count(2), before);
        // On the next pass both recorded notes play back.
        run(&mut r, 16 * 6000);
        assert_eq!(r.h.hit_count(2), before + 2);
    }

    #[test]
    fn held_notes_record_their_length_and_play_it_back() {
        let mut r = rig();
        r.h.set_pad(1, slot(PlayMode::Hold, 480_000));
        r.h.set_tempo(120.0);
        r.h.set_pattern(&Pattern { bars: 1, notes: vec![] });
        r.h.play();
        r.h.set_recording(true);
        run(&mut r, 100);
        r.h.hit(1);
        run(&mut r, 3 * 6000);
        r.h.release(1);
        run(&mut r, 64);
        let lens: Vec<u8> = r.seq.try_iter().filter_map(|e| if let SeqEvent::Len { len, .. } = e { Some(len) } else { None }).collect();
        assert_eq!(lens, vec![3]);
        // Next pass: the note starts on step 0 and is released three steps later.
        run(&mut r, 13 * 6000 - 164 + 600);
        assert_eq!(r.h.active_mask() & 2, 2, "playing again after the pattern wrapped");
        run(&mut r, 3 * 6000);
        assert_eq!(r.h.active_mask() & 2, 0, "released after three steps");
    }

    #[test]
    fn loops_wait_for_the_bar_when_quantised() {
        let mut r = rig();
        r.h.set_pad(5, slot(PlayMode::Loop, 2000));
        r.h.set_tempo(120.0);
        r.h.set_launch_quant(16);
        r.h.set_pattern(&Pattern { bars: 1, notes: vec![] });
        r.h.play();
        run(&mut r, 2 * 6000);
        r.h.hit(5);
        run(&mut r, 6000);
        assert_eq!(r.h.queued_mask(), 1 << 5);
        assert_eq!(r.h.active_mask(), 0);
        run(&mut r, 14 * 6000);
        assert_eq!(r.h.active_mask(), 1 << 5, "started on the bar line");
        assert_eq!(r.h.queued_mask(), 0);
    }

    #[test]
    fn synced_loops_follow_the_tempo() {
        let mut r = rig();
        let mut s = slot(PlayMode::Loop, 48_000);
        s.sync_bpm = 100.0;
        r.h.set_pad(0, s);
        r.h.set_tempo(200.0);
        r.h.hit(0);
        run(&mut r, 12_000);
        let p = r.h.progress(0).unwrap();
        assert!((p - 0.5).abs() < 0.02, "twice the tempo plays twice as fast, got {p}");
    }
}
