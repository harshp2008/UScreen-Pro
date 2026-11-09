#!/usr/bin/env bash
# UScreen installer — works from a source checkout or a release tarball.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Package names verified against real systems, not from memory: an Arch
# container, a Debian container and a Fedora container, each asked what it
# actually has. Three of the four families had at least one name wrong.
install_deps() {
    . /etc/os-release 2>/dev/null || true
    local id="${ID:-unknown}"
    local like="${ID_LIKE:-}"

    case "$id $like" in
        *fedora*|*rhel*|*centos*)
            if command -v rpm-ostree &>/dev/null && [ -e /run/ostree-booted ]; then
                info "Immutable Fedora detected — layering packages (reboot needed afterwards)"
                # Bazzite and Nobara ship evdi in the base image; plain
                # Silverblue does not, and it is not layerable either.
                sudo rpm-ostree install --idempotent --allow-inactive \
                    ffmpeg android-tools || \
                    warn "Layering failed — check the names against your image"
            else
                # ffmpeg on Fedora needs RPM Fusion; the stock repositories
                # only carry ffmpeg-free, which cannot do what we ask of it.
                if ! dnf -q info ffmpeg >/dev/null 2>&1; then
                    info "Enabling RPM Fusion (ffmpeg is not in the stock repositories)"
                    sudo dnf install -y \
                        "https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm" \
                        || warn "Could not enable RPM Fusion"
                fi
                # --allowerasing because Fedora preinstalls ffmpeg-free,
                # which the RPM Fusion build replaces; without it dnf refuses
                # the whole transaction rather than swapping the two.
                sudo dnf install -y --allowerasing ffmpeg android-tools || \
                    warn "Install ffmpeg and android-tools manually"
            fi
            # evdi is not packaged for Fedora at all — not in the stock
            # repositories and not in RPM Fusion. Checked, rather than
            # assumed, because the old installer asked dnf for it and hid the
            # failure behind `|| true`.
            if [ ! -e /sys/devices/evdi ] && ! ls /usr/lib*/libevdi.so* >/dev/null 2>&1; then
                warn "evdi is not packaged for Fedora. Build it from source:"
                warn "    git clone https://github.com/DisplayLink/evdi"
                warn "    cd evdi && make && sudo make install"
                warn "(Bazzite and Nobara already ship it.)"
            fi
            ;;
        *debian*|*ubuntu*)
            sudo apt-get update
            # libevdi0 does not exist: the runtime library is libevdi1, and
            # libevdi0-dev is only a transitional package. Asking for the
            # wrong one made the whole line fail, and the fallback quietly
            # installed no library at all.
            # Userspace first, kernel module second: a dkms build that fails
            # (no headers, unsupported kernel) must not stop ffmpeg and adb
            # from being installed.
            sudo apt-get install -y ffmpeg adb libevdi1 libevdi-dev || \
            sudo apt-get install -y ffmpeg android-tools-adb libevdi1 || \
                warn "Check the package names for your release"
            sudo apt-get install -y evdi-dkms || \
                warn "evdi-dkms did not install — you may need linux-headers-$(uname -r)"
            ;;
        *arch*|*manjaro*|*endeavouros*|*cachyos*)
            sudo pacman -S --needed --noconfirm ffmpeg android-tools
            # evdi is not in the official repositories on Arch — it only
            # exists in the AUR, so asking pacman for it can never succeed.
            if pacman -Qq evdi-dkms >/dev/null 2>&1 || pacman -Qq evdi >/dev/null 2>&1; then
                info "evdi already installed"
            elif command -v yay >/dev/null 2>&1; then
                yay -S --needed --noconfirm evdi-dkms
            elif command -v paru >/dev/null 2>&1; then
                paru -S --needed --noconfirm evdi-dkms
            else
                warn "evdi lives in the AUR. Install it with an AUR helper, e.g."
                warn "    yay -S evdi-dkms"
                warn "then run this script again."
            fi
            ;;
        *suse*)
            # The one distribution that has all of it in the default repos.
            # Split in two: the evdi kernel module package is tied to the
            # running kernel's ABI, and when that does not resolve it should
            # not take ffmpeg and adb down with it.
            sudo zypper --non-interactive install --no-recommends \
                ffmpeg android-tools || \
                warn "Install ffmpeg and android-tools manually"
            # libevdi1 requires evdi-kmp, so the library and the kernel module
            # stand or fall together here — nothing to be gained by splitting
            # them further.
            sudo zypper --non-interactive install --no-recommends evdi libevdi1 || \
                warn "evdi did not install — usually a kernel/module version mismatch"
            ;;
        *)
            warn "Unknown distro. Install manually: ffmpeg, adb (android-tools), evdi + libevdi"
            ;;
    esac
}

# Say what is missing before the build fails on it in a less obvious way.
check_deps() {
    local missing=0
    command -v ffmpeg >/dev/null 2>&1 || { warn "ffmpeg not found"; missing=1; }
    command -v adb    >/dev/null 2>&1 || { warn "adb not found"; missing=1; }
    ls /usr/lib*/libevdi.so* >/dev/null 2>&1 || \
        ls /usr/lib/*/libevdi.so* >/dev/null 2>&1 || \
        { warn "libevdi not found — the helper will not build"; missing=1; }
    [ "$missing" = 0 ] && info "Dependencies look complete"
    return 0
}

# ~/.local/bin is only added to PATH at login on most distributions, and only
# if it already exists. Installing into a directory we just created therefore
# gives "uscreen: command not found" straight after a successful install.
check_path() {
    case ":${PATH}:" in
        *":${BIN_DIR}:"*) return 0 ;;
    esac
    warn "$BIN_DIR is not in your PATH. Add it with:"
    warn "    echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
    warn "or log out and back in — most shells pick it up once the directory exists."
}

build_if_needed() {
    # Release tarballs ship prebuilt binaries next to this script's parent
    if [ -f "$PROJECT_DIR/bin/uscreen" ]; then
        return
    fi
    if [ ! -f "$PROJECT_DIR/target/release/uscreen" ]; then
        info "Building from source (needs rust + gcc)..."
        make -C "$PROJECT_DIR" build
    fi
}

install_files() {
    mkdir -p "$BIN_DIR" "$APP_DIR"
    local src_bin
    if [ -f "$PROJECT_DIR/bin/uscreen" ]; then
        src_bin="$PROJECT_DIR/bin"
    else
        src_bin="$PROJECT_DIR/target/release"
    fi

    rm -f "$BIN_DIR/uscreen" "$BIN_DIR/uscreen-gui" "$BIN_DIR/evdi_helper"
    cp "$src_bin/uscreen" "$BIN_DIR/uscreen"
    cp "$src_bin/uscreen-gui" "$BIN_DIR/uscreen-gui" 2>/dev/null || warn "uscreen-gui not found, skipping"
    if [ -f "$src_bin/evdi_helper" ]; then
        cp "$src_bin/evdi_helper" "$BIN_DIR/evdi_helper"
        # The release helper finds libevdi next to itself ($ORIGIN rpath).
        [ -f "$src_bin/libevdi.so.1.15.0" ] && cp -P "$src_bin"/libevdi.so.1* "$BIN_DIR/"
    else
        cp "$PROJECT_DIR/host/evdi/evdi_helper" "$BIN_DIR/evdi_helper"
    fi
    chmod +x "$BIN_DIR/uscreen" "$BIN_DIR/evdi_helper"
    info "Binaries installed to $BIN_DIR"

    DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/uscreen"
    mkdir -p "$DATA_DIR"
    if [ -f "$PROJECT_DIR/android/app/build/outputs/apk/debug/app-debug.apk" ]; then
        cp "$PROJECT_DIR/android/app/build/outputs/apk/debug/app-debug.apk" "$DATA_DIR/app-debug.apk"
        info "APK copied to $DATA_DIR/app-debug.apk"
    fi

    # Absolute path: the app menu does not necessarily have ~/.local/bin on
    # its PATH, so a bare "uscreen-gui" can be a menu entry that does nothing.
    sed "s|^Exec=.*|Exec=$BIN_DIR/uscreen-gui|" "$SCRIPT_DIR/uscreen.desktop" > "$APP_DIR/uscreen.desktop" \
        && info "Desktop entry installed (UScreen in the app menu)"

    # The menu entry and the tray look the icon up by name in the hicolor
    # theme; without this they fall back to a generic or blank picture.
    ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
    if [ -f "$PROJECT_DIR/packaging/icons/uscreen.svg" ]; then
        mkdir -p "$ICON_DIR"
        cp "$PROJECT_DIR/packaging/icons/uscreen.svg" "$PROJECT_DIR/packaging/icons/uscreen-pen.svg" "$ICON_DIR/"
        # KDE keys its icon cache on the theme directory's mtime.
        touch "${ICON_DIR%/scalable/apps}" 2>/dev/null || true
        command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "${ICON_DIR%/scalable/apps}" 2>/dev/null || true
        command -v kbuildsycoca6 >/dev/null 2>&1 && kbuildsycoca6 >/dev/null 2>&1 || true
    fi

    mkdir -p "${HOME}/.config/systemd/user"
    cp "$SCRIPT_DIR/uscreen.service" "${HOME}/.config/systemd/user/" 2>/dev/null || true
    systemctl --user daemon-reload 2>/dev/null || true

    mkdir -p "${HOME}/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app"
    cp "$PROJECT_DIR/extensions/uscreen-app-lock/metadata.json" "$PROJECT_DIR/extensions/uscreen-app-lock/extension.js" "${HOME}/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/" 2>/dev/null || true
    mkdir -p "${HOME}/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/schemas"
    cp "$PROJECT_DIR"/extensions/uscreen-app-lock/schemas/*.gschema.xml "${HOME}/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/schemas/" 2>/dev/null || true
    glib-compile-schemas "${HOME}/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/schemas" 2>/dev/null || true
    info "GNOME Shell extension installed: uscreen-app-lock@uscreen.app"
}

system_setup() {
    info "System setup (needs sudo): EVDI device at every boot"
    # Neither directory is guaranteed to exist on a minimal install, and with
    # set -e a missing one used to kill the whole script here, silently, with
    # the binaries already copied and the udev rule not yet installed.
    sudo mkdir -p /etc/modprobe.d /etc/modules-load.d /etc/udev/rules.d
    printf "options evdi initial_device_count=2\nsoftdep evdi pre: drm_kms_helper i915\n" | sudo tee /etc/modprobe.d/uscreen-evdi.conf >/dev/null \
        || warn "Could not write /etc/modprobe.d/uscreen-evdi.conf"
    printf "evdi\nuinput\n" | sudo tee /etc/modules-load.d/uscreen.conf >/dev/null \
        || warn "Could not write /etc/modules-load.d/uscreen.conf"
    sudo modprobe uinput 2>/dev/null || true
    # /dev/uinput is root-only on a stock system. Bazzite ships a rule that
    # opens it to the seat user; everyone else needs this one.
    if [ ! -e /etc/udev/rules.d/60-uscreen-uinput.rules ] && [ ! -e /usr/lib/udev/rules.d/60-uscreen-uinput.rules ]; then
        if [ -f "$PROJECT_DIR/packaging/60-uscreen-uinput.rules" ]; then
            sudo install -Dm644 "$PROJECT_DIR/packaging/60-uscreen-uinput.rules" /etc/udev/rules.d/60-uscreen-uinput.rules
            sudo udevadm control --reload 2>/dev/null || true
            sudo udevadm trigger --name-match=uinput 2>/dev/null || true
        else
            warn "packaging/60-uscreen-uinput.rules not found — /dev/uinput may stay root-only"
        fi
    fi

    # EVDI DRM device uaccess rule: ensures /dev/dri/card* nodes created by evdi
    # receive user seat permissions without race conditions.
    if [ ! -e /etc/udev/rules.d/99-uscreen-evdi.rules ] && [ ! -e /usr/lib/udev/rules.d/99-uscreen-evdi.rules ]; then
        if [ -f "$PROJECT_DIR/packaging/99-uscreen-evdi.rules" ]; then
            sudo install -Dm644 "$PROJECT_DIR/packaging/99-uscreen-evdi.rules" /etc/udev/rules.d/99-uscreen-evdi.rules
            sudo udevadm control --reload 2>/dev/null || true
            sudo udevadm trigger 2>/dev/null || true
        fi
    fi

    # initial_device_count is only read when the module loads, so writing the
    # modprobe.d file does nothing to a module that is already resident. That
    # is the usual state after installing evdi-dkms by hand, and it is why the
    # daemon could sit in a retry loop on a machine that looked correctly set
    # up: /sys/devices/evdi/count stays 0, and creating a device needs a write
    # to /sys/devices/evdi/add that only root can do.
    if lsmod 2>/dev/null | grep -q '^evdi'; then
        if ! sudo modprobe -r evdi 2>/dev/null; then
            warn "evdi is loaded and in use — reboot to pick up the new setting"
        fi
    fi
    sudo modprobe evdi 2>/dev/null || warn "evdi module not available yet (reboot after installing evdi-dkms)"

    # Belt and braces: whatever happened above, make sure a device exists now.
    if [ "$(cat /sys/devices/evdi/count 2>/dev/null || echo 0)" = "0" ]; then
        echo 1 | sudo tee /sys/devices/evdi/add >/dev/null 2>&1 || true
    fi

    if [ "$(cat /sys/devices/evdi/count 2>/dev/null || echo 0)" = "0" ]; then
        warn "No EVDI device could be created. Reboot, then run: uscreen doctor"
    else
        info "EVDI device ready (count=$(cat /sys/devices/evdi/count))"
    fi
}

# spawn-at places the long-press menu window exactly at the pen (https://github.com/harshp2008/spawn-at).
# Not installed: install the latest release. Installed: update it, unless it is a dev build, which
# is somebody's own working copy and is left alone. A failure here never stops the UScreen install.
install_spawn_at() {
    local url="https://raw.githubusercontent.com/harshp2008/spawn-at/main/install.sh"
    if command -v spawn-at >/dev/null 2>&1; then
        local ver
        ver="$(spawn-at -V 2>/dev/null | head -n1 || true)"
        case "$ver" in
            *"DEV BUILD"*)
                info "spawn-at is a dev build (${ver}) — leaving it as it is"
                return 0
                ;;
        esac
        info "spawn-at found (${ver:-unknown version}) — checking for an update"
        spawn-at update --headless || warn "spawn-at update failed — keeping the installed version"
        return 0
    fi
    info "Installing spawn-at (places the long-press menu)"
    if ! command -v curl >/dev/null 2>&1; then
        warn "curl is missing — skipping spawn-at; the pen menu will not be available"
        return 0
    fi
    if ! curl -fsSL "$url" | bash -s -- --headless; then
        warn "spawn-at did not install — the pen menu will not be available (UScreen itself is fine)"
    fi
}

main() {
    echo "================================================"
    echo "  UScreen installer"
    echo "================================================"
    install_deps
    check_deps
    build_if_needed
    install_files
    install_spawn_at
    system_setup
    echo ""
    info "Done! Launch 'UScreen' from your app menu (or run: uscreen-gui)"
    info "Install the APK on your tablet, enable USB debugging, plug in — that's it."
    check_path
}

main "$@"
