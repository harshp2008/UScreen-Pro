# shellcheck shell=bash
# UScreen installer: any distribution no other file claims.
#
# Owns the fallback: install UScreen's own files, say which packages to install by hand, and build EVDI from
# upstream source if possible. It never claims an os-release ID, so it is not asked in `us_distro_family`.
# Interface: see _template.sh.

unknown_order() { echo 99; }

unknown_ids() { echo ""; }

unknown_core_packages() { echo ""; }

unknown_evdi_strategy() { echo source; }

unknown_install_deps() {
    warn "Unknown distribution. Install manually: ffmpeg, adb (android-tools), evdi + libevdi"
}

# No known package manager: the caller prints the warning about the GTK 4 runtime.
unknown_install_gtk() { return 1; }

unknown_evdi_install() { return 1; }
