# shellcheck shell=bash
# UScreen installer: the Arch family (Arch, Manjaro, EndeavourOS, CachyOS).
#
# Owns everything specific to pacman systems: the os-release IDs, the package names, and getting evdi-dkms,
# which exists only in the AUR (so an AUR helper is needed and its absence is explained).
# Does not own the shared EVDI steps (evdi.sh) or detection (platform.sh). Interface: see _template.sh.
# PLATFORM(arch): package manager, GTK runtime and EVDI strategy of the arch family

# Tried third.
arch_order() { echo 30; }

# os-release IDs of this family.
arch_ids() { echo "arch manjaro endeavouros cachyos"; }

# adb is in android-tools.
arch_core_packages() { echo "ffmpeg android-tools"; }

# evdi-dkms from the AUR.
arch_evdi_strategy() { echo dkms; }

# Installs the userspace packages with pacman.
arch_install_deps() { run $SUDO pacman -S --needed --noconfirm ffmpeg android-tools; }

# The GTK 4 runtime of the fallback pen menu.
arch_install_gtk() { run $SUDO pacman -S --needed --noconfirm gtk4; }

# evdi-dkms is only in the AUR: use yay or paru if there is one.
arch_evdi_install() {
    if pacman -Qq evdi-dkms >/dev/null 2>&1; then return 0; fi
    if command -v yay >/dev/null 2>&1; then run yay -S --needed --noconfirm evdi-dkms && return 0
    elif command -v paru >/dev/null 2>&1; then run paru -S --needed --noconfirm evdi-dkms && return 0
    fi
    warn "evdi-dkms is only in the AUR: install an AUR helper (yay, paru) and run this again"
    return 1
}
