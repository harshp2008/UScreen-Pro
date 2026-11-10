#!/usr/bin/env bash
# Supervised live test for experiments_and_findings/56_evdi_mutter_freeze_rca.md
#
# Runs the freshly built daemon MANUALLY (never via the enabled service) in a
# time-limited transient user unit, while an independent probe measures how long
# GNOME Shell's main loop takes to answer. If the shell stops answering, the
# probe kills the daemon, its helper and ffmpeg from outside the daemon, so the
# monitor goes away even if the daemon's own watchdog could not run.
#
# Preconditions (the only steps that need you):
#   sudo modprobe evdi initial_device_count=2     # nothing here uses sudo
#   (optional) plug in / wake the tablet and open the UScreen app
#
# Usage:  scripts/rca56_guarded_test.sh [seconds=120]
set -u
cd "$(dirname "$0")/.."
SECS=${1:-120}
UNIT=uscreen-rca56
BIN="$PWD/target/debug/uscreen"

[ -x "$BIN" ] || { echo "build first: (cd host && cargo build)"; exit 2; }
lsmod | grep -q '^evdi' || { echo "evdi is not loaded: sudo modprobe evdi initial_device_count=2"; exit 2; }
systemctl --user is-enabled uscreen.service 2>/dev/null | grep -q enabled && { echo "uscreen.service is enabled; disable it first"; exit 2; }
pgrep -f "uscreen start" >/dev/null && { echo "a uscreen daemon is already running"; exit 2; }

OUT="$PWD/experiments_and_findings/56_evidence/live_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$OUT"
STATE_HOME=$(mktemp -d /tmp/w11c-sandbox.XXXXXX)   # keep the real guard.json untouched

# Stops the test service and kills any helper or encoder left over.
kill_all() {
  systemctl --user kill -s KILL "$UNIT.service" 2>/dev/null
  systemctl --user stop "$UNIT.service" 2>/dev/null
  pkill -9 -f evdi_helper 2>/dev/null; pkill -9 -f "ffmpeg.*uscreen" 2>/dev/null
}
# On exit: stops everything, removes the private state directory (never a mount point) and prints where the evidence is.
cleanup() {
  kill_all
  findmnt "$STATE_HOME" >/dev/null 2>&1 || rm -rf -- "$STATE_HOME"
  echo "evidence in: $OUT"
}
trap cleanup EXIT INT TERM

# Prints how many milliseconds a GNOME Mutter display-config call takes, or -1 when it does not answer within 3 s: the compositor-stall probe.
probe() { # prints latency in ms, or -1 on timeout
  local t0 t1
  t0=$(date +%s%N)
  if timeout 3 busctl --user call org.gnome.Mutter.DisplayConfig /org/gnome/Mutter/DisplayConfig \
       org.gnome.Mutter.DisplayConfig GetCurrentState >/dev/null 2>&1; then
    t1=$(date +%s%N); echo $(( (t1 - t0) / 1000000 ))
  else
    echo -1
  fi
}

systemd-run --user --unit="$UNIT" --collect -p RuntimeMaxSec="$SECS" -p Nice=5 -p CPUWeight=50 -p OOMScoreAdjust=300 \
  --setenv=XDG_STATE_HOME="$STATE_HOME" --setenv=XDG_CURRENT_DESKTOP="${XDG_CURRENT_DESKTOP:-}" \
  --setenv=XDG_SESSION_TYPE="${XDG_SESSION_TYPE:-}" --setenv=WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}" \
  --setenv=DISPLAY="${DISPLAY:-}" ${USCREEN_CURSOR_EVENTS:+--setenv=USCREEN_CURSOR_EVENTS="$USCREEN_CURSOR_EVENTS"} --working-directory="$PWD" "$BIN" start >/dev/null || exit 1
echo "daemon started in $UNIT.service (hard limit ${SECS}s); probing every 250 ms -> $OUT/probe.csv"

echo "t_ms,latency_ms" > "$OUT/probe.csv"
start=$(date +%s%N); bad=0
while systemctl --user is-active --quiet "$UNIT.service"; do
  l=$(probe); now=$(( ($(date +%s%N) - start) / 1000000 ))
  echo "$now,$l" >> "$OUT/probe.csv"
  if [ "$l" -lt 0 ] || [ "$l" -gt 2000 ]; then bad=$((bad + 1)); else bad=0; fi
  if [ "$bad" -ge 2 ]; then
    echo "!! shell main loop stalled at ${now} ms - killing daemon, helper and ffmpeg"
    echo "STALL at ${now} ms" > "$OUT/RESULT"; kill_all; break
  fi
  sleep 0.25
done
journalctl --user -u "$UNIT.service" --no-pager -o short-precise > "$OUT/daemon.log" 2>&1
journalctl -k --since "-$((SECS + 30))s" --no-pager > "$OUT/kernel.log" 2>&1
journalctl --user -t gnome-shell --since "-$((SECS + 30))s" --no-pager > "$OUT/gnome-shell.log" 2>&1
[ -f "$OUT/RESULT" ] || awk -F, 'NR>1 && $2>m {m=$2} END{print "NO STALL; worst probe " m " ms over " NR-1 " samples"}' "$OUT/probe.csv" | tee "$OUT/RESULT"
cat "$OUT/RESULT"
