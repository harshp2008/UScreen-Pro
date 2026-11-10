//! The pure detectors: the hotplug rate limiter and the compositor stall detector

use super::*;

/// Rate-limits virtual-monitor topology changes. Each add/remove makes the compositor re-layout, and doing it in a tight loop is what historically froze Mutter.
#[derive(Default)]
pub struct HotplugLimiter {
    changes: VecDeque<Instant>,
}

impl HotplugLimiter {
    /// How long to wait before the next topology change is allowed.
    pub fn delay(&mut self, now: Instant) -> Duration {
        while self
            .changes
            .front()
            .is_some_and(|t| now.duration_since(*t) > HOTPLUG_WINDOW)
        {
            self.changes.pop_front();
        }
        let gap = self
            .changes
            .back()
            .map(|last| HOTPLUG_MIN_GAP.saturating_sub(now.duration_since(*last)))
            .unwrap_or_default();
        if self.changes.len() >= HOTPLUG_MAX_PER_WINDOW {
            let last = *self.changes.back().unwrap();
            let cool = HOTPLUG_COOLDOWN.saturating_sub(now.duration_since(last));
            return gap.max(cool);
        }
        gap
    }

    /// Note that a topology change happened at `now`.
    pub fn record(&mut self, now: Instant) {
        self.changes.push_back(now);
    }
}

/// One probe result: `None` = no answer within the timeout.
pub type Sample = Option<Duration>;

/// Counts consecutive compositor probes that failed or were slow; trips the breaker after [`STALLS_TO_TRIP`] in a row.
#[derive(Default)]
pub struct StallDetector {
    consecutive: u32,
}

impl StallDetector {
    /// Feed a sample; returns true exactly when the breaker should trip.
    pub fn feed(&mut self, s: Sample) -> bool {
        let stalled = s.is_none_or(|d| d >= STALL_PROBE);
        if stalled {
            self.consecutive += 1;
        } else {
            self.consecutive = 0;
        }
        stalled && self.consecutive == STALLS_TO_TRIP
    }
    /// Current run of consecutive stalled probes (for logging).
    pub fn consecutive(&self) -> u32 {
        self.consecutive
    }
}
