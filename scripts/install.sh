#!/usr/bin/env bash
# UScreen installer: from a source checkout or a release tarball.
#
#   ./install.sh                 asks what to install in a text interface when run in a terminal
#   ./install.sh --headless      no questions; installs everything (what CI and scripts use)
#   ./install.sh --dry-run       shows every command it would run and changes nothing
#   ./install.sh --plan          prints what this system needs (packages, EVDI method, restarts)
#
# Options: --tui --headless -y  --dry-run --plan  --skip-<part> (core menu extension spawnat system)
#          --no-build  --help
# At the end a line `UScreen-INSTALL-RESULT: relogin=yes|no reboot=yes|no` and the same in
# ~/.local/share/uscreen/install-result.json say whether a log out/in or a reboot is needed.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/uscreen"
EXT_ID="uscreen-app-lock@uscreen.app"
EXT_DIR="${HOME}/.local/share/gnome-shell/extensions/${EXT_ID}"

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
# Prints a green [INFO] line.
info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
# Prints a yellow [WARN] line.
warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
# Prints a red [ERROR] line on stderr.
error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }

# shellcheck source=lib/platform.sh
. "$SCRIPT_DIR/lib/platform.sh"
# shellcheck source=lib/evdi.sh
. "$SCRIPT_DIR/lib/evdi.sh"
# shellcheck source=lib/tui.sh
. "$SCRIPT_DIR/lib/tui.sh"

MODE=""            # tui | headless (chosen below)
DRY_RUN=0
PLAN_ONLY=0
NO_BUILD=0
PARTS="core menu extension spawnat system"
US_EXT_CHANGED=0
US_EVDI_NEEDS_REBOOT=0
US_PKG_LAYERED=0
SUDO=""

# Prints the option summary from the header comment of this file.
usage() { sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; }

# ---- running things -------------------------------------------------------------------------

# Run a command, or in a dry run only print it. Everything that changes the system goes through here.
run() {
    if [ "$DRY_RUN" = 1 ]; then
        printf '+ %s\n' "$*"
        return 0
    fi
    "$@"
}

# run_tee FILE: write stdin to FILE (as root where needed).
run_tee() {
    if [ "$DRY_RUN" = 1 ]; then
        printf '+ write %s: %s\n' "$1" "$(tr '\n' ';' | head -c 120)"
        return 0
    fi
    # shellcheck disable=SC2086
    $SUDO tee "$1" >/dev/null
}

# write_file FILE: write stdin to a file in the user's own directories.
write_file() {
    if [ "$DRY_RUN" = 1 ]; then
        printf '+ write %s\n' "$1"
        cat >/dev/null
        return 0
    fi
    cat >"$1"
}

# Root needs no sudo (containers have none); everyone else needs it, without a password prompt when headless.
setup_privilege() {
    if [ "${US_UID:-$(id -u)}" = 0 ]; then
        SUDO=""
    elif command -v sudo >/dev/null 2>&1; then
        SUDO="sudo"
        if [ "$MODE" = headless ] && [ "$DRY_RUN" = 0 ] && ! sudo -n true 2>/dev/null; then
            error "Headless install needs passwordless sudo or root (sudo -n failed)."
            error "Run as root, or run in a terminal so sudo can ask, or use --skip-system --skip-core."
            exit 2
        fi
    elif [ "$DRY_RUN" = 1 ]; then
        SUDO="sudo"
    else
        error "This needs root: run as root or install sudo."
        exit 2
    fi
}

# True when [part] (core, menu, extension, spawnat, system) is still selected.
has_part() { case " $PARTS " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

# ---- dependencies ---------------------------------------------------------------------------

# Package names verified against real systems, not from memory: an Arch container, a Debian
# container and a Fedora container, each asked what it actually has.
install_deps() { "$(us_distro_family)_install_deps"; }

# Say what is missing before a build fails on it in a less obvious way.
check_deps() {
    local missing=0
    command -v ffmpeg >/dev/null 2>&1 || { warn "ffmpeg not found"; missing=1; }
    command -v adb    >/dev/null 2>&1 || { warn "adb not found"; missing=1; }
    ls /usr/lib*/libevdi.so* >/dev/null 2>&1 || ls /usr/lib/*/libevdi.so* >/dev/null 2>&1 \
        || { warn "libevdi not found — the helper will not build"; missing=1; }
    [ "$missing" = 0 ] && info "Dependencies look complete"
    return 0
}

# ~/.local/bin is only on PATH at login on most distributions, and only if it already existed then.
check_path() {
    case ":${PATH}:" in *":${BIN_DIR}:"*) return 0 ;; esac
    warn "$BIN_DIR is not in your PATH. Add it with:"
    warn "    echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
    warn "or log out and back in."
}

# Builds the binaries with make unless --no-build, or a release tarball already ships them, or they are already built.
build_if_needed() {
    [ "$NO_BUILD" = 1 ] && return 0
    [ -f "$PROJECT_DIR/bin/uscreen" ] && return 0     # release tarballs ship prebuilt binaries
    if [ ! -f "$PROJECT_DIR/target/release/uscreen" ]; then
        info "Building from source (needs rust + gcc)..."
        run make -C "$PROJECT_DIR" build
    fi
}

# ---- files ----------------------------------------------------------------------------------

# The integer `version` of an extension's metadata.json, or nothing. The installer compares it to decide whether a log out is needed.
ext_version() { { sed -n 's/.*"version"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$1" 2>/dev/null || true; } | head -n1; }

# Copies the binaries, helper, icons, desktop entry and systemd unit into the user's home; fails clearly when a binary is missing.
install_files() {
    run mkdir -p "$BIN_DIR" "$APP_DIR" "$DATA_DIR"
    local src_bin
    if [ -f "$PROJECT_DIR/bin/uscreen" ]; then src_bin="$PROJECT_DIR/bin"; else src_bin="$PROJECT_DIR/target/release"; fi

    # uscreen-gui is the retired egui app; an old install may still have it.
    run rm -f "$BIN_DIR/uscreen" "$BIN_DIR/uscreen-gui" "$BIN_DIR/uscreen-pro-gui" "$BIN_DIR/evdi_helper"
    if [ "$DRY_RUN" = 1 ] || [ -f "$src_bin/uscreen" ]; then
        run cp "$src_bin/uscreen" "$BIN_DIR/uscreen"
    else
        error "$src_bin/uscreen is missing: build first (make build) or use a release tarball"
        return 1
    fi
    if [ -f "$src_bin/uscreen-pro-gui" ] || [ "$DRY_RUN" = 1 ]; then
        run cp "$src_bin/uscreen-pro-gui" "$BIN_DIR/uscreen-pro-gui"
    else
        warn "uscreen-pro-gui not found, skipping"
    fi
    if [ -f "$src_bin/evdi_helper" ]; then
        run cp "$src_bin/evdi_helper" "$BIN_DIR/evdi_helper"
        # The release helper finds libevdi next to itself ($ORIGIN rpath).
        if ls "$src_bin"/libevdi.so.1* >/dev/null 2>&1; then run cp -P "$src_bin"/libevdi.so.1* "$BIN_DIR/"; fi
    elif [ -f "$PROJECT_DIR/host/evdi/evdi_helper" ]; then
        run cp "$PROJECT_DIR/host/evdi/evdi_helper" "$BIN_DIR/evdi_helper"
    else
        warn "evdi_helper not found: Extended Display mode will be unavailable"
    fi
    if [ -f "$src_bin/uscreen-menu" ]; then
        run cp "$src_bin/uscreen-menu" "$BIN_DIR/uscreen-menu"
    else
        warn "uscreen-menu not found, the fallback pen menu window will be unavailable"
    fi
    run chmod +x "$BIN_DIR/uscreen" "$BIN_DIR/evdi_helper" 2>/dev/null || true
    info "Binaries installed to $BIN_DIR"

    if [ -f "$PROJECT_DIR/android/app/build/outputs/apk/debug/app-debug.apk" ]; then
        run cp "$PROJECT_DIR/android/app/build/outputs/apk/debug/app-debug.apk" "$DATA_DIR/app-debug.apk"
        info "APK copied to $DATA_DIR/app-debug.apk"
    fi

    # Absolute path: the app menu does not necessarily have ~/.local/bin on its PATH.
    sed "s|^Exec=.*|Exec=$BIN_DIR/uscreen-pro-gui|" "$SCRIPT_DIR/uscreen.desktop" | write_file "$APP_DIR/uscreen-pro.desktop"
    run rm -f -- "$APP_DIR/uscreen.desktop"     # older installs wrote a second, identical icon
    info "Desktop entry installed (UScreen in the app menu)"

    local icon_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
    if [ -f "$PROJECT_DIR/packaging/icons/uscreen.svg" ]; then
        run mkdir -p "$icon_dir"
        run cp "$PROJECT_DIR/packaging/icons/uscreen.svg" "$PROJECT_DIR/packaging/icons/uscreen-pen.svg" "$icon_dir/"
        run touch "${icon_dir%/scalable/apps}"
        command -v gtk-update-icon-cache >/dev/null 2>&1 && run gtk-update-icon-cache -q -t "${icon_dir%/scalable/apps}" || true
    fi

    run mkdir -p "${HOME}/.config/systemd/user"
    run cp "$SCRIPT_DIR/uscreen.service" "${HOME}/.config/systemd/user/"
    # PLATFORM(systemd): reload user units after installing uscreen.service
    run systemctl --user daemon-reload 2>/dev/null || true
}

# The GNOME Shell extension. A change needs a log out/in on Wayland, so a reinstall of the same
# version must not ask for one.
install_extension() {
    local src="$PROJECT_DIR/extensions/uscreen-app-lock"
    [ -f "$src/extension.js" ] || { warn "extension sources not found; skipping the Shell extension"; return 0; }
    if [ "$(us_desktop)" != gnome ]; then
        info "Not running GNOME: the Shell extension is installed but only GNOME Shell uses it"
    fi
    local have want
    have="$(ext_version "$EXT_DIR/metadata.json")"
    want="$(ext_version "$src/metadata.json")"
    run mkdir -p "$EXT_DIR/schemas" "$EXT_DIR/lib" "$EXT_DIR/compat"
    run cp "$src/metadata.json" "$src/extension.js" "$src/stylesheet.css" "$EXT_DIR/"
    # The modules replace the ones of the previous version, so a module removed upstream does not linger.
    run rm -f "$EXT_DIR"/lib/*.js "$EXT_DIR"/compat/*.js
    run cp "$src"/lib/*.js "$EXT_DIR/lib/"
    run cp "$src"/compat/*.js "$EXT_DIR/compat/"
    if ls "$src"/schemas/*.gschema.xml >/dev/null 2>&1; then
        run cp "$src"/schemas/*.gschema.xml "$EXT_DIR/schemas/"
        # PLATFORM(gnome): compiles the extension's gsettings schema
        run glib-compile-schemas "$EXT_DIR/schemas" 2>/dev/null || true
    fi
    if [ "$have" != "$want" ]; then
        US_EXT_CHANGED=1
        info "GNOME Shell extension installed: $EXT_ID (version ${have:-none} -> ${want:-?})"
    else
        info "GNOME Shell extension already at version $want"
    fi
}

# The pen menu window is a GTK 4 program; best effort, the menu is optional.
install_menu_runtime() {
    if ldconfig -p 2>/dev/null | grep -q "libgtk-4.so" || dpkg -s libgtk-4-1 >/dev/null 2>&1; then return 0; fi
    info "Installing the GTK 4 runtime for the pen menu"
    "$(us_distro_family)_install_gtk" || warn "Could not install GTK 4 — the fallback pen menu window will not be available"
}

# spawn-at places the pen-menu fallback window (https://github.com/harshp2008/spawn-at).
# Missing: install. Installed: update unless it is a dev build. Never stops the install.
install_spawn_at() {
    local url="https://raw.githubusercontent.com/harshp2008/spawn-at/main/install.sh" before after
    if command -v spawn-at >/dev/null 2>&1; then
        before="$(spawn-at -V 2>/dev/null | head -n1 || true)"
        case "$before" in
            *"DEV BUILD"*) info "spawn-at is a dev build (${before}) — leaving it as it is"; return 0 ;;
        esac
        info "spawn-at found (${before:-unknown version}) — checking for an update"
        run spawn-at update --headless || warn "spawn-at update failed — keeping the installed version"
        after="$(spawn-at -V 2>/dev/null | head -n1 || true)"
        [ "$before" != "$after" ] && US_EXT_CHANGED=1
        return 0
    fi
    info "Installing spawn-at"
    command -v curl >/dev/null 2>&1 || { warn "curl is missing — skipping spawn-at"; return 0; }
    if [ "$DRY_RUN" = 1 ]; then
        echo "+ curl -fsSL $url | bash -s -- --headless"
        return 0
    fi
    if curl -fsSL "$url" | bash -s -- --headless; then US_EXT_CHANGED=1; else
        warn "spawn-at did not install — the fallback pen menu will not be available (UScreen itself is fine)"
    fi
}

# ---- system -------------------------------------------------------------------------------------

# The system part: gets and activates the EVDI module and installs the udev rules for /dev/uinput and EVDI. Needs root or sudo.
system_setup() {
    info "System setup: virtual display driver and input permissions"
    evdi_ensure || warn "No EVDI module could be installed: Extended Display mode will not work (stylus and touchpad modes do not need it)"
    evdi_activate
    run $SUDO mkdir -p /etc/udev/rules.d
    # PLATFORM(uinput): loads the uinput module; the udev rule below makes /dev/uinput usable without root
    run $SUDO modprobe uinput 2>/dev/null || true
    # /dev/uinput is root-only on a stock system; Bazzite ships its own rule.
    if [ ! -e /etc/udev/rules.d/60-uscreen-uinput.rules ] && [ ! -e /usr/lib/udev/rules.d/60-uscreen-uinput.rules ]; then
        if [ -f "$PROJECT_DIR/packaging/60-uscreen-uinput.rules" ]; then
            run $SUDO install -Dm644 "$PROJECT_DIR/packaging/60-uscreen-uinput.rules" /etc/udev/rules.d/60-uscreen-uinput.rules
            run $SUDO udevadm control --reload 2>/dev/null || true
            run $SUDO udevadm trigger --name-match=uinput 2>/dev/null || true
        else
            warn "packaging/60-uscreen-uinput.rules not found — /dev/uinput may stay root-only"
        fi
    fi
    if [ ! -e /etc/udev/rules.d/99-uscreen-evdi.rules ] && [ ! -e /usr/lib/udev/rules.d/99-uscreen-evdi.rules ] \
        && [ -f "$PROJECT_DIR/packaging/99-uscreen-evdi.rules" ]; then
        run $SUDO install -Dm644 "$PROJECT_DIR/packaging/99-uscreen-evdi.rules" /etc/udev/rules.d/99-uscreen-evdi.rules
        run $SUDO udevadm control --reload 2>/dev/null || true
    fi
}

# ---- result ---------------------------------------------------------------------------------------

# Prints the UScreen-INSTALL-RESULT line, writes install-result.json and explains any restart the user needs.
report_result() {
    export US_EXT_CHANGED US_EVDI_NEEDS_REBOOT US_PKG_LAYERED
    local needs
    needs="$(us_restart_needs)"
    echo ""
    echo "UScreen-INSTALL-RESULT: $needs"
    if [ "$DRY_RUN" = 0 ]; then
        mkdir -p "$DATA_DIR"
        local rl rb
        case "$needs" in *relogin=yes*) rl=true ;; *) rl=false ;; esac
        case "$needs" in *reboot=yes*) rb=true ;; *) rb=false ;; esac
        printf '{"relogin":%s,"reboot":%s}\n' "$rl" "$rb" >"$DATA_DIR/install-result.json"
    fi
    us_restart_explain | while read -r line; do info "$line"; done
    info "Done! Launch 'UScreen' from your app menu (or run: uscreen-pro-gui)"
    info "Install the APK on your tablet, enable USB debugging, plug in — that's it."
    if [ "$MODE" = tui ]; then
        tui_message "$(us_restart_explain)"
    fi
}

# --plan: prints what this system needs (family, desktop, image based, packages, EVDI method) and changes nothing.
print_plan() {
    echo "family:        $(us_distro_family)"
    echo "desktop:       $(us_desktop) / session $(us_session)"
    echo "image based:   $(us_immutable && echo yes || echo no)"
    echo "core packages: $(us_core_packages)"
    echo "evdi method:   $(us_evdi_strategy)"
    echo "gnome wayland: $(us_gnome_wayland && echo yes || echo no)"
}

# ---- main -----------------------------------------------------------------------------------------

# Reads the command-line options; an unknown option prints the usage and exits with 64.
parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --headless|-y|--yes) MODE=headless ;;
            --tui) MODE=tui ;;
            --dry-run) DRY_RUN=1 ;;
            --plan) PLAN_ONLY=1 ;;
            --no-build) NO_BUILD=1 ;;
            --skip-core|--skip-menu|--skip-extension|--skip-spawnat|--skip-system)
                local part="${1#--skip-}"
                PARTS="$(echo " $PARTS " | sed "s/ $part / /" | xargs)" ;;
            -h|--help) usage; exit 0 ;;
            *) error "unknown option: $1"; usage; exit 64 ;;
        esac
        shift
    done
}

# Picks the text interface when a person is at a terminal with whiptail or dialog, otherwise headless.
choose_mode() {
    [ -n "$MODE" ] && return 0
    if tui_interactive && [ -n "$(tui_tool)" ]; then MODE=tui; else MODE=headless; fi
}

# Runs the selected parts in order, then reports the result.
main() {
    parse_args "$@"
    if [ "$PLAN_ONLY" = 1 ]; then print_plan; exit 0; fi
    choose_mode
    echo "================================================"
    echo "  UScreen installer (${MODE}$([ "$DRY_RUN" = 1 ] && echo ', dry run'))"
    echo "================================================"
    if [ "$MODE" = tui ]; then
        if ! PARTS="$(tui_components)"; then info "Cancelled"; exit 0; fi
    fi
    setup_privilege
    has_part core      && { install_deps; check_deps; build_if_needed; install_files; }
    has_part menu      && install_menu_runtime
    has_part extension && install_extension
    has_part spawnat   && install_spawn_at
    has_part system    && system_setup
    report_result
    check_path
}

main "$@"
