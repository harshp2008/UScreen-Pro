# shellcheck shell=bash
# UScreen installer: what system is this, and what does it need?
#
# Pure functions. Every input comes from a variable, so the tests can pretend to be any system:
#   US_OS_RELEASE         path of the os-release file            (default /etc/os-release)
#   UNAME_R               kernel release                          (default `uname -r`)
#   XDG_CURRENT_DESKTOP   desktop name(s), e.g. ubuntu:GNOME
#   XDG_SESSION_TYPE      wayland | x11
#   US_OSTREE             1 when the system is image based (rpm-ostree); default: /run/ostree-booted exists
# Nothing in this file changes the system.
#
# Everything that differs between distribution families lives in distros/<family>.sh (one file each, all
# functions prefixed with the family name). This file finds the family and calls its functions.

US_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Files starting with `_` (the template) are not families.
for _us_family_file in "$US_LIB_DIR"/distros/[a-z]*.sh; do
    # shellcheck disable=SC1090
    . "$_us_family_file"
done
unset _us_family_file

# The family names, in the order they are tried against os-release (each file's <family>_order, then its name).
us_distro_families() {
    local f fam
    for f in "$US_LIB_DIR"/distros/[a-z]*.sh; do
        fam="$(basename "$f" .sh)"
        [ "$fam" = unknown ] && continue
        printf '%s %s\n' "$("${fam}_order")" "$fam"
    done | sort -n -k1,1 -k2,2 | cut -d' ' -f2
}

# Read one KEY from the os-release file ("" when missing).
us_os_value() {
    local key="$1" file="${US_OS_RELEASE:-/etc/os-release}"
    [ -r "$file" ] || return 0
    # shellcheck disable=SC2016
    (. "$file" 2>/dev/null; eval "printf '%s' \"\${$key:-}\"")
}

# The family of this system: the name of a file in distros/, or `unknown`. os-release ID first, then ID_LIKE.
us_distro_family() {
    local id like fam tok
    id="$(us_os_value ID)"
    like="$(us_os_value ID_LIKE)"
    for fam in $(us_distro_families); do
        for tok in $("${fam}_ids"); do
            case " $id $like " in *" $tok "*) echo "$fam"; return ;; esac
        done
    done
    echo unknown
}

# 0 (true) when the root file system is image based, so packages are layered and need a reboot.
us_immutable() {
    case "${US_OSTREE:-}" in
        1) return 0 ;;
        0) return 1 ;;
    esac
    [ -e /run/ostree-booted ]
}

# gnome | kde | other | none
us_desktop() {
    local d
    d="$(printf '%s' "${XDG_CURRENT_DESKTOP:-}" | tr '[:upper:]' '[:lower:]')"
    case "$d" in
        *gnome*|*unity*) echo gnome ;;
        *kde*|*plasma*) echo kde ;;
        "") echo none ;;
        *) echo other ;;
    esac
}

# wayland | x11 | unknown
us_session() {
    case "$(printf '%s' "${XDG_SESSION_TYPE:-}" | tr '[:upper:]' '[:lower:]')" in
        wayland) echo wayland ;;
        x11) echo x11 ;;
        *) echo unknown ;;
    esac
}

# True on GNOME Shell under Wayland: the one place where the Shell extension cannot be reloaded in
# place, so a new or changed extension needs a log out and back in.
us_gnome_wayland() {
    [ "$(us_desktop)" = gnome ] && [ "$(us_session)" = wayland ]
}

# Kernel release with the flavour split off: "7.0.0-42-generic" -> "generic".
us_kernel_flavour() {
    local k="${UNAME_R:-$(uname -r)}"
    printf '%s' "${k##*-}"
}

# The packages the daemon needs, per family (names checked against real images; see docs/installer.md).
# Echoes space separated names. `adb` is `android-tools` on everything but Debian-likes.
us_core_packages() { "$(us_distro_family)_core_packages"; }

# How a missing EVDI kernel module is obtained on this family:
#   prebuilt  the distro ships a signed module for its kernel (Ubuntu)
#   dkms      the distro's dkms package (Debian, Arch via AUR, ...) with upstream source as the fallback
#   package   a plain kernel-module package (openSUSE)
#   source    nothing is packaged: build upstream (Fedora)
#   bundled   the image already has it (Bazzite, Nobara)
us_evdi_strategy() { "$(us_distro_family)_evdi_strategy"; }

# What the user has to do after the install. Reads these flags (set by the install steps):
#   US_EXT_CHANGED=1       the GNOME Shell extension (or spawn-at's) was installed or updated
#   US_EVDI_NEEDS_REBOOT   1 when the module could not be (re)loaded now
#   US_PKG_LAYERED=1       packages were layered on an image-based system
# Prints:  relogin=yes|no reboot=yes|no   (one line, easy to parse)
us_restart_needs() {
    local relogin=no reboot=no
    if [ "${US_EXT_CHANGED:-0}" = 1 ] && us_gnome_wayland; then
        relogin=yes
    fi
    if [ "${US_EVDI_NEEDS_REBOOT:-0}" = 1 ] || [ "${US_PKG_LAYERED:-0}" = 1 ]; then
        reboot=yes
    fi
    echo "relogin=$relogin reboot=$reboot"
}

# Human readable reasons that go with `us_restart_needs`.
us_restart_explain() {
    local needs
    needs="$(us_restart_needs)"
    case "$needs" in *relogin=yes*)
        echo "Log out and back in once: GNOME on Wayland only loads a new Shell extension at login (needed for the pen menu and app locking)." ;;
    esac
    case "$needs" in *reboot=yes*)
        if [ "${US_PKG_LAYERED:-0}" = 1 ]; then
            echo "Reboot once: this system layers packages into the next boot image."
        else
            echo "Reboot once: the EVDI virtual-display module is in use and cannot be reloaded now."
        fi ;;
    esac
    case "$needs" in "relogin=no reboot=no")
        echo "Nothing to restart: you can use UScreen right away." ;;
    esac
}
