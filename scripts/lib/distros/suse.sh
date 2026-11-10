# shellcheck shell=bash
# UScreen installer: the SUSE family (openSUSE Leap and Tumbleweed, SLES).
#
# Owns everything specific to zypper systems: the os-release IDs, the package names and the packaged EVDI
# kernel module. Does not own the shared EVDI steps (evdi.sh) or detection (platform.sh). Interface: see _template.sh.

# Tried fourth.
suse_order() { echo 40; }

# os-release IDs of this family.
suse_ids() { echo "suse opensuse opensuse-leap opensuse-tumbleweed sles"; }

# adb is in android-tools.
suse_core_packages() { echo "ffmpeg android-tools"; }

# The evdi kernel module is a package.
suse_evdi_strategy() { echo package; }

# Installs the userspace packages with zypper.
suse_install_deps() {
    run $SUDO zypper --non-interactive install --no-recommends ffmpeg android-tools \
        || warn "Install ffmpeg and android-tools manually"
}

# The GTK 4 runtime of the fallback pen menu.
suse_install_gtk() { run $SUDO zypper --non-interactive install --no-recommends libgtk-4-1; }

# Installs the packaged evdi and libevdi; on failure the installer falls back to upstream source.
suse_evdi_install() {
    run $SUDO zypper --non-interactive install --no-recommends evdi libevdi1 && return 0
    return 1
}
