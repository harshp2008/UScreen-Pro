#!/usr/bin/env bash
# Checks the touchpad and stylus engines against the deployed daemon and the real tablet.
#
# Real fingers and a real pen are created on the tablet as virtual HID devices over /dev/uhid
# (scripts/tablet_inject.py), so the app receives genuine MotionEvents; what the computer's virtual
# devices then emit is read back with evdev. Settings are changed in config.toml; the file is
# restored (and the restore verified) on exit. Set BROWSER=1 to also pinch inside Chrome.
set -u
SB=$(mktemp -d /tmp/w11c-sandbox.XXXXXX)
CFG="$HOME/.config/uscreen/config.toml"
cp -p "$CFG" "$SB/config.toml.bak"
cleanup() {
  echo quit >&7 2>/dev/null; exec 7>&-; sleep 1
  jobs -p | xargs -r kill 2>/dev/null
  pkill -f "user-data-dir=$SB/chrome" 2>/dev/null
  # Restore, then check it stuck (the daemon may write a stale copy over it a moment later).
  for _ in 1 2 3 4 5; do cp -p "$SB/config.toml.bak" "$CFG"; sleep 2; cmp -s "$SB/config.toml.bak" "$CFG" && break; done
  if findmnt "$SB" >/dev/null 2>&1; then echo "refusing to rm: $SB is a mount"; else rm -rf "$SB"; fi
  echo "[cleanup] config.toml restored"
}
trap cleanup EXIT
SERIAL=${SERIAL:-R52X708AEHK}
HERE="$(cd "$(dirname "$0")" && pwd)"
SHOTS="${1:-$HERE/../experiments_and_findings/screenshots}"
global_mode() { grep -m1 '^input_mode' "$CFG"; }

setdev() {  # setdev KEY VALUE: a device override in [tablets.SERIAL]
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

cat > "$SB/cap.py" <<'PY'
# Prints what the computer's virtual devices emit: keys and buttons, wheel, relative moves, and the
# number of absolute position reports of the pen and the pointer.
import select, sys, time
import evdev
from evdev import ecodes as e, InputDevice
want = {"UScreen Touch", "UScreen Pen", "UScreen Keyboard", "UScreen Touchpad", "UScreen Pointer"}
devs = {}
def rescan():
    for path in evdev.list_devices():
        if path in devs: continue
        try: d = InputDevice(path)
        except OSError: continue
        if d.name in want: devs[path] = d
t0 = time.time(); last = 0
while time.time() - t0 < float(sys.argv[1]):
    if time.time() - last > 0.4: rescan(); last = time.time()
    try:
        r, _, _ = select.select(list(devs.values()), [], [], 0.2)
        for d in r:
            for ev in d.read():
                if ev.type == e.EV_KEY:
                    n = e.bytype[e.EV_KEY].get(ev.code, ev.code); n = n[0] if isinstance(n, list) else n
                    print(f"{d.name}|{n}|{'down' if ev.value else 'up'}", flush=True)
                elif ev.type == e.EV_REL and ev.code in (e.REL_WHEEL, e.REL_HWHEEL, e.REL_X, e.REL_Y):
                    print(f"{d.name}|{e.bytype[e.EV_REL][ev.code]}|{ev.value}", flush=True)
                elif ev.type == e.EV_ABS and ev.code == e.ABS_X:
                    print(f"{d.name}|ABS_X|{ev.value}", flush=True)
    except OSError:
        devs.clear()
PY
mark() { echo "##### $1" >> "$SB/ev.txt"; }
python3 "$SB/cap.py" 200 >> "$SB/ev.txt" 2>&1 &
sleep 1
run() { local label="$1"; shift; mark "$label"; "$@"; sleep 0.7; }

echo "== setup: Touchpad mode, Standard Rectangle (2133x1600 centred). global: $(global_mode) =="
setdev input_mode '"touchpad"'; setdev touchpad_shape '"standard_rectangle"'
sleep 9
adb -s $SERIAL exec-out screencap -p > "$SHOTS/72_tablet_touchpad_standard_rectangle.png"
TP_EV=$(grep -l 'UScreen Touchpad' /sys/class/input/event*/device/name 2>/dev/null | head -1)
if [ -n "$TP_EV" ]; then
  PROPS=$(cat "$(dirname "$TP_EV")/properties")
  echo "UScreen Touchpad input properties bitmask: $PROPS (1 = INPUT_PROP_POINTER only; 2 would be BUTTONPAD, 4 DIRECT)"
fi

if [ -n "${SEQ:-}" ]; then
IFS=';' read -ra STEPS <<< "$SEQ"
for st in "${STEPS[@]}"; do run "$st" inj $st; done
else
echo "== touchpad: taps, zone, shape =="
run "outside-left(100,800): nothing" inj tap-at 100 800
run "outside-right(2500,800): nothing" inj tap-at 2500 800
run "inside(1280,800): BTN_LEFT" inj tap-at 1280 800
run "bottom-right inside(2000,1500): BTN_RIGHT" inj tap-at 2000 1500
run "bottom-left inside(600,1500): BTN_LEFT" inj tap-at 600 1500
run "bottom-right outside(2500,1500): nothing" inj tap-at 2500 1500
echo "== touchpad: multi-finger taps (fingers drift 18 px, land 15 ms apart, 100 ms tap) =="
run "3 fingers: BTN_MIDDLE" inj tap 3 18 100
run "4 fingers: Meta+PageUp" inj tap 4 18 100
run "2 fingers (zone method): nothing" inj tap 2 6 100
echo "== touchpad: movement, scroll, pinch, tap-drag =="
run "slow drag 40 px in 100 steps (0.4 px/step)" inj drag 1000 800 1040 800 100 16
run "fast drag 400 px in 25 steps" inj drag 1000 800 1400 800 25 16
run "two-finger scroll 300 px" inj scroll2 300 30
run "pinch apart" inj pinch 300 700
run "pinch together" inj pinch 700 300
run "tap-drag 200 px" inj tap-drag 1000 800 200
fi
sleep 1

if [ "${BROWSER:-0}" = 1 ]; then
echo "== pinch inside Chrome (Wayland kiosk probe page; a native pinch reaches Chromium as ctrl+wheel) =="
cat > "$SB/srv.py" <<'PY'
import http.server, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/log"):
            open(sys.argv[2], "a").write(self.path + "\n"); self.send_response(204); self.end_headers(); return
        self.send_response(200); self.send_header("Content-Type", "text/html"); self.end_headers(); self.wfile.write(open(sys.argv[3], "rb").read())
    def log_message(self, *a): pass
http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
cat > "$SB/page.html" <<'HTML'
<!doctype html><meta charset=utf-8><title>pinch probe</title>
<body style="margin:0;background:#101820;color:#fff;font:48px sans-serif;display:flex;align-items:center;justify-content:center;height:100vh"><div id=o>waiting for a pinch</div>
<script>let n=0;addEventListener('wheel',e=>{e.preventDefault();if(!e.ctrlKey)return;n++;document.getElementById('o').textContent='pinch events: '+n;fetch('/log?kind=pinch&dy='+e.deltaY)},{passive:false});fetch('/log?kind=loaded')</script>
HTML
python3 "$SB/srv.py" 18765 "$SB/events.txt" "$SB/page.html" &
sleep 0.5
google-chrome --user-data-dir="$SB/chrome" --no-first-run --no-default-browser-check --ozone-platform=wayland --kiosk "http://127.0.0.1:18765/" > "$SB/chrome.log" 2>&1 &
for _ in $(seq 1 60); do grep -q "kind=loaded" "$SB/events.txt" 2>/dev/null && break; sleep 0.5; done
sleep 2
mark "chrome pinch apart then together"; inj pinch 300 700; sleep 0.6; inj pinch 700 300; sleep 1.2
python3 - "$SB/events.txt" <<'PY'
import re,sys
dys=[float(re.search(r'dy=(-?[\d.]+)',l).group(1)) for l in open(sys.argv[1]).read().splitlines() if "kind=pinch" in l]
print(f"Chrome received {len(dys)} pinch (ctrl+wheel) events: {sum(d<0 for d in dys)} zoom-in, {sum(d>0 for d in dys)} zoom-out; deltaY sum {sum(dys):+.0f}, largest {max(map(abs,dys),default=0):.0f}")
PY
pkill -f "user-data-dir=$SB/chrome" 2>/dev/null; sleep 1
fi

if [ -z "${SEQ:-}" ]; then
echo "== Stylus mode (per-device override), global stays: $(global_mode) =="
setdev input_mode '"stylus"'; sleep 9
echo "global input mode after the device switched to stylus: $(global_mode)"
echo "devices now: $(grep -o 'Name="UScreen[^"]*"' /proc/bus/input/devices | sed 's/Name=//' | tr '\n' ' ')"
run "stylus: finger tap + drag + 3-finger tap: nothing" inj tap-at 1280 800
run "stylus: finger drag" inj drag 800 800 1600 800 30 16
run "stylus: 3-finger tap" inj tap 3 18 100
run "stylus: pen hovers across the screen" inj pen-move 400 800 2200 800 120
run "stylus: pen side button held 400 ms (default: middle click)" inj pen-button 1280 800 400
setdev stylus_button_primary '"right_click"'; sleep 4
run "stylus: side button after changing the setting to right click" inj pen-button 1280 800 400
echo "global input mode at the end: $(global_mode)"
fi
sleep 1

echo "== what the virtual devices emitted =="
python3 - "$SB/ev.txt" <<'PY'
import re,sys,collections
sec=None; data=collections.OrderedDict()
for l in open(sys.argv[1]).read().splitlines():
    if l.startswith("#####"): sec=l[6:]; data[sec]=[]; continue
    if sec and "|" in l: data[sec].append(l.split("|"))
for sec,ev in data.items():
    toks=[]
    for d,n,v in ev:
        dev=d.replace("UScreen ","")
        if n in ("BTN_TOOL_PEN","BTN_TOUCH","BTN_DIGI") and dev=="Pen": continue
        if n in ("REL_X","REL_Y"): t="move"
        elif n=="REL_WHEEL": t="wheel"
        elif n=="ABS_X": t=f"{dev.lower()}-position"
        else: t=f"{dev}:{n.replace('BTN_','').replace('KEY_','')}-{v}"
        toks.append(t)
    out=[]
    for t in toks:
        if out and out[-1][0]==t: out[-1][1]+=1
        else: out.append([t,1])
    line=" ".join(t if c==1 else f"{t}x{c}" for t,c in out)
    rx=sum(int(v) for d,n,v in ev if n=="REL_X"); wh=sum(int(v) for d,n,v in ev if n=="REL_WHEEL")
    extra=[]
    if rx: extra.append(f"x total {rx}")
    if wh: extra.append(f"wheel total {wh}")
    print(f"[{sec}]\n    {line or '(nothing)'}" + (f"   ({', '.join(extra)})" if extra else ""))
PY
