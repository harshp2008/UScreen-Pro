//! Stylus profiles: a plug-in layer between what a stylus reports and what the computer does.
//!
//! The tablet app forwards raw stylus facts as [`StylusEvent`]s (side button, S Pen air button and
//! air motion, pen roll). The profile that is active for the device turns each one into a
//! [`HostAction`] (a mouse click, a scroll notch or a key combination such as `Ctrl+Shift+Z`) and
//! the input layer executes it through uinput.
//!
//! Profiles are objects behind the [`StylusProfile`] trait. Two are built in
//! ([`StandardProfile`] for any Android stylus, [`SamsungSPenProfile`] for the S Pen Remote) and
//! custom ones are JSON files in `~/.config/uscreen/stylus_profiles/`, loaded by the
//! [`StylusProfileRegistry`] and reloaded whenever the directory changes:
//!
//! ```json
//! { "id": "copy-pen", "name": "Copy pen", "mappings": { "button_primary": "ctrl+c" } }
//! ```
//!
//! An action is written as text: `mouse_left`, `mouse_right`, `mouse_middle`, `scroll_up`,
//! `scroll_down`, `scroll_left`, `scroll_right`, or a key combination like `ctrl+shift+z`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;
use tokio::sync::watch;
use tracing::{info, warn};

// ---------------------------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------------------------

/// A mouse button a profile can click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// A direction of one wheel notch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollDir {
    Up,
    Down,
    Left,
    Right,
}

/// A modifier held while the main key of a combination is tapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Meta,
}

impl Modifier {
    /// Linux `KEY_LEFT*` code.
    pub fn code(self) -> u16 {
        match self {
            Modifier::Ctrl => 29,
            Modifier::Shift => 42,
            Modifier::Alt => 56,
            Modifier::Meta => 125,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Modifier::Ctrl => "ctrl",
            Modifier::Shift => "shift",
            Modifier::Alt => "alt",
            Modifier::Meta => "meta",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "ctrl" | "control" => Modifier::Ctrl,
            "shift" => Modifier::Shift,
            "alt" => Modifier::Alt,
            "meta" | "super" | "win" | "cmd" => Modifier::Meta,
            _ => return None,
        })
    }
}

/// Names and Linux key codes (`linux/input-event-codes.h`) of the keys a combination may end in.
const KEYS: &[(&str, u16)] = &[
    ("esc", 1),
    ("1", 2),
    ("2", 3),
    ("3", 4),
    ("4", 5),
    ("5", 6),
    ("6", 7),
    ("7", 8),
    ("8", 9),
    ("9", 10),
    ("0", 11),
    ("minus", 12),
    ("equal", 13),
    ("backspace", 14),
    ("tab", 15),
    ("q", 16),
    ("w", 17),
    ("e", 18),
    ("r", 19),
    ("t", 20),
    ("y", 21),
    ("u", 22),
    ("i", 23),
    ("o", 24),
    ("p", 25),
    ("enter", 28),
    ("a", 30),
    ("s", 31),
    ("d", 32),
    ("f", 33),
    ("g", 34),
    ("h", 35),
    ("j", 36),
    ("k", 37),
    ("l", 38),
    ("z", 44),
    ("x", 45),
    ("c", 46),
    ("v", 47),
    ("b", 48),
    ("n", 49),
    ("m", 50),
    ("space", 57),
    ("f1", 59),
    ("f2", 60),
    ("f3", 61),
    ("f4", 62),
    ("f5", 63),
    ("f6", 64),
    ("f7", 65),
    ("f8", 66),
    ("f9", 67),
    ("f10", 68),
    ("f11", 87),
    ("f12", 88),
    ("home", 102),
    ("up", 103),
    ("pageup", 104),
    ("left", 105),
    ("right", 106),
    ("end", 107),
    ("down", 108),
    ("pagedown", 109),
    ("insert", 110),
    ("delete", 111),
];

/// Linux key code of a key name from [`KEYS`].
pub fn key_code(name: &str) -> Option<u16> {
    KEYS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// What the computer does in response to a stylus event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostAction {
    /// Press and release a mouse button.
    MouseClick(MouseButton),
    /// One wheel notch.
    Scroll(ScrollDir),
    /// Tap `key` while `mods` are held.
    KeyPress {
        mods: Vec<Modifier>,
        key: &'static str,
    },
}

impl HostAction {
    /// Linux key codes to press in order for a [`HostAction::KeyPress`]: modifiers, then the key.
    pub fn key_codes(&self) -> Vec<u16> {
        match self {
            HostAction::KeyPress { mods, key } => {
                mods.iter().map(|m| m.code()).chain(key_code(key)).collect()
            }
            _ => Vec::new(),
        }
    }
}

impl fmt::Display for HostAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostAction::MouseClick(MouseButton::Left) => f.write_str("mouse_left"),
            HostAction::MouseClick(MouseButton::Right) => f.write_str("mouse_right"),
            HostAction::MouseClick(MouseButton::Middle) => f.write_str("mouse_middle"),
            HostAction::Scroll(ScrollDir::Up) => f.write_str("scroll_up"),
            HostAction::Scroll(ScrollDir::Down) => f.write_str("scroll_down"),
            HostAction::Scroll(ScrollDir::Left) => f.write_str("scroll_left"),
            HostAction::Scroll(ScrollDir::Right) => f.write_str("scroll_right"),
            HostAction::KeyPress { mods, key } => {
                for m in mods {
                    write!(f, "{}+", m.name())?;
                }
                f.write_str(key)
            }
        }
    }
}

impl FromStr for HostAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let t = s.trim().to_ascii_lowercase();
        Ok(match t.as_str() {
            "mouse_left" => HostAction::MouseClick(MouseButton::Left),
            "mouse_right" => HostAction::MouseClick(MouseButton::Right),
            "mouse_middle" => HostAction::MouseClick(MouseButton::Middle),
            "scroll_up" => HostAction::Scroll(ScrollDir::Up),
            "scroll_down" => HostAction::Scroll(ScrollDir::Down),
            "scroll_left" => HostAction::Scroll(ScrollDir::Left),
            "scroll_right" => HostAction::Scroll(ScrollDir::Right),
            _ => {
                let parts: Vec<&str> = t.split('+').map(str::trim).collect();
                let (key, mod_parts) = parts.split_last().ok_or("empty action")?;
                let key = KEYS
                    .iter()
                    .find(|(n, _)| n == key)
                    .map(|(n, _)| *n)
                    .ok_or_else(|| format!("unknown key {:?} in {:?}", key, s))?;
                let mut mods = Vec::new();
                for m in mod_parts {
                    let m = Modifier::parse(m)
                        .ok_or_else(|| format!("unknown modifier {:?} in {:?}", m, s))?;
                    if !mods.contains(&m) {
                        mods.push(m);
                    }
                }
                HostAction::KeyPress { mods, key }
            }
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------------------------

/// A physical stylus button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StylusButton {
    /// `BUTTON_STYLUS_PRIMARY`: the S Pen side button, or a pen's first barrel button.
    Primary,
    /// `BUTTON_STYLUS_SECONDARY`: a pen's second barrel button.
    Secondary,
    /// The S Pen Remote button, used with the pen away from the screen. A tap is reported once,
    /// when the button is released without an air gesture.
    Air,
}

/// A direction of an air swipe or a pen roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Which way the pen was rolled between the fingers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollDir {
    Clockwise,
    CounterClockwise,
}

/// Something the stylus did, independent of which hardware or SDK reported it.
#[derive(Clone, Debug, PartialEq)]
pub enum StylusEvent {
    ButtonPress(StylusButton),
    /// Raw relative motion of the S Pen in the air while its button is held.
    AirMotion {
        dx: f64,
        dy: f64,
    },
    /// A recognised air swipe (the S Pen button held while moving the pen).
    AirSwipe(Direction),
    /// The pen's `AXIS_ORIENTATION`, in radians.
    Orientation {
        radians: f64,
    },
    /// A recognised roll: the orientation turned by [`ROLL_STEP_RAD`].
    Roll(RollDir),
}

impl StylusEvent {
    /// The key a profile maps this event under, or `None` for events a profile cannot map
    /// (releases and raw motion).
    pub fn trigger(&self) -> Option<&'static str> {
        Some(match self {
            StylusEvent::ButtonPress(StylusButton::Primary) => "button_primary",
            StylusEvent::ButtonPress(StylusButton::Secondary) => "button_secondary",
            StylusEvent::ButtonPress(StylusButton::Air) => "air_button",
            StylusEvent::AirSwipe(Direction::Left) => "air_swipe_left",
            StylusEvent::AirSwipe(Direction::Right) => "air_swipe_right",
            StylusEvent::AirSwipe(Direction::Up) => "air_swipe_up",
            StylusEvent::AirSwipe(Direction::Down) => "air_swipe_down",
            StylusEvent::Roll(RollDir::Clockwise) => "roll_cw",
            StylusEvent::Roll(RollDir::CounterClockwise) => "roll_ccw",
            _ => return None,
        })
    }
}

/// Events a profile can map, in the order the UIs list them: key, label.
pub const TRIGGERS: &[(&str, &str)] = &[
    ("button_primary", "Press S Pen / stylus side button"),
    ("button_secondary", "Press second stylus button"),
    ("air_button", "Tap S Pen air button (Samsung)"),
    ("air_swipe_left", "Air swipe left (Samsung)"),
    ("air_swipe_right", "Air swipe right (Samsung)"),
    ("air_swipe_up", "Air swipe up (Samsung)"),
    ("air_swipe_down", "Air swipe down (Samsung)"),
    ("roll_cw", "Roll pen clockwise"),
    ("roll_ccw", "Roll pen counter-clockwise"),
];

/// Roll needed before a [`StylusEvent::Roll`] fires: 30 degrees of change in `AXIS_ORIENTATION`.
pub const ROLL_STEP_RAD: f64 = std::f64::consts::PI / 6.0;

/// Total S Pen air travel (in the SDK's own units) that turns a button press into a swipe
/// instead of a tap.
pub const SWIPE_MIN: f64 = 30.0;

// ---------------------------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------------------------

/// Source-independent mapping from stylus events to host actions.
pub trait StylusProfile: Send + Sync {
    /// Stable id: the `stylus_profile` setting and, for custom profiles, the file stem.
    fn id(&self) -> &str;
    /// Name shown in the pickers.
    fn name(&self) -> &str;
    /// True for profiles compiled in; false for JSON files.
    fn builtin(&self) -> bool;
    /// The action for `event`. `None` means "not mapped": the side buttons then pass through to
    /// the virtual pen as `BTN_STYLUS` (what applications such as GIMP expect), and every other
    /// event is ignored.
    fn map(&self, event: &StylusEvent) -> Option<HostAction>;
    /// Trigger key to action text for every mapping this profile has, for the editors.
    fn mappings(&self) -> BTreeMap<String, String>;
}

fn key_combo(mods: &[Modifier], key: &'static str) -> HostAction {
    HostAction::KeyPress {
        mods: mods.to_vec(),
        key,
    }
}

/// Any Android stylus: the side buttons reach applications as the stylus button and nothing else
/// is mapped. This is exactly how the pen behaved before profiles existed.
pub struct StandardProfile;

impl StylusProfile for StandardProfile {
    fn id(&self) -> &str {
        "standard"
    }
    fn name(&self) -> &str {
        "Standard (any stylus)"
    }
    fn builtin(&self) -> bool {
        true
    }
    fn map(&self, _event: &StylusEvent) -> Option<HostAction> {
        None
    }
    fn mappings(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
}

/// Samsung S Pen with the S Pen Remote: a presentation remote. The air button taps to the next
/// slide and swipes step through them; the side button still passes through as the stylus button.
pub struct SamsungSPenProfile;

impl StylusProfile for SamsungSPenProfile {
    fn id(&self) -> &str {
        "samsung_spen"
    }
    fn name(&self) -> &str {
        "Samsung S Pen Remote"
    }
    fn builtin(&self) -> bool {
        true
    }
    fn map(&self, event: &StylusEvent) -> Option<HostAction> {
        Some(match event.trigger()? {
            "air_button" | "air_swipe_right" | "air_swipe_down" => key_combo(&[], "right"),
            "air_swipe_left" | "air_swipe_up" => key_combo(&[], "left"),
            _ => return None,
        })
    }
    fn mappings(&self) -> BTreeMap<String, String> {
        [
            ("air_button", "right"),
            ("air_swipe_right", "right"),
            ("air_swipe_down", "right"),
            ("air_swipe_left", "left"),
            ("air_swipe_up", "left"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }
}

/// A profile loaded from `~/.config/uscreen/stylus_profiles/<id>.json`.
pub struct CustomProfile {
    id: String,
    name: String,
    actions: BTreeMap<String, HostAction>,
}

/// On-disk form of a [`CustomProfile`].
#[derive(Serialize, Deserialize)]
struct ProfileFile {
    #[serde(default)]
    id: String,
    name: String,
    #[serde(default)]
    mappings: BTreeMap<String, String>,
}

impl CustomProfile {
    /// Build a profile from its JSON form. Unknown triggers and unparsable actions are an error,
    /// so a typo is reported where it is made instead of silently doing nothing.
    pub fn from_json(fallback_id: &str, v: &Value) -> Result<Self, String> {
        let f: ProfileFile = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
        let id = clean_id(if f.id.is_empty() { fallback_id } else { &f.id })
            .ok_or("profile id must be 1-48 characters: letters, digits, '-' or '_'")?;
        if is_builtin_id(&id) {
            return Err(format!("{id:?} is a built-in profile; pick another id"));
        }
        let name = f.name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err("profile name must be 1-60 characters".into());
        }
        let mut actions = BTreeMap::new();
        for (trigger, text) in &f.mappings {
            if !TRIGGERS.iter().any(|(k, _)| k == trigger) {
                return Err(format!("unknown stylus event {trigger:?}"));
            }
            if text.trim().is_empty() || text.trim().eq_ignore_ascii_case("none") {
                continue;
            }
            actions.insert(trigger.clone(), text.parse::<HostAction>()?);
        }
        Ok(Self {
            id,
            name: name.to_string(),
            actions,
        })
    }

    /// The JSON form written to disk.
    pub fn to_json(&self) -> Value {
        json!({ "id": self.id, "name": self.name, "mappings": self.mappings() })
    }
}

impl StylusProfile for CustomProfile {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn builtin(&self) -> bool {
        false
    }
    fn map(&self, event: &StylusEvent) -> Option<HostAction> {
        self.actions.get(event.trigger()?).cloned()
    }
    fn mappings(&self) -> BTreeMap<String, String> {
        self.actions
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect()
    }
}

fn is_builtin_id(id: &str) -> bool {
    matches!(id, "standard" | "samsung_spen")
}

/// A valid profile id (1-48 of `a-z 0-9 - _`, lower-cased), or `None`. Ids become file names, so
/// nothing that could leave the profiles directory gets through.
pub fn clean_id(id: &str) -> Option<String> {
    let id = id.trim().to_ascii_lowercase();
    let ok = !id.is_empty()
        && id.len() <= 48
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    ok.then_some(id)
}

// ---------------------------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------------------------

/// Every profile the daemon knows: the built-in ones and the JSON files in a directory.
pub struct StylusProfileRegistry {
    dir: PathBuf,
    profiles: Vec<Arc<dyn StylusProfile>>,
}

impl StylusProfileRegistry {
    /// Scan `dir` for `*.json` profiles (a missing directory just means none). Files that do not
    /// parse are skipped with a warning.
    pub fn load(dir: &Path) -> Self {
        let mut profiles: Vec<Arc<dyn StylusProfile>> =
            vec![Arc::new(StandardProfile), Arc::new(SamsungSPenProfile)];
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        for path in files {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let loaded = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
                .and_then(|v| CustomProfile::from_json(stem, &v));
            match loaded {
                Ok(p) if profiles.iter().all(|q| q.id() != p.id()) => profiles.push(Arc::new(p)),
                Ok(p) => warn!(
                    "Stylus profile {:?} skipped: id {:?} is taken",
                    path,
                    p.id()
                ),
                Err(e) => warn!("Stylus profile {:?} skipped: {}", path, e),
            }
        }
        Self {
            dir: dir.to_path_buf(),
            profiles,
        }
    }

    /// The profile with this id.
    pub fn get(&self, id: &str) -> Option<Arc<dyn StylusProfile>> {
        self.profiles.iter().find(|p| p.id() == id).cloned()
    }

    /// The `stylus_profiles` message sent to tablets.
    pub fn message(&self) -> Value {
        let list: Vec<Value> = self
            .profiles
            .iter()
            .map(|p| {
                json!({
                    "id": p.id(),
                    "name": p.name(),
                    "builtin": p.builtin(),
                    "mappings": p.mappings(),
                })
            })
            .collect();
        let triggers: Vec<Value> = TRIGGERS
            .iter()
            .map(|(k, l)| json!({ "key": k, "label": l }))
            .collect();
        json!({ "type": "stylus_profiles", "profiles": list, "triggers": triggers })
    }

    /// Validate `profile` and write it as `<id>.json`.
    pub fn save(&self, profile: &Value) -> Result<String, String> {
        let p = CustomProfile::from_json("", profile)?;
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let path = self.dir.join(format!("{}.json", p.id));
        let text = serde_json::to_string_pretty(&p.to_json()).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        Ok(p.id)
    }

    /// Delete a custom profile's file.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let id = clean_id(id).ok_or("bad profile id")?;
        if is_builtin_id(&id) {
            return Err("built-in profiles cannot be deleted".into());
        }
        std::fs::remove_file(self.dir.join(format!("{id}.json"))).map_err(|e| e.to_string())
    }
}

/// `~/.config/uscreen/stylus_profiles/`, next to `config.toml`.
pub fn profiles_dir() -> PathBuf {
    crate::config::config_path()
        .parent()
        .map(|p| p.join("stylus_profiles"))
        .unwrap_or_else(|| PathBuf::from("stylus_profiles"))
}

struct Live {
    registry: RwLock<StylusProfileRegistry>,
    tx: watch::Sender<String>,
}

fn live() -> &'static Live {
    static LIVE: OnceLock<Live> = OnceLock::new();
    LIVE.get_or_init(|| {
        let reg = StylusProfileRegistry::load(&profiles_dir());
        let tx = watch::channel(reg.message().to_string()).0;
        Live {
            registry: RwLock::new(reg),
            tx,
        }
    })
}

/// The profile with this id, falling back to the standard one when it was deleted.
pub fn resolve(id: &str) -> Arc<dyn StylusProfile> {
    let reg = live().registry.read().unwrap_or_else(|e| e.into_inner());
    reg.get(id)
        .or_else(|| reg.get("standard"))
        .unwrap_or_else(|| Arc::new(StandardProfile))
}

/// Live `stylus_profiles` message: changes whenever a profile file is added, edited or removed.
pub fn subscribe() -> watch::Receiver<String> {
    live().tx.subscribe()
}

/// The profiles as they are on disk right now, as the `stylus_profiles` message (for the CLI).
pub fn list_message() -> Value {
    StylusProfileRegistry::load(&profiles_dir()).message()
}

/// Validate and save a custom profile from a tablet or the GUI, then reload at once.
pub fn save(profile: &Value) -> Result<String, String> {
    let id = live()
        .registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .save(profile)?;
    reload();
    Ok(id)
}

/// Delete a custom profile, then reload at once.
pub fn delete(id: &str) -> Result<(), String> {
    live()
        .registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .delete(id)?;
    reload();
    Ok(())
}

fn reload() {
    let l = live();
    let fresh = StylusProfileRegistry::load(&profiles_dir());
    let msg = fresh.message().to_string();
    *l.registry.write().unwrap_or_else(|e| e.into_inner()) = fresh;
    l.tx.send_if_modified(|cur| {
        if *cur == msg {
            false
        } else {
            *cur = msg;
            true
        }
    });
}

/// A cheap fingerprint of the profiles directory: names, sizes and modification times.
fn fingerprint(dir: &Path) -> Vec<(String, u64, Option<std::time::SystemTime>)> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((
                e.file_name().to_string_lossy().into_owned(),
                m.len(),
                m.modified().ok(),
            ))
        })
        .collect();
    v.sort();
    v
}

/// Reload the profiles when the directory changes (a hand-edited file, the GUI saving one).
pub fn spawn_watcher() {
    tokio::spawn(async {
        let dir = profiles_dir();
        let mut last = fingerprint(&dir);
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let now = fingerprint(&dir);
            if now != last {
                last = now;
                info!("Stylus profiles changed on disk — reloading");
                reload();
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Turning raw SDK facts into events
// ---------------------------------------------------------------------------------------------

/// Per-connection state that turns the tablet's raw messages (S Pen air button and motion, pen
/// orientation) into [`StylusEvent`]s: air swipes and roll steps.
#[derive(Default)]
pub struct StylusTracker {
    air_down: bool,
    air_travel: (f64, f64),
    roll_ref: Option<f64>,
}

impl StylusTracker {
    /// The S Pen Remote button changed. Nothing happens on press: releasing it ends the gesture,
    /// which is a swipe if the pen moved far enough and an air-button tap
    /// ([`StylusEvent::ButtonPress`] of [`StylusButton::Air`]) otherwise.
    pub fn air_button(&mut self, down: bool) -> Vec<StylusEvent> {
        if down {
            self.air_down = true;
            self.air_travel = (0.0, 0.0);
            return Vec::new();
        }
        if !self.air_down {
            return Vec::new();
        }
        self.air_down = false;
        let (x, y) = self.air_travel;
        if x.abs().max(y.abs()) < SWIPE_MIN {
            return vec![StylusEvent::ButtonPress(StylusButton::Air)];
        }
        vec![StylusEvent::AirSwipe(if x.abs() >= y.abs() {
            if x > 0.0 {
                Direction::Right
            } else {
                Direction::Left
            }
        } else if y > 0.0 {
            Direction::Down
        } else {
            Direction::Up
        })]
    }

    /// Air motion while the button is held.
    pub fn air_motion(&mut self, dx: f64, dy: f64) -> Vec<StylusEvent> {
        if self.air_down {
            self.air_travel.0 += dx;
            self.air_travel.1 += dy;
        }
        vec![StylusEvent::AirMotion { dx, dy }]
    }

    /// The pen's orientation changed. Every [`ROLL_STEP_RAD`] of turning is one roll event.
    pub fn orientation(&mut self, radians: f64) -> Vec<StylusEvent> {
        let mut out = vec![StylusEvent::Orientation { radians }];
        let r = *self.roll_ref.get_or_insert(radians);
        // Shortest signed turn, so crossing 0/2π is not a full revolution.
        let tau = std::f64::consts::TAU;
        let delta = (radians - r + std::f64::consts::PI).rem_euclid(tau) - std::f64::consts::PI;
        if delta.abs() >= ROLL_STEP_RAD {
            self.roll_ref = Some(radians);
            out.push(StylusEvent::Roll(if delta > 0.0 {
                RollDir::Clockwise
            } else {
                RollDir::CounterClockwise
            }));
        }
        out
    }

    /// Forget the roll reference (the pen left the screen), so the next hover starts fresh.
    pub fn pen_gone(&mut self) {
        self.roll_ref = None;
    }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_round_trip_through_text() {
        for t in [
            "mouse_left",
            "mouse_middle",
            "scroll_down",
            "ctrl+c",
            "ctrl+shift+z",
            "meta+pageup",
            "alt+f4",
            "enter",
        ] {
            let a: HostAction = t.parse().unwrap();
            assert_eq!(a.to_string(), t);
        }
        let a: HostAction = " Ctrl + Shift + Z ".parse().unwrap();
        assert_eq!(a.to_string(), "ctrl+shift+z");
        // Modifiers go down first, in the order written, then the key.
        assert_eq!(a.key_codes(), vec![29, 42, 44]);
        assert_eq!(
            "ctrl+c".parse::<HostAction>().unwrap().key_codes(),
            vec![29, 46]
        );
    }

    #[test]
    fn bad_actions_are_rejected() {
        assert!("ctrl+".parse::<HostAction>().is_err());
        assert!("hyper+c".parse::<HostAction>().is_err());
        assert!("ctrl+nonsense".parse::<HostAction>().is_err());
        assert!("".parse::<HostAction>().is_err());
    }

    #[test]
    fn ids_cannot_escape_the_directory() {
        assert_eq!(clean_id(" My-Pen_2 ").as_deref(), Some("my-pen_2"));
        assert!(clean_id("../etc/passwd").is_none());
        assert!(clean_id("a/b").is_none());
        assert!(clean_id("").is_none());
        assert!(clean_id(&"x".repeat(49)).is_none());
    }

    #[test]
    fn built_in_profiles() {
        let press = StylusEvent::ButtonPress(StylusButton::Primary);
        assert_eq!(StandardProfile.map(&press), None, "passes through");
        let s = SamsungSPenProfile;
        assert_eq!(
            s.map(&StylusEvent::AirSwipe(Direction::Left))
                .unwrap()
                .to_string(),
            "left"
        );
        assert_eq!(s.map(&press), None, "the side button still passes through");
    }

    #[test]
    fn custom_profile_maps_the_button() {
        let p = CustomProfile::from_json(
            "x",
            &json!({"id":"copy","name":"Copy","mappings":{"button_primary":"ctrl+c","roll_cw":"scroll_down","air_button":"none"}}),
        )
        .unwrap();
        let press = StylusEvent::ButtonPress(StylusButton::Primary);
        assert_eq!(p.map(&press).unwrap().to_string(), "ctrl+c");
        assert_eq!(p.map(&StylusEvent::ButtonPress(StylusButton::Air)), None);
        assert_eq!(p.mappings().len(), 2, "'none' is not stored");
        // A typo is an error, not a silent no-op.
        assert!(CustomProfile::from_json(
            "x",
            &json!({"name":"n","mappings":{"button_primary":"ctrl+"}})
        )
        .is_err());
        assert!(
            CustomProfile::from_json("x", &json!({"name":"n","mappings":{"bogus":"ctrl+c"}}))
                .is_err()
        );
        assert!(CustomProfile::from_json("x", &json!({"id":"standard","name":"n"})).is_err());
    }

    #[test]
    fn registry_loads_saves_and_deletes_json_files() {
        let dir = std::env::temp_dir().join(format!("uscreen-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reg = StylusProfileRegistry::load(&dir);
        assert!(reg.get("standard").is_some() && reg.get("samsung_spen").is_some());
        assert!(reg.get("copy").is_none());
        let id = reg
            .save(&json!({"name":"Copy pen","id":"copy","mappings":{"button_primary":"ctrl+c"}}))
            .unwrap();
        assert_eq!(id, "copy");
        std::fs::write(dir.join("broken.json"), "{ nope").unwrap();
        let reg = StylusProfileRegistry::load(&dir);
        let p = reg.get("copy").expect("loaded from disk");
        assert!(!p.builtin());
        assert_eq!(
            p.map(&StylusEvent::ButtonPress(StylusButton::Primary))
                .unwrap()
                .to_string(),
            "ctrl+c"
        );
        assert!(reg.get("broken").is_none(), "unparsable files are skipped");
        let msg = reg.message();
        assert_eq!(msg["type"], "stylus_profiles");
        assert!(msg["profiles"].as_array().unwrap().len() >= 3);
        reg.delete("copy").unwrap();
        assert!(reg.delete("standard").is_err());
        assert!(StylusProfileRegistry::load(&dir).get("copy").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn air_gestures_are_a_tap_or_a_swipe() {
        let mut t = StylusTracker::default();
        t.air_button(true);
        t.air_motion(2.0, -1.0);
        assert_eq!(
            t.air_button(false),
            vec![StylusEvent::ButtonPress(StylusButton::Air)],
            "little travel is a tap"
        );
        t.air_button(true);
        t.air_motion(-20.0, 3.0);
        t.air_motion(-20.0, 3.0);
        let up = t.air_button(false);
        assert!(up.contains(&StylusEvent::AirSwipe(Direction::Left)));
        t.air_button(true);
        t.air_motion(1.0, 40.0);
        assert!(t
            .air_button(false)
            .contains(&StylusEvent::AirSwipe(Direction::Down)));
        // Motion with the button up is not a gesture.
        assert!(t.air_button(false).is_empty());
    }

    #[test]
    fn rolling_the_pen_steps_every_thirty_degrees() {
        let mut t = StylusTracker::default();
        let has = |v: &[StylusEvent], r| v.contains(&StylusEvent::Roll(r));
        assert!(
            !has(&t.orientation(0.1), RollDir::Clockwise),
            "first sample is the reference"
        );
        assert!(!has(&t.orientation(0.3), RollDir::Clockwise));
        assert!(has(&t.orientation(0.7), RollDir::Clockwise));
        assert!(has(&t.orientation(0.1), RollDir::CounterClockwise));
        // Across the 0 / 2π seam is a small turn, not a revolution.
        let mut t = StylusTracker::default();
        t.orientation(6.2);
        assert!(!has(&t.orientation(0.1), RollDir::CounterClockwise));
    }
}
