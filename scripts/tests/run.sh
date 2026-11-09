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
cleanup() {
    # Only ever remove our own sandbox, and never anything that is a mount point.
    case "$ROOT_SANDBOX" in /tmp/w11c-sandbox.*) ;; *) return 0 ;; esac
    if findmnt -rn "$ROOT_SANDBOX" >/dev/null 2>&1; then echo "refusing to remove a mount point: $ROOT_SANDBOX" >&2; return 0; fi
    rm -rf -- "$ROOT_SANDBOX"
}
trap cleanup EXIT

# ---- assertions ----------------------------------------------------------------------------------
CUR=""
fail() { echo "    FAIL: $*"; CUR_FAILED=1; }
eq()       { [ "$1" = "$2" ] || fail "expected '$2', got '$1' ${3:+($3)}"; }
has()      { printf '%s' "$1" | grep -qF -- "$2" || fail "output lacks '$2' ${3:+($3)}"; }
lacks()    { ! printf '%s' "$1" | grep -qF -- "$2" || fail "output unexpectedly has '$2' ${3:+($3)}"; }
called()   { grep -qE -- "$1" "$CALLS" || fail "never called: $1"; }
not_called() { ! grep -qE -- "$1" "$CALLS" || fail "should not have called: $1"; }
order()    { # order A B: the first match of A comes before the first match of B
    local a b; a="$(grep -nE -- "$1" "$CALLS" | head -1 | cut -d: -f1)"; b="$(grep -nE -- "$2" "$CALLS" | head -1 | cut -d: -f1)"
    { [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ]; } || fail "expected '$1' before '$2'"
}

run_test() {
    local name="$1"
    if [ -n "$FILTER" ] && [[ "$name" != *"$FILTER"* ]]; then return 0; fi
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
t_family_missing_os_release_is_unknown() { US_OS_RELEASE=/nonexistent; export US_OS_RELEASE; eq "$(us_distro_family)" unknown; }
t_evdi_strategy_per_distro() {
    local pair f want
    for pair in ubuntu-24.04:prebuilt ubuntu-22.04:prebuilt debian-12:dkms linuxmint:dkms pop:dkms fedora-41:source rocky:source \
                bazzite:bundled nobara:bundled arch:dkms manjaro:dkms opensuse-leap:package opensuse-tumbleweed:package nixos:source; do
        f="${pair%%:*}"; want="${pair##*:}"; with_release "$f"; eq "$(us_evdi_strategy)" "$want" "$f"
    done
}
t_core_packages_use_the_right_adb_name() {
    with_release ubuntu-24.04; has "$(us_core_packages)" "adb"; has "$(us_core_packages)" "libevdi1"
    with_release fedora-41;    has "$(us_core_packages)" "android-tools"
    with_release arch;         has "$(us_core_packages)" "android-tools"
    with_release nixos;        eq "$(us_core_packages)" ""
}
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
t_the_template_defines_the_same_interface_and_is_not_loaded() {
    local fn
    for fn in $DISTRO_INTERFACE; do grep -q "^FAMILY_${fn}()" "$SCRIPTS/lib/distros/_template.sh" || fail "_template.sh lacks FAMILY_${fn}"; done
    ! declare -F FAMILY_ids >/dev/null || fail "the template must not be sourced"
}
t_family_order_is_debian_fedora_arch_suse_and_unknown_is_never_asked() {
    eq "$(us_distro_families | tr '\n' ' ')" "debian fedora arch suse "
}
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
t_each_family_names_its_packages() {
    local fam
    for fam in debian fedora arch suse; do [ -n "$("${fam}_core_packages")" ] || fail "$fam has no core packages"; done
    eq "$(unknown_core_packages)" ""
}
t_each_strategy_is_one_of_the_known_ones() {
    local fam s
    for fam in debian fedora arch suse unknown; do
        s="$("${fam}_evdi_strategy")"
        case "$s" in prebuilt|dkms|package|source|bundled) ;; *) fail "$fam: unknown strategy '$s'" ;; esac
    done
}
t_desktop_and_session_detection() {
    local d s
    for pair in "ubuntu:GNOME:gnome" "GNOME:gnome" "KDE:kde" "X-Cinnamon:other" "sway:other" ":none"; do
        d="${pair%:*}"; XDG_CURRENT_DESKTOP="$d"; eq "$(us_desktop)" "${pair##*:}" "$pair"
    done
    XDG_SESSION_TYPE=Wayland; eq "$(us_session)" wayland
    XDG_SESSION_TYPE=x11; eq "$(us_session)" x11
    XDG_SESSION_TYPE=tty; eq "$(us_session)" unknown
    unset XDG_SESSION_TYPE; eq "$(us_session)" unknown
}
t_immutable_detection_is_overridable() {
    US_OSTREE=1; us_immutable || fail "US_OSTREE=1 must be immutable"
    US_OSTREE=0; ! us_immutable || fail "US_OSTREE=0 must not be"
}
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
t_plan_on_every_fixture_exits_zero() {
    local f
    for f in "$FIX"/os-release.*; do
        mk_sandbox "${f##*os-release.}"
        run_install --plan >/dev/null || fail "plan failed for ${f##*.}"
    done
}
t_dry_run_changes_nothing() {
    mk_sandbox ubuntu-24.04
    local before after; before="$(find "$SB/home" | sort)"
    run_install --headless --dry-run >/dev/null; local rc=$?
    after="$(find "$SB/home" | sort)"
    eq "$rc" 0; eq "$after" "$before" "dry run wrote files"
    # system commands are printed, not executed: the fake sudo was never invoked
    not_called '^sudo '
}
t_dry_run_prints_the_commands() {
    mk_sandbox fedora-41; local o; o="$(run_install --headless --dry-run)"
    has "$o" "+ sudo dnf install -y --allowerasing ffmpeg android-tools"
}
t_unknown_option_exits_64() { mk_sandbox ubuntu-24.04; run_install --bogus >/dev/null; eq $? 64; }
t_help_exits_zero() { mk_sandbox ubuntu-24.04; local o; o="$(run_install --help)"; eq $? 0; has "$o" "--headless"; }
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
t_old_duplicate_desktop_entry_is_removed() {
    mk_sandbox ubuntu-24.04; mkdir -p "$HOME/.local/share/applications"; echo "[Desktop Entry]" >"$HOME/.local/share/applications/uscreen.desktop"
    run_install --headless --skip-system --skip-spawnat >/dev/null
    [ ! -e "$HOME/.local/share/applications/uscreen.desktop" ] || fail "old uscreen.desktop still there"
}
t_missing_binaries_fail_clearly() {
    mk_sandbox ubuntu-24.04; rm -f "$SB/project/target/release/uscreen"
    local o; o="$(run_install --headless --skip-system --skip-spawnat --no-build)"; local rc=$?
    [ "$rc" != 0 ] || fail "must fail without a built binary"; has "$o" "uscreen is missing"
}
t_result_gnome_wayland_new_extension_needs_relogin() {
    mk_sandbox ubuntu-24.04 ubuntu:GNOME wayland
    local o; o="$(run_install --headless --skip-system --skip-spawnat)"
    has "$o" "UScreen-INSTALL-RESULT: relogin=yes reboot=no"
    has "$(cat "$HOME/.local/share/uscreen/install-result.json")" '"relogin":true'
}
t_result_second_run_needs_nothing() {
    mk_sandbox ubuntu-24.04 ubuntu:GNOME wayland
    run_install --headless --skip-system --skip-spawnat >/dev/null
    local o; o="$(run_install --headless --skip-system --skip-spawnat)"
    has "$o" "relogin=no reboot=no" "reinstalling the same version must not ask for a restart"
}
t_result_extension_version_bump_needs_relogin_again() {
    mk_sandbox ubuntu-24.04 ubuntu:GNOME wayland
    run_install --headless --skip-system --skip-spawnat >/dev/null
    sed -i 's/"version": *[0-9]*/"version": 999/' "$SB/project/extensions/uscreen-app-lock/metadata.json"
    local o; o="$(run_install --headless --skip-system --skip-spawnat)"
    has "$o" "relogin=yes"
}
t_result_kde_never_needs_relogin() {
    mk_sandbox fedora-41 KDE wayland
    local o; o="$(run_install --headless --skip-system --skip-spawnat)"
    has "$o" "relogin=no reboot=no"
}
t_result_gnome_x11_never_needs_relogin() {
    mk_sandbox ubuntu-24.04 ubuntu:GNOME x11
    local o; o="$(run_install --headless --skip-system --skip-spawnat)"
    has "$o" "relogin=no"
}
t_immutable_fedora_layers_and_needs_reboot() {
    mk_sandbox fedora-41 GNOME wayland; export US_OSTREE=1
    local o; o="$(run_install --headless --skip-system --skip-spawnat --skip-menu)"
    called 'rpm-ostree install --idempotent'; not_called 'dnf install'
    has "$o" "reboot=yes"
}
t_fedora_uses_dnf_with_allowerasing() {
    mk_sandbox fedora-41 KDE wayland; export US_OSTREE=0
    run_install --headless --skip-system --skip-spawnat --skip-menu >/dev/null
    called 'dnf install -y --allowerasing ffmpeg android-tools'
}
t_arch_uses_pacman() {
    mk_sandbox arch KDE wayland
    run_install --headless --skip-system --skip-spawnat --skip-menu >/dev/null
    called 'pacman -S --needed --noconfirm ffmpeg android-tools'
}
t_suse_uses_zypper() {
    mk_sandbox opensuse-tumbleweed KDE wayland
    run_install --headless --skip-system --skip-spawnat --skip-menu >/dev/null
    called 'zypper --non-interactive install --no-recommends ffmpeg android-tools'
}
t_unknown_distro_warns_and_still_installs_files() {
    mk_sandbox nixos KDE wayland
    local o; o="$(run_install --headless --skip-system --skip-spawnat --skip-menu)"; eq $? 0
    has "$o" "Unknown distribution"; [ -x "$HOME/.local/bin/uscreen" ] || fail "files not installed"
}
t_root_does_not_use_sudo() {
    mk_sandbox ubuntu-24.04; export US_UID=0
    run_install --headless --skip-spawnat >/dev/null
    not_called '^sudo '; called '^apt-get install'
}
t_non_root_without_passwordless_sudo_fails_headless() {
    mk_sandbox ubuntu-24.04; export SUDO_N_RC=1
    local o; o="$(run_install --headless --skip-spawnat)"; local rc=$?
    eq "$rc" 2; has "$o" "passwordless sudo"
}
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
t_ubuntu_missing_module_uses_the_prebuilt_package() {
    mk_sandbox ubuntu-24.04; export MODINFO_RC=1 PREBUILT="linux-main-modules-evdi-7.0.0-42-generic"
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    called 'apt-get install -y linux-main-modules-evdi-7.0.0-42-generic'; not_called 'apt-get install -y evdi-dkms'
}
t_ubuntu_prefers_the_hwe_metapackage_when_the_exact_one_is_absent() {
    mk_sandbox ubuntu-24.04; export MODINFO_RC=1 PREBUILT="linux-modules-evdi-generic-hwe-24.04"
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    called 'apt-get install -y linux-modules-evdi-generic-hwe-24.04'
}
t_half_installed_dkms_package_is_removed_first() {
    mk_sandbox ubuntu-24.04; export MODINFO_RC=0 EVDI_DKMS_STATE="iF "
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    called 'dpkg --purge --force-remove-reinstreq evdi-dkms'
}
t_half_installed_dkms_with_a_built_module_is_left_alone() {
    mk_sandbox ubuntu-24.04; export MODINFO_RC=0 EVDI_DKMS_STATE="iF "
    shim dkms 'echo "evdi/1.14.2, 7.0.0-42-generic, x86_64: installed"'
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    not_called 'dpkg --purge'
}
t_failed_dkms_build_is_cleaned_up_and_upstream_source_is_tried() {
    mk_sandbox debian-12 GNOME wayland; export MODINFO_RC=1 EVDI_DKMS_STATE="iF "
    shim git 'case "$*" in *ls-remote*) printf "abc\trefs/tags/v1.14.9\nabc\trefs/tags/v1.14.16\n";; esac'
    run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat >/dev/null
    called 'dpkg --purge --force-remove-reinstreq evdi-dkms'
    called 'git clone --depth 1 --branch v1.14.16'; called 'dkms build -m evdi -v 1.14.16'
    order 'dpkg --purge' 'git clone'
}
t_latest_upstream_tag_is_sorted_by_version_not_text() {
    mk_sandbox debian-12; . "$SB/project/scripts/lib/evdi.sh"
    shim git 'printf "a\trefs/tags/v1.9.0\na\trefs/tags/v1.14.2\na\trefs/tags/v1.14.16\na\trefs/tags/v1.14.4-rc1\n"'
    eq "$(evdi_latest_tag)" "v1.14.16"
}
t_source_build_without_dkms_fails_softly() {
    mk_sandbox fedora-41 KDE wayland; export MODINFO_RC=1 US_OSTREE=0
    rm -f "$SB/bin/dkms"
    local o; o="$(run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat)"; eq $? 0 "a missing EVDI must not abort the install"
    has "$o" "dkms is needed"
}
t_arch_without_an_aur_helper_explains_the_aur() {
    mk_sandbox arch KDE wayland; export MODINFO_RC=1; rm -f "$SB/bin/yay" "$SB/bin/paru"
    shim pacman 'exit 1'
    local o; o="$(run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat)"
    has "$o" "only in the AUR"
}
t_bazzite_never_installs_evdi() {
    mk_sandbox bazzite KDE wayland; export MODINFO_RC=1 US_OSTREE=1
    local o; o="$(run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat)"
    not_called 'git clone'; has "$o" "should already ship EVDI"
}
t_module_in_use_needs_a_reboot() {
    mk_sandbox ubuntu-24.04 KDE wayland; export MODINFO_RC=0
    shim lsmod 'echo "evdi 90112 2"'; export SUDO_FAIL='^modprobe -r'
    local o; o="$(run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat)"
    has "$o" "reboot=yes"
}
t_module_that_unloads_cleanly_needs_no_reboot() {
    mk_sandbox ubuntu-24.04 KDE wayland; export MODINFO_RC=0
    shim lsmod 'echo "evdi 90112 0"'
    local o; o="$(run_install --headless --skip-core --skip-menu --skip-extension --skip-spawnat)"
    has "$o" "reboot=no"
}
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
t_spawn_at_release_build_is_updated() {
    mk_sandbox ubuntu-24.04; shim spawn-at 'echo "spawn-at 0.2.3"'
    run_install --headless --skip-core --skip-system --skip-extension --skip-menu >/dev/null
    called 'spawn-at update --headless'
}
t_spawn_at_missing_is_installed_and_needs_relogin_on_gnome() {
    mk_sandbox ubuntu-24.04 ubuntu:GNOME wayland
    local o; o="$(run_install --headless --skip-core --skip-system --skip-extension --skip-menu)"
    called 'curl -fsSL https://raw.githubusercontent.com/harshp2008/spawn-at'; has "$o" "relogin=yes"
}
t_spawn_at_offline_does_not_abort() {
    mk_sandbox ubuntu-24.04; shim curl 'exit 6'
    local o; o="$(run_install --headless --skip-core --skip-system --skip-extension --skip-menu)"; eq $? 0
    has "$o" "did not install"
}
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
t_tui_cancel_installs_nothing() {
    mk_sandbox ubuntu-24.04
    shim whiptail 'exit 1'
    if ! command -v script >/dev/null 2>&1; then echo "    (skipped: no script(1))"; return 0; fi
    ( cd "$SB/project" && script -qec "./scripts/install.sh --tui" /dev/null ) >"$SB/tui.out" 2>&1
    [ ! -e "$HOME/.local/bin/uscreen" ] || fail "cancelled install must not copy files"
}
t_no_terminal_picks_headless() {
    mk_sandbox ubuntu-24.04
    local o; o="$(run_install --dry-run </dev/null)"; has "$o" "(headless"
}

# ================================ run =================================================================
for fn in $(declare -F | awk '{print $3}' | grep '^t_'); do run_test "$fn"; done
echo
echo "passed: $PASS  failed: $FAIL"
[ "$FAIL" = 0 ] || { printf 'failed: %s\n' "${FAILED[@]}"; exit 1; }
