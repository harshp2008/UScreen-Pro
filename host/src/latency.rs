//! End-to-end latency measurement.
//!
//! Every access unit leaving the encoder gets a sequence number and a host
//! timestamp. The tablet echoes the sequence number back over the existing
//! input WebSocket once the frame has actually been rendered, so the round trip
//! is measured entirely on the host's clock — no clock synchronisation between
//! the two machines, which is what makes naive "timestamp in the stream"
//! approaches useless.
//!
//! What this measures is `encoder output → visible on the tablet`: transport
//! queueing, decode and render. In addition, high-resolution stage timestamps
//! from EVDI helper and FFmpeg are recorded to report complete stage-by-stage
//! latency and true end-to-end (update_ready → photon).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::info;

/// How many in-flight frames to remember. At 60 fps this is ~4s of history,
/// far more than any sane round trip, and bounded so a tablet that stops
/// reporting cannot grow this without limit.
const MAX_TRACKED: usize = 256;

/// Samples kept for the percentile report. One report covers ~5s.
const MAX_SAMPLES: usize = 1024;

/// One acknowledged frame, kept for ~60 s so `uscreen doctor --latency` can ask
/// for the percentiles of "the last N seconds" over the control socket.
struct Recent {
    at: Instant,
    /// Encoder output -> on the tablet's screen, microseconds.
    total_us: u32,
    /// Capture-ready -> on screen (host stages + wire/2 + decode), when the
    /// helper gave detailed timing.
    e2e_us: Option<u32>,
    host_us: Option<u32>,
}

fn recent() -> &'static Mutex<VecDeque<Recent>> {
    static R: std::sync::OnceLock<Mutex<VecDeque<Recent>>> = std::sync::OnceLock::new();
    R.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Percentiles over the last `secs` seconds of acknowledged frames, as one JSON line.
pub fn recent_json(secs: u64) -> String {
    let now = Instant::now();
    let (mut total, mut e2e, mut host) = (Vec::new(), Vec::new(), Vec::new());
    if let Ok(g) = recent().lock() {
        for r in g
            .iter()
            .filter(|r| now.duration_since(r.at).as_secs() < secs)
        {
            total.push(r.total_us);
            e2e.extend(r.e2e_us);
            host.extend(r.host_us);
        }
    }
    let pct = |v: &mut Vec<u32>, p: f64| -> f64 {
        if v.is_empty() {
            return 0.0;
        }
        v.sort_unstable();
        v[((v.len() as f64 - 1.0) * p).round() as usize] as f64 / 1000.0
    };
    serde_json::json!({
        "secs": secs,
        "samples": total.len(),
        "encode_to_display_p50_ms": pct(&mut total, 0.50),
        "encode_to_display_p95_ms": pct(&mut total, 0.95),
        "true_e2e_samples": e2e.len(),
        "true_e2e_p50_ms": pct(&mut e2e, 0.50),
        "true_e2e_p95_ms": pct(&mut e2e, 0.95),
        "host_stages_p50_ms": pct(&mut host, 0.50),
        "host_stages_p95_ms": pct(&mut host, 0.95),
    })
    .to_string()
}

/// Per-stage host timings of one frame, in microseconds, as recorded by the helper's shared timing ring.
#[derive(Clone, Copy, Debug, Default)]
pub struct DetailedTiming {
    pub is_fresh: bool,
    pub req_to_ready_us: u32,
    pub grab_us: u32,
    pub conv_us: u32,
    pub fifo_wait_write_us: u32,
    pub encode_us: u32,
    pub host_total_us: u32,
}

#[derive(Default)]
struct Inner {
    /// (seq, time the access unit left the encoder, detailed timing), oldest first.
    sent: VecDeque<(u32, Instant, Option<DetailedTiming>)>,
    /// Round-trip latencies in microseconds, for the current report window.
    samples: Vec<u32>,
    /// Of that round trip, the part the tablet spent decoding and rendering.
    decode_samples: Vec<u32>,
    /// Wire transit samples.
    wire_samples: Vec<u32>,
    // Sub-stage breakdown samples (microseconds):
    req_to_ready_samples: Vec<u32>,
    grab_samples: Vec<u32>,
    conv_samples: Vec<u32>,
    fifo_samples: Vec<u32>,
    encode_samples: Vec<u32>,
    host_total_samples: Vec<u32>,
    e2e_samples: Vec<u32>,
    /// Frames that were sent but whose acknowledgement never arrived before
    /// they aged out — a direct sign of frames being dropped downstream.
    lost: u64,
    last_report: Option<Instant>,
}

/// Collects end-to-end latency samples (send time vs the tablet's render acknowledgement) and periodically logs percentiles. Cheap to clone; clones share state.
#[derive(Clone, Default)]
pub struct LatencyTracker {
    inner: Arc<Mutex<Inner>>,
}

impl LatencyTracker {
    /// Create an empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// An access unit just left the encoder without detailed timing.
    #[cfg(feature = "inproc-encoder")]
    pub fn on_encoded(&self, seq: u32) {
        self.on_encoded_with_timing(seq, None);
    }

    /// An access unit just left the encoder with detailed stage timing.
    pub fn on_encoded_with_timing(&self, seq: u32, timing: Option<DetailedTiming>) {
        let Ok(mut g) = self.inner.lock() else { return };
        if g.last_report.is_none() {
            g.last_report = Some(Instant::now());
        }
        g.sent.push_back((seq, Instant::now(), timing));
        while g.sent.len() > MAX_TRACKED {
            g.sent.pop_front();
            g.lost += 1;
        }
    }

    /// The tablet reports that this frame is on screen.
    pub fn on_rendered(&self, seq: u32, decode_us: i64) {
        let Ok(mut g) = self.inner.lock() else { return };
        // Everything queued before this frame is now known to be behind it;
        // drop it so the deque tracks only genuinely in-flight frames.
        let Some(pos) = g.sent.iter().position(|(s, _, _)| *s == seq) else {
            return;
        };
        let (_, at, timing) = g.sent[pos];
        let micros = at.elapsed().as_micros().min(u32::MAX as u128) as u32;
        g.sent.drain(..=pos);
        let mut rec = Recent {
            at: Instant::now(),
            total_us: micros,
            e2e_us: None,
            host_us: None,
        };
        if g.samples.len() < MAX_SAMPLES {
            g.samples.push(micros);
            let dec = if decode_us > 0 {
                let d = (decode_us as u128).min(u32::MAX as u128) as u32;
                g.decode_samples.push(d);
                d
            } else {
                0
            };
            let wire = micros.saturating_sub(dec);
            g.wire_samples.push(wire);

            if let Some(t) = timing {
                if t.is_fresh {
                    g.req_to_ready_samples.push(t.req_to_ready_us);
                    g.grab_samples.push(t.grab_us);
                    g.conv_samples.push(t.conv_us);
                    g.fifo_samples.push(t.fifo_wait_write_us);
                    g.host_total_samples.push(t.host_total_us);
                    // True one-way end-to-end latency:
                    // Host total (ready -> encoded packet) + wire/2 (transit) + tablet decode & render
                    let e2e = t.host_total_us + (wire / 2) + dec;
                    g.e2e_samples.push(e2e);
                    rec.e2e_us = Some(e2e);
                    rec.host_us = Some(t.host_total_us);
                }
                g.encode_samples.push(t.encode_us);
            }
        }
        if let Ok(mut r) = recent().lock() {
            // Cheap bounded history: drop what is older than a minute.
            while r
                .front()
                .is_some_and(|f| rec.at.duration_since(f.at).as_secs() > 60)
            {
                r.pop_front();
            }
            if r.len() < 30_000 {
                r.push_back(rec);
            }
        }
    }

    /// Log percentiles if the report window has elapsed. Cheap to call often.
    pub fn maybe_report(&self) {
        let Ok(mut g) = self.inner.lock() else { return };
        let Some(last) = g.last_report else { return };
        if last.elapsed().as_secs() < 5 {
            return;
        }
        g.last_report = Some(Instant::now());

        if g.samples.is_empty() {
            // Silence here means the tablet never acknowledged anything, which
            // is itself worth saying out loud.
            if g.lost > 0 {
                info!(
                    "Latency: no frames acknowledged by the tablet ({} aged out)",
                    g.lost
                );
                g.lost = 0;
            }
            return;
        }

        let mut s = std::mem::take(&mut g.samples);
        let mut d = std::mem::take(&mut g.decode_samples);
        let mut w = std::mem::take(&mut g.wire_samples);
        let mut req_ready = std::mem::take(&mut g.req_to_ready_samples);
        let mut grab = std::mem::take(&mut g.grab_samples);
        let mut conv = std::mem::take(&mut g.conv_samples);
        let mut fifo = std::mem::take(&mut g.fifo_samples);
        let mut encode = std::mem::take(&mut g.encode_samples);
        let mut host_total = std::mem::take(&mut g.host_total_samples);
        let mut e2e = std::mem::take(&mut g.e2e_samples);

        let lost = std::mem::take(&mut g.lost);
        let inflight = g.sent.len();
        drop(g);

        s.sort_unstable();
        let pct = |v: &[u32], p: f64| -> f64 {
            if v.is_empty() {
                return 0.0;
            }
            let idx = ((v.len() as f64 - 1.0) * p).round() as usize;
            v[idx] as f64 / 1000.0
        };
        let total_p50 = pct(&s, 0.50);
        info!(
            "Latency encode→display: p50 {:.1}ms  p95 {:.1}ms  max {:.1}ms  ({} samples, {} in flight, {} aged out)",
            total_p50,
            pct(&s, 0.95),
            pct(&s, 1.0),
            s.len(),
            inflight,
            lost
        );

        if !d.is_empty() {
            d.sort_unstable();
            w.sort_unstable();
            let decode_p50 = pct(&d, 0.50);
            let wire_p50 = pct(&w, 0.50);
            info!(
                "  of which tablet decode+render p50 {:.1}ms  p95 {:.1}ms  → wire ~{:.1}ms",
                decode_p50,
                pct(&d, 0.95),
                wire_p50
            );
        }

        if !e2e.is_empty() {
            req_ready.sort_unstable();
            grab.sort_unstable();
            conv.sort_unstable();
            fifo.sort_unstable();
            encode.sort_unstable();
            host_total.sort_unstable();
            e2e.sort_unstable();

            info!(
                "Stage latency breakdown (p50 / p95):\n  req→ready: {:.1}/{:.1}ms | grab: {:.1}/{:.1}ms | conv: {:.1}/{:.1}ms | fifo: {:.1}/{:.1}ms | encode: {:.1}/{:.1}ms | host: {:.1}/{:.1}ms | TRUE E2E: {:.1}ms (p95 {:.1}ms)",
                pct(&req_ready, 0.50), pct(&req_ready, 0.95),
                pct(&grab, 0.50), pct(&grab, 0.95),
                pct(&conv, 0.50), pct(&conv, 0.95),
                pct(&fifo, 0.50), pct(&fifo, 0.95),
                pct(&encode, 0.50), pct(&encode, 0.95),
                pct(&host_total, 0.50), pct(&host_total, 0.95),
                pct(&e2e, 0.50), pct(&e2e, 0.95)
            );
        }
    }
}
