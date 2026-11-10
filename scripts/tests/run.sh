#!/usr/bin/env bash
# Installer test suite: runs anywhere with bash (no root, no network, nothing outside a sandbox).
#
#   scripts/tests/run.sh            all tests
#   scripts/tests/run.sh ubuntu     only tests whose name contains "ubuntu"
#
# Three layers:
#   1. platform.sh as pure functions, against os-release fixtures for every supported family
#   2. the installer in --dry-run / --plan mode (must change nothing)
#   3. real installer runs in a sandbox HOME with fake `sudo`, `apt-get`, `dnf`, ... that only
#      record what they were asked to do, so every branch (broken packages, offline, root, ...)
#      can be driven and checked on any machine, including CI containers of other distributions.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS="$(dirname "$HERE")"
REPO="$(dirname "$SCRIPTS")"
FIX="$HERE/fixtures"
FILTER="${1:-}"

PASS=0; FAIL=0; FAILED=()
ROOT_SANDBOX="$(mktemp -d /tmp/w11c-sandbox.XXXXXX)"
# Removes the test sandbox on exit; never a path outside /tmp/w11c-sandbox.* and never a mount point.
cleanup() {
    # Only ever remove our own sandbox, and never anything that is a mount point.
    case "$ROOT_SANDBOX" in /tmp/w11c-sandbox.*) ;; *) return 0 ;; esac
    if findmnt -rn "$ROOT_SANDBOX" >/dev/null 2>&1; then echo "refusing to remove a mount point: $ROOT_SANDBOX" >&2; return 0; fi
    rm -rf -- "$ROOT_SANDBOX"
}
trap cleanup EXIT

# ---- assertions ----------------------------------------------------------------------------------
CUR=""
# Records a failed assertion for the current test.
fail() { echo "    FAIL: $*"; CUR_FAILED=1; }
# Asserts that two strings are equal.
eq()       { [ "$1" = "$2" ] || fail "expected '$2', got '$1' ${3:+($3)}"; }
# Asserts that [output] contains a string.
has()      { printf '%s' "$1" | grep -qF -- "$2" || fail "output lacks '$2' ${3:+($3)}"; }
# Asserts that [output] does not contain a string.
lacks()    { ! printf '%s' "$1" | grep -qF -- "$2" || fail "output unexpectedly has '$2' ${3:+($3)}"; }
# Asserts that a fake system tool was called with a command line matching a regex.
called()   { grep -qE -- "$1" "$CALLS" || fail "never called: $1"; }
# Asserts that no fake tool call matches a regex.
not_called() { ! grep -qE -- "$1" "$CALLS" || fail "should not have called: $1"; }
# Asserts that the first call matching A comes before the first call matching B.
order()    { # order A B: the first match of A comes before the first match of B
    local a b; a="$(grep -nE -- "$1" "$CALLS" | head -1 | cut -d: -f1)"; b="$(grep -nE -- "$2" "$CALLS" | head -1 | cut -d: -f1)"
    { [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ]; } || fail "expected '$1' before '$2'"
}

# Runs one test in a subshell, prints ok or FAIL with its output, and counts it.
run_test() {
    local name="$1"
    if [ -n "$FILTER" ] && [[ "$name" != *"$FILTER"* ]]; then return 0; fi
    # shellcheck disable=SC2034  # read by fail() in the test subshell
    CUR="$name"; CUR_FAILED=0
    ( "$name" ) 2>&1 | sed 's/^/    /' >"$ROOT_SANDBOX/out.$$"
    local rc=${PIPESTATUS[0]}
    if grep -q "FAIL:" "$ROOT_SANDBOX/out.$$" || [ "$rc" != 0 ]; then
        FAIL=$((FAIL+1)); FAILED+=("$name"); echo "FAIL  $name"; cat "$ROOT_SANDBOX/out.$$"
    else
        PASS=$((PASS+1)); echo "ok    $name"
    fi
}

# ---- platform helpers ----------------------------------------------------------------------------
# shellcheck source=../lib/platform.sh
. "$SCRIPTS/lib/platform.sh"
# Points the platform functions at the os-release fixture of the named distribution.
with_release() { US_OS_RELEASE="$FIX/os-release.$1"; export US_OS_RELEASE; }

# ---- sandbox: a project copy, an empty HOME, fake system tools -------------------------------------
# mk_sandbox <fixture> [desktop] [session]: echoes nothing, sets SB, HOME, CALLS, PATH and the env.
mk_sandbox() {
    SB="$(mktemp -d "$ROOT_SANDBOX/sb.XXXXXX")"
    mkdir -p "$SB/project/target/release" "$SB/home" "$SB/bin" "$SB/project/scripts" "$SB/project/packaging" "$SB/project/extensions"
    cp -r "$SCRIPTS"/install.sh "$SCRIPTS"/lib "$SCRIPTS"/uscreen.desktop "$SCRIPTS"/uscreen.service "$SB/project/scripts/"
    cp "$REPO"/packaging/*.rules "$SB/project/packaging/" 2>/dev/null || true
    cp -r "$REPO/extensions/uscreen-app-lock" "$SB/project/extensions/"
    for b in uscreen uscreen-pro-gui evdi_helper uscreen-menu; do printf '#!/bin/sh\nexit 0\n' >"$SB/project/target/release/$b"; chmod +x "$SB/project/target/release/$b"; done
    HOME="$SB/home"; export HOME
    XDG_DATA_HOME="$SB/home/.local/share"; export XDG_DATA_HOME
    CALLS="$SB/calls.log"; : >"$CALLS"; export CALLS
    export US_OS_RELEASE="$FIX/os-release.$1"
    export XDG_CURRENT_DESKTOP="${2:-ubuntu:GNOME}" XDG_SESSION_TYPE="${3:-wayland}"
    export UNAME_R="7.0.0-42-generic" US_UID=1000
    unset US_OSTREE US_EXT_CHANGED || true
    shim_defaults
    hermetic_tools
    PATH="$SB/bin:$SB/basic"; export PATH
}
# Only the basic tools plus the fakes are on PATH, so a real spawn-at, dkms, whiptail ... on the
# machine running the tests can never leak in, and "tool missing" is just "no shim".
hermetic_tools() {
    mkdir -p "$SB/basic"
    local t p
    for t in echo env bash sh sed grep cat tr head cp rm mkdir id dirname tee xargs find sort touch chmod uname printf ls cut tail mktemp awk true false sleep date wc readlink ln script rmdir basename; do
        # `command -v` answers with the bare name for shell builtins: look for the real file instead.
        p="$(PATH="$ORIG_PATH" type -P "$t" 2>/dev/null)" && ln -sf "$p" "$SB/basic/$t"
    done
}
ORIG_PATH="$PATH"

# shim NAME 'shell body' : a fake command that logs its call and then runs BODY.
shim() { printf '#!/bin/sh\necho "%s $*" >>"$CALLS"\n%s\n' "$1" "${2:-exit 0}" >"$SB/bin/$1"; chmod +x "$SB/bin/$1"; }
# Installs the default fake tools: sudo records and runs nothing, the package managers and system tools succeed and log.
shim_defaults() {
    # sudo records the whole command and runs nothing: nothing outside the sandbox is ever touched.
    printf '#!/bin/sh\necho "sudo $*" >>"$CALLS"\nif [ "$1" = "-n" ]; then exit "${SUDO_N_RC:-0}"; fi\nif [ -n "${SUDO_FAIL:-}" ] && echo "$*" | grep -qE "$SUDO_FAIL"; then exit 1; fi\nexit 0\n' >"$SB/bin/sudo"; chmod +x "$SB/bin/sudo"
    for t in apt-get dnf pacman zypper rpm-ostree systemctl udevadm glib-compile-schemas gtk-update-icon-cache make yay paru git dkms; do shim "$t"; done
    shim modprobe; shim lsmod 'exit 0'; shim ldconfig 'echo "libgtk-4.so.1 (libc6,x86-64) => /usr/lib/libgtk-4.so.1"'
    shim modinfo 'exit "${MODINFO_RC:-0}"'
    shim apt-cache 'case " $* " in *" show "*) for c in $PREBUILT; do [ "$c" = "$2" ] && exit 0; done; exit 100;; esac; exit 0'
    shim dpkg-query 'echo "${EVDI_DKMS_STATE:-}"'
    shim dpkg 'exit 0'
    shim curl 'echo "exit 0"'
    shim spawn-at 'exit 0'; rm -f "$SB/bin/spawn-at"     # absent unless a test adds it
    shim ffmpeg; shim adb
}

# Runs the sandboxed copy of install.sh with the given options and returns its combined output.
run_install() { ( cd "$SB/project" && ./scripts/install.sh "$@" 2>&1 ); }

# ================================ layer 1: platform detection ====================================
t_family_for_every_fixture() {
    local pair f want
    for pair in ubuntu-24.04:debian ubuntu-22.04:debian debian-12:debian linuxmint:debian pop:debian \
                fedora-41:fedora bazzite:fedora nobara:fedora rocky:fedora arch:arch manjaro:arch endeavouros:arch \
                opensuse-tumbleweed:suse opensuse-leap:suse nixos:unknown empty:unknown; do
        f="${pair%%:*}"; want="${pair##*:}"; with_release "$f"
        eq "$(us_distro_family)" "$want" "$f"
    done
}
# A missing os-release file gives the unknown family.
t_family_missing_os_release_is_unknown() { US_OS_RELEASE=/nonexistent; export US_OS_RELEASE; eq "$(us_distro_family)" unknown; }
# Each fixture gets the expected EVDI method; Bazzite and Nobara are bundled.
t_evdi_strategy_per_distro() {
    local pair f want
    for pair in ubuntu-24.04:prebuilt ubuntu-22.04:prebuilt debian-12:dkms linuxmint:dkms pop:dkms fedora-41:source rocky:source \
                bazzite:bundled nobara:bundled arch:dkms manjaro:dkms opensuse-leap:package opensuse-tumbleweed:package nixos:source; do
        f="${pair%%:*}"; want="${pair##*:}"; with_release "$f"; eq "$(us_evdi_strategy)" "$want" "$f"
    done
}
# adb is `adb` on Debian-likes and `android-tools` elsewhere; an unknown system has no list.
t_core_packages_use_the_right_adb_name() {
    with_release ubuntu-24.04; has "$(us_core_packages)" "adb"; has "$(us_core_packages)" "libevdi1"
    with_release fedora-41;    has "$(us_core_packages)" "android-tools"
    with_release arch;         has "$(us_core_packages)" "android-tools"
    with_release nixos;        eq "$(us_core_packages)" ""
}
# The prebuilt EVDI package names are derived from the running kernel, release and flavour (generic, lowlatency).
t_ubuntu_prebuilt_candidates_follow_the_running_kernel() {
    with_release ubuntu-24.04; UNAME_R=7.0.0-42-generic
    local c; c="$(debian_evdi_prebuilt_candidates)"
    has "$c" "linux-main-modules-evdi-7.0.0-42-generic"
    has "$c" "linux-modules-evdi-generic-hwe-24.04"
    UNAME_R=6.8.0-45-lowlatency; c="$(debian_evdi_prebuilt_candidates)"
    has "$c" "linux-main-modules-evdi-6.8.0-45-lowlatency"
    eq "$(us_kernel_flavour)" lowlatency
}
# ---- distros/<family>.sh: one file per family, one interface -----------------------------------------
# The functions every family file must define (see scripts/lib/distros/_template.sh).
DISTRO_INTERFACE="order ids core_packages evdi_strategy install_deps install_gtk evdi_install"
# Every distros/<family>.sh defines all the functions of the interface, so a half-written family fails here and not on a user's machine.
t_every_family_file_implements_the_whole_interface() {
    local f fam fn n=0
    for f in "$SCRIPTS"/lib/distros/[a-z]*.sh; do
        fam="$(basename "$f" .sh)"; n=$((n+1))
        for fn in $DISTRO_INTERFACE; do
            declare -F "${fam}_${fn}" >/dev/null || fail "$fam.sh does not define ${fam}_${fn}"
        done
    done
    [ "$n" -ge 5 ] || fail "expected debian, fedora, arch, suse and unknown, found $n family files"
}
# _template.sh lists the same functions as the real families and is not sourced.
t_the_template_defines_the_same_interface_and_is_not_loaded() {
    local fn
    for fn in $DISTRO_INTERFACE; do grep -q "^FAMILY_${fn}()" "$SCRIPTS/lib/distros/_template.sh" || fail "_template.sh lacks FAMILY_${fn}"; done
    ! declare -F FAMILY_ids >/dev/null || fail "the template must not be sourced"
}
# Families are tried in the order of their <family>_order values, and unknown is the fallback, never matched.
t_family_order_is_debian_fedora_arch_suse_and_unknown_is_never_asked() {
    eq "$(us_distro_families | tr '\n' ' ')" "debian fedora arch suse "
}
# No os-release ID belongs to two families, or the first one tried would win silently.
t_each_family_claims_only_its_own_ids() {
    # No os-release ID may belong to two families, or the first one tried would silently win.
    local fam tok seen=" "
    for fam in $(us_distro_families); do
        for tok in $("${fam}_ids"); do
            case "$seen" in *" $tok "*) fail "id '$tok' is claimed twice" ;; esac
            seen="$seen$tok "
        done
    done
}
# Every real family lists core packages; unknown lists none.
t_each_family_names_its_packages() {
    local fam
    for fam in debian fedora arch suse; do [ -n "$("${fam}_core_packages")" ] || fail "$fam has no core packages"; done
    eq "$(unknown_core_packages)" ""
}
# Every family returns one of prebuilt, dkms, package, source or bundled.
t_each_strategy_is_one_of_the_known_ones() {
    local fam s
    for fam in debian fedora arch suse unknown; do
        s="$("${fam}_evdi_strategy")"
        case "$s" in prebuilt|dkms|package|source|bundled) ;; *) fail "$fam: unknown strategy '$s'" ;; esac
    done
}
# XDG_CURRENT_DESKTOP and XDG_SESSION_TYPE are classified as gnome, kde, other, none and wayland, x11, unknown.
t_desktop_and_session_detection() {
    local d s
    for pair in "ubuntu:GNOME:gnome" "GNOME:gnome" "KDE:kde" "X-Cinnamon:other" "sway:other" ":none"; do
        d="${pair%:*}"; XDG_CURRENT_DESKTOP="$d"; eq "$(us_desktop)" "${pair##*:}" "$pair"
    done
    XDG_SESSION_TYPE=Wayland; eq "$(us_session)" wayland
    XDG_SESSION_TYPE=x11; eq "$(us_session)" x11
    XDG_SESSION_TYPE="tty"; eq "$(us_session)" unknown
    unset XDG_SESSION_TYPE; eq "$(us_session)" unknown
}
# US_OSTREE=1 or 0 overrides the image-based check.
t_immutable_detection_is_overridable() {
    US_OSTREE=1; us_immutable || fail "US_OSTREE=1 must be immutable"
    US_OSTREE=0; ! us_immutable || fail "US_OSTREE=0 must not be"
}
# The restart rules over every combination of changed extension, EVDI reboot, layered packages, desktop and session.
t_restart_matrix() {
    local cases=(
        # ext_changed evdi_reboot layered desktop session -> expected
        "1 0 0 ubuntu:GNOME wayland relogin=yes reboot=no"
        "1 0 0 ubuntu:GNOME x11     relogin=no  reboot=no"
        "0 0 0 ubuntu:GNOME wayland relogin=no  reboot=no"
        "1 0 0 KDE          wayland relogin=no  reboot=no"
        "1 0 0 X-Cinnamon   x11     relogin=no  reboot=no"
        "1 0 0 ''           ''      relogin=no  reboot=no"
        "0 1 0 KDE          wayland relogin=no  reboot=yes"
        "0 0 1 GNOME        wayland relogin=no  reboot=yes"
        "1 1 0 ubuntu:GNOME wayland relogin=yes reboot=yes"
        "1 0 1 ubuntu:GNOME wayland relogin=yes reboot=yes"
    )
    local c ext evdi lay desk sess want
    for c in "${cases[@]}"; do
        eval "set -- $c"; ext=$1; evdi=$2; lay=$3; desk=$4; sess=$5; shift 5; want="$*"
        got="$(US_EXT_CHANGED=$ext US_EVDI_NEEDS_REBOOT=$evdi US_PKG_LAYERED=$lay XDG_CURRENT_DESKTOP=$desk XDG_SESSION_TYPE=$sess us_restart_needs)"
        eq "$got" "$want" "$c"
    done
}
# The restart text names the real reason: a log out for the Shell extension, layering, or the module in use.
t_restart_explanations_name_the_reason() {
    local e
    e="$(US_EXT_CHANGED=1 XDG_CURRENT_DESKTOP=GNOME XDG_SESSION_TYPE=wayland us_restart_explain)"; has "$e" "Log out"; lacks "$e" "Reboot"
    e="$(US_PKG_LAYERED=1 XDG_CURRENT_DESKTOP=KDE XDG_SESSION_TYPE=wayland us_restart_explain)"; has "$e" "layers packages"
    e="$(US_EVDI_NEEDS_REBOOT=1 XDG_CURRENT_DESKTOP=KDE us_restart_explain)"; has "$e" "EVDI"
    e="$(XDG_CURRENT_DESKTOP=KDE us_restart_explain)"; has "$e" "Nothing to restart"
}

# ================================ layer 2: dry run and plan ======================================
t_plan_prints_the_system() {
    mk_sandbox ubuntu-24.04
    local o; o="$(run_install --plan)"
    has "$o" "family:        debian"; has "$o" "evdi method:   prebuilt"; has "$o" "gnome wayland: yes"
}
# --plan works on every distribution fixture.
t_plan_on_every_fixture_exits_zero() {
    local f
    for f in "$FIX"/os-release.*; do
        mk_sandbox "${f##*os-release.}"
        run_install --plan >/dev/null || fail "plan failed for ${f##*.}"
    done
}
# A dry run leaves the home directory byte for byte as it was and never calls sudo.
t_dry_run_changes_nothing() {
    mk_sandbox ubuntu-24.04
    local before after; before="$(find "$SB/home" | sort)"
    run_install --headless --dry-run >/dev/null; local rc=$?
    after="$(find "$SB/home" | sort)"
    eq "$rc" 0; eq "$after" "$before" "dry run wrote files"
    # system commands are printed, not executed: the fake sudo was never invoked
    not_called '^sudo '
}
# A dry run prints the system commands it would run.
t_dry_run_prints_the_commands() {
    mk_sandbox fedora-41; local o; o="$(run_install --headless --dry-run)"
    has "$o" "+ sudo dnf install -y --allowerasing ffmpeg android-tools"
}
# An unknown option exits with status 64.
t_unknown_option_exits_64() { mk_sandbox ubuntu-24.04; run_install --bogus >/dev/null; eq $? 64; }
# --help exits 0 and lists the options.
t_help_exits_zero() { mk_sandbox ubuntu-24.04; local o; o="$(run_install --help)"; eq $? 0; has "$o" "--headless"; }
# --skip-<part> removes that part from the run.
t_skip_flags_remove_parts() {
    mk_sandbox ubuntu-24.04
    local o; o="$(run_install --headless --dry-run --skip-system --skip-extension --skip-spawnat --skip-menu)"
    lacks "$o" "EVDI"; lacks "$o" "extension"; has "$o" "Binaries installed"
}

# ================================ layer 3: real runs with fake system tools ===========================
t_headless_install_copies_files_into_home() {
    mk_sandbox ubuntu-24.04
    run_install --headless --skip-system --skip-spawnat >/dev/null; eq $? 0
    [ -x "$HOME/.local/bin/uscreen" ] || fail "binary missing"
    [ -f "$HOME/.local/share/applications/uscreen-pro.desktop" ] || fail "desktop entry missing"
    grep -q "Exec=$HOME/.local/bin/uscreen-pro-gui" "$HOME/.local/share/applications/uscreen-pro.desktop" || fail "Exec path not absolute"
    [ -f "$HOME/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/extension.js" ] || fail "extension missing"
}
# The extension is a set of ES modules: extension.js imports lib/ and compat/, so a copy that lacks them does not load.
t_extension_modules_are_installed_and_stale_ones_removed() {
    mk_sandbox ubuntu-24.04
    local ext="$HOME/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app"
    mkdir -p "$ext/lib"; echo "old" >"$ext/lib/removed-module.js"
    run_install --headless --skip-system --skip-spawnat >/dev/null; eq $? 0
    local f
    for f in lib/appLock.js lib/penRouting.js lib/contextProbe.js lib/penMenu.js lib/dataFiles.js compat/index.js compat/versions.js compat/monitors.js; do
        [ -f "$ext/$f" ] || fail "extension module missing: $f"
    done
    [ ! -e "$ext/lib/removed-module.js" ] || fail "a module of an older version was left behind"
    [ ! -e "$ext/tests" ] || fail "the extension's tests were installed"
}
# An old uscreen.desktop from earlier versions is removed so the app does not appear twice.
t_old_duplicate_desktop_entry_is_removed() {
    mk_sandbox ubuntu-24.04; mkdir -p "$HOME/.local/share/applications"; echo "[Desktop Entry]" >"$HOME/.local/share/applications/uscreen.desktop"
    run_install --headless --skip-system --skip-spawnat >/dev/null
    [ ! -e "$HOME/.local/share/applications/uscreen.desktop" ] || fail "old uscreen.desktop still there"
}
# Without a built binary the install fails with a message naming it.
t_missing_binaries_fail_clearly() {
    mk_sandbox ubuntu-24.04; rm -f "$SB/project/target/release/uscreen"
    local o; o="$(run_install --headless --skip-system --skip-spawnat --no-build)"; local rc=$?
    [ "$rc" != 0 ] || fail "must fail without a built binary"; has "$o" "uscreen is missing"
}
# A newly installed extension on GNOME Wayland asks for a log out, in the result line and in install-result.json.
t_result_gnome_wayland_new_extension_needs_relogin() {
    mk_sandbox ubuntu-24.04 ubuntu:GNOME wayland
    local o; o="$(run_install --headless --skip-system --skip-spawnat)"
    has "$o" "UScreen-INSTALL-RESULT: relogin=yes reboot=no"
    has "$(cat "$HOME/.local/share/uscreen/install-result.json")" '"relogin":true'
}
# Installing the same version again asks for no restart.
t_result_second_run_needs_nothing() {
    mk_sandbox ubuntu-24.04 ubuntu:GNOME wayland
    run_install --headless --skip-system --skip-spawnat >/dev/null
    local o; o="$(run_install --headless --skip-system --skip-spawnat)"
    has "$o" "relogin=no reboot=no" "reinstalling the same version must not ask for a restart"
}
# A higher extension version asks for a log out again.
t_result_extension_version_bump_needs_relogin_again() {
    mk_sandbox ubuntu-24.04 ubuntu:GNOME wayland
    run_install --headless --skip-system --skip-spawnat >/dev/null
    sed -i 's/"version": *[0-9]*/"version": 999/' "$SB/project/extensions/uscreen-app-lock/metadata.json"
    local o; o="$(run_install --headless --skip-system --skip-spawnat)"
    has "$o" "relogin=yes"
}
# KDE never needs a log out.
t_result_kde_never_needs_relogin() {
    mk_sandbox fedora-41 KDE wayland
    local o; o="$(run_install --headless --skip-system --skip-spawnat)"
    has "$o" "relogin=no reboot=no"
}
# GNOME on X11 never needs a log out.
t_result_gnome_x11_never_needs_relogin() {
    mk_sandbox ubuntu-24.04 ubuntu:GNOME x11
    local o; o="$(run_install --headless --skip-system --skip-spawnat)"
    has "$o" "relogin=no"
}
# On an image-based Fedora packages are layered with rpm-ostree, dnf is not used, and a reboot is requested.
t_immutable_fedora_layers_and_needs_reboot() {
    mk_sandbox fedora-41 GNOME wayland; export US_OSTREE=1
    local o; o="$(run_install --headless --skip-system --skip-spawnat --skip-menu)"
    called 'rpm-ostree install --idempotent'; not_called 'dnf install'
    has "$o" "reboot=yes"
}
# Fedora installs with dnf --allowerasing so RPM Fusion's ffmpeg replaces ffmpeg-free.
t_fedora_uses_dnf_with_allowerasing() {
    mk_sandbox fedora-41 KDE wayland; export US_OSTREE=0
    run_install --headless --skip-system --skip-spawnat --skip-menu >/dev/null
    called 'dnf install -y --allowerasing ffmpeg android-tools'
}
# Arch installs with pacman.
t_arch_uses_pacman() {
    mk_sandbox arch KDE wayland
    run_install --headless --skip-system --skip-spawnat --skip-menu >/dev/null
    called 'pacman -S --needed --noconfirm ffmpeg android-tools'
}
# openSUSE installs with zypper.
t_suse_uses_zypper() {
    mk_sandbox opensuse-tumbleweed KDE wayland
    run_install --headless --skip-system --skip-spawnat --skip-menu >/dev/null
    called 'zypper --non-interactive install --no-recommends ffmpeg android-tools'
}
# An unknown distribution gets a warning and still gets UScreen's own files.
t_unknown_distro_warns_and_still_installs_files() {
    mk_sandbox nixos KDE wayland
    local o; o="$(run_install --headless --skip-system --skip-spawnat --skip-menu)"; eq $? 0
    has "$o" "Unknown distribution"; [ -x "$HOME/.local/bin/uscreen" ] || fail "files not installed"
}
# Running as root never calls sudo.
t_root_does_not_use_sudo() {
    mk_sandbox ubuntu-24.04; export US_UID=0
    run_install --headless --skip-spawnat >/dev/null
    not_called '^sudo '; called '^apt-get install'
}
# A headless install without passwordless sudo exits 2 and says so.
t_non_root_without_passwordless_sudo_fails_headless() {
    mk_sandbox ubuntu-24.04; export SUDO_N_RC=1
    local o; o="$(run_install --headless --skip-spawnat)"; local rc=$?
    eq "$rc" 2; has "$o" "passwordless sudo"
}
# A non-root install on a system without sudo exits 2 and says so.
t_non_root_without_sudo_binary_fails() {
    mk_sandbox ubuntu-24.04; rm -f "$SB/bin/sudo"
    local o; o="$(run_install --headless --skip-spawnat)"; local rc=$?
    eq "$rc" 2; has "$o" "run as root or install sudo"
}

# ---- EVDI ---------------------------------------------------------------------------------------
t_evdi_present_installs_nothing() {
    mk_sandbox ubuntu-24.04; export MODINFO_RC=0
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    not_called 'apt-get install -y linux-'; not_called 'apt-get install -y evdi-dkms'; not_called 'git clone'
}
# On Ubuntu a missing module is installed from the prebuilt package for the running kernel, not from dkms.
t_ubuntu_missing_module_uses_the_prebuilt_package() {
    mk_sandbox ubuntu-24.04; export MODINFO_RC=1 PREBUILT="linux-main-modules-evdi-7.0.0-42-generic"
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    called 'apt-get install -y linux-main-modules-evdi-7.0.0-42-generic'; not_called 'apt-get install -y evdi-dkms'
}
# If the exact-kernel package does not exist the HWE package for the release is used.
t_ubuntu_prefers_the_hwe_metapackage_when_the_exact_one_is_absent() {
    mk_sandbox ubuntu-24.04; export MODINFO_RC=1 PREBUILT="linux-modules-evdi-generic-hwe-24.04"
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    called 'apt-get install -y linux-modules-evdi-generic-hwe-24.04'
}
# A half-installed evdi-dkms package is purged first so apt keeps working.
t_half_installed_dkms_package_is_removed_first() {
    mk_sandbox ubuntu-24.04; export MODINFO_RC=0 EVDI_DKMS_STATE="iF "
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    called 'dpkg --purge --force-remove-reinstreq evdi-dkms'
}
# If dkms already has a built module, a half-configured package is not purged.
t_half_installed_dkms_with_a_built_module_is_left_alone() {
    mk_sandbox ubuntu-24.04; export MODINFO_RC=0 EVDI_DKMS_STATE="iF "
    shim dkms 'echo "evdi/1.14.2, 7.0.0-42-generic, x86_64: installed"'
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    not_called 'dpkg --purge'
}
# After a failed distribution build the broken package is purged, then the newest upstream release is cloned and built, in that order.
t_failed_dkms_build_is_cleaned_up_and_upstream_source_is_tried() {
    mk_sandbox debian-12 GNOME wayland; export MODINFO_RC=1 EVDI_DKMS_STATE="iF "
    shim git 'case "$*" in *ls-remote*) printf "abc\trefs/tags/v1.14.9\nabc\trefs/tags/v1.14.16\n";; esac'
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    called 'dpkg --purge --force-remove-reinstreq evdi-dkms'
    called 'git clone --depth 1 --branch v1.14.16'; called 'dkms build -m evdi -v 1.14.16'
    order 'dpkg --purge' 'git clone'
}
# The newest upstream tag is chosen by version (1.14.16 beats 1.9.0) and release candidates are ignored.
t_latest_upstream_tag_is_sorted_by_version_not_text() {
    mk_sandbox debian-12; . "$SB/project/scripts/lib/evdi.sh"
    shim git 'printf "a\trefs/tags/v1.9.0\na\trefs/tags/v1.14.2\na\trefs/tags/v1.14.16\na\trefs/tags/v1.14.4-rc1\n"'
    eq "$(evdi_latest_tag)" "v1.14.16"
}
# Building from source without dkms warns but does not abort the install.
t_source_build_without_dkms_fails_softly() {
    mk_sandbox fedora-41 KDE wayland; export MODINFO_RC=1 US_OSTREE=0
    rm -f "$SB/bin/dkms"
    local o; o="$(run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat)"; eq $? 0 "a missing EVDI must not abort the install"
    has "$o" "dkms is needed"
}
# On Arch without yay or paru the message says the package is only in the AUR.
t_arch_without_an_aur_helper_explains_the_aur() {
    mk_sandbox arch KDE wayland; export MODINFO_RC=1; rm -f "$SB/bin/yay" "$SB/bin/paru"
    shim pacman 'exit 1'
    local o; o="$(run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat)"
    has "$o" "only in the AUR"
}
# Bazzite never builds or installs EVDI and tells the user the image should ship it.
t_bazzite_never_installs_evdi() {
    mk_sandbox bazzite KDE wayland; export MODINFO_RC=1 US_OSTREE=1
    local o; o="$(run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat)"
    not_called 'git clone'; has "$o" "should already ship EVDI"
}
# If the module is loaded and cannot be unloaded, a reboot is requested.
t_module_in_use_needs_a_reboot() {
    mk_sandbox ubuntu-24.04 KDE wayland; export MODINFO_RC=0
    shim lsmod 'echo "evdi 90112 2"'; export SUDO_FAIL='^modprobe -r'
    local o; o="$(run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat)"
    has "$o" "reboot=yes"
}
# If the module unloads and reloads, no reboot is requested.
t_module_that_unloads_cleanly_needs_no_reboot() {
    mk_sandbox ubuntu-24.04 KDE wayland; export MODINFO_RC=0
    shim lsmod 'echo "evdi 90112 0"'
    local o; o="$(run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat)"
    has "$o" "reboot=no"
}
# The modprobe configuration that creates two EVDI devices is written.
t_modprobe_config_is_written_with_two_devices() {
    mk_sandbox ubuntu-24.04; export MODINFO_RC=0
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    called 'tee /etc/modprobe.d/uscreen-evdi.conf'
}

# ---- spawn-at ---------------------------------------------------------------------------------------
t_spawn_at_dev_build_is_left_alone() {
    mk_sandbox ubuntu-24.04; shim spawn-at 'echo "spawn-at DEV BUILD 0.2.4"'
    run_install --headless --skip-core --skip-system --skip-extension --skip-menu >/dev/null
    not_called 'spawn-at update'; not_called 'curl'
}
# A released spawn-at is updated.
t_spawn_at_release_build_is_updated() {
    mk_sandbox ubuntu-24.04; shim spawn-at 'echo "spawn-at 0.2.3"'
    run_install --headless --skip-core --skip-system --skip-extension --skip-menu >/dev/null
    called 'spawn-at update --headless'
}
# A missing spawn-at is installed from the project, and on GNOME Wayland that asks for a log out.
t_spawn_at_missing_is_installed_and_needs_relogin_on_gnome() {
    mk_sandbox ubuntu-24.04 ubuntu:GNOME wayland
    local o; o="$(run_install --headless --skip-core --skip-system --skip-extension --skip-menu)"
    called 'curl -fsSL https://raw.githubusercontent.com/harshp2008/spawn-at'; has "$o" "relogin=yes"
}
# If the download fails the install still succeeds and says so.
t_spawn_at_offline_does_not_abort() {
    mk_sandbox ubuntu-24.04; shim curl 'exit 6'
    local o; o="$(run_install --headless --skip-core --skip-system --skip-extension --skip-menu)"; eq $? 0
    has "$o" "did not install"
}
# Without curl spawn-at is skipped with a message.
t_spawn_at_without_curl_is_skipped() {
    mk_sandbox ubuntu-24.04; rm -f "$SB/bin/curl"
    local o; o="$(run_install --headless --skip-core --skip-system --skip-extension --skip-menu)"; eq $? 0
    has "$o" "curl is missing"
}

# ---- text interface ----------------------------------------------------------------------------------
t_tui_component_choice_is_honoured() {
    mk_sandbox ubuntu-24.04
    shim whiptail 'echo "\"core\" \"extension\"" >&2'    # whiptail answers on stderr
    # tui needs a terminal: script(1) supplies one
    if ! command -v script >/dev/null 2>&1; then echo "    (skipped: no script(1))"; return 0; fi
    ( cd "$SB/project" && script -qec "./scripts/install.sh --tui --skip-system" /dev/null ) >"$SB/tui.out" 2>&1
    [ -f "$HOME/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/extension.js" ] || fail "chosen part (extension) not installed"
    not_called '^spawn-at '; not_called '^curl'
}
# Cancelling the text interface copies nothing.
t_tui_cancel_installs_nothing() {
    mk_sandbox ubuntu-24.04
    shim whiptail 'exit 1'
    if ! command -v script >/dev/null 2>&1; then echo "    (skipped: no script(1))"; return 0; fi
    ( cd "$SB/project" && script -qec "./scripts/install.sh --tui" /dev/null ) >"$SB/tui.out" 2>&1
    [ ! -e "$HOME/.local/bin/uscreen" ] || fail "cancelled install must not copy files"
}
# Without a terminal the installer runs headless.
t_no_terminal_picks_headless() {
    mk_sandbox ubuntu-24.04
    local o; o="$(run_install --dry-run </dev/null)"; has "$o" "(headless"
}

# ================================ run =================================================================
for fn in $(declare -F | awk '{print $3}' | grep '^t_'); do run_test "$fn"; done
echo
echo "passed: $PASS  failed: $FAIL"
[ "$FAIL" = 0 ] || { printf 'failed: %s\n' "${FAILED[@]}"; exit 1; }
