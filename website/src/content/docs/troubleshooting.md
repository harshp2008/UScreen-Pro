---
title: "Troubleshooting and FAQ"
---

Run `uscreen doctor` first. It checks the kernel modules, `/dev/uinput` permissions, tools, orphaned processes, the tablet over adb, the negotiated display mode, the tablet's codec support, colour settings and the config file, and prints a fix for each problem it finds.

## "Failed to start helper" / "evdi-helper exited prematurely"

No EVDI device exists and creating one needs root. Check `cat /sys/devices/evdi/count`. If it prints 0:

```bash
# for this boot
echo 1 | sudo tee /sys/devices/evdi/add
# for every boot
echo 'options evdi initial_device_count=2' | sudo tee /etc/modprobe.d/uscreen-evdi.conf
sudo modprobe -r evdi && sudo modprobe evdi
```

The `modprobe -r` matters: `initial_device_count` is read only when the module loads. If it says the module is in use, **reboot instead**. The installer does all of this for you.

## "Failed to open /dev/uinput"

`/dev/uinput` is root-only on a stock system. The installer puts a udev rule in place. By hand:

```bash
sudo modprobe uinput
sudo install -Dm644 packaging/60-uscreen-uinput.rules /etc/udev/rules.d/60-uscreen-uinput.rules
sudo udevadm control --reload && sudo udevadm trigger --name-match=uinput
```

## The app keeps opening and closing, or "did not authenticate"

The app is older than the daemon. A session token is required, so install the APK from the same release as the Linux side. The daemon retries delivering the token with a growing interval, so after updating the app it connects within seconds.

## Black screen on the tablet

1. `uscreen status`: is the daemon running?
2. `adb reverse --list`: are ports 8890 and 8891 forwarded?
3. Start the daemon in a terminal with `RUST_LOG=uscreen=debug uscreen start` and look for "Enabling EVDI output" and "Encoder started".
4. Check your display settings: the virtual output may be disabled there.

## Touch or pen land on the wrong screen

The daemon maps the tablet's input devices onto the virtual output. On GNOME it uses GNOME's per-device settings. If it did not stick, restart the daemon with the tablet attached, or assign "UScreen Pro Pen" and "UScreen Pro Touch" to the UScreen Pro output in your desktop's tablet settings.

## The pen takes the cursor from the mouse, or a second cursor stays behind

Both were investigated in experiments 100 to 103: a resting pen is parked after 300 ms, and the pointer is snapped to whole pixels so a software cursor leaves no ghost. If it still happens, run `uscreen debug-dump` right after and attach `~/.local/share/uscreen/pen_dump.txt` to a report.

## Wi-Fi is stuttery

It is a fallback. Median latency can match USB but the tail does not (see [Findings](/UScreen-Pro/findings/) and the benchmarks page under Project documents). Use the cable when you can.

## Screenshots of the whole desktop take 5 to 10 seconds

Flameshot's GUI is slow to open, or `gnome-screenshot` takes ages, while a tablet is connected as a second display. The tool is not at fault: the desktop is captured as one picture at the largest display scale of any monitor, and a high-scale virtual display makes that picture huge (a 4320 x 1080 desktop with a 2.5x phone became a 10800 x 2700 PNG). Time grows with the pixel count, about 0.65 s per megapixel. A window or region screenshot stays fast (0.8 s). Lowering the scale of the virtual displays helps (10 s to 6 s in the measurement) at the price of smaller text. Details and numbers: [Findings](/UScreen-Pro/findings/) (record 104).

## After a crash the virtual display never comes back

The compositor-safety guard starts in safe mode after a start that did not end cleanly. Run `uscreen reset-guard`, then restart the daemon.

## FAQ

**Does it need Wi-Fi or USB tethering?** No. A data cable with USB debugging is enough; Wi-Fi is optional.

**Is it a mirror or an extension?** Extended Display mode is a real second monitor. Touchpad and Stylus modes are not displays.

**Which Android versions?** 8.1 and newer, with a hardware H.264 decoder.

**Does it send anything to the cloud?** No. The only outbound request is the optional daily update check (`check_updates = false` turns it off).

**How do I remove it completely?** The list of files is on [Files and paths](/UScreen-Pro/files-and-paths/); the commands are in `SECURITY.md`.

## Sources

- `docs/troubleshooting.md`, `docs/faq.md`, `SECURITY.md`
- `host/src/cli/doctor/` (what the doctor checks)
- `experiments_and_findings/100_hover_park_and_recorder.md`, `101_click_slop.md`, `102_pen_menu_and_desktop_layer.md`, `103_scanout_capture_dropped.md`, `104_screenshots_slow_with_virtual_displays.md`
