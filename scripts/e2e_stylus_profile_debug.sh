#!/usr/bin/env bash
# Stylus mode with the Standard and the Samsung S Pen profile: does the pen pipeline behave the same?
# A virtual pen digitizer on the tablet (scripts/tablet_inject.py) hovers across the screen; the host's
# UScreen Pen device is read back for event count, rate and gaps. Also checks which virtual devices
# exist in each mode. Restores config.toml on exit.
set -u
SB=$(mktemp -d /tmp/w11c-sandbox.XXXXXX)
CFG="$HOME/.config/uscreen/config.toml"
cp -p "$CFG" "$SB/config.toml.bak"
cleanup() {
  # Restore, then check it stuck: the daemon persists a slot-0 mode change as the global mode and can
  # write a stale copy over the restore a moment later.
  for _ in 1 2 3 4 5; do cp -p "$SB/config.toml.bak" "$CFG"; sleep 2; cmp -s "$SB/config.toml.bak" "$CFG" && break; done
  echo quit >&7 2>/dev/null; exec 7>&-; sleep 1
  jobs -p | xargs -r kill 2>/dev/null
  if findmnt "$SB" >/dev/null 2>&1; then echo "refusing to rm: $SB is a mount"; else rm -rf "$SB"; fi
  echo "[cleanup] config.toml restored"
}
trap cleanup EXIT
SERIAL=${SERIAL:-R52X708AEHK}
HERE="$(cd "$(dirname "$0")" && pwd)"
setdev() {
python3 - "$CFG" "$SERIAL" "$1" "$2" <<'PY'
import re,sys
p,serial,key,val=sys.argv[1:5]
s=open(p).read()
m=re.search(r'^\[tablets\.%s\]\n(.*?)(?=^\[|\Z)'%re.escape(serial),s,re.S|re.M)
body=m.group(1)
if re.search(r'^%s = .*$'%key,body,re.M): nb=re.sub(r'^%s = .*$'%key,'%s = %s'%(key,val),body,flags=re.M)
else: nb=body.rstrip('\n')+'\n%s = %s\n'%(key,val)
open(p,'w').write(s[:m.start(1)]+nb+s[m.end(1):])
PY
}
mkfifo "$SB/cmd"
python3 "$HERE/tablet_inject.py" "$SERIAL" 90 < "$SB/cmd" > "$SB/ack" 2> "$SB/err" &
exec 7> "$SB/cmd"
acks=0
inj() { echo "$*" >&7; acks=$((acks+1)); for _ in $(seq 1 900); do [ "$(grep -c '^ok' "$SB/ack")" -ge $acks ] && return; sleep 0.1; done; echo "injector did not answer: $*"; }
for _ in $(seq 1 100); do grep -q '^ready' "$SB/ack" && break; sleep 0.1; done

cat > "$SB/pen.py" <<'PY'
# Reads the host's virtual pen: counts position reports and the gaps between them.
import select, sys, time
import evdev
from evdev import ecodes as e, InputDevice
dev = next((d for d in map(InputDevice, evdev.list_devices()) if d.name == "UScreen Pen"), None)
if dev is None: sys.exit("no UScreen Pen")
t0 = time.time(); stamps = []; tool = 0
while time.time() - t0 < float(sys.argv[1]):
    r, _, _ = select.select([dev], [], [], 0.2)
    for ev in dev.read() if r else []:
        if ev.type == e.EV_ABS and ev.code == e.ABS_X: stamps.append(time.time())
        if ev.type == e.EV_KEY and ev.code in (e.BTN_TOOL_PEN, e.BTN_TOOL_RUBBER) and ev.value: tool += 1
gaps = [b - a for a, b in zip(stamps, stamps[1:])]
print(f"RESULT reports={len(stamps)} tool_enter={tool} max_gap_ms={max(gaps)*1000 if gaps else 0:.0f} "
      f"median_gap_ms={sorted(gaps)[len(gaps)//2]*1000 if gaps else 0:.1f}", flush=True)
PY
devnames() { grep -o 'Name="UScreen[^"]*"' /proc/bus/input/devices | sed 's/Name=//' | tr '\n' ' '; }

echo "== mode switch: Touchpad (a click creates the relative mouse), then Stylus =="
setdev input_mode '"touchpad"'; sleep 3.5
inj tap-at 1280 800; inj pinch 300 700; sleep 0.5
echo "devices in Touchpad mode: $(devnames)"
setdev input_mode '"stylus"'; sleep 3
echo "devices in Stylus mode:   $(devnames)"
case "$(devnames)" in *Touchpad*|*Gestures*) echo "  -> FAIL: a touchpad-class device survived Stylus mode";; *) echo "  -> ok: no UScreen Touchpad / Gestures in Stylus mode";; esac

for profile in standard samsung_spen standard samsung_spen; do
  setdev stylus_profile "\"$profile\""; sleep 3
  echo "== Stylus mode, profile=$profile =="
  adb -s $SERIAL logcat -c
  python3 "$SB/pen.py" 9 > "$SB/pen.txt" 2>&1 &
  sleep 1
  inj pen-move 400 800 2200 800 200
  inj pen-hover 1280 800 100
  inj pen-move 2200 300 400 1400 200
  wait %python3 2>/dev/null; sleep 9.5
  grep RESULT "$SB/pen.txt" || cat "$SB/pen.txt"
  adb -s $SERIAL logcat -d -s UScreenSPen:* | tail -2 | cut -c1-120
done
