# shellcheck shell=bash
# UScreen installer: TEMPLATE for a new distribution family. This file is not loaded (names starting with `_`
# are skipped). To add a family: copy it to `distros/<family>.sh`, replace `FAMILY` with that name in every
# function name, fill in the bodies, add an `os-release` fixture to scripts/tests/fixtures and the fixture to
# the family and strategy tests in scripts/tests/run.sh. Full guide: docs/adding-support.md.
#
# Every function below must exist (scripts/tests/run.sh checks it for every file in distros/). They run with
# `run`, `info`, `warn` and `$SUDO` from install.sh available; `run` only prints in a dry run. The ones that echo
# are pure: they change nothing and may be called by `--plan`.

# Position when families are tried against os-release (lower first). Only matters when two families could claim the same system.
FAMILY_order() { echo 50; }

# Space separated os-release ID or ID_LIKE values that belong to this family.
FAMILY_ids() { echo "id-one id-two"; }

# Package names the daemon needs (echoed, space separated). Empty when unknown.
FAMILY_core_packages() { echo "ffmpeg android-tools"; }

# How a missing EVDI kernel module is obtained here: prebuilt | dkms | package | source | bundled.
FAMILY_evdi_strategy() { echo dkms; }

# Install the userspace packages. Set US_PKG_LAYERED=1 if they only take effect after a reboot (image-based systems).
FAMILY_install_deps() { run $SUDO your-package-manager install ffmpeg android-tools; }

# Install the GTK 4 runtime of the fallback pen menu. Return non-zero if it cannot.
FAMILY_install_gtk() { run $SUDO your-package-manager install gtk4; }

# Get the EVDI module with this family's own method. Return 0 when installed; non-zero lets the installer
# fall back to building upstream source (evdi.sh). Needed even if it only does `return 1`.
FAMILY_evdi_install() { return 1; }

# Optional: run before anything else in the EVDI step, for example to repair a broken package state.
# FAMILY_evdi_prepare() { :; }
