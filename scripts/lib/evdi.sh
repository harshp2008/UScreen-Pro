# shellcheck shell=bash
# UScreen installer: getting the EVDI virtual-display kernel module, without ever leaving the
# package manager half-configured.
#
# Needs from the caller: `run` (executes or, in a dry run, only prints), `info`, `warn`, `$SUDO`.
# Sets US_EVDI_NEEDS_REBOOT=1 when the module cannot be (re)loaded in the running system.

# 0 when an EVDI module for the running kernel can be loaded.
evdi_module_available() { modinfo evdi >/dev/null 2>&1; }

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

# Ubuntu: the kernel packages ship signed EVDI modules; install the one for the running kernel.
evdi_install_prebuilt_ubuntu() {
    local cand
    while read -r cand; do
        [ -n "$cand" ] || continue
        if apt-cache show "$cand" >/dev/null 2>&1; then
            info "Installing the distribution's prebuilt EVDI module ($cand)"
            run $SUDO apt-get install -y "$cand" && return 0
        fi
    done < <(us_ubuntu_evdi_candidates)
    return 1
}

# The newest upstream EVDI release tag (v1.14.x), from the repository itself, not from memory.
evdi_latest_tag() {
    git ls-remote --tags --refs https://github.com/DisplayLink/evdi 2>/dev/null \
        | sed 's|.*refs/tags/||' | grep -E '^v?[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -n1
}

# Build upstream EVDI with dkms: for kernels newer than the distribution's package supports.
evdi_install_from_source() {
    command -v git >/dev/null 2>&1 || { warn "git is needed to build EVDI from source"; return 1; }
    command -v dkms >/dev/null 2>&1 || { warn "dkms is needed to build EVDI from source"; return 1; }
    local tag tmp
    tag="$(evdi_latest_tag)"
    [ -n "$tag" ] || { warn "could not find the latest EVDI release (offline?)"; return 1; }
    info "Building EVDI $tag from source with dkms"
    tmp="$(mktemp -d /tmp/uscreen-evdi.XXXXXX)"
    # shellcheck disable=SC2064
    trap "rm -rf -- '$tmp'" RETURN
    run git clone --depth 1 --branch "$tag" https://github.com/DisplayLink/evdi "$tmp/evdi" || return 1
    local ver="${tag#v}"
    run $SUDO rm -rf "/usr/src/evdi-$ver"
    run $SUDO cp -r "$tmp/evdi/module" "/usr/src/evdi-$ver"
    run $SUDO dkms add -m evdi -v "$ver" || true
    run $SUDO dkms build -m evdi -v "$ver" || { warn "EVDI $tag does not build on kernel ${UNAME_R:-$(uname -r)}"; return 1; }
    run $SUDO dkms install -m evdi -v "$ver" || return 1
}

# Get a loadable EVDI module for the running kernel using the cheapest method this family has.
# Returns 0 on success. Never leaves the package manager in a broken state.
evdi_ensure() {
    # A package left half-configured by an earlier failed build breaks every later apt call. Remove
    # it, unless dkms has really built and installed a module from it.
    if [ "$(us_distro_family)" = debian ] && dpkg_package_broken evdi-dkms; then
        if dkms status evdi 2>/dev/null | grep -q 'installed'; then
            warn "evdi-dkms is half-configured but dkms has a built module: leaving it alone"
        else
            dpkg_clean_broken evdi-dkms
        fi
    fi
    if evdi_module_available; then
        info "EVDI module already available for kernel ${UNAME_R:-$(uname -r)}"
        return 0
    fi
    local strategy family
    strategy="$(us_evdi_strategy)"
    family="$(us_distro_family)"
    info "EVDI module missing — strategy for this system: $strategy"
    case "$strategy" in
        bundled)
            warn "This image should already ship EVDI; if it is missing, update the image"
            return 1 ;;
        prebuilt)
            dpkg_clean_broken evdi-dkms
            evdi_install_prebuilt_ubuntu && return 0
            ;;
        package)
            run $SUDO zypper --non-interactive install --no-recommends evdi libevdi1 && return 0
            ;;
        dkms)
            case "$family" in
                debian)
                    dpkg_clean_broken evdi-dkms
                    run $SUDO apt-get install -y "linux-headers-${UNAME_R:-$(uname -r)}" dkms || true
                    if run $SUDO apt-get install -y evdi-dkms && evdi_module_available; then return 0; fi
                    # The distribution's source was too old for this kernel: do not leave it half-configured.
                    dpkg_clean_broken evdi-dkms
                    ;;
                arch)
                    if pacman -Qq evdi-dkms >/dev/null 2>&1; then return 0; fi
                    if command -v yay >/dev/null 2>&1; then run yay -S --needed --noconfirm evdi-dkms && return 0
                    elif command -v paru >/dev/null 2>&1; then run paru -S --needed --noconfirm evdi-dkms && return 0
                    fi
                    warn "evdi-dkms is only in the AUR: install an AUR helper (yay, paru) and run this again"
                    ;;
            esac
            ;;
    esac
    # Last resort for every family: upstream source.
    evdi_install_from_source && return 0
    return 1
}

# Make the module load with two devices and load it now; if it is in use, a reboot is needed.
evdi_activate() {
    run $SUDO mkdir -p /etc/modprobe.d /etc/modules-load.d
    local conf="options evdi initial_device_count=2"
    # `softdep ... pre: i915` is only valid where i915 exists; the xe driver (newer Intel) has no such need.
    printf '%s\n' "$conf" | run_tee /etc/modprobe.d/uscreen-evdi.conf
    printf 'evdi\nuinput\n' | run_tee /etc/modules-load.d/uscreen.conf
    if lsmod 2>/dev/null | grep -q '^evdi'; then
        if ! run $SUDO modprobe -r evdi 2>/dev/null; then
            warn "EVDI is loaded and in use — it will pick up its settings after a reboot"
            US_EVDI_NEEDS_REBOOT=1
            return 0
        fi
    fi
    if ! run $SUDO modprobe evdi 2>/dev/null; then
        evdi_module_available && US_EVDI_NEEDS_REBOOT=1
        warn "EVDI could not be loaded now"
        return 0
    fi
    if [ "$(cat /sys/devices/evdi/count 2>/dev/null || echo 0)" = "0" ]; then
        echo 1 | run_tee /sys/devices/evdi/add >/dev/null 2>&1 || true
    fi
}
