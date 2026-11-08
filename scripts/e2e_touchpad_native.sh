#!/usr/bin/env bash
# End-to-end touchpad check against the deployed daemon and the real tablet: real multi-touch is
# injected into the tablet's touchscreen with scripts/tablet_inject.py, so the app sees genuine
# fingers. Restores config.toml on exit.
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
ROT=${ROT:-90}
# One long-lived injector (it owns the virtual touchscreen and pen on the tablet), driven through a FIFO.
mkfifo "$SB/cmd"
python3 "$HERE/tablet_inject.py" "$SERIAL" "$ROT" < "$SB/cmd" > "$SB/ack" 2> "$SB/err" &
exec 7> "$SB/cmd"
acks=0
inj() {  # inj COMMAND... ; returns when the injector has finished it
  echo "$*" >&7; acks=$((acks+1))
  for _ in $(seq 1 900); do [ "$(grep -c '^ok' "$SB/ack")" -ge $acks ] && return; sleep 0.1; done
  echo "injector did not answer: $*"; cat "$SB/err"
}
for _ in $(seq 1 100); do grep -q '^ready' "$SB/ack" && break; sleep 0.1; done
SHOT_DIR="${1:-$HERE/../experiments_and_findings/screenshots}"

setdev() {  # setdev key value -> device override in [tablets.SERIAL]
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

cat > "$SB/cap.py" <<'PY'
import select, sys, time
import evdev
from evdev import ecodes as e, InputDevice
want = {"UScreen Touch", "UScreen Pen", "UScreen Keyboard", "UScreen Touchpad", "UScreen Gestures", "UScreen Pointer"}
devs = {}
def rescan():
    for path in evdev.list_devices():
        if path in devs:
            continue
        try:
            d = InputDevice(path)
        except OSError:
            continue
        if d.name in want:
            devs[path] = d
            print("capturing:", d.name, flush=True)
t0 = time.time()
last = 0
while time.time() - t0 < float(sys.argv[1]):
    if time.time() - last > 0.4:
        rescan(); last = time.time()
    try:
        r, _, _ = select.select(list(devs.values()), [], [], 0.2)
        for d in r:
            for ev in d.read():
                if ev.type == e.EV_KEY:
                    n = e.bytype[e.EV_KEY].get(ev.code, ev.code)
                    n = n[0] if isinstance(n, list) else n
                    print(f"{d.name}|{n}|{'down' if ev.value else 'up'}", flush=True)
                elif ev.type == e.EV_REL and ev.code in (e.REL_WHEEL, e.REL_HWHEEL):
                    print(f"{d.name}|{'REL_WHEEL' if ev.code == e.REL_WHEEL else 'REL_HWHEEL'}|{ev.value}", flush=True)
    except OSError:
        devs.clear()
PY
mark() { echo "##### $1" >> "$SB/ev.txt"; echo "##### $1" >> "$SB/li.txt"; }
python3 "$SB/cap.py" 150 >> "$SB/ev.txt" 2>&1 &
stdbuf -oL libinput debug-events >> "$SB/li.txt" 2>&1 &
sleep 1

echo "== setup: Touchpad mode, Standard Rectangle (4:3, 2133x1600 centred on 2560x1600: x 213..2347) =="
# The Samsung profile is active on purpose: it must not touch fingers or the touchpad devices.
setdev input_mode '"touchpad"'; setdev touchpad_shape '"standard_rectangle"'; setdev show_stats true
setdev stylus_profile '"samsung_spen"'
# Each settings change restarts the host pipeline for a few seconds, during which the tablet app
# shows a "settings updating" cover that swallows touches; wait it out.
sleep 9
adb -s $SERIAL exec-out screencap -p > "$SHOT_DIR/71_tablet_touchpad_standard_rectangle.png"

run() {  # run LABEL cmd...
  local label="$1"; shift; mark "$label"; "$@"; sleep 0.7
}
[ "${ONLY:-}" = gestures ] || {
echo "== 1. shape restriction =="
run "outside-left(100,800): expect nothing" inj tap-at 100 800
run "outside-right(2500,800): expect nothing" inj tap-at 2500 800
run "inside(1280,800): expect BTN_LEFT" inj tap-at 1280 800
echo "== 2. bottom-right click zone (inside the rectangle) =="
run "inside bottom-right(2000,1500): expect BTN_RIGHT" inj tap-at 2000 1500
run "inside bottom-left(600,1500): expect BTN_LEFT" inj tap-at 600 1500
run "right of the rectangle, bottom(2500,1500): expect nothing" inj tap-at 2500 1500
echo "== 3. multi-finger taps (fingers drift 18 px, land and lift 15 ms apart) =="
run "3 fingers: expect BTN_MIDDLE" inj tap 3 18 150
run "4 fingers: expect Meta+PageUp" inj tap 4 18 150
run "2 fingers (zone method): expect nothing" inj tap 2 6 120
}
echo "== 4. native pinch / rotate =="
if [ -n "${SEQ:-}" ]; then
IFS=';' read -ra STEPS <<< "$SEQ"
for st in "${STEPS[@]}"; do run "$st" inj $st; done
elif [ "${ONLY:-}" = gestures ]; then
run "pinch together FIRST" inj pinch 700 300
run "rotate 70 deg" inj rotate 70
run "pinch apart" inj pinch 300 700
else
run "pinch apart" inj pinch 300 700
run "pinch together" inj pinch 700 300
run "rotate 70 deg" inj rotate 70
fi
sleep 1

echo "== virtual devices (evdev), in order =="
cut -c1-110 "$SB/ev.txt"
echo "== libinput view of the gesture pad (per gesture) =="
python3 - "$SB/li.txt" <<'PY'
import re,sys
lines=open(sys.argv[1]).read().splitlines()
nodes={m.group(1) for l in lines for m in [re.match(r'\s*-?(event\d+)\s+DEVICE_ADDED\s+UScreen Gestures',l)] if m}
section=None; data={}
for l in lines:
    if l.startswith("#####"): section=l[6:]; continue
    m=re.match(r'\s*-?(event\d+)\s+(GESTURE_\w+)\s+\+[\d.]+s\s*(.*)',l)
    if m and m.group(1) in nodes and section: data.setdefault(section,[]).append((m.group(2),m.group(3)))
for sec,evs in data.items():
    kinds={}
    for k,_ in evs: kinds[k]=kinds.get(k,0)+1
    print(f"[{sec}] "+", ".join(f"{k.replace('GESTURE_','')}={v}" for k,v in kinds.items()))
    ups=[re.search(r'([\d.]+) @ (-?[\d.]+)\s*$',r) for k,r in evs if k=="GESTURE_PINCH_UPDATE"]
    ups=[(float(m.group(1)),float(m.group(2))) for m in ups if m]
    if ups: print(f"    last scale {ups[-1][0]:.2f}; rotation, summed over the updates: {sum(a for _,a in ups):+.1f} deg (n={len(ups)})")
PY
