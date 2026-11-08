#!/usr/bin/env python3
"""Real multi-touch fingers and a real pen on an Android tablet, driven from the PC over adb.

`adb shell input` can only make single-pointer taps, and the kernel input nodes are not writable
from the shell (SELinux). The shell *can* open /dev/uhid, so this creates virtual HID devices on
the tablet: a 5-contact touchscreen and a pen digitizer. Android treats them as real hardware, so
the tablet app receives genuine MotionEvents with several pointers, or hovering stylus events.
The devices live exactly as long as this process (they vanish when it ends).

One process serves a whole test run: it reads one command per line from stdin and prints `ok`
after each (so a shell script can drive it through a FIFO):

  tap-at X Y                      one finger (view coordinates, landscape 2560x1600)
  rawtap RX RY                    one finger at raw panel coordinates (calibration)
  tap FINGERS [JITTER [HOLD_MS]]  fingers land and lift 15 ms apart around the centre
  pinch START END                 two fingers, distance in px
  rotate DEGREES                  two fingers turning around the centre
  pen-hover X Y COUNT             pen hovering over (X, Y), wiggling slightly
  pen-move X0 Y0 X1 Y1 STEPS      pen hovering from one point to another
  pen-button X Y MS               pen hovering with its barrel button held for MS milliseconds
  drag X0 Y0 X1 Y1 STEPS [MS]     one finger travelling in equal steps
  scroll2 DY STEPS                two fingers moving together, DY pixels down
  tap-drag X Y DX                 tap, touch again at once, drag DX pixels
  sleep SECONDS
  quit
"""
import math
import shlex
import struct
import subprocess
import sys
import time

UHID_DESTROY, UHID_CREATE2, UHID_INPUT2 = 1, 11, 12
BUS_USB = 3

RAW_W, RAW_H = 1600, 2560  # the panel's natural (portrait) size
VIEW_W, VIEW_H = 2560, 1600  # what the app sees (display rotation 90)

# --- HID report descriptors -------------------------------------------------------------------


def touch_descriptor(contacts=5):
    d = bytearray([0x05, 0x0D, 0x09, 0x04, 0xA1, 0x01, 0x85, 0x01])
    for _ in range(contacts):
        d += bytes([
            0x09, 0x22, 0xA1, 0x02,
            0x09, 0x42, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x01, 0x81, 0x02,  # tip switch
            0x75, 0x07, 0x95, 0x01, 0x81, 0x03,  # padding
            0x09, 0x51, 0x75, 0x08, 0x95, 0x01, 0x15, 0x00, 0x25, 0x7F, 0x81, 0x02,  # contact id
            0x05, 0x01, 0x09, 0x30, 0x15, 0x00, 0x26, (RAW_W - 1) & 0xFF, (RAW_W - 1) >> 8,
            0x75, 0x10, 0x95, 0x01, 0x81, 0x02,  # X
            0x09, 0x31, 0x15, 0x00, 0x26, (RAW_H - 1) & 0xFF, (RAW_H - 1) >> 8,
            0x81, 0x02,  # Y
            0x05, 0x0D, 0xC0,
        ])
    d += bytes([0x05, 0x0D, 0x09, 0x54, 0x15, 0x00, 0x25, contacts, 0x75, 0x08, 0x95, 0x01, 0x81, 0x02, 0xC0])
    return bytes(d)


def pen_descriptor():
    return bytes([
        0x05, 0x0D, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x02, 0x09, 0x20, 0xA1, 0x00,
        0x09, 0x42, 0x09, 0x44, 0x09, 0x45, 0x09, 0x32,  # tip, barrel, eraser, in range
        0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x04, 0x81, 0x02,
        0x75, 0x04, 0x95, 0x01, 0x81, 0x03,
        0x05, 0x01, 0x09, 0x30, 0x15, 0x00, 0x26, (RAW_W - 1) & 0xFF, (RAW_W - 1) >> 8,
        0x75, 0x10, 0x95, 0x01, 0x81, 0x02,
        0x09, 0x31, 0x26, (RAW_H - 1) & 0xFF, (RAW_H - 1) >> 8, 0x81, 0x02,
        0x05, 0x0D, 0x09, 0x30, 0x26, 0xFF, 0x0F, 0x75, 0x10, 0x95, 0x01, 0x81, 0x02,  # pressure
        0xC0, 0xC0,
    ])


class Uhid:
    """One virtual HID device on the tablet, fed through a long-lived `cat > /dev/uhid`."""

    def __init__(self, serial, name, descriptor, product):
        self.p = subprocess.Popen(["adb", "-s", serial, "shell", "cat > /dev/uhid"], stdin=subprocess.PIPE)
        req = (name.encode().ljust(128, b"\0") + b"\0" * 64 + b"\0" * 64
               + struct.pack("<HHIIII", len(descriptor), BUS_USB, 0x4553, product, 1, 0) + descriptor)
        self._write(struct.pack("<I", UHID_CREATE2) + req)

    def _write(self, data):
        self.p.stdin.write(data)
        self.p.stdin.flush()
        # One uhid event per write(): `cat` forwards whatever one read() returned as one write, so
        # two events sent back to back can be merged into one and the second lost. A pause per
        # event keeps them apart (the lifts at the end of a gesture were being dropped this way).
        time.sleep(0.012)

    def report(self, data):
        self._write(struct.pack("<IH", UHID_INPUT2, len(data)) + data)

    def close(self):
        try:
            self._write(struct.pack("<I", UHID_DESTROY))
            self.p.stdin.close()
        except BrokenPipeError:
            pass
        self.p.wait()


class Screen:
    """Landscape view coordinates to raw panel coordinates (portrait), by display rotation."""

    def __init__(self, rotation=90):
        self.rotation = rotation

    def raw(self, x, y):
        x = min(max(x, 0), VIEW_W - 1)
        y = min(max(y, 0), VIEW_H - 1)
        if self.rotation == 90:
            return int(RAW_W - 1 - y), int(x)
        if self.rotation == 270:
            return int(y), int(RAW_H - 1 - x)
        return int(x), int(y)


class Tablet:
    def __init__(self, serial, rotation):
        self.serial, self.screen = serial, Screen(rotation)
        self.touch = Uhid(serial, "uscreen-test-touchscreen", touch_descriptor(), 0x7101)
        self.pen = Uhid(serial, "uscreen-test-pen", pen_descriptor(), 0x7102)
        time.sleep(2.5)  # Android notices the new devices and maps them to the display

    # fingers: list of (id, rawx, rawy, tip)
    def touch_frame(self, fingers):
        b = bytearray([1])
        for i in range(5):
            if i < len(fingers):
                fid, x, y, tip = fingers[i]
                b += bytes([tip & 1, fid]) + struct.pack("<HH", x, y)
            else:
                b += bytes([0, 0]) + struct.pack("<HH", 0, 0)
        b.append(len(fingers))
        self.touch.report(bytes(b))

    def pen_frame(self, rx, ry, in_range=True, tip=False, pressure=0, barrel=False):
        flags = (1 if tip else 0) | (2 if barrel else 0) | (8 if in_range else 0)
        self.pen.report(bytes([2, flags]) + struct.pack("<HHH", rx, ry, pressure))

    # --- commands ----------------------------------------------------------------------------
    def view_fingers(self, pts, tip=1):
        return [(i, *self.screen.raw(x, y), tip) for i, (x, y) in enumerate(pts)]

    def tap(self, pts, jitter=0.0, hold_ms=120, stagger_ms=15, settle=True):
        n = len(pts)
        for k in range(1, n + 1):  # fingers land one after another
            self.touch_frame(self.view_fingers(pts[:k]))
            time.sleep(stagger_ms / 1000)
        time.sleep(hold_ms / 2000)
        if jitter:  # the whole hand drifts a little while it rests
            drift = [(x + jitter, y + jitter * 0.6) for x, y in pts]
            self.touch_frame(self.view_fingers(drift))
            pts = drift
        time.sleep(hold_ms / 2000)
        for k in range(n, 0, -1):  # and lift one after another
            fs = self.view_fingers(pts[:k - 1]) + [(k - 1, *self.screen.raw(*pts[k - 1]), 0)]
            self.touch_frame(fs)
            time.sleep(stagger_ms / 1000)
        self.touch_frame([])
        if settle:
            time.sleep(0.25)

    def two_fingers(self, pose, steps=24, step_ms=16):
        cx, cy = VIEW_W / 2, VIEW_H / 2

        def place(t):
            d, a = pose(t)
            return [(cx - d / 2 * math.cos(a), cy - d / 2 * math.sin(a)),
                    (cx + d / 2 * math.cos(a), cy + d / 2 * math.sin(a))]

        self.touch_frame(self.view_fingers(place(0)))
        time.sleep(0.08)
        for i in range(1, steps + 1):
            self.touch_frame(self.view_fingers(place(i / steps)))
            time.sleep(step_ms / 1000)
        time.sleep(0.1)
        pts = place(1)
        self.touch_frame([(0, *self.screen.raw(*pts[0]), 1), (1, *self.screen.raw(*pts[1]), 0)])
        self.touch_frame([(0, *self.screen.raw(*pts[0]), 0)])
        self.touch_frame([])
        time.sleep(0.25)

    def pen_path(self, x0, y0, x1, y1, steps, interval_ms=8):
        for i in range(steps + 1):
            t = i / max(steps, 1)
            self.pen_frame(*self.screen.raw(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t))
            time.sleep(interval_ms / 1000)
        self.pen_frame(*self.screen.raw(x1, y1), in_range=False)
        time.sleep(0.25)

    def drag(self, x0, y0, x1, y1, steps, interval_ms=16, hold_first=True):
        """One finger travelling from (x0, y0) to (x1, y1) in `steps` equal steps."""
        self.touch_frame(self.view_fingers([(x0, y0)]))
        time.sleep(0.05)
        for i in range(1, steps + 1):
            t = i / steps
            self.touch_frame(self.view_fingers([(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t)]))
            time.sleep(interval_ms / 1000)
        time.sleep(0.05)
        self.touch_frame([(0, *self.screen.raw(x1, y1), 0)])
        self.touch_frame([])
        time.sleep(0.25)

    def scroll2(self, dy, steps):
        """Two fingers moving down (dy > 0) or up together."""
        cx, cy = VIEW_W / 2, VIEW_H / 2
        pts = lambda t: [(cx - 100, cy + dy * t), (cx + 100, cy + dy * t)]
        self.touch_frame(self.view_fingers(pts(0)))
        time.sleep(0.06)
        for i in range(1, steps + 1):
            self.touch_frame(self.view_fingers(pts(i / steps)))
            time.sleep(0.016)
        time.sleep(0.05)
        end = pts(1)
        self.touch_frame([(0, *self.screen.raw(*end[0]), 1), (1, *self.screen.raw(*end[1]), 0)])
        self.touch_frame([(0, *self.screen.raw(*end[0]), 0)])
        self.touch_frame([])
        time.sleep(0.25)

    def tap_drag(self, x, y, dx, steps=20):
        """Tap, then touch again at once and drag: a touchpad's tap-and-drag."""
        # The second touch must land within Android's double-tap window (300 ms) of the first lift.
        self.tap([(x, y)], hold_ms=80, stagger_ms=0, settle=False)
        time.sleep(0.09)
        self.drag(x, y, x + dx, y, steps)

    def pen_button(self, x, y, ms):
        """Hover the pen over (x, y) holding its barrel button for `ms` milliseconds."""
        rx, ry = self.screen.raw(x, y)
        t0 = time.time()
        self.pen_frame(rx, ry)
        time.sleep(0.05)
        while (time.time() - t0) * 1000 < ms:
            self.pen_frame(rx, ry, barrel=True)
            time.sleep(0.02)
        self.pen_frame(rx, ry)
        time.sleep(0.05)
        self.pen_frame(rx, ry, in_range=False)
        time.sleep(0.25)

    def run(self, line):
        a = shlex.split(line)
        if not a:
            return True
        c, args = a[0], a[1:]
        f = [float(v) for v in args]
        if c == "quit":
            return False
        if c == "sleep":
            time.sleep(f[0])
        elif c == "tap-at":
            self.tap([(f[0], f[1])], hold_ms=90, stagger_ms=0)
        elif c == "rawtap":
            self.touch_frame([(0, int(f[0]), int(f[1]), 1)])
            time.sleep(0.09)
            self.touch_frame([(0, int(f[0]), int(f[1]), 0)])
            self.touch_frame([])
            time.sleep(0.25)
        elif c == "tap":
            n = int(f[0])
            spacing, cx, cy = 160, VIEW_W / 2, VIEW_H / 2
            pts = [(cx + (i - (n - 1) / 2) * spacing, cy) for i in range(n)]
            self.tap(pts, jitter=f[1] if len(f) > 1 else 0, hold_ms=int(f[2]) if len(f) > 2 else 120)
        elif c == "pinch":
            self.two_fingers(lambda t: (f[0] + (f[1] - f[0]) * t, 0.0))
        elif c == "rotate":
            self.two_fingers(lambda t: (400, math.radians(f[0]) * t))
        elif c == "pen-hover":
            for i in range(int(f[2])):
                self.pen_frame(*self.screen.raw(f[0] + (i % 7) * 4, f[1] + (i % 5) * 3))
                time.sleep(0.008)
            self.pen_frame(*self.screen.raw(f[0], f[1]), in_range=False)
            time.sleep(0.25)
        elif c == "drag":
            self.drag(f[0], f[1], f[2], f[3], int(f[4]), int(f[5]) if len(f) > 5 else 16)
        elif c == "scroll2":
            self.scroll2(f[0], int(f[1]))
        elif c == "tap-drag":
            self.tap_drag(f[0], f[1], f[2])
        elif c == "pen-button":
            self.pen_button(f[0], f[1], f[2])
        elif c == "pen-move":
            self.pen_path(f[0], f[1], f[2], f[3], int(f[4]))
        else:
            print(f"unknown command {c!r}", file=sys.stderr)
        return True

    def close(self):
        self.touch.close()
        self.pen.close()


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    rotation = int(sys.argv[2]) if len(sys.argv) > 2 else 90
    tablet = Tablet(sys.argv[1], rotation)
    print("ready", flush=True)
    try:
        for line in sys.stdin:
            if not tablet.run(line):
                break
            print("ok", flush=True)
    finally:
        tablet.close()


if __name__ == "__main__":
    main()
