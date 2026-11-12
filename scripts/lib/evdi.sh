# shellcheck shell=bash
# UScreen installer: getting the EVDI virtual-display kernel module, without ever leaving the
# package manager half-configured.
#
# Holds the steps every family shares: is the module there, build upstream source with dkms, load it with two
# devices. What differs per family (package names, prebuilt/dkms/package methods) is in distros/<family>.sh.
#
# Needs from the caller: `run` (executes or, in a dry run, only prints), `info`, `warn`, `$SUDO`.
# Sets US_EVDI_NEEDS_REBOOT=1 when the module cannot be (re)loaded in the running system.
# PLATFORM(evdi): getting the EVDI kernel module: packages, DKMS or a source build

# 0 when an EVDI module for the running kernel can be loaded.
evdi_module_available() { modinfo evdi >/dev/null 2>&1; }

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
# The family's own method is `<family>_evdi_install` (distros/<family>.sh); building upstream source is the
# last resort for every family.
evdi_ensure() {
    local family strategy
    family="$(us_distro_family)"
    # Optional per-family repair step (Debian: a half-configured evdi-dkms breaks every later apt call).
    if declare -F "${family}_evdi_prepare" >/dev/null; then "${family}_evdi_prepare"; fi
    if evdi_module_available; then
        info "EVDI module already available for kernel ${UNAME_R:-$(uname -r)}"
        return 0
    fi
    strategy="$(us_evdi_strategy)"
    info "EVDI module missing — strategy for this system: $strategy"
    if [ "$strategy" = bundled ]; then
        warn "This image should already ship EVDI; if it is missing, update the image"
        return 1
    fi
    "${family}_evdi_install" && return 0
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
        # shellcheck disable=SC2034  # read by the caller (us_restart_needs)
        evdi_module_available && US_EVDI_NEEDS_REBOOT=1
        warn "EVDI could not be loaded now"
        return 0
    fi
    if [ "$(cat /sys/devices/evdi/count 2>/dev/null || echo 0)" = "0" ]; then
        echo 1 | run_tee /sys/devices/evdi/add >/dev/null 2>&1 || true
    fi
}
