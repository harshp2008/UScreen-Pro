//! Physical drawing tablets (XP-Pen, Wacom, Huion, ...): read the real device through evdev and feed
//! its pen into the same ghost-tracking path as the Android pen (see `ghost`).
//!
//! The kernel's tablet protocol is turned into the pen protocol the injector already speaks:
//! `BTN_TOOL_PEN`/`BTN_TOOL_RUBBER` is range (hover) in and out, `BTN_TOUCH` the tip, `BTN_STYLUS`
//! and `BTN_STYLUS2` the side buttons, `ABS_X/Y/PRESSURE/TILT_X/TILT_Y` the sample, all closed by
//! `SYN_REPORT`. The conversion is a plain state machine ([`PhysState`]) so it is tested without
//! hardware.
//!
//! To keep the desktop from also seeing the tablet (two cursors) the device is grabbed exclusively
//! (`EVIOCGRAB`) while support is on; the kernel undoes the grab by itself when we exit. The
//! persistent alternative is the udev `LIBINPUT_IGNORE_DEVICE` rule, see [`tablet_rules`].

use super::*;
use evdev::{AbsoluteAxisCode, Device, EventType};
use std::os::fd::AsRawFd;
use std::time::Duration;

/// Used when the tablet's `EVIOCGABS` cannot be read: 15-bit full scale (0..=32767) for X and Y.
const ABS_RANGE_FALLBACK: i32 = 32767;
/// XP-Pen / Huion tablets report tilt as raw units over ±127 for a ±60° pen.
const TILT_FULL_SCALE_DEG: f64 = 60.0;

/// Axis ranges of the physical tablet, read once from its `EVIOCGABS`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AbsRange {
    pub x_max: i32,
    pub y_max: i32,
    pub p_max: i32,
    /// Degrees per raw tilt unit.
    pub tilt_deg_per_unit: f64,
}

impl Default for AbsRange {
    /// Defaults for a tablet whose ranges could not be read: 32767 on both axes, pressure 8191, tilt 60° over ±127 raw units.
    fn default() -> Self {
        Self {
            x_max: ABS_RANGE_FALLBACK,
            y_max: ABS_RANGE_FALLBACK,
            p_max: 8191,
            tilt_deg_per_unit: TILT_FULL_SCALE_DEG / 127.0,
        }
    }
}

/// The kernel's pen events folded into [`PenSample`]s, one batch per `SYN_REPORT`.
#[derive(Debug, Default)]
pub(super) struct PhysState {
    pub range: AbsRange,
    x: i32,
    y: i32,
    p: i32,
    tx: i32,
    ty: i32,
    prox: bool,
    pen_tool: bool,
    rubber: bool,
    touch: bool,
    s1: bool,
    s2: bool,
    // The state at the previous report.
    was_prox: bool,
    was_touch: bool,
    was_s1: bool,
    was_s2: bool,
    moved: bool,
}

impl PhysState {
    /// A state machine for a tablet with this axis range.
    pub fn new(range: AbsRange) -> Self {
        Self {
            range,
            ..Self::default()
        }
    }

    /// One raw event. Returns the samples it completes (only `SYN_REPORT` does).
    pub fn feed(&mut self, kind: u16, code: u16, value: i32) -> Vec<PenSample> {
        match (kind, code) {
            (EV_ABS, ABS_X) => {
                self.x = value;
                self.moved = true;
            }
            (EV_ABS, ABS_Y) => {
                self.y = value;
                self.moved = true;
            }
            (EV_ABS, ABS_PRESSURE) => {
                self.p = value;
                self.moved = true;
            }
            (EV_ABS, ABS_TILT_X) => {
                self.tx = value;
                self.moved = true;
            }
            (EV_ABS, ABS_TILT_Y) => {
                self.ty = value;
                self.moved = true;
            }
            (EV_KEY, BTN_TOOL_PEN) => {
                self.pen_tool = value != 0;
                self.prox = self.pen_tool || self.rubber;
            }
            (EV_KEY, BTN_TOOL_RUBBER) => {
                self.rubber = value != 0;
                self.prox = self.pen_tool || self.rubber;
            }
            (EV_KEY, BTN_TOUCH) => self.touch = value != 0,
            (EV_KEY, BTN_STYLUS) => self.s1 = value != 0,
            (EV_KEY, BTN_STYLUS2) => self.s2 = value != 0,
            (EV_SYN, SYN_REPORT) => return self.report(),
            _ => {}
        }
        Vec::new()
    }

    /// Build the pen sample for `action` from the current axis values, scaled to the injector's
    /// units.
    fn sample(&self, action: u8) -> PenSample {
        let r = &self.range;
        let scale = |v: i32, max: i32, to: i32| {
            ((v.clamp(0, max.max(1)) as i64 * to as i64) / max.max(1) as i64) as i32
        };
        let tilt = |v: i32| ((v as f64 * r.tilt_deg_per_unit).round() as i32).clamp(-90, 90);
        PenSample {
            x: scale(self.x, r.x_max, COORD_MAX),
            y: scale(self.y, r.y_max, COORD_MAX),
            pressure: scale(self.p, r.p_max, PEN_PRESSURE_MAX),
            tilt_x: tilt(self.tx),
            tilt_y: tilt(self.ty),
            eraser: self.rubber,
            action,
        }
    }

    /// Turn the current state into the samples one `SYN_REPORT` produces.
    fn report(&mut self) -> Vec<PenSample> {
        let mut out = Vec::new();
        if self.prox {
            // A tip that touches in the very report that brings the pen into range still gets its
            // hover first, so the injector sees the pen appear before it touches.
            if !self.was_prox && self.touch {
                out.push(self.sample(3));
            }
            match (self.was_touch && self.was_prox, self.touch) {
                (false, true) => out.push(self.sample(0)),
                (true, false) => out.push(self.sample(1)),
                (true, true) if self.moved => out.push(self.sample(2)),
                (false, false) if self.moved || !self.was_prox => out.push(self.sample(3)),
                _ => {}
            }
            if self.s1 != self.was_s1 {
                out.push(self.sample(if self.s1 { 5 } else { 6 }));
            }
            if self.s2 != self.was_s2 {
                out.push(self.sample(if self.s2 { 7 } else { 8 }));
            }
        } else if self.was_prox {
            // Out of range: lift and release anything held first, then say so.
            if self.was_touch {
                out.push(self.sample(1));
            }
            if self.was_s1 {
                out.push(self.sample(6));
            }
            if self.was_s2 {
                out.push(self.sample(8));
            }
            out.push(self.sample(4));
            self.touch = false;
            self.s1 = false;
            self.s2 = false;
        }
        self.was_prox = self.prox;
        self.was_touch = self.touch && self.prox;
        self.was_s1 = self.s1;
        self.was_s2 = self.s2;
        self.moved = false;
        out
    }
}

/// Linux key code of the pen's second barrel button (`BTN_STYLUS2`).
const BTN_STYLUS2: u16 = 0x14c;

/// Whether a device with these capabilities is a pen tablet's pen interface. A tablet exposes several
/// event nodes (a pad-as-mouse, express keys, the pen): only the one with absolute X, Y and pressure
/// and a pen tool button is the pen. Our own virtual devices never count.
pub(super) fn is_pen_tablet(name: &str, vendor: u16, abs: &[u16], keys: &[u16]) -> bool {
    if name.starts_with("UScreen") || vendor == UINPUT_VENDOR {
        return false;
    }
    abs.contains(&ABS_X)
        && abs.contains(&ABS_Y)
        && abs.contains(&ABS_PRESSURE)
        && keys.contains(&BTN_TOOL_PEN)
        && keys.contains(&BTN_TOUCH)
}

/// `28bd:0931` as `(0x28bd, 0x0931)`; `None` for anything else (including the empty "first found").
pub(super) fn parse_vid_pid(s: &str) -> Option<(u16, u16)> {
    let (v, p) = s.trim().split_once(':')?;
    Some((
        u16::from_str_radix(v, 16).ok()?,
        u16::from_str_radix(p, 16).ok()?,
    ))
}

/// The first evdev device that looks like a pen tablet (matching `want` as vendor:product when
/// given).
fn find_pen(want: &str) -> Option<(std::path::PathBuf, Device)> {
    let want = parse_vid_pid(want);
    evdev::enumerate().find_map(|(path, dev)| {
        let id = dev.input_id();
        let abs: Vec<u16> = dev
            .supported_absolute_axes()
            .map(|s| s.iter().map(|a| a.0).collect())
            .unwrap_or_default();
        let keys: Vec<u16> = dev
            .supported_keys()
            .map(|s| s.iter().map(|k| k.0).collect())
            .unwrap_or_default();
        let ok = is_pen_tablet(dev.name().unwrap_or(""), id.vendor(), &abs, &keys)
            && want.is_none_or(|(v, p)| id.vendor() == v && id.product() == p);
        ok.then_some((path, dev))
    })
}

/// Vendor and product id of the first pen tablet plugged in, for `uscreen tablet-rules`.
pub fn detect_tablet() -> Option<(u16, u16)> {
    find_pen("").map(|(_, d)| (d.input_id().vendor(), d.input_id().product()))
}

/// Read the tablet's axis ranges.
fn read_range(dev: &Device) -> AbsRange {
    let mut r = AbsRange::default();
    if let Ok(info) = dev.get_absinfo() {
        for (axis, i) in info {
            match axis {
                AbsoluteAxisCode::ABS_X if i.maximum() > 0 => r.x_max = i.maximum(),
                AbsoluteAxisCode::ABS_Y if i.maximum() > 0 => r.y_max = i.maximum(),
                AbsoluteAxisCode::ABS_PRESSURE if i.maximum() > 0 => r.p_max = i.maximum(),
                AbsoluteAxisCode::ABS_TILT_X => {
                    // A tilt axis with a resolution is in units per degree; without one the HID
                    // range is taken to span the pen's ±60°.
                    r.tilt_deg_per_unit = if i.resolution() > 0 {
                        1.0 / i.resolution() as f64
                    } else {
                        TILT_FULL_SCALE_DEG / i.maximum().max(i.minimum().abs()).max(1) as f64
                    };
                }
                _ => {}
            }
        }
    }
    r
}

// PLATFORM(linux): reads a physical tablet through evdev (/dev/input/event*), which needs access granted by the udev rule
/// Run the physical tablet reader for the daemon's lifetime. It follows `physical_tablet_support`
/// in `config.toml` live (checked every second), finds the pen, grabs it, and feeds the shared
/// injector until the tablet is unplugged or support is switched off.
pub(super) fn spawn(
    uinput: Arc<std::sync::Mutex<InjectDevices>>,
    instance: u32,
    running: Arc<AtomicBool>,
) {
    let r = std::thread::Builder::new().name("uscreen-tablet".into()).spawn(move || {
        let mut told_x11 = false;
        while running.load(Ordering::SeqCst) {
            let cfg = crate::config::FileConfig::load();
            if !cfg.physical_tablet_support {
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            if session_is_x11() {
                if !told_x11 {
                    info!("Physical tablet support is not needed on X11 (the desktop already drives one cursor from the tablet)");
                    told_x11 = true;
                }
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
            let Some((path, mut dev)) = find_pen(&cfg.physical_tablet_device) else {
                std::thread::sleep(Duration::from_secs(2));
                continue;
            };
            let name = dev.name().unwrap_or("?").to_string();
            let id = dev.input_id();
            info!(
                "Physical tablet {:?} at {} ({:04x}:{:04x})",
                name,
                path.display(),
                id.vendor(),
                id.product()
            );
            if cfg.physical_tablet_grab {
                match dev.grab() {
                    Ok(()) => info!("Tablet grabbed: the desktop no longer sees it directly"),
                    Err(e) => warn!("Could not grab the tablet ({}): the desktop may show a second cursor", e),
                }
            }
            read_loop(&mut dev, &uinput, instance, &running);
            let _ = dev.ungrab();
            info!("Physical tablet {:?} released", name);
            // Nothing the tablet held stays held.
            if let Ok(mut g) = uinput.lock() {
                g.release_all();
            }
        }
    });
    if let Err(e) = r {
        warn!("Could not start the physical tablet reader: {}", e);
    }
}

/// Read the tablet's events until `running` is cleared, feeding its samples to the injector.
fn read_loop(
    dev: &mut Device,
    uinput: &Arc<std::sync::Mutex<InjectDevices>>,
    instance: u32,
    running: &Arc<AtomicBool>,
) {
    if dev.set_nonblocking(true).is_err() {
        return;
    }
    let mut state = PhysState::new(read_range(dev));
    let mut last_cfg_check = std::time::Instant::now();
    loop {
        if !running.load(Ordering::SeqCst) {
            return;
        }
        if last_cfg_check.elapsed() >= Duration::from_secs(1) {
            last_cfg_check = std::time::Instant::now();
            if !crate::config::FileConfig::load().physical_tablet_support {
                return;
            }
        }
        let mut pfd = libc::pollfd {
            fd: dev.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd for the duration of the call.
        let n = unsafe { libc::poll(&mut pfd, 1, 250) };
        if n <= 0 {
            continue;
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return; // unplugged
        }
        let events = match dev.fetch_events() {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(_) => return,
        };
        let mut samples = Vec::new();
        for ev in events {
            let kind = match ev.event_type() {
                EventType::SYNCHRONIZATION => EV_SYN,
                EventType::KEY => EV_KEY,
                EventType::ABSOLUTE => EV_ABS,
                _ => continue,
            };
            samples.extend(state.feed(kind, ev.code(), ev.value()));
        }
        if samples.is_empty() {
            continue;
        }
        let opts = crate::config::options::current_options(instance);
        if let Ok(mut g) = uinput.lock() {
            for s in &samples {
                tracing::debug!(
                    "Physical tablet moved to {},{} (action {})",
                    s.x,
                    s.y,
                    s.action
                );
                apply_physical(&mut g, &opts, s);
            }
        }
    }
}

// PLATFORM(linux): udev rules text for 60-uscreen-tablet-access.rules; the rule file name must sort before 73-seat-late.rules
/// The udev rules that make a tablet usable without root and, optionally, invisible to the desktop:
///
/// * `access`: gives the logged-in seat user the pen's event node (`uaccess`), no `input` group needed;
/// * `hide`: sets `LIBINPUT_IGNORE_DEVICE=1` on every event node of the tablet, the same mechanism
///   OpenTabletDriver ships, so Mutter/KWin/X never see the real device and only the virtual ones remain.
///
/// `vid`/`pid` are the lowercase 4-digit hex ids.
pub fn tablet_rules(vid: u16, pid: u16) -> (String, String) {
    let access = format!(
        "# UScreen: let the logged-in user read the pen of tablet {vid:04x}:{pid:04x} without root.\n\
         # Install as /etc/udev/rules.d/60-uscreen-tablet-access.rules (must sort before 73-seat-late.rules).\n\
         SUBSYSTEM==\"input\", KERNEL==\"event[0-9]*\", ENV{{ID_INPUT_TABLET}}==\"1\", ATTRS{{idVendor}}==\"{vid:04x}\", ATTRS{{idProduct}}==\"{pid:04x}\", TAG+=\"uaccess\"\n"
    );
    let hide = format!(
        "# UScreen: hide tablet {vid:04x}:{pid:04x} from libinput (Mutter, KWin, X) so only UScreen's virtual devices remain.\n\
         # Install as /etc/udev/rules.d/99-uscreen-hide-tablet.rules. While this is installed the tablet does nothing\n\
         # unless the uscreen daemon runs with physical tablet support on: remove the file to get it back.\n\
         ACTION!=\"remove\", SUBSYSTEM==\"input\", KERNEL==\"event[0-9]*\", ENV{{ID_VENDOR_ID}}==\"{vid:04x}\", ENV{{ID_MODEL_ID}}==\"{pid:04x}\", ENV{{LIBINPUT_IGNORE_DEVICE}}=\"1\"\n"
    );
    (access, hide)
}

/// Tests of the physical-pen state machine: report assembly on `SYN_REPORT`, scaling and device selection.
#[cfg(test)]
mod tests;
