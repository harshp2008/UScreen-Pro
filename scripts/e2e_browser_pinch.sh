#!/usr/bin/env bash
# Pinch on the tablet, in a real browser: Chrome (Wayland) in kiosk mode loads a page that reports the
# wheel events it receives. A native touchpad pinch reaches Chromium as wheel events with ctrlKey set.
# Real fingers are injected on the tablet (scripts/tablet_inject.py). Restores config.toml on exit.
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
  pkill -f "user-data-dir=$SB/chrome" 2>/dev/null; sleep 1
  if findmnt "$SB" >/dev/null 2>&1; then echo "refusing to rm: $SB is a mount"; else rm -rf "$SB"; fi
  echo "[cleanup] config.toml restored, browser closed"
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

cat > "$SB/srv.py" <<'PY'
import http.server, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/log"):
            open(sys.argv[2], "a").write(self.path + "\n")
            self.send_response(204); self.end_headers(); return
        body = open(sys.argv[3], "rb").read()
        self.send_response(200); self.send_header("Content-Type", "text/html"); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
cat > "$SB/page.html" <<'HTML'
<!doctype html><meta charset=utf-8><title>pinch probe</title>
<body style="margin:0;background:#101820;color:#fff;font:48px sans-serif;display:flex;align-items:center;justify-content:center;height:100vh">
<div id=o>pinch probe: waiting for a pinch</div>
<script>
let n = 0, sum = 0;
addEventListener('wheel', e => {
  e.preventDefault();
  if (!e.ctrlKey) { fetch('/log?kind=wheel&dy=' + e.deltaY); return; }
  n++; sum += e.deltaY;
  document.getElementById('o').textContent = 'pinch events: ' + n + ', total deltaY ' + sum.toFixed(1);
  fetch('/log?kind=pinch&dy=' + e.deltaY);
}, { passive: false });
fetch('/log?kind=loaded');
</script>
HTML
python3 "$SB/srv.py" 18765 "$SB/events.txt" "$SB/page.html" &
sleep 0.5

echo "== tablet: Touchpad mode =="
setdev input_mode '"touchpad"'; setdev touchpad_shape '"full_screen"'
sleep 3.5
echo "== Chrome (Wayland, kiosk) opens the probe page =="
google-chrome --user-data-dir="$SB/chrome" --no-first-run --no-default-browser-check --ozone-platform=wayland \
  --kiosk "http://127.0.0.1:18765/" > "$SB/chrome.log" 2>&1 &
for _ in $(seq 1 60); do grep -q "kind=loaded" "$SB/events.txt" 2>/dev/null && break; sleep 0.5; done
grep -q "kind=loaded" "$SB/events.txt" 2>/dev/null && echo "page loaded" || echo "page did NOT load"
sleep 2
echo "== pinch apart, then together, then rotate, on the tablet =="
inj pinch 300 700; sleep 0.6; inj pinch 700 300; sleep 0.6; inj rotate 60; sleep 1.2
python3 - "$SB/events.txt" <<'PY'
import re,sys
ev=[l for l in open(sys.argv[1]).read().splitlines() if "kind=pinch" in l]
dys=[float(re.search(r'dy=(-?[\d.]+)',l).group(1)) for l in ev]
print(f"ctrl+wheel (pinch) events received by the page: {len(dys)}")
if dys:
    print(f"  deltaY: first {dys[0]:+.1f}, last {dys[-1]:+.1f}; zoom-in events (dy<0): {sum(d<0 for d in dys)}, zoom-out events (dy>0): {sum(d>0 for d in dys)}")
PY
