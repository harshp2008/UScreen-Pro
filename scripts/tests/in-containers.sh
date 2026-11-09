#!/usr/bin/env bash
# Run the installer test suite inside one container per distribution family (needs docker or podman).
#   scripts/tests/in-containers.sh               every image below
#   scripts/tests/in-containers.sh fedora:41     only these
# The repository is mounted read-only; the tests write only inside the container's /tmp.
set -uo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENGINE="$(command -v docker || command -v podman || true)"
[ -n "$ENGINE" ] || { echo "docker or podman is needed"; exit 2; }
IMAGES=("$@")
[ ${#IMAGES[@]} -gt 0 ] || IMAGES=(ubuntu:24.04 ubuntu:22.04 debian:12 fedora:41 archlinux:latest opensuse/tumbleweed:latest)
fail=0
for img in "${IMAGES[@]}"; do
    echo "=== $img"
    # Most minimal images lack a tool or two the suite uses (findmnt, awk, script): add them first.
    prep='true'
    case "$img" in
        ubuntu*|debian*) prep='apt-get update -qq >/dev/null && apt-get install -y -qq util-linux gawk findutils >/dev/null' ;;
        fedora*)         prep='dnf install -y -q util-linux gawk findutils >/dev/null' ;;
        archlinux*)      prep='pacman -Sy --noconfirm --needed util-linux gawk findutils >/dev/null' ;;
        opensuse*)       prep='zypper -n -q install util-linux gawk findutils >/dev/null' ;;
    esac
    if ! "$ENGINE" run --rm -v "$REPO:/repo:ro" "$img" bash -c "$prep; bash /repo/scripts/tests/run.sh 2>&1 | tail -n 15"; then
        echo "FAILED on $img"; fail=1
    fi
done
exit $fail
