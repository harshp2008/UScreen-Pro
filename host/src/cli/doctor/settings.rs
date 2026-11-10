//! Checks of the release version and of config.toml

use super::*;

/// Doctor check: the running version against the latest release (skipped when update checks are
/// off).
pub(super) async fn check_version(r: &mut Report, cfg: &FileConfig) {
    if !cfg.check_updates {
        r.line(
            Level::Ok,
            "version",
            &format!(
                "{} (update check disabled)",
                crate::daemon::update::current_version()
            ),
        );
        return;
    }
    // One-shot: doctor is its own process and cannot read the daemon's daily
    // result, and ten seconds on the network is fine for a diagnostic.
    let cur = crate::daemon::update::current_version();
    match crate::daemon::update::latest_release_tag().await {
        Some(tag) => {
            let latest = tag.trim_start_matches('v').to_string();
            if crate::daemon::update::is_newer(&latest, cur) {
                r.line(
                    Level::Warn,
                    "version",
                    &format!("{} — {} is available", cur, latest),
                );
                r.hint(crate::daemon::update::RELEASES_PAGE);
            } else {
                r.line(Level::Ok, "version", &format!("{} (latest)", cur));
            }
        }
        None => r.line(
            Level::Ok,
            "version",
            &format!("{} (could not check for updates)", cur),
        ),
    }
}

/// Doctor check: config.toml values are in range, including stale out-of-range values still on
/// disk.
pub(super) fn check_config(r: &mut Report, cfg: &FileConfig) {
    let path = config::config_path();
    // `cfg` has already been clamped, so compare against the raw file too: a
    // stale 200 Mbps on disk is worth reporting even though the daemon would
    // no longer act on it.
    let on_disk: Option<FileConfig> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| toml::from_str(&t).ok());
    r.line(Level::Ok, "config file", &format!("{}", path.display()));
    if cfg.max_tablets > 1 {
        let cards = crate::display::vdisplay::evdi_cards().len() as u32;
        if cards >= cfg.max_tablets {
            r.line(
                Level::Ok,
                "tablet slots",
                &format!("{} (EVDI devices: {})", cfg.max_tablets, cards),
            );
        } else {
            r.line(
                Level::Warn,
                "tablet slots",
                &format!(
                    "{} wanted, but only {} EVDI device(s) exist",
                    cfg.max_tablets, cards
                ),
            );
            r.hint(&format!(
                "for this boot: echo 1 | sudo tee /sys/devices/evdi/add   (repeat {} time(s)); \
                 for every boot: initial_device_count={} in /etc/modprobe.d/uscreen-evdi.conf",
                cfg.max_tablets - cards,
                cfg.max_tablets
            ));
        }
    }
    r.line(
        Level::Ok,
        "screen position",
        match cfg.position.as_str() {
            "left" => "left of the other screens",
            "above" => "above the other screens",
            "below" => "below the other screens",
            _ => "right of the other screens",
        },
    );
    r.line(
        Level::Ok,
        "mode",
        if cfg.pen_only {
            "graphics tablet — no display is streamed (switchable from the tablet)"
        } else {
            "second screen (switchable from the tablet)"
        },
    );

    let raw_bitrate = on_disk.as_ref().map(|c| c.bitrate).unwrap_or(cfg.bitrate);
    if raw_bitrate > MAX_BITRATE_KBPS {
        r.line(
            Level::Warn,
            "bitrate on disk",
            &format!(
                "{} Mbps — clamped to {} Mbps at runtime",
                raw_bitrate / 1000,
                cfg.bitrate / 1000
            ),
        );
        r.hint("rewrite it via UScreen Pro (or the tablet settings) to make the file agree");
    } else {
        r.line(
            Level::Ok,
            "bitrate",
            &format!("{} Mbps", cfg.bitrate as f64 / 1000.0),
        );
    }

    let raw_fps = on_disk.as_ref().map(|c| c.fps).unwrap_or(cfg.fps);
    if raw_fps > MAX_FPS {
        r.line(
            Level::Warn,
            "fps on disk",
            &format!("{} — clamped to {} at runtime", raw_fps, cfg.fps),
        );
        r.hint("the generated EDID is capped at 90 Hz, so anything above is duplicate frames");
    } else {
        r.line(Level::Ok, "fps", &format!("{}", cfg.fps));
    }

    r.line(
        Level::Ok,
        "resolution",
        &format!(
            "{}x{}{}",
            cfg.width,
            cfg.height,
            if cfg.auto_resolution {
                " (auto, follows the tablet)"
            } else {
                " (fixed)"
            }
        ),
    );
}
