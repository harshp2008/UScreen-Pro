#!/usr/bin/env bash
# End-to-end check against the deployed daemon + tablet. Restores config.toml and removes its profile on exit.
set -u
SB=$(mktemp -d /tmp/w11c-sandbox.XXXXXX)
CFG="$HOME/.config/uscreen/config.toml"
PROF="$HOME/.config/uscreen/stylus_profiles/copy-pen.json"
cp -p "$CFG" "$SB/config.toml.bak"
cleanup() {
  # Restore, then check it stuck: the daemon persists a slot-0 mode change as the global mode and can
  # write a stale copy over the restore a moment later.
  for _ in 1 2 3 4 5; do cp -p "$SB/config.toml.bak" "$CFG"; sleep 2; cmp -s "$SB/config.toml.bak" "$CFG" && break; done
  rm -f "$PROF"; rmdir "$HOME/.config/uscreen/stylus_profiles" 2>/dev/null
  jobs -p | xargs -r kill 2>/dev/null
  if findmnt "$SB" >/dev/null 2>&1; then echo "refusing to rm: $SB is a mount"; else rm -rf "$SB"; fi
  echo "[cleanup] config.toml restored, test profile removed"
}
trap cleanup EXIT
BIN="$HOME/.local/bin/uscreen"
TOKEN_FILE="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/uscreen/token"
SERIAL=R52X708AEHK
OUT="$1"   # repo dir for screenshots

cat > "$SB/ws.py" <<'PY'
import asyncio, json, sys, websockets
async def main(msgs):
    token = open(sys.argv[1]).read().strip()
    async with websockets.connect("ws://127.0.0.1:8891") as ws:
        await ws.send(json.dumps({"type": "auth", "token": token}))
        await asyncio.sleep(0.3)
        for m in msgs:
            if isinstance(m, (int, float)):
                await asyncio.sleep(m)
            else:
                await ws.send(json.dumps(m))
        await asyncio.sleep(0.4)
asyncio.run(main(json.loads(sys.argv[2])))
PY
ws() { python3 "$SB/ws.py" "$TOKEN_FILE" "$1"; }
setdev() {  # setdev key value  -> device override in [tablets.SERIAL]
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

echo "== 0. install profile: S Pen button -> Ctrl+C, make it the active one =="
echo '{"id":"copy-pen","name":"Copy pen","mappings":{"button_primary":"ctrl+c"}}' | "$BIN" stylus-profile-save
python3 - "$CFG" <<'PY'
import re,sys
p=sys.argv[1]; s=open(p).read()
top,rest=s.split('\n[',1) if '\n[' in s else (s,'')
if re.search(r'^stylus_profile = ',top,re.M): top=re.sub(r'^stylus_profile = .*$','stylus_profile = "copy-pen"',top,flags=re.M)
else: top=top.rstrip('\n')+'\nstylus_profile = "copy-pen"\n'
open(p,'w').write(top+('\n['+rest if rest else ''))
PY
sleep 1.5

cat > "$SB/cap.py" <<'PY'
import select, sys, time
import evdev
from evdev import ecodes as e, InputDevice
want = {"UScreen Touch", "UScreen Pen", "UScreen Keyboard", "UScreen Touchpad"}
devs = [d for d in map(InputDevice, evdev.list_devices()) if d.name in want]
print("capturing:", sorted(d.name for d in devs), flush=True)
t0 = time.time()
while time.time() - t0 < float(sys.argv[1]):
    r, _, _ = select.select(devs, [], [], 0.2)
    for d in r:
        for ev in d.read():
            if ev.type == e.EV_KEY:
                n = e.bytype[e.EV_KEY].get(ev.code, ev.code)
                n = n[0] if isinstance(n, list) else n
                print(f"{d.name}|{n}|{'down' if ev.value else 'up'}", flush=True)
            elif ev.type == e.EV_REL and ev.code in (e.REL_WHEEL, e.REL_HWHEEL):
                print(f"{d.name}|{'REL_WHEEL' if ev.code == e.REL_WHEEL else 'REL_HWHEEL'}|{ev.value}", flush=True)
PY
python3 "$SB/cap.py" 80 > "$SB/li.txt" 2>&1 &
sleep 1.5

echo "== 1. Ctrl+C through the real daemon (fake tablet: pen button 5/6 over the input socket) =="
python3 - <<'PY' &
import tkinter as tk
r=tk.Tk(); r.geometry("+200+200"); e=tk.Entry(r,width=40); e.insert(0,"uscreen-copy-test"); e.pack(); r.update()
e.focus_force(); e.select_range(0,'end'); r.after(9000,r.destroy); r.mainloop()
PY
sleep 2.5
printf 'clobber' | wl-copy >/dev/null 2>&1; pkill -x wl-copy; printf 'clobber' | xclip -selection clipboard >/dev/null 2>&1
ws '[{"type":"pen","x":0,"y":0,"pressure":0,"tilt_x":0,"tilt_y":0,"eraser":false,"action":5},0.15,{"type":"pen","x":0,"y":0,"pressure":0,"tilt_x":0,"tilt_y":0,"eraser":false,"action":6}]'
sleep 1
echo "clipboard (wl-paste):  [$(wl-paste -n 2>/dev/null)]"
echo "clipboard (xclip):     [$(xclip -o -selection clipboard 2>/dev/null)]"

echo "== 2. Stylus mode: fingers do nothing =="
grep -q 'input_mode = "stylus"' "$CFG" && echo "device mode in config: stylus"
sleep 0.5; before=$(grep -c "UScreen Touch|BTN_TOUCH|down" "$SB/li.txt")
adb -s $SERIAL shell input tap 800 800; adb -s $SERIAL shell input tap 1900 1400
ws '[{"type":"touch","x":0.5,"y":0.5,"pressure":0.5,"action":0,"slot":0},0.1,{"type":"touch","x":0.5,"y":0.5,"pressure":0,"action":1,"slot":0}]'
sleep 1; after=$(grep -c "UScreen Touch|BTN_TOUCH|down" "$SB/li.txt")
echo "BTN_TOUCH presses on the virtual touchscreen: before=$before after=$after (2 real adb finger taps + 1 forged touch message)"

echo "== 3. Touchpad mode on the tablet: tap, bottom-right tap, haptics =="
setdev input_mode '"touchpad"'; setdev show_stats true
sleep 3
adb -s $SERIAL shell dumpsys vibrator_manager > "$SB/vib0.txt" 2>&1
adb -s $SERIAL shell input tap 600 400; sleep 0.6
adb -s $SERIAL shell input tap 1900 1500; sleep 0.8
adb -s $SERIAL exec-out screencap -p > "$OUT/experiments_and_findings/screenshots/69_tablet_touchpad_zone_no_stats.png"
adb -s $SERIAL shell dumpsys vibrator_manager > "$SB/vib1.txt" 2>&1
echo "vibrations (new entries mentioning duration=50 / ONE_SHOT):"
diff "$SB/vib0.txt" "$SB/vib1.txt" | grep -iE "oneshot|duration=50|50ms|amplitude" | head -4

echo "== 4. Touchpad gestures from the tablet protocol: pinch, rotate, 4/5-finger =="
ws '[{"type":"gesture","kind":"pinch","amount":40},0.1,{"type":"gesture","kind":"rotate","amount":30},0.1,{"type":"gesture","kind":"workspace_prev"},0.1,{"type":"gesture","kind":"workspace_next"}]'
sleep 1

echo "== what the virtual devices emitted (evdev, in order) =="
sleep 1
cut -c1-120 "$SB/li.txt"
pgrep -af "cap.py" | cut -c1-80 | grep -v pgrep || echo "(capture process has exited)"
echo "== daemon log =="
journalctl --user -u uscreen.service --since "-3 min" --no-pager 2>&1 | grep -E "Stylus|stylus|Tablet changed|keyboard|mouse '" | cut -c60-200
pkill -x wl-copy; pkill -x xclip; true
