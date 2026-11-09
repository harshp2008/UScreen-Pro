//! Pen stillness: tells a pen that is resting near the glass from one that is being moved.
//!
//! A pen left lying on the tablet is still "in range", and the tablet keeps reporting hover samples
//! with a little noise. Each one used to be written to the pointer, so a parked pen kept taking the
//! cursor back from the mouse or touchpad. [`HoverFilter`] parks such a pen: once its hover samples
//! stay inside a small radius for a while, nothing more is written until it moves clearly (a larger
//! radius, so noise cannot wake it) or touches down.
//!
//! Distances are in millimetres on the tablet, from the pen's normalised position and the
//! tablet's physical size, so the thresholds mean the same on every tablet.

use std::time::{Duration, Instant};

/// Thresholds of the hover filter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct StillCfg {
    /// Hover samples within this radius of the anchor count as "not moving".
    pub park_radius_mm: f64,
    /// How long the pen must stay inside that radius before it is parked.
    pub park_after: Duration,
    /// A parked pen wakes when it gets farther than this from where it was parked.
    pub wake_radius_mm: f64,
    /// Click slop: after the tip lands on the mouse route, the pointer stays at the press point
    /// until the pen gets farther than this from it (a hand wobbles when the tip lands, and an app
    /// reads a few pixels of movement with the button down as the start of a drag).
    pub click_slop_mm: f64,
}

impl Default for StillCfg {
    fn default() -> Self {
        Self {
            park_radius_mm: 1.5,
            park_after: Duration::from_millis(300),
            wake_radius_mm: 5.0,
            click_slop_mm: 2.0,
        }
    }
}

/// What to do with a hover sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HoverVerdict {
    /// Write it as usual.
    Follow,
    /// The pen has just been parked: stop owning the cursor (take the tablet tool out of range).
    Park,
    /// The pen is parked: write nothing.
    Skip,
    /// The parked pen moved clearly: it is back; write the sample.
    Wake,
}

#[derive(Debug, Default)]
pub(super) struct HoverFilter {
    anchor: Option<(i32, i32)>,
    since: Option<Instant>,
    parked: bool,
}

/// Position range of the normalised pen coordinates.
const SPAN: f64 = 65536.0;

pub(super) fn dist_mm(a: (i32, i32), b: (i32, i32), mm: (f64, f64)) -> f64 {
    let dx = (a.0 - b.0) as f64 * mm.0 / SPAN;
    let dy = (a.1 - b.1) as f64 * mm.1 / SPAN;
    dx.hypot(dy)
}

impl HoverFilter {
    #[cfg(test)]
    pub fn is_parked(&self) -> bool {
        self.parked
    }

    /// Forget everything: the pen touched, lifted or left range, so the next hover starts fresh.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Judge one hover sample at `now`, with the tablet's physical size `mm` (width, height).
    pub fn sample(&mut self, x: i32, y: i32, now: Instant, mm: (f64, f64), cfg: &StillCfg) -> HoverVerdict {
        let p = (x, y);
        if self.parked {
            let anchor = self.anchor.unwrap_or(p);
            if dist_mm(anchor, p, mm) > cfg.wake_radius_mm {
                self.parked = false;
                self.anchor = Some(p);
                self.since = Some(now);
                return HoverVerdict::Wake;
            }
            return HoverVerdict::Skip;
        }
        match (self.anchor, self.since) {
            (Some(a), Some(t)) if dist_mm(a, p, mm) <= cfg.park_radius_mm => {
                if now.duration_since(t) >= cfg.park_after {
                    self.parked = true;
                    return HoverVerdict::Park;
                }
                HoverVerdict::Follow
            }
            _ => {
                self.anchor = Some(p);
                self.since = Some(now);
                HoverVerdict::Follow
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MM: (f64, f64) = (266.0, 166.0);

    /// Units for `mm` millimetres along x.
    fn dx(mm: f64) -> i32 {
        (mm * SPAN / MM.0) as i32
    }

    fn ms(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_millis(n)
    }

    #[test]
    fn a_still_pen_is_followed_then_parked_then_skipped() {
        let (mut f, cfg, t0) = (HoverFilter::default(), StillCfg::default(), Instant::now());
        assert_eq!(f.sample(1000, 1000, t0, MM, &cfg), HoverVerdict::Follow);
        // jitter well inside 1.5 mm
        assert_eq!(f.sample(1000 + dx(0.4), 1000, ms(t0, 100), MM, &cfg), HoverVerdict::Follow);
        assert_eq!(f.sample(1000, 1000 + 20, ms(t0, 250), MM, &cfg), HoverVerdict::Follow);
        assert_eq!(f.sample(1000 + dx(0.5), 1000, ms(t0, 320), MM, &cfg), HoverVerdict::Park);
        assert!(f.is_parked());
        assert_eq!(f.sample(1000 + dx(0.6), 1000, ms(t0, 340), MM, &cfg), HoverVerdict::Skip);
        assert_eq!(f.sample(1000 + dx(2.0), 1000, ms(t0, 900), MM, &cfg), HoverVerdict::Skip, "noise cannot wake it");
    }

    #[test]
    fn a_parked_pen_wakes_on_clear_movement_and_is_followed_again() {
        let (mut f, cfg, t0) = (HoverFilter::default(), StillCfg::default(), Instant::now());
        f.sample(1000, 1000, t0, MM, &cfg);
        assert_eq!(f.sample(1000, 1000, ms(t0, 400), MM, &cfg), HoverVerdict::Park);
        assert_eq!(f.sample(1000 + dx(6.0), 1000, ms(t0, 500), MM, &cfg), HoverVerdict::Wake);
        assert!(!f.is_parked());
        assert_eq!(f.sample(1000 + dx(7.0), 1000, ms(t0, 520), MM, &cfg), HoverVerdict::Follow);
    }

    #[test]
    fn a_pen_that_keeps_moving_is_never_parked() {
        let (mut f, cfg, t0) = (HoverFilter::default(), StillCfg::default(), Instant::now());
        for i in 0..50u64 {
            assert_eq!(f.sample(1000 + dx(2.0) * i as i32, 1000, ms(t0, i * 30), MM, &cfg), HoverVerdict::Follow);
        }
    }

    #[test]
    fn a_slow_drift_re_anchors_instead_of_parking() {
        // Moving 1 mm per 100 ms: each sample leaves the 1.5 mm circle of the previous anchor
        // within two samples, so the pen never sits still for 300 ms.
        let (mut f, cfg, t0) = (HoverFilter::default(), StillCfg::default(), Instant::now());
        for i in 0..20u64 {
            let v = f.sample(1000 + dx(1.0) * i as i32, 1000, ms(t0, i * 100), MM, &cfg);
            assert_ne!(v, HoverVerdict::Park);
            assert_ne!(v, HoverVerdict::Skip);
        }
    }

    #[test]
    fn reset_starts_over() {
        let (mut f, cfg, t0) = (HoverFilter::default(), StillCfg::default(), Instant::now());
        f.sample(1000, 1000, t0, MM, &cfg);
        f.sample(1000, 1000, ms(t0, 400), MM, &cfg);
        assert!(f.is_parked());
        f.reset();
        assert!(!f.is_parked());
        assert_eq!(f.sample(1000, 1000, ms(t0, 500), MM, &cfg), HoverVerdict::Follow);
    }

    #[test]
    fn distances_use_the_physical_size() {
        // The same normalised step is a bigger distance on a bigger tablet.
        let a = dist_mm((0, 0), (6553, 0), (266.0, 166.0));
        let b = dist_mm((0, 0), (6553, 0), (133.0, 83.0));
        assert!((a - 2.0 * b).abs() < 0.01);
    }
}
