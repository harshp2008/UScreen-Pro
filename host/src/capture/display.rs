//! Placement of the virtual (EVDI) display on the desktop, and the FIFO the helper writes frames to.
//!
//! Why this is separate from the supervisor: every compositor (KWin, Mutter, Hyprland, wlroots,
//! X11) wants a different incantation to enable, position and later remove the new output, and
//! none of it has anything to do with encoding. Keeping it here lets the supervisor stay a
//! plain "start helper, start encoder, forward frames" loop.

use super::*;

// PLATFORM(evdi): explains a missing evdi kernel module or missing devices by reading /sys/module/evdi and /sys/devices/evdi
/// Why the helper cannot get an EVDI device, in the words of someone who can
/// fix it. Returns None when the state looks fine and the failure was
/// something else.
pub(super) fn evdi_setup_problem() -> Option<String> {
    evdi_setup_problem_in(std::path::Path::new("/sys/devices/evdi"))
}

/// Split out so it can be tested against a temporary directory. Unloading the
/// real module needs root and takes Xwayland down with it, so the three states
/// this has to tell apart are otherwise unreachable from a test.
fn evdi_setup_problem_in(dir: &std::path::Path) -> Option<String> {
    let module_loaded = dir.exists();
    if !module_loaded {
        return Some(
            "The evdi kernel module is not loaded. Install it (evdi-dkms; on Arch it is in \
             the AUR) and run: sudo modprobe evdi"
                .to_string(),
        );
    }

    let count: u32 = std::fs::read_to_string(dir.join("count"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    if count > 0 {
        return None;
    }

    // The module is there but has no devices, and creating one needs a write
    // to a root-only file. `initial_device_count` fixes it for good, but only
    // takes effect when the module loads — which is why writing the
    // modprobe.d file is not enough on a machine where evdi is already
    // resident.
    Some(
        "No EVDI device exists and /sys/devices/evdi/add is root-only, so one cannot be \
         created. Fix it for this boot with:\n    echo 1 | sudo tee /sys/devices/evdi/add\n\
         and for every boot with:\n    echo 'options evdi initial_device_count=2' | sudo tee \
         /etc/modprobe.d/uscreen-evdi.conf\n    sudo modprobe -r evdi && sudo modprobe evdi\n\
         Then run: uscreen doctor"
            .to_string(),
    )
}

impl CaptureManager {
    /// Create a fresh capture FIFO at `path`, replacing whatever is there.
    pub(super) fn ensure_fifo(path: &str) -> Result<()> {
        // Anything already there is replaced, whatever it is: a stale FIFO
        // may carry old permissions, and a regular file or symlink in this
        // spot is not ours. mkfifo itself never follows symlinks.
        let _ = std::fs::remove_file(path);
        let c = std::ffi::CString::new(path).context("fifo path")?;
        // 0600: the helper writes it and the encoder reads it, both as this
        // user. Nobody else has any business with a live copy of the screen.
        let rc = unsafe { libc::mkfifo(c.as_ptr(), 0o600) };
        if rc != 0 {
            return Err(std::io::Error::last_os_error()).context("mkfifo");
        }
        Ok(())
    }

    // PLATFORM(linux): if-ladder over desktop backends: Hyprland, COSMIC, GNOME (does it by itself), X11, then inline kscreen-doctor code for KDE
    /// Enable the EVDI output and place it to the right of every other screen.
    ///
    /// The output is identified by the DRM connector names sysfs reports for
    /// EVDI cards, not by whether the name happens to contain "DVI": a real DVI
    /// monitor on a dock produces exactly the same name pattern, and acting on
    /// it would enable and move the user's physical screen instead of ours.
    ///
    /// The position is derived at runtime from the existing display geometry so
    /// it works regardless of the laptop's screen resolution or scaling factor.
    pub(super) async fn enable_evdi_display(card: Option<u32>, position: crate::config::Position) {
        crate::desktop::display::enable_output(card, position).await;
    }

    /// Turn the virtual output back off so its windows return to the real
    /// screens. Without this the desktop keeps a monitor nobody can see after
    /// the tablet is unplugged, and windows left there are effectively lost.
    pub async fn disable_evdi_display(card: Option<u32>) {
        crate::desktop::display::disable_output(card).await;
    }
}

/// Tests for the EVDI problem report, using temporary directories in place of `/sys`.
#[cfg(test)]
mod tests {
    use super::*;

    /// A missing EVDI module is reported as not loaded.
    #[test]
    fn evdi_problem_reports_a_missing_module() {
        let dir = std::env::temp_dir().join("uscreen-test-evdi-absent");
        let _ = std::fs::remove_dir_all(&dir);
        let msg = evdi_setup_problem_in(&dir).expect("absent module is a problem");
        assert!(msg.contains("not loaded"), "got: {msg}");
    }
    /// A module with no devices (count 0, `add` writable only by root) is reported as a problem.
    #[test]
    fn evdi_problem_reports_a_module_with_no_devices() {
        // The state the Arch report landed in: module resident, count 0, and
        // /sys/devices/evdi/add writable only by root.
        let dir = std::env::temp_dir().join("uscreen-test-evdi-empty");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("count"), "0\n").unwrap();
        let msg = evdi_setup_problem_in(&dir).expect("no devices is a problem");
        assert!(msg.contains("initial_device_count"), "got: {msg}");
        assert!(
            msg.contains("modprobe -r evdi"),
            "must give the reload, got: {msg}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// No problem is reported when a device exists.
    #[test]
    fn evdi_problem_stays_quiet_when_a_device_exists() {
        let dir = std::env::temp_dir().join("uscreen-test-evdi-ok");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("count"), "1\n").unwrap();
        assert!(
            evdi_setup_problem_in(&dir).is_none(),
            "a working setup must not be blamed for an unrelated failure"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
