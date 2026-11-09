//! The live per-tablet registry: slots, watchers that republish options when config.toml changes, and the accessors the rest of the host uses

use super::*;

pub(super) struct Slot {
    /// Profile keys of the device in this slot (hardware serial, adb serial), best first.
    pub(super) keys: Vec<String>,
    pub(super) tx: watch::Sender<DeviceOptions>,
    /// Derived from `tx`: the device is in Stylus mode (what the input mapping calls "pen-only").
    pub(super) pen_only: watch::Sender<bool>,
    /// Whether the desktop pointer is on this tablet's screen. `true` until the helper says
    /// otherwise, so a missing signal can never dim a tablet.
    pub(super) pointer: watch::Sender<bool>,
    /// Pinged by every touch / pen / touchpad event from the tablet.
    pub(super) activity: Arc<Notify>,
    pub(super) idle_task: bool,
}

pub(super) fn slots() -> &'static Mutex<HashMap<u32, Slot>> {
    static SLOTS: OnceLock<Mutex<HashMap<u32, Slot>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn with_slot<R>(instance: u32, f: impl FnOnce(&mut Slot) -> R) -> R {
    let mut map = slots().lock().unwrap_or_else(|e| e.into_inner());
    let slot = map.entry(instance).or_insert_with(|| {
        let first = DeviceOptions::resolve(&FileConfig::load(), None);
        Slot {
            keys: Vec::new(),
            pen_only: watch::channel(first.input_mode == InputMode::Stylus).0,
            tx: watch::channel(first).0,
            pointer: watch::channel(true).0,
            activity: Arc::new(Notify::new()),
            idle_task: false,
        }
    });
    f(slot)
}

/// Live options of a slot.
pub fn options_rx(instance: u32) -> watch::Receiver<DeviceOptions> {
    with_slot(instance, |s| s.tx.subscribe())
}

/// Whether the slot's device is in Stylus mode, live.
pub fn pen_only_rx(instance: u32) -> watch::Receiver<bool> {
    with_slot(instance, |s| s.pen_only.subscribe())
}

/// The slot's current options.
pub fn current_options(instance: u32) -> DeviceOptions {
    with_slot(instance, |s| s.tx.borrow().clone())
}

/// The slot's current input mode.
pub fn current_mode(instance: u32) -> InputMode {
    with_slot(instance, |s| s.tx.borrow().input_mode)
}

pub(super) fn profile_of<'a>(cfg: &'a FileConfig, keys: &[String]) -> Option<&'a TabletProfile> {
    keys.iter().find_map(|k| cfg.tablets.get(k))
}

pub(super) fn publish(slot: &mut Slot, cfg: &FileConfig) {
    let next = DeviceOptions::resolve(cfg, profile_of(cfg, &slot.keys));
    let stylus = next.input_mode == InputMode::Stylus;
    slot.tx.send_if_modified(|cur| {
        if *cur == next {
            false
        } else {
            *cur = next;
            true
        }
    });
    slot.pen_only.send_if_modified(|cur| {
        if *cur == stylus {
            false
        } else {
            *cur = stylus;
            true
        }
    });
}

/// Tell the registry which device sits in `instance`, so its profile is the one merged over the
/// global values from now on.
pub fn bind(instance: u32, keys: &[&str]) {
    let cfg = FileConfig::load();
    with_slot(instance, |s| {
        s.keys = keys
            .iter()
            .filter(|k| !k.is_empty())
            .map(|k| k.to_string())
            .collect();
        publish(s, &cfg);
    });
}

/// Recompute every slot from `config.toml`.
pub fn refresh_all() {
    let cfg = FileConfig::load();
    let mut map = slots().lock().unwrap_or_else(|e| e.into_inner());
    for slot in map.values_mut() {
        publish(slot, &cfg);
    }
}

/// Watch `config.toml` and push changes into every slot. Cheap: one `stat` every 500 ms.
pub fn spawn_config_watcher() {
    tokio::spawn(async {
        let mtime = || {
            std::fs::metadata(crate::config::config_path())
                .and_then(|m| m.modified())
                .ok()
        };
        let mut last = mtime();
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let now = mtime();
            if now != last {
                last = now;
                refresh_all();
            }
        }
    });
}

/// A setting changed on the tablet itself: store it as that device's override. Returns whether
/// anything changed. Unknown keys, bad values and unpaired devices are ignored.
pub fn set_from_device(instance: u32, key: &str, value: &Value) -> bool {
    let keys = with_slot(instance, |s| s.keys.clone());
    let mut cfg = FileConfig::load();
    let Some(profile_key) = keys
        .iter()
        .find(|k| cfg.tablets.contains_key(*k))
        .cloned()
        .or_else(|| {
            cfg.auto_pair_new_devices
                .then(|| keys.first().cloned())
                .flatten()
        })
    else {
        warn!("Tablet setting {} ignored: this device has no profile", key);
        return false;
    };
    let p = cfg.tablets.entry(profile_key).or_default();
    match set_override(p, key, value) {
        Ok(true) => {}
        Ok(false) => return false,
        Err(()) => {
            warn!("Tablet setting {}={} rejected", key, value);
            return false;
        }
    }
    if cfg.save().is_err() {
        return false;
    }
    info!("Tablet changed {} to {}", key, value);
    refresh_all();
    true
}

/// The helper reported whether the desktop pointer is on this tablet's screen.
pub fn set_pointer(instance: u32, on_tablet: bool) {
    with_slot(instance, |s| {
        s.pointer.send_if_modified(|cur| {
            if *cur == on_tablet {
                false
            } else {
                *cur = on_tablet;
                true
            }
        });
    });
}

/// A touch, pen or touchpad event arrived from this slot's tablet.
pub fn note_activity(instance: u32) {
    let n = with_slot(instance, |s| s.activity.clone());
    n.notify_one();
}
