//! The virtual multi-touch touchpad used for native gestures: its size/pressure constants, the
//! per-finger and per-pad state, and `inject_pad`, which replays the tablet's full finger set as one
//! evdev frame.

use super::*;

/// The virtual touchpad's surface, in the physical size libinput reasons about: 640 x 420 mm at
/// 40 units/mm. The tablet centres its touch area on the pad and scales finger positions by the
/// sensitivity setting (up to 2x), so even the largest tablet at the highest sensitivity stays
/// clear of libinput's edge-palm zones and the clickpad button area along the pad's border.
pub(in crate::input) const PAD_UNITS_PER_MM: i32 = 40;
pub(in crate::input) const PAD_WIDTH_MM: f64 = 640.0;
pub(in crate::input) const PAD_HEIGHT_MM: f64 = 420.0;
/// Contacts the pad can hold at once (slots 0..PAD_SLOTS).
pub(in crate::input) const PAD_SLOTS: usize = 10;
/// ABS_MT_PRESSURE runs 0..=PAD_PRESSURE_MAX. Android pressure is a 0..1 estimate that is often
/// tiny; libinput drops contacts below its "touching" threshold and treats very high ones as palms,
/// so the tablet's value is squeezed into a band that is always a plain finger.
pub(in crate::input) const PAD_PRESSURE_MAX: i32 = 255;
const PAD_PRESSURE_LOW: f64 = 50.0;
const PAD_PRESSURE_SPAN: f64 = 40.0;

/// One finger on the tablet's touchpad, as the host receives it: Android pointer id and a
/// position in millimetres from the touch area's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::input) struct PadFinger {
    pub(in crate::input) id: i32,
    pub(in crate::input) x_mm: f64,
    pub(in crate::input) y_mm: f64,
    /// Android's 0..1 pressure estimate.
    pub(in crate::input) pressure: f64,
}

/// What the virtual touchpad has been told so far: which Android pointer id sits in which MT slot,
/// the next tracking id, and which `BTN_TOOL_*` finger-count code is currently set.
#[derive(Debug)]
pub(in crate::input) struct PadState {
    pub(in crate::input) slots: [Option<i32>; PAD_SLOTS],
    pub(in crate::input) next_tracking: i32,
    pub(in crate::input) tool: Option<u16>,
}

impl PadState {
    /// A pad with no fingers on it.
    pub(in crate::input) fn new() -> Self {
        Self {
            slots: [None; PAD_SLOTS],
            next_tracking: 0,
            tool: None,
        }
    }
    /// How many fingers are on the pad.
    pub(in crate::input) fn count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
}

impl UInputDevice {
    /// Put the tablet's fingers on the pad: the full set that is down right now. Fingers that are
    /// no longer listed are lifted (tracking id -1), new ones get the lowest free slot and a fresh
    /// tracking id, the rest just move. The finger-count tool button follows the count and the
    /// frame ends with one SYN_REPORT, the way a real pad reports.
    ///
    /// Sending the whole set every time (instead of down / move / up events) means a lost message
    /// can never leave a finger stuck on the pad: the next frame simply does not list it.
    pub(in crate::input) fn inject_pad(
        &mut self,
        st: &mut PadState,
        fingers: &[PadFinger],
    ) -> Result<()> {
        if fingers.is_empty() && st.count() == 0 {
            return Ok(());
        }
        // Lift what is gone.
        for slot in 0..PAD_SLOTS {
            if let Some(id) = st.slots[slot] {
                if !fingers.iter().any(|f| f.id == id) {
                    self.emit(EV_ABS, ABS_MT_SLOT, slot as i32)?;
                    self.emit(EV_ABS, ABS_MT_TRACKING_ID, -1)?;
                    st.slots[slot] = None;
                }
            }
        }
        // Place and move what is down.
        let mut first: Option<(i32, i32)> = None;
        for f in fingers {
            let (slot, is_new) = match st.slots.iter().position(|s| *s == Some(f.id)) {
                Some(s) => (s, false),
                None => match st.slots.iter().position(|s| s.is_none()) {
                    Some(s) => {
                        st.slots[s] = Some(f.id);
                        (s, true)
                    }
                    None => continue, // more fingers than the pad has slots
                },
            };
            let x = ((f.x_mm * PAD_UNITS_PER_MM as f64) as i32)
                .clamp(0, (PAD_WIDTH_MM * PAD_UNITS_PER_MM as f64) as i32 - 1);
            let y = ((f.y_mm * PAD_UNITS_PER_MM as f64) as i32)
                .clamp(0, (PAD_HEIGHT_MM * PAD_UNITS_PER_MM as f64) as i32 - 1);
            let pressure =
                (PAD_PRESSURE_LOW + f.pressure.clamp(0.0, 1.0) * PAD_PRESSURE_SPAN) as i32;
            self.emit(EV_ABS, ABS_MT_SLOT, slot as i32)?;
            if is_new {
                self.emit(EV_ABS, ABS_MT_TRACKING_ID, st.next_tracking)?;
                st.next_tracking = (st.next_tracking + 1) & 0xffff;
            }
            self.emit(EV_ABS, ABS_MT_POSITION_X, x)?;
            self.emit(EV_ABS, ABS_MT_POSITION_Y, y)?;
            self.emit(EV_ABS, ABS_MT_PRESSURE, pressure)?;
            if first.is_none() {
                first = Some((x, y));
            }
        }
        // Single-touch mirror of the first finger, and the finger-count buttons.
        let count = st.count();
        if let Some((x, y)) = first {
            self.emit(EV_ABS, ABS_X, x)?;
            self.emit(EV_ABS, ABS_Y, y)?;
        }
        let tool = match count {
            0 => None,
            1 => Some(BTN_TOOL_FINGER),
            2 => Some(BTN_TOOL_DOUBLETAP),
            3 => Some(BTN_TOOL_TRIPLETAP),
            4 => Some(BTN_TOOL_QUADTAP),
            _ => Some(BTN_TOOL_QUINTTAP),
        };
        if tool != st.tool {
            if let Some(old) = st.tool {
                self.emit(EV_KEY, old, 0)?;
            }
            if let Some(new) = tool {
                self.emit(EV_KEY, new, 1)?;
            }
            if st.tool.is_none() || tool.is_none() {
                self.emit(EV_KEY, BTN_TOUCH, tool.is_some() as i32)?;
            }
            st.tool = tool;
        }
        self.syn()
    }
}
