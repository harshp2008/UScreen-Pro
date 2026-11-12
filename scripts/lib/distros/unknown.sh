# shellcheck shell=bash
# UScreen installer: any distribution no other file claims.
#
# Owns the fallback: install UScreen's own files, say which packages to install by hand, and build EVDI from
# upstream source if possible. It never claims an os-release ID, so it is not asked in `us_distro_family`.
# Interface: see _template.sh.
# PLATFORM(linux): package manager, GTK runtime and EVDI strategy of the unknown family

# Never tried: this family is the fallback.
unknown_order() { echo 99; }

# Claims no IDs.
unknown_ids() { echo ""; }

# No known package names.
unknown_core_packages() { echo ""; }

# Nothing is packaged for an unknown system, so upstream source is tried.
unknown_evdi_strategy() { echo source; }

# Names the packages to install by hand.
unknown_install_deps() {
    warn "Unknown distribution. Install manually: ffmpeg, adb (android-tools), evdi + libevdi"
}

# No known package manager: the caller prints the warning about the GTK 4 runtime.
unknown_install_gtk() { return 1; }

# No package method; the installer falls back to upstream source.
unknown_evdi_install() { return 1; }
