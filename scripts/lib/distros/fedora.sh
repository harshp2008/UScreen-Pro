# shellcheck shell=bash
# UScreen installer: the Fedora family (Fedora, RHEL and its rebuilds, Nobara, Bazzite).
#
# Owns everything specific to dnf and rpm-ostree systems: the os-release IDs, the package names, RPM Fusion
# for ffmpeg, layering on image-based systems, and the fact that no EVDI module is packaged (it is built
# from upstream source, or already bundled in the image on Bazzite and Nobara).
# Does not own the shared EVDI steps (evdi.sh) or detection (platform.sh). Interface: see _template.sh.

fedora_order() { echo 20; }

fedora_ids() { echo "fedora rhel centos rocky almalinux nobara bazzite"; }

fedora_core_packages() { echo "ffmpeg android-tools"; }

# Bazzite and Nobara ship the module in their image; plain Fedora has no package, so it is built from source.
fedora_evdi_strategy() {
    case "$(us_os_value ID)" in
        bazzite|nobara) echo bundled ;;
        *) echo source ;;
    esac
}

fedora_install_deps() {
    if us_immutable && command -v rpm-ostree >/dev/null 2>&1; then
        info "Image-based Fedora detected: layering packages (needs a reboot afterwards)"
        run $SUDO rpm-ostree install --idempotent --allow-inactive ffmpeg android-tools \
            && US_PKG_LAYERED=1 || warn "Layering failed — check the names against your image"
    else
        # ffmpeg on Fedora needs RPM Fusion; the stock repositories only carry ffmpeg-free.
        if ! dnf -q info ffmpeg >/dev/null 2>&1; then
            info "Enabling RPM Fusion (ffmpeg is not in the stock repositories)"
            run $SUDO dnf install -y \
                "https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm" \
                || warn "Could not enable RPM Fusion"
        fi
        # --allowerasing: Fedora preinstalls ffmpeg-free, which RPM Fusion's build replaces.
        run $SUDO dnf install -y --allowerasing ffmpeg android-tools \
            || warn "Install ffmpeg and android-tools manually"
    fi
}

fedora_install_gtk() { run $SUDO dnf install -y gtk4; }

# Nothing is packaged for Fedora: evdi_ensure builds upstream source after this returns non-zero.
fedora_evdi_install() { return 1; }
