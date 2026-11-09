# shellcheck shell=bash
# UScreen installer: the SUSE family (openSUSE Leap and Tumbleweed, SLES).
#
# Owns everything specific to zypper systems: the os-release IDs, the package names and the packaged EVDI
# kernel module. Does not own the shared EVDI steps (evdi.sh) or detection (platform.sh). Interface: see _template.sh.

suse_order() { echo 40; }

suse_ids() { echo "suse opensuse opensuse-leap opensuse-tumbleweed sles"; }

suse_core_packages() { echo "ffmpeg android-tools"; }

suse_evdi_strategy() { echo package; }

suse_install_deps() {
    run $SUDO zypper --non-interactive install --no-recommends ffmpeg android-tools \
        || warn "Install ffmpeg and android-tools manually"
}

suse_install_gtk() { run $SUDO zypper --non-interactive install --no-recommends libgtk-4-1; }

suse_evdi_install() {
    run $SUDO zypper --non-interactive install --no-recommends evdi libevdi1 && return 0
    return 1
}
