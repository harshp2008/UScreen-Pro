# shellcheck shell=bash
# UScreen installer: the Debian family (Ubuntu, Debian, Linux Mint, Pop!_OS, Raspberry Pi OS).
#
# Owns everything specific to apt/dpkg systems: which os-release IDs belong here, the package names,
# how the EVDI kernel module is obtained (Ubuntu: a prebuilt signed module; elsewhere: dkms) and how a
# half-installed evdi-dkms package is cleaned up so apt keeps working.
# Does not own detection of the desktop or the restart rules (platform.sh), nor the steps shared by all
# families (evdi.sh: dkms from upstream source, activating the module).
# The interface every family file implements is described in _template.sh.
# PLATFORM(debian): package manager, GTK runtime and EVDI strategy of the debian family

# Tried first among the families.
debian_order() { echo 10; }

# os-release IDs of this family.
debian_ids() { echo "ubuntu debian linuxmint pop raspbian"; }

# Package names checked against real Debian and Ubuntu containers, not from memory.
debian_core_packages() { echo "ffmpeg adb libevdi1 libevdi-dev"; }

# Ubuntu ships a signed EVDI module for each kernel; everything else builds one with dkms.
debian_evdi_strategy() {
    [ "$(us_os_value ID)" = ubuntu ] && echo prebuilt || echo dkms
}

# Package names to try, in order, for a prebuilt EVDI module on Ubuntu for the running kernel.
debian_evdi_prebuilt_candidates() {
    local k="${UNAME_R:-$(uname -r)}" v
    v="$(us_os_value VERSION_ID)"
    echo "linux-main-modules-evdi-$k"
    echo "linux-modules-evdi-$k"
    echo "linux-modules-evdi-$(us_kernel_flavour)-hwe-$v"
    echo "linux-modules-evdi-$(us_kernel_flavour)"
}

# The userspace packages. Userspace first: a failing kernel-module build must not stop ffmpeg and adb.
debian_install_deps() {
    run $SUDO apt-get update || warn "apt-get update failed — continuing with the cached lists"
    # shellcheck disable=SC2046
    run $SUDO apt-get install -y ffmpeg adb libevdi1 libevdi-dev \
        || run $SUDO apt-get install -y ffmpeg android-tools-adb libevdi1 \
        || warn "Check the package names for your release"
}

# The GTK 4 runtime of the fallback pen menu.
debian_install_gtk() { run $SUDO apt-get install -y libgtk-4-1; }

# Before anything else: a package left half-configured by an earlier failed build breaks every later
# apt call. Remove it, unless dkms has really built and installed a module from it.
debian_evdi_prepare() {
    if dpkg_package_broken evdi-dkms; then
        if dkms status evdi 2>/dev/null | grep -q 'installed'; then
            warn "evdi-dkms is half-configured but dkms has a built module: leaving it alone"
        else
            dpkg_clean_broken evdi-dkms
        fi
    fi
}

# Get the EVDI module with this family's own method. Return 0 when the module is now installed;
# non-zero lets evdi_ensure fall back to building upstream source.
debian_evdi_install() {
    case "$(debian_evdi_strategy)" in
        prebuilt)
            dpkg_clean_broken evdi-dkms
            debian_evdi_install_prebuilt && return 0
            ;;
        dkms)
            dpkg_clean_broken evdi-dkms
            run $SUDO apt-get install -y "linux-headers-${UNAME_R:-$(uname -r)}" dkms || true
            if run $SUDO apt-get install -y evdi-dkms && evdi_module_available; then return 0; fi
            # The distribution's source was too old for this kernel: do not leave it half-configured.
            dpkg_clean_broken evdi-dkms
            ;;
    esac
    return 1
}

# Ubuntu: the kernel packages ship signed EVDI modules; install the one for the running kernel.
debian_evdi_install_prebuilt() {
    local cand
    while read -r cand; do
        [ -n "$cand" ] || continue
        if apt-cache show "$cand" >/dev/null 2>&1; then
            info "Installing the distribution's prebuilt EVDI module ($cand)"
            run $SUDO apt-get install -y "$cand" && return 0
        fi
    done < <(debian_evdi_prebuilt_candidates)
    return 1
}

# 0 when a Debian-family package is half-installed or half-configured (dpkg state F, H, U or W...).
# That state makes every later `apt` call fail, so it is cleaned up before anything else.
dpkg_package_broken() {
    local st
    st="$(dpkg-query -W -f='${db:Status-Abbrev}' "$1" 2>/dev/null || true)"
    case "$st" in
        ""|ii*|rc*|un*) return 1 ;;
        *) return 0 ;;
    esac
}

# Remove a half-installed package so apt works again. Purge, not remove: a failed configure leaves
# state that `remove` keeps. Never touches modules owned by another package.
dpkg_clean_broken() {
    local pkg="$1"
    if dpkg_package_broken "$pkg"; then
        warn "$pkg is half-installed (its build failed) — removing it so apt works again"
        run $SUDO dpkg --purge --force-remove-reinstreq "$pkg" || warn "could not remove $pkg"
    fi
}
