//! Direct HID driver for the Sony DualSense / DualSense Edge.
//!
//! Runs on its own thread: finds the controller, parses input reports (USB and
//! Bluetooth), reports button edges through a callback with minimal latency, and
//! drives the lightbar, player LEDs, mute LED and rumble motors.

use crate::input::Input;
use crossbeam_channel::Receiver;
use hidapi::{HidApi, HidDevice};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SONY_VID: u16 = 0x054C;
const MODELS: [(u16, &str); 2] = [(0x0CE6, "DualSense"), (0x0DF2, "DualSense Edge")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Usb,
    Bluetooth,
}

#[derive(Clone, Debug)]
pub struct ConnInfo {
    pub model: &'static str,
    pub link: Link,
}

/// Live controller state, shared with the UI for visualisation.
#[derive(Clone, Debug, Default)]
pub struct PadSnapshot {
    pub conn: Option<ConnInfo>,
    pub buttons: u32,
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
    pub l2: f32,
    pub r2: f32,
    /// Normalised (0..1) touch positions.
    pub touches: [Option<(f32, f32)>; 2],
    pub battery: Option<u8>,
    pub charging: bool,
    pub lightbar: [u8; 3],
    pub error: Option<String>,
}

impl PadSnapshot {
    pub fn down(&self, input: Input) -> bool {
        self.buttons & input.bit() != 0
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HidEvent {
    Pressed(Input),
    Released(Input),
    /// Raw stick axes (lx, ly, rx, ry in -1..1, y down = +) and seconds since the last report.
    Sticks([f32; 4], f32),
    /// First finger on the touchpad (0..1, y down), or None when lifted.
    Touch(Option<(f32, f32)>),
    Connected,
    Disconnected,
}

#[derive(Clone, Copy, Debug)]
pub enum HidCmd {
    /// strong = low-frequency motor, weak = high-frequency motor.
    Rumble { strong: f32, weak: f32, ms: u64 },
    Flash([u8; 3]),
    BaseColor([u8; 3]),
    PlayerLeds(u8),
    MuteLed(bool),
    /// How far L2/R2 must be pressed (0..1) to count as a button press.
    TriggerPoint(f32),
}

struct Outputs {
    base: [u8; 3],
    flash: Option<([u8; 3], Instant)>,
    rumble_until: Option<Instant>,
    strong: u8,
    weak: u8,
    player_leds: u8,
    mute_led: bool,
    trigger_point: f32,
    released_default_light: bool,
    bt_seq: u8,
    last: Option<[u8; 47]>,
    last_write: Instant,
}

const FLASH_MS: f32 = 260.0;

impl Outputs {
    fn new() -> Self {
        Self {
            base: [94, 234, 212],
            flash: None,
            rumble_until: None,
            strong: 0,
            weak: 0,
            player_leds: 0x04,
            mute_led: false,
            trigger_point: 0.3,
            released_default_light: false,
            bt_seq: 0,
            last: None,
            last_write: Instant::now(),
        }
    }

    fn apply(&mut self, cmd: HidCmd) {
        match cmd {
            HidCmd::Rumble { strong, weak, ms } => {
                self.strong = (strong.clamp(0.0, 1.0) * 255.0) as u8;
                self.weak = (weak.clamp(0.0, 1.0) * 255.0) as u8;
                self.rumble_until = Some(Instant::now() + Duration::from_millis(ms));
            }
            HidCmd::Flash(c) => self.flash = Some((c, Instant::now())),
            HidCmd::BaseColor(c) => self.base = c,
            HidCmd::PlayerLeds(l) => self.player_leds = l,
            HidCmd::MuteLed(on) => self.mute_led = on,
            HidCmd::TriggerPoint(v) => self.trigger_point = if v.is_finite() { v.clamp(0.05, 0.9) } else { 0.3 },
        }
    }

    fn color(&self, now: Instant) -> [u8; 3] {
        match self.flash {
            Some((c, t0)) => {
                let t = (now.duration_since(t0).as_secs_f32() * 1000.0 / FLASH_MS).min(1.0);
                let ease = t * t;
                [0, 1, 2].map(|i| (c[i] as f32 + (self.base[i] as f32 - c[i] as f32) * ease) as u8)
            }
            None => self.base,
        }
    }

    /// The 47-byte "common" output block shared by the USB and Bluetooth reports.
    fn common(&mut self, now: Instant) -> [u8; 47] {
        if self.rumble_until.is_some_and(|t| now >= t) {
            self.rumble_until = None;
        }
        if self.flash.is_some_and(|(_, t0)| now.duration_since(t0).as_millis() as f32 > FLASH_MS) {
            self.flash = None;
        }
        let (strong, weak) = if self.rumble_until.is_some() { (self.strong, self.weak) } else { (0, 0) };
        let color = self.color(now);

        let mut c = [0u8; 47];
        c[0] = 0x01 | 0x02; // compatible vibration + haptics select
        c[1] = 0x01 | 0x04 | 0x10; // mic mute LED + lightbar + player LEDs
        c[2] = weak; // right motor (high frequency)
        c[3] = strong; // left motor (low frequency)
        c[8] = self.mute_led as u8;
        if !self.released_default_light {
            c[38] = 0x02; // lightbar setup control
            c[41] = 0x02; // fade out the default blue light
        }
        c[42] = 0x00; // player LED brightness: high
        c[43] = self.player_leds | 0x20; // instant change
        c[44] = color[0];
        c[45] = color[1];
        c[46] = color[2];
        c
    }

    fn flush(&mut self, dev: &HidDevice, link: Link, snapshot: &Mutex<PadSnapshot>) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_write) < Duration::from_millis(8) {
            return true;
        }
        let common = self.common(now);
        if self.last == Some(common) {
            return true;
        }
        let ok = match link {
            Link::Usb => {
                let mut r = [0u8; 48];
                r[0] = 0x02;
                r[1..].copy_from_slice(&common);
                dev.write(&r).is_ok()
            }
            Link::Bluetooth => {
                let mut r = [0u8; 78];
                r[0] = 0x31;
                r[1] = self.bt_seq << 4;
                r[2] = 0x10;
                r[3..50].copy_from_slice(&common);
                let mut h = crc32fast::Hasher::new();
                h.update(&[0xA2]);
                h.update(&r[..74]);
                r[74..].copy_from_slice(&h.finalize().to_le_bytes());
                self.bt_seq = (self.bt_seq + 1) & 0x0F;
                dev.write(&r).is_ok()
            }
        };
        if ok {
            self.released_default_light = true;
            self.last = Some(common);
            self.last_write = now;
            if let Ok(mut s) = snapshot.lock() {
                s.lightbar = [common[44], common[45], common[46]];
            }
        }
        ok
    }
}

struct Parsed {
    link: Link,
    buttons: u32,
    sticks: [u8; 4],
    l2: u8,
    r2: u8,
    touches: [Option<(f32, f32)>; 2],
    status: Option<u8>,
}

fn decode_buttons(b0: u8, b1: u8, b2: u8) -> u32 {
    let mut m = 0u32;
    let hat = b0 & 0x0F;
    if matches!(hat, 0 | 1 | 7) {
        m |= Input::Up.bit();
    }
    if matches!(hat, 1..=3) {
        m |= Input::Right.bit();
    }
    if matches!(hat, 3..=5) {
        m |= Input::Down.bit();
    }
    if matches!(hat, 5..=7) {
        m |= Input::Left.bit();
    }
    let map: [(u8, u8, Input); 13] = [
        (0, 0x10, Input::Square),
        (0, 0x20, Input::Cross),
        (0, 0x40, Input::Circle),
        (0, 0x80, Input::Triangle),
        (1, 0x01, Input::L1),
        (1, 0x02, Input::R1),
        (1, 0x10, Input::Create),
        (1, 0x20, Input::Options),
        (1, 0x40, Input::L3),
        (1, 0x80, Input::R3),
        (2, 0x01, Input::Ps),
        (2, 0x04, Input::Mute),
        // touchpad click is resolved into left/right halves later
        (2, 0x02, Input::TouchLeft),
    ];
    let bytes = [b0, b1, b2];
    for (idx, mask, input) in map {
        if bytes[idx as usize] & mask != 0 {
            m |= input.bit();
        }
    }
    m
}

fn parse(data: &[u8]) -> Option<Parsed> {
    let (link, s) = match (data.first()?, data.len()) {
        (0x01, n) if n >= 64 => (Link::Usb, &data[1..]),
        (0x31, n) if n >= 78 => (Link::Bluetooth, &data[2..]),
        // Bluetooth "simple" report before full mode is enabled.
        (0x01, n) if n >= 10 => {
            return Some(Parsed {
                link: Link::Bluetooth,
                buttons: decode_buttons(data[5], data[6], data[7]),
                sticks: [data[1], data[2], data[3], data[4]],
                l2: data[8],
                r2: data[9],
                touches: [None, None],
                status: None,
            })
        }
        _ => return None,
    };
    let touch = |p: &[u8]| -> Option<(f32, f32)> {
        if p[0] & 0x80 != 0 {
            return None;
        }
        let x = p[1] as u16 | ((p[2] & 0x0F) as u16) << 8;
        let y = (p[2] >> 4) as u16 | (p[3] as u16) << 4;
        Some(((x as f32 / 1919.0).clamp(0.0, 1.0), (y as f32 / 1079.0).clamp(0.0, 1.0)))
    };
    Some(Parsed {
        link,
        buttons: decode_buttons(s[7], s[8], s[9]),
        sticks: [s[0], s[1], s[2], s[3]],
        l2: s[4],
        r2: s[5],
        touches: [touch(&s[32..36]), touch(&s[36..40])],
        status: Some(s[52]),
    })
}

fn open_controller(api: &mut HidApi) -> Option<(HidDevice, &'static str)> {
    api.refresh_devices().ok()?;
    let mut candidates: Vec<_> = api
        .device_list()
        .filter(|d| d.vendor_id() == SONY_VID && MODELS.iter().any(|(pid, _)| *pid == d.product_id()))
        .collect();
    // Prefer the gamepad collection (usage page 1 / usage 5).
    candidates.sort_by_key(|d| !(d.usage_page() == 0x01 && d.usage() == 0x05));
    for info in candidates {
        let model = MODELS.iter().find(|(pid, _)| *pid == info.product_id()).map(|m| m.1).unwrap_or("DualSense");
        if let Ok(dev) = info.open_device(api) {
            return Some((dev, model));
        }
    }
    None
}

fn axis(v: u8) -> f32 {
    ((v as f32 - 127.5) / 127.5).clamp(-1.0, 1.0)
}

/// Spawn the controller thread. `on_event` runs on that thread, so keep it fast.
pub fn spawn(
    snapshot: Arc<Mutex<PadSnapshot>>,
    cmds: Receiver<HidCmd>,
    mut on_event: impl FnMut(HidEvent) + Send + 'static,
) {
    std::thread::Builder::new()
        .name("dualsense".into())
        .spawn(move || {
            let mut api = loop {
                match HidApi::new() {
                    Ok(api) => break api,
                    Err(e) => {
                        snapshot.lock().unwrap().error = Some(format!("HID init failed: {e}"));
                        std::thread::sleep(Duration::from_secs(2));
                    }
                }
            };
            let mut out = Outputs::new();
            loop {
                while let Ok(cmd) = cmds.try_recv() {
                    out.apply(cmd);
                }
                let Some((dev, model)) = open_controller(&mut api) else {
                    std::thread::sleep(Duration::from_millis(800));
                    continue;
                };
                // Reading the calibration feature report switches Bluetooth into full report mode.
                let mut feat = [0u8; 64];
                feat[0] = 0x05;
                let _ = dev.get_feature_report(&mut feat);

                run_session(&dev, model, &snapshot, &cmds, &mut out, &mut on_event);

                {
                    let mut s = snapshot.lock().unwrap();
                    *s = PadSnapshot { lightbar: s.lightbar, ..Default::default() };
                }
                on_event(HidEvent::Disconnected);
                out.last = None;
                out.released_default_light = false;
                std::thread::sleep(Duration::from_millis(300));
            }
        })
        .expect("spawn dualsense thread");
}

fn run_session(
    dev: &HidDevice,
    model: &'static str,
    snapshot: &Mutex<PadSnapshot>,
    cmds: &Receiver<HidCmd>,
    out: &mut Outputs,
    on_event: &mut impl FnMut(HidEvent),
) {
    let mut buf = [0u8; 128];
    let mut prev: u32 = 0;
    let mut link: Option<Link> = None;
    let mut last_report = Instant::now();
    let mut simple_since: Option<Instant> = None;
    let (mut l2_on, mut r2_on) = (false, false);
    let mut last_touch: Option<(f32, f32)> = None;

    loop {
        let n = match dev.read_timeout(&mut buf, 4) {
            Ok(n) => n,
            Err(_) => return,
        };
        if n == 0 {
            if last_report.elapsed() > Duration::from_secs(3) {
                return; // controller went quiet (e.g. Bluetooth dropped)
            }
        } else if let Some(p) = parse(&buf[..n]) {
            let dt = last_report.elapsed().as_secs_f32().min(0.05);
            last_report = Instant::now();
            if link.is_none() {
                link = Some(p.link);
                let mut s = snapshot.lock().unwrap();
                s.conn = Some(ConnInfo { model, link: p.link });
                s.error = None;
                drop(s);
                on_event(HidEvent::Connected);
            }
            // Still in Bluetooth simple mode? Ask again for full reports now and then.
            if p.status.is_none() {
                let t = simple_since.get_or_insert_with(Instant::now);
                if t.elapsed() > Duration::from_secs(1) {
                    let mut feat = [0u8; 64];
                    feat[0] = 0x05;
                    let _ = dev.get_feature_report(&mut feat);
                    simple_since = Some(Instant::now());
                }
            }

            let (l2, r2) = (p.l2 as f32 / 255.0, p.r2 as f32 / 255.0);
            // Analog triggers with hysteresis so a resting finger does not fire a pad.
            let (on_at, off_at) = (out.trigger_point, out.trigger_point * 0.4);
            l2_on = if l2_on { l2 > off_at } else { l2 > on_at };
            r2_on = if r2_on { r2 > off_at } else { r2 > on_at };
            let mut buttons = p.buttons & !(Input::TouchLeft.bit());
            if l2_on {
                buttons |= Input::L2.bit();
            }
            if r2_on {
                buttons |= Input::R2.bit();
            }
            // Touchpad click: decide the half when the click starts, keep it until release.
            if p.buttons & Input::TouchLeft.bit() != 0 {
                let held = prev & (Input::TouchLeft.bit() | Input::TouchRight.bit());
                buttons |= if held != 0 {
                    held
                } else if p.touches[0].is_some_and(|(x, _)| x >= 0.5) {
                    Input::TouchRight.bit()
                } else {
                    Input::TouchLeft.bit()
                };
            }

            let sticks = [axis(p.sticks[0]), axis(p.sticks[1]), axis(p.sticks[2]), axis(p.sticks[3])];
            on_event(HidEvent::Sticks(sticks, dt));
            if p.touches[0] != last_touch {
                last_touch = p.touches[0];
                on_event(HidEvent::Touch(last_touch));
            }
            {
                let mut s = snapshot.lock().unwrap();
                s.buttons = buttons;
                s.lx = sticks[0];
                s.ly = sticks[1];
                s.rx = sticks[2];
                s.ry = sticks[3];
                s.l2 = l2;
                s.r2 = r2;
                s.touches = p.touches;
                if let Some(st) = p.status {
                    let level = st & 0x0F;
                    let charge = (st & 0xF0) >> 4;
                    s.battery = Some((level as u32 * 10 + 5).min(100) as u8);
                    s.charging = charge == 0x1;
                    if charge == 0x2 {
                        s.battery = Some(100);
                    }
                }
            }

            let changed = buttons ^ prev;
            if changed != 0 {
                for input in Input::ALL {
                    if changed & input.bit() != 0 {
                        on_event(if buttons & input.bit() != 0 {
                            HidEvent::Pressed(input)
                        } else {
                            HidEvent::Released(input)
                        });
                    }
                }
                prev = buttons;
            }
        }

        while let Ok(cmd) = cmds.try_recv() {
            out.apply(cmd);
        }
        if let Some(l) = link {
            if !out.flush(dev, l, snapshot) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usb_report() {
        let mut r = [0u8; 64];
        r[0] = 0x01;
        r[1..5].copy_from_slice(&[128, 128, 255, 0]);
        r[5] = 200; // L2
        r[8] = 0x20 | 0x08; // Cross + hat=none
        r[9] = 0x01; // L1
        r[10] = 0x02; // touchpad click
        r[1 + 32] = 0x01; // touch 0 active, x=1500
        r[1 + 33] = (1500 & 0xFF) as u8;
        r[1 + 34] = (1500 >> 8) as u8;
        r[1 + 52] = 0x17; // 75% battery, charging
        let p = parse(&r).unwrap();
        assert_eq!(p.link, Link::Usb);
        assert!(p.buttons & Input::Cross.bit() != 0);
        assert!(p.buttons & Input::L1.bit() != 0);
        assert!(p.buttons & Input::TouchLeft.bit() != 0);
        assert!(p.buttons & Input::Up.bit() == 0);
        assert_eq!(p.l2, 200);
        assert!(p.touches[0].unwrap().0 > 0.75);
        assert_eq!(p.status, Some(0x17));
    }

    #[test]
    fn parses_bluetooth_report_and_dpad() {
        let mut r = [0u8; 78];
        r[0] = 0x31;
        r[2 + 7] = 0x01; // hat NE
        assert_eq!(parse(&r).unwrap().link, Link::Bluetooth);
        let b = parse(&r).unwrap().buttons;
        assert!(b & Input::Up.bit() != 0 && b & Input::Right.bit() != 0);
    }

    #[test]
    fn bluetooth_output_crc_layout() {
        let mut out = Outputs::new();
        let c = out.common(Instant::now());
        assert_eq!(c.len(), 47);
        assert_eq!([c[44], c[45], c[46]], [94, 234, 212]);
    }
}
