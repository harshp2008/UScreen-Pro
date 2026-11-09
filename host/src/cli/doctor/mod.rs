//! `uscreen doctor` — one-shot check of everything that has to be true for the
//! pipeline to work, with an actionable hint for every failure.
//!
//! This is deliberately the first thing to run whenever the stream "is laggy
//! again". The most common cause is not the pipeline at all but orphaned
//! `evdi_helper`/`ffmpeg` processes from a previous run: several writers on the
//! shared capture FIFO interleave at pipe granularity, and no amount of
//! restarting the daemon fixes it until they are killed.
//!
//! Layout:
//! * `system` - checks of the machine itself: kernel modules, required tools, running processes
//! * `tablet` - checks of the connected tablet: adb, transport, app version
//! * `display` - checks of the virtual display and the desktop around it: compositor, on-screen keyboard, autostart, colour
//! * `settings` - checks of the release version and of config.toml
//! * `latency` - `uscreen doctor --latency`: end-to-end latency report and the mouse sweep

mod display;
mod latency;
mod settings;
mod system;
mod tablet;
use display::*;
pub use latency::run_latency;
use settings::*;
use system::*;
use tablet::*;

use crate::capture::fifo_path;
use crate::config::{self, FileConfig, MAX_BITRATE_KBPS, MAX_FPS};
use crate::display::vdisplay;
use anyhow::Result;
use std::path::Path;

#[derive(PartialEq)]
enum Level {
    Ok,
    Warn,
    Fail,
}

struct Report {
    warnings: u32,
    failures: u32,
}

impl Report {
    /// A report with no warnings or failures yet.
    fn new() -> Self {
        Self {
            warnings: 0,
            failures: 0,
        }
    }

    /// Print one result line (`ok`, `warn` or `FAIL`) and count it.
    fn line(&mut self, level: Level, label: &str, detail: &str) {
        let mark = match level {
            Level::Ok => "  ok  ",
            Level::Warn => " warn ",
            Level::Fail => " FAIL ",
        };
        match level {
            Level::Warn => self.warnings += 1,
            Level::Fail => self.failures += 1,
            Level::Ok => {}
        }
        if detail.is_empty() {
            println!("[{}] {}", mark, label);
        } else {
            println!("[{}] {:<34} {}", mark, label, detail);
        }
    }

    /// A remedy printed under the finding it belongs to.
    fn hint(&self, text: &str) {
        println!("         → {}", text);
    }
}

/// Print a section heading.
fn section(title: &str) {
    println!("\n{}", title);
    println!("{}", "-".repeat(title.len()));
}

/// Run `program` and return its stdout, or `None` if it could not be run.
async fn output_of(program: &str, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Whether a file called `name` exists in any directory of `$PATH`.
fn command_exists(name: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| Path::new(dir).join(name).exists())
}

/// PIDs whose executable name matches exactly (`pgrep -x`).
async fn pids_exact(name: &str) -> Vec<u32> {
    parse_pids(output_of("pgrep", &["-x", name]).await)
}

/// PIDs whose full command line matches a pattern (`pgrep -f`).
async fn pids_full(pattern: &str) -> Vec<u32> {
    parse_pids(output_of("pgrep", &["-f", pattern]).await)
}

/// Turn `pidof`/`pgrep` output (one pid per line) into pids.
fn parse_pids(out: Option<String>) -> Vec<u32> {
    out.unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .collect()
}

/// Entry point of `uscreen doctor`: probe the kernel, EVDI, compositor, encoders and adb, and print a pass/warn/fail report with remediation hints.
pub async fn run() -> Result<()> {
    println!("=== uscreen doctor ===");

    let mut r = Report::new();
    // Loaded once: every load logs a warning when it clamps a stale value, and
    // repeating that between report sections buries the report itself.
    let cfg = FileConfig::load();

    section("Kernel modules");
    check_modules(&mut r);

    section("Tools");
    check_tools(&mut r, &cfg).await;

    section("Processes");
    check_processes(&mut r).await;

    section("Tablet");
    check_tablet(&mut r, &cfg).await;

    section("Virtual display");
    check_virtual_display(&mut r, &cfg).await;

    section("Autostart");
    check_autostart(&mut r).await;

    section("Desktop");
    check_osk(&mut r).await;

    section("Colour");
    check_colour(&mut r).await;

    section("Configuration");
    check_version(&mut r, &cfg).await;
    check_config(&mut r, &cfg);

    println!();
    if r.failures > 0 {
        println!(
            "{} problem(s) and {} warning(s) found — fix the FAIL lines first.",
            r.failures, r.warnings
        );
    } else if r.warnings > 0 {
        println!("No blocking problems, {} warning(s).", r.warnings);
    } else {
        println!("Everything checks out.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_kwin_version_from_its_banner() {
        assert_eq!(kwin_version("kwin 6.8.2\n"), Some((6, 8, 2)));
        assert_eq!(kwin_version("kwin 6.7.5"), Some((6, 7, 5)));
        assert_eq!(kwin_version("kwin 6.8"), Some((6, 8, 0)));
        assert_eq!(kwin_version(""), None);
        assert_eq!(kwin_version("kwin"), None);
    }
}
