//! The allowed ranges of bitrate, fps, quality, idle timeout and fade time

/// Highest bitrate the USB transport actually sustains. Beyond this the encoder
/// outruns the link, frames pile up in every queue along the way and latency
/// grows without bound — the stream does not get sharper, only later.
///
/// This is a hard ceiling rather than a hint because the value is persisted:
/// a bad number pushed once from the tablet stays in the config file and keeps
/// poisoning every subsequent run.
pub const MAX_BITRATE_KBPS: u32 = 60_000;

/// Lowest bitrate accepted; below this the picture is unusable and rate control misbehaves.
pub const MIN_BITRATE_KBPS: u32 = 1_000;

/// The generated EDID caps the virtual mode at 90 Hz (EDID 1.4 stores the pixel
/// clock in 16 bits, and 2960x1848@120 overflows it), so anything above 90
/// would only ever produce duplicate frames.
pub const MAX_FPS: u32 = 90;

/// Lowest frame rate accepted.
pub const MIN_FPS: u32 = 10;

/// Constant-quality target for the encoder (lower = sharper, more bits).
///
/// 18 rather than a more conservative value because bandwidth stopped being the
/// constraint: in constant-quality mode a desktop streams at a few Mbps against
/// a ceiling tens of times higher, so spending bits on crisp text is close to
/// free. Text sharpness is ultimately limited by 4:2:0 chroma subsampling, not
/// by this number — below roughly 16 there is nothing left to gain.
pub const DEFAULT_QUALITY: u32 = 18;

/// Sharpest constant-quality value accepted; lower numbers only burn bandwidth.
pub const MIN_QUALITY: u32 = 12;

/// Blurriest constant-quality value accepted.
pub const MAX_QUALITY: u32 = 32;

/// Shortest and longest idle timeout (seconds) before the tablet is dimmed.
pub const MIN_IDLE_TIMEOUT_SECS: u32 = 10;

pub const MAX_IDLE_TIMEOUT_SECS: u32 = 300;

/// Shortest and longest fade (milliseconds) of the idle dim / undim animation.
pub const MIN_FADE_MS: u32 = 100;

pub const MAX_FADE_MS: u32 = 30_000;
