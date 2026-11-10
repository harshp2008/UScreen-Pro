#!/usr/bin/env bash
# Checks the touchpad and stylus engines against the deployed daemon and the real tablet.
#
# Real fingers and a real pen are created on the tablet as virtual HID devices over /dev/uhid
# (scripts/tablet_inject.py): contacts have a width, a height and a pressure, fingers land and lift
# a few milliseconds apart, a slow drag has a hand's tremor. The app receives genuine MotionEvents;
# what the computer's virtual devices then emit is read back with evdev and checked. Settings are
# changed in config.toml; the file is restored (and the restore verified) on exit.
#
#   scripts/e2e_input_engine.sh            run everything, print PASS/FAIL per check
#   SEQ='slowdrag 800 800 1400 800 5' scripts/e2e_input_engine.sh   run only these injector commands
#   BROWSER=1 scripts/e2e_input_engine.sh  also pinch inside Chrome
set -u
SB=$(mktemp -d /tmp/w11c-sandbox.XXXXXX)
CFG="$HOME/.config/uscreen/config.toml"
cp -p "$CFG" "$SB/config.toml.bak"
# Ends the run: stops the injector and background jobs, restores the saved config.toml (retrying, since the daemon may overwrite it with a stale copy) and removes the sandbox unless it is a mount point.
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
# Prints the global input_mode line of the config.
global_mode() { grep -m1 '^input_mode' "$CFG"; }

# setdev KEY VALUE: sets a per-device override in the tablet's [tablets.SERIAL] section of the config.
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
# Sends one command line to the injector and waits (up to 90 s) for its ack.
inj() { echo "$*" >&7; acks=$((acks+1)); for _ in $(seq 1 900); do [ "$(grep -c '^ok' "$SB/ack")" -ge $acks ] && return; sleep 0.1; done; echo "injector did not answer: $*"; }
for _ in $(seq 1 100); do grep -q '^ready' "$SB/ack" && break; sleep 0.1; done

cat > "$SB/cap.py" <<'PY'
# Prints what the computer's virtual devices emit, with a timestamp: keys and buttons, wheel,
# relative moves, and the absolute position reports of the pen and the pointer.
import select, sys, time
import evdev
from evdev import ecodes as e, InputDevice
want = {"UScreen Touch", "UScreen Pen", "UScreen Keyboard", "UScreen Touchpad", "UScreen Pointer"}
devs = {}
# (embedded Python) Opens any of UScreen's virtual input devices that have appeared since the last scan.
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
                ts = ev.timestamp()
                if ev.type == e.EV_KEY:
                    n = e.bytype[e.EV_KEY].get(ev.code, ev.code); n = n[0] if isinstance(n, list) else n
                    print(f"{ts:.3f}|{d.name}|{n}|{'down' if ev.value else 'up'}", flush=True)
                elif ev.type == e.EV_REL and ev.code in (e.REL_WHEEL, e.REL_HWHEEL, e.REL_X, e.REL_Y):
                    print(f"{ts:.3f}|{d.name}|{e.bytype[e.EV_REL][ev.code]}|{ev.value}", flush=True)
                elif ev.type == e.EV_ABS and ev.code == e.ABS_X:
                    print(f"{ts:.3f}|{d.name}|ABS_X|{ev.value}", flush=True)
    except OSError:
        devs.clear()
PY
# Writes a label line to the event log so the captured events can be split per step.
mark() { echo "##### $1" >> "$SB/ev.txt"; }
python3 "$SB/cap.py" 300 >> "$SB/ev.txt" 2>&1 &
sleep 1
# run LABEL COMMAND...: marks LABEL in the log, runs the command, then waits 0.8 s for the events to settle.
run() { local label="$1"; shift; mark "$label"; "$@"; sleep 0.8; }

echo "== setup: Touchpad mode, Standard Rectangle (2133x1600 centred). global: $(global_mode) =="
setdev input_mode '"touchpad"'; setdev touchpad_shape '"standard_rectangle"'; setdev right_click_method '"bottom_right_zone"'
sleep 9

if [ -n "${SEQ:-}" ]; then
IFS=';' read -ra STEPS <<< "$SEQ"
for st in "${STEPS[@]}"; do run "$st" inj $st; done
else
echo "== touchpad: taps, zone, shape =="
run "tap outside-left (100,800)" inj tap-at 100 800
run "tap inside (1000,800)" inj tap-at 1000 800
run "tap bottom-right zone (2000,1500)" inj tap-at 2000 1500
run "tap bottom-left (600,1500)" inj tap-at 600 1500
run "tap bottom-right outside the shape (2500,1500)" inj tap-at 2500 1500
echo "== touchpad: multi-finger taps (fingers drift, land 15 ms apart) =="
run "tap with 2 fingers" inj tap 2 6 100
run "tap with 3 fingers" inj tap 3 18 100
echo "== touchpad: movement =="
run "slow drag with tremor: 400 px in 5 s" inj slowdrag 1000 800 1400 800 5 1.5
run "fast drag 400 px" inj drag 1000 800 1400 800 25 16
echo "== touchpad: scroll, pinch, swipes, tap-drag =="
run "two-finger scroll 300 px" inj scroll2 300 30
run "pinch apart" inj pinch 300 700
run "pinch together" inj pinch 700 300
run "4-finger swipe up" inj swipe 4 0 -300 450
run "3-finger swipe left" inj swipe 3 -300 0 450
run "tap-drag 200 px" inj tap-drag 1000 800 200
fi
sleep 1

if [ "${BROWSER:-0}" = 1 ]; then
echo "== pinch inside Chrome (Wayland kiosk probe page; a native pinch reaches Chromium as ctrl+wheel) =="
cat > "$SB/srv.py" <<'PY'
import http.server, sys
# (embedded Python) Serves the probe page and appends every /log request to the events file.
class H(http.server.BaseHTTPRequestHandler):
    # (embedded Python) Answers /log with 204 after recording it, anything else with the probe page.
    def do_GET(self):
        if self.path.startswith("/log"):
            open(sys.argv[2], "a").write(self.path + "\n"); self.send_response(204); self.end_headers(); return
        self.send_response(200); self.send_header("Content-Type", "text/html"); self.end_headers(); self.wfile.write(open(sys.argv[3], "rb").read())
    # (embedded Python) Silences the request log.
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
print(f"Chrome received {len(dys)} pinch (ctrl+wheel) events: {sum(d<0 for d in dys)} zoom-in, {sum(d>0 for d in dys)} zoom-out; deltaY sum {sum(dys):+.0f}")
PY
pkill -f "user-data-dir=$SB/chrome" 2>/dev/null; sleep 1
fi

if [ -z "${SEQ:-}" ]; then
echo "== Stylus mode (per-device override), global stays: $(global_mode) =="
setdev input_mode '"stylus"'; sleep 9
echo "global input mode after the device switched to stylus: $(global_mode)"
echo "devices now: $(grep -o 'Name="UScreen[^"]*"' /proc/bus/input/devices | sed 's/Name=//' | tr '\n' ' ')"
run "stylus: finger tap" inj tap-at 1280 800
run "stylus: finger drag" inj drag 800 800 1600 800 30 16
run "stylus: 3-finger tap" inj tap 3 18 100
run "stylus: pen hovers across the screen" inj pen-move 400 800 2200 800 120
run "stylus: pen down, 10+ moves, up (text selection)" inj pen-drag 600 800 1600 800 24
run "stylus: side button held over the screen" inj pen-button 1280 800 400
run "stylus: drag with the side button held" inj pen-drag 600 900 1600 900 24 300
setdev stylus_button_primary '"middle"'; sleep 4
run "stylus: side button after changing the setting to middle" inj pen-button 1280 800 400
echo "global input mode at the end: $(global_mode)"
fi
sleep 1

python3 - "$SB/ev.txt" "${SEQ:+seq}" <<'PY'
import sys,collections
sec=None; data=collections.OrderedDict()
for l in open(sys.argv[1]).read().splitlines():
    if l.startswith("#####"): sec=l[6:]; data[sec]=[]; continue
    p=l.split("|")
    if sec and len(p)==4: data[sec].append((float(p[0]),p[1].replace("UScreen ",""),p[2],p[3]))

# (embedded Python) Turns a list of device events into a compact readable sequence, merging repeats.
def summary(ev):
    toks=[]
    for ts,dev,n,v in ev:
        if dev=="Pen" and n in ("BTN_TOOL_PEN","BTN_TOUCH","BTN_DIGI"): continue
        if n in ("REL_X","REL_Y"): t="move"
        elif n=="REL_WHEEL": t="wheel"
        elif n=="ABS_X": t=f"{dev.lower()}-position"
        else: t=f"{dev}:{n.replace('BTN_','').replace('KEY_','')}-{v}"
        toks.append(t)
    out=[]
    for t in toks:
        if out and out[-1][0]==t: out[-1][1]+=1
        else: out.append([t,1])
    return " ".join(t if c==1 else f"{t}x{c}" for t,c in out) or "(nothing)"

print("== what the virtual devices emitted ==")
for s,ev in data.items(): print(f"[{s}]\n    {summary(ev)}")
if len(sys.argv)>2 and sys.argv[2]: sys.exit(0)

# (embedded Python) The key and button events of one device, optionally of one name.
def keys(ev,dev,name=None):
    return [(ts,n,v) for ts,d,n,v in ev if d==dev and n.startswith(("BTN_","KEY_")) and (name is None or n==name)]
# (embedded Python) The events recorded under the step whose label starts with [prefix].
def sec(prefix):
    for k in data:
        if k.startswith(prefix): return data[k]
    return []
results=[]
# (embedded Python) Prints PASS or FAIL for one check and records the result.
def check(name, ok, detail=""):
    results.append(ok); print(("PASS " if ok else "FAIL ")+name+(f"  ({detail})" if detail else ""))

print("\n== checks ==")
# (embedded Python) The values (down or up) of button [btn] on the touchpad in [ev].
def clicks(ev,btn): return [v for ts,n,v in keys(ev,"Touchpad") if n==btn]
check("tap outside the shape does nothing", not keys(sec("tap outside-left"),"Touchpad"))
check("tap inside = one left click", clicks(sec("tap inside"),"BTN_LEFT")==["down","up"], str(clicks(sec("tap inside"),"BTN_LEFT")))
check("tap in bottom-right zone = right click", clicks(sec("tap bottom-right zone"),"BTN_RIGHT")==["down","up"])
check("tap bottom-left = left click", clicks(sec("tap bottom-left"),"BTN_LEFT")==["down","up"])
check("tap with 2 fingers = right click", clicks(sec("tap with 2 fingers"),"BTN_RIGHT")==["down","up"])
check("tap with 3 fingers = middle click", clicks(sec("tap with 3 fingers"),"BTN_MIDDLE")==["down","up"])
sd=sec("slow drag with tremor")
rel=[(ts,int(v)) for ts,d,n,v in sd if d=="Touchpad" and n=="REL_X"]
total=sum(v for _,v in rel)
gaps=[b[0]-a[0] for a,b in zip(rel,rel[1:]) if a[0]>rel[0][0]+0.6]
check("slow drag with tremor moves the right distance (400 px x gain 1.6 = 640)", 540<=total<=740, f"x total {total}")
check("slow drag never stalls", bool(rel) and max(gaps,default=0)<0.2, f"{len(rel)} reports, longest gap {max(gaps,default=0)*1000:.0f} ms")
check("slow drag makes no click", not keys(sd,"Touchpad"))
sc=sec("two-finger scroll"); wheel=sum(int(v) for ts,d,n,v in sc if n=="REL_WHEEL")
check("two-finger scroll turns the wheel, no click", wheel!=0 and not keys(sc,"Touchpad"), f"wheel total {wheel}")
for name in ("pinch apart","pinch together"):
    ev=sec(name); kb=[(n,v) for ts,d,n,v in ev if d=="Keyboard"]
    check(f"{name}: Ctrl held once with wheel steps", kb==[("KEY_LEFTCTRL","down"),("KEY_LEFTCTRL","up")] and any(n=="REL_WHEEL" for ts,d,n,v in ev), str(kb))
sw4=[(n,v) for ts,d,n,v in sec("4-finger swipe up") if d=="Keyboard"]
check("4-finger swipe up detected once (overview: Super)", sw4==[("KEY_LEFTMETA","down"),("KEY_LEFTMETA","up")], str(sw4))
sw3=[(n,v) for ts,d,n,v in sec("3-finger swipe left") if d=="Keyboard"]
check("3-finger swipe left = workspace next (Super+PageDown)", ("KEY_PAGEDOWN","down") in sw3 and sw3[0]==("KEY_LEFTMETA","down"), str(sw3))
td=sec("tap-drag"); ev=[(ts,d,n,v) for ts,d,n,v in td if d=="Touchpad"]
btn=[(i,v) for i,(ts,d,n,v) in enumerate(ev) if n=="BTN_LEFT"]
moves=[i for i,(ts,d,n,v) in enumerate(ev) if n=="REL_X" and btn and i>btn[0][0]]
check("tap-drag: button down before the motion, up after it, one pair", [v for _,v in btn]==["down","up"] and moves and btn[0][0]<moves[0] and btn[1][0]>moves[-1], str([v for _,v in btn]))
# stylus
check("stylus: finger tap, drag and 3-finger tap do nothing", not (keys(sec("stylus: finger tap"),"Pointer") or keys(sec("stylus: finger tap"),"Touchpad") or [1 for k in ("stylus: finger drag","stylus: 3-finger tap") for x in sec(k) if x[2] in ("BTN_TOUCH","BTN_LEFT","BTN_MIDDLE","REL_X","ABS_X")]))
pd=sec("stylus: pen down, 10+ moves")
pk=[(ts,n,v) for ts,n,v in keys(pd,"Pointer") if n=="BTN_LEFT"]
pos=[ts for ts,d,n,v in pd if d=="Pointer" and n=="ABS_X"]
held=[t for t in pos if len(pk)==2 and pk[0][0]<t<pk[1][0]]
check("stylus drag-select: BTN_LEFT down at pen-down, up at pen-up, nothing between", [v for _,_,v in pk]==["down","up"], str([v for _,_,v in pk]))
check("stylus drag-select: the pointer moved while BTN_LEFT was held", len(held)>=10, f"{len(held)} position reports while held")
hv=sec("stylus: pen hovers across")
check("stylus hover moves the cursor (pointer positions, no click)", len([1 for ts,d,n,v in hv if d=="Pointer" and n=="ABS_X"])>20 and not keys(hv,"Pointer"))
sb=[(n,v) for ts,n,v in keys(sec("stylus: side button held over"),"Pointer")]
check("stylus side button = right click (default)", sb==[("BTN_RIGHT","down"),("BTN_RIGHT","up")], str(sb))
sb2=[(n,v) for ts,n,v in keys(sec("stylus: side button after"),"Pointer")]
check("stylus side button after the setting changed = middle click", sb2==[("BTN_MIDDLE","down"),("BTN_MIDDLE","up")], str(sb2))
print(f"\n{sum(results)}/{len(results)} checks passed")
PY
