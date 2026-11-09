//! Checks of the machine itself: kernel modules, required tools, running processes

use super::*;

/// Doctor check: the required and optional external tools are installed (ffmpeg, adb,
/// kscreen-doctor, ...).
pub(super) async fn check_tools(r: &mut Report, cfg: &FileConfig) {
    for (tool, fatal) in [("ffmpeg", true), ("adb", true), ("kscreen-doctor", false)] {
        if command_exists(tool) {
            r.line(Level::Ok, tool, "found");
        } else if fatal {
            r.line(Level::Fail, tool, "not installed");
            r.hint(&format!("install {} with your package manager", tool));
        } else {
            if crate::display::hyprland::active() {
                r.line(Level::Ok, tool, "not needed on Hyprland (hyprctl is used)");
            } else if crate::display::gnome::active()
                || crate::display::x11::active()
                || crate::display::cosmic::active()
            {
                r.line(Level::Ok, tool, "not needed on this desktop");
            } else {
                r.line(Level::Warn, tool, "not installed (KDE only)");
            }
        }
    }

    // By actually encoding a frame: `ffmpeg -encoders` lists what ffmpeg was
    // built with, and a distribution build is built with everything. An AMD
    // laptop had h264_nvenc "available" by that measure (#15).
    if output_of("ffmpeg", &["-version"]).await.is_some() {
        if let Err(why) = crate::capture::encoders::probe(&cfg.encoder).await {
            r.line(
                Level::Fail,
                "configured encoder",
                &format!("{} cannot encode on this machine", cfg.encoder),
            );
            r.hint(&format!("ffmpeg: {}", why));
            let mut alternatives = Vec::new();
            for e in crate::capture::encoders::CANDIDATES {
                if e != cfg.encoder && crate::capture::encoders::works(e).await {
                    alternatives.push(e);
                }
            }
            if alternatives.is_empty() {
                r.hint("no H.264 encoder works at all — on Fedora, install ffmpeg from RPM Fusion");
            } else {
                r.hint(&format!(
                    "works here: {} — the daemon switches to the first of these by itself on start",
                    alternatives.join(", ")
                ));
            }
        } else {
            r.line(
                Level::Ok,
                "configured encoder",
                &format!("{} encodes a test frame", cfg.encoder),
            );
            if cfg.encoder.contains("vaapi") {
                r.line(
                    Level::Ok,
                    "VAAPI device",
                    &crate::capture::encoders::vaapi_device(),
                );
                if crate::capture::encoders::nvidia_gpu_present() {
                    r.hint(
                        "an NVIDIA GPU is also in this machine. With VAAPI on the other GPU, steady \
                         motion at full size can make frames queue up in front of the encoder. If it \
                         feels laggy, try encoder = \"h264_nvenc\", or stream_scale = 2",
                    );
                }
            }
        }
    }
}

/// Doctor check: the evdi kernel module is loaded and exposes devices.
pub(super) fn check_modules(r: &mut Report) {
    match std::fs::read_to_string("/sys/devices/evdi/count") {
        Ok(text) => {
            let count: i32 = text.trim().parse().unwrap_or(-1);
            if count > 0 {
                r.line(Level::Ok, "evdi module", &format!("{} device(s)", count));
            } else {
                r.line(Level::Fail, "evdi module", "loaded, but no device created");
                r.hint("for this boot:   echo 1 | sudo tee /sys/devices/evdi/add");
                // The one-shot write above does not survive a reboot, and
                // initial_device_count is only read when the module loads —
                // so writing the modprobe.d file is not enough on a machine
                // where evdi is already resident. Both halves matter.
                r.hint(
                    "for every boot: echo 'options evdi initial_device_count=2' | sudo tee \
                     /etc/modprobe.d/uscreen-evdi.conf && sudo modprobe -r evdi && sudo modprobe evdi",
                );
            }
        }
        Err(_) => {
            r.line(Level::Fail, "evdi module", "not loaded");
            r.hint("install evdi-dkms (on Arch it is in the AUR: yay -S evdi-dkms), then: sudo modprobe evdi");
            r.hint("sudo modprobe evdi   (install evdi-dkms if that fails)");
        }
    }

    let uinput = Path::new("/dev/uinput");
    if !uinput.exists() {
        r.line(Level::Fail, "uinput device", "/dev/uinput missing");
        r.hint("sudo modprobe uinput");
    } else {
        // Existence is not enough — the daemon runs unprivileged and needs to
        // open it for writing, which is what actually fails in practice.
        match std::fs::OpenOptions::new().write(true).open(uinput) {
            Ok(_) => r.line(Level::Ok, "uinput device", "writable"),
            Err(e) => {
                r.line(
                    Level::Fail,
                    "uinput device",
                    &format!("not writable: {}", e),
                );
                r.hint("sudo install -Dm644 packaging/60-uscreen-uinput.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger --name-match=uinput");
            }
        }
    }
}

/// The check that matters most: more than one helper or encoder means several
/// processes are writing/reading the same FIFO and every frame is corrupt.
pub(super) async fn check_processes(r: &mut Report) {
    let daemons = pids_exact("uscreen").await;
    let helpers = pids_exact("evdi_helper").await;
    let encoders = pids_full(&format!("ffmpeg.*{}", fifo_path())).await;

    // The PID file is the daemon's single slot; anything running beside it is
    // untracked and `uscreen stop` will never reach it.
    let pid_file = crate::config::handler::get_pid_path();
    let tracked: Option<u32> = std::fs::read_to_string(&pid_file)
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .filter(|pid| Path::new(&format!("/proc/{}", pid)).exists());

    match tracked {
        Some(pid) => r.line(Level::Ok, "daemon", &format!("running, PID {}", pid)),
        None if pid_file.exists() => {
            r.line(Level::Warn, "daemon", "stale PID file, not running");
            r.hint(&format!("rm {}", pid_file.display()));
        }
        None => r.line(Level::Ok, "daemon", "not running"),
    }

    // `pgrep -x uscreen` also matches this very process — excluding it is not
    // cosmetic: reporting ourselves as an orphan would send the user off to
    // `pkill -x uscreen` chasing a process that never existed.
    let self_pid = std::process::id();
    let untracked: Vec<u32> = daemons
        .iter()
        .copied()
        .filter(|p| *p != self_pid && Some(*p) != tracked)
        .collect();
    if !untracked.is_empty() {
        r.line(
            Level::Fail,
            "orphaned uscreen daemons",
            &format!("{:?}", untracked),
        );
        r.hint("these fight over the same FIFO and ports — kill them: pkill -x uscreen");
    }

    // Several helpers are fine with several tablets — each writes its own
    // FIFO. Two on the same FIFO are the fault.
    let mut per_fifo: std::collections::HashMap<String, Vec<u32>> = Default::default();
    for pid in &helpers {
        let args: Vec<String> = std::fs::read(format!("/proc/{}/cmdline", pid))
            .map(|b| {
                b.split(|c| *c == 0)
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .collect()
            })
            .unwrap_or_default();
        let fifo = args
            .iter()
            .position(|a| a == "--capture-fifo")
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_default();
        per_fifo.entry(fifo).or_default().push(*pid);
    }
    if let Some((fifo, pids)) = per_fifo.iter().find(|(_, p)| p.len() > 1) {
        r.line(
            Level::Fail,
            "evdi_helper processes",
            &format!("{} writing {}: {:?}", pids.len(), fifo, pids),
        );
        r.hint("several writers interleave on the FIFO — torn frames: pkill -x evdi_helper");
    } else if helpers.len() == 1 && tracked.is_none() {
        r.line(Level::Fail, "evdi_helper", "orphaned (no daemon owns it)");
        r.hint("pkill -x evdi_helper");
    } else {
        r.line(
            Level::Ok,
            "evdi_helper processes",
            &format!("{}", helpers.len()),
        );
    }

    if encoders.len() > 1 {
        r.line(
            Level::Fail,
            "ffmpeg on capture FIFO",
            &format!("{} running: {:?}", encoders.len(), encoders),
        );
        r.hint("two readers on one pipe corrupt frames — kill the strays");
    } else if encoders.len() == 1 && tracked.is_none() {
        r.line(Level::Fail, "ffmpeg on capture FIFO", "orphaned");
        r.hint(&format!("pkill -f 'ffmpeg.*{}'", fifo_path()));
    } else {
        r.line(
            Level::Ok,
            "ffmpeg on capture FIFO",
            &format!("{}", encoders.len()),
        );
    }
}
