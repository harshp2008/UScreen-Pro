# 104: Full-desktop screenshots take 10 s while a virtual display is connected

Date: 2026-11-11. Host: Ubuntu, GNOME Shell 46 on Wayland, laptop panel 1920x1080 (scale 1). Two UScreen virtual displays, both created through
EVDI: slot 1 a phone (2340x1080@90, GNOME chose scale 2.5), slot 2 a Galaxy Tab S9 FE+ (2560x1600@90, scale 1.75). Tools: Flameshot 14.0.0 (Qt 6.4.2),
gnome-screenshot, `gdbus` against `org.gnome.Mutter.DisplayConfig`.

## Symptom
Flameshot (and any tool that photographs the whole desktop) takes ages to open its GUI or never seems to. The cause was suspected to be "the software display".

## Result in one paragraph
**Capture time is proportional to the number of pixels in the captured canvas, about 0.65 s per megapixel, and the virtual displays only add pixels.** There is
no fixed penalty for EVDI, and no stall in the compositor. The canvas is the bounding rectangle of all monitors, drawn at the *largest* scale of any monitor
(GNOME captures the whole stage at one scale), so a 2.5x phone inflates the picture of the 1x laptop too. The work is done on one CPU thread, twice (see below),
so it cannot use the other cores. Nothing in UScreen's capture or encode path is involved.

## Measurements

### 1. Virtual displays on, off, on (daemon stopped in between)
| State | Monitors | `flameshot full` | `gnome-screenshot` | Flameshot GUI window appears |
|---|---|---|---|---|
| daemon running | 3 | 16.7 s, 25.6 s | 15.7 s, 21.2 s | 5.6 s |
| daemon stopped | 1 | 0.81 s, 0.81 s | 0.78 s, 0.82 s | 0.79 s |
| daemon running again | 3 | 10.4 s, 9.4 s | 9.3 s, 9.4 s | 5.7 s |

Runs with the daemon on vary a lot (9 to 25 s) while `ffmpeg` and the helpers were busy; with it stopped they do not vary at all.

### 2. The picture is much bigger than the desktop
`gnome-screenshot` wrote a **10800 x 2700** PNG (12.5 MB, 29.2 MP) for a desktop that is 4320 x 1080 in logical pixels. 4320 x 2.5 = 10800 and 1080 x 2.5 = 2700:
the whole stage was captured at the phone's scale.

### 3. Scale only (Mutter `ApplyMonitorsConfig`, method 1 = temporary, restored afterwards)
| Phone scale | Tablet scale | PNG size | `gnome-screenshot` | `flameshot full` |
|---|---|---|---|---|
| 2.5 (as is) | 1.75 | 10800 x 2700 (29.2 MP) | about 10 s | 5 to 10 s |
| 1.0 | 1.75 | 10009 x 1889 (18.9 MP) | 10.9 s, 10.1 s | 5.3 s |
| 1.0 | 1.0 | 6820 x 1600 (10.9 MP) | 6.2 s, 6.2 s | 3.2 s |

The predicted size matched exactly each time (the second row's width is (1920 + 2340 + 1464) x 1.7486 = 10009), which confirms the "largest scale" rule.
The time did not fall in the second row alone, because the tablet's 1.75 still left 18.9 MP.

### 4. Adding the virtual screens one at a time (all at scale 1)
| Monitors | PNG size | MP | `gnome-screenshot` |
|---|---|---|---|
| laptop only (the virtual outputs disabled in Mutter, daemon still running) | 1920 x 1080 | 2.1 | 0.8 s, 0.8 s |
| laptop + phone | 4260 x 1080 | 4.6 | 2.9 s, 3.0 s |
| laptop + tablet | 4480 x 1600 | 7.2 | 4.4 s, 4.5 s |
| laptop + phone + tablet | 6820 x 1600 | 10.9 | 6.7 s, 6.9 s |

A straight line through these is about 0.68 s per megapixel. Disabling the virtual outputs in Mutter while the daemon (helpers, encoders) keeps running gives
the fast 0.8 s, so the daemon's own load and the EVDI devices are not the cost. The pixels are.

### 5. Where the time goes (per-thread CPU during one capture)
About 4 s of one thread named `pool-gnome-shell` at 100 %, then about 4 to 5 s of one thread named `pool-gnome-screenshot` at 100 %. gnome-shell's main thread and
its KMS thread stay near 10 to 20 %. So it is single-threaded CPU work, first inside the shell and then again in the client. My inference is that these are the
PNG encode in the shell and the decode and re-save in the client; I did not profile the code, so that part is not confirmed.

### 6. Capturing less is fast
`gnome-screenshot -w` (the focused window): 0.77 s, a normal 1920 x 1040 image, with all virtual screens on.
`flameshot screen -n 0` (laptop only) still took 5.7 s: it renders the whole stage and crops afterwards.
`QT_QPA_PLATFORM=xcb flameshot full` was faster (6.4 s against 11 s) but goes through XWayland; I did not check what it captures.

### 7. The portal alone, and GNOME's own Print Screen
A small Python script asked `org.freedesktop.portal.Screenshot` for a non-interactive screenshot and timed the `Response` signal, with no client tool involved:
**3.56 s and 3.58 s**, writing an 8.4 MB PNG. So of the roughly 10 s that `gnome-screenshot` takes, about 3.6 s is the shell (capture and PNG write) and the rest is the
client loading and re-saving the file; Flameshot's GUI (5.6 s) adds about 2 s on top of the portal. The shell's own `org.gnome.Shell.Screenshot.Screenshot` call is not
allowed for ordinary clients (`AccessDenied`), so the shell-only time could not be measured directly.
The user reports that GNOME's own **Print Screen overlay takes about 0.4 s** with the same three monitors. I did not measure it. My explanation, from how GNOME 46 is built and
not tested here: the overlay keeps the capture as an in-memory image and encodes a PNG only for the area or screen you choose, whereas the portal always writes the whole desktop
to a file first. That would make the PNG write of the full canvas, not the capture, the expensive step.

## What it is not (tested)
* **Not a frame clock stall on the idle EVDI outputs.** Constant redraw on the virtual monitors did not change the time (about 10 s either way). The test windows
  may not have landed on those monitors, so this is weak evidence; the linear model in table 4 is the stronger one.
* **Not a fixed cost per virtual display.** Table 4: removing both virtual screens from Mutter's layout, with the daemon still running, gives 0.8 s.
* **Not the daemon's load.** Same table. Runs with the daemon busy are noisier (up to 25 s) but not slower on average per pixel.
* `uscreen pause-device` does not remove the virtual display, so it cannot be used to isolate one display; Mutter's own layout was used instead.

## What can be changed
Everything that makes the picture smaller helps, in this order of effect:
| Lever | Effect on the whole-desktop capture | Cost |
|---|---|---|
| Lower the scale of the virtual displays (largest one counts) | 29 MP to 18.9 MP to 10.9 MP, measured above (10 s to 6.2 s) | smaller text on the phone and tablet |
| Lower the virtual display's resolution (Stream detail Half) | e.g. 1280x800 + 1170x540 at 1x: 4.7 MP, about 3 s *(predicted from the line above, not measured)* | a softer picture on the tablet |
| Capture a window or a region, not the desktop | 0.8 s, measured | none |
| GNOME's own Print Screen overlay | about 0.4 s as reported by the user, not measured here | none; it is not a tool Flameshot can use |
| A fast capture inside the shell (for example from UScreen's GNOME extension: capture the stage as an image and write the PNG with low compression) | would cut the shell's 3.6 s; not built, and a Shell extension change only loads after logging out and in | patches GNOME internals, GNOME 46 only |

The EDID UScreen generates carries the tablet's *real* physical size on purpose (the compositor derives DPI from it), and that is what makes GNOME pick 2.5 for
the phone. Changing the scale therefore means either a per-device "interface size" that overrides GNOME's choice through `DisplayConfig`, or a different
physical size in the EDID. Neither is implemented.

## Not verified
* Other desktops (KDE, Hyprland): only GNOME Shell 46 was measured.
* Other tools (Spectacle, grim, GNOME's built-in Print screen UI) were not tried.
* A scale change at runtime was applied only as a temporary Mutter configuration and restored; the persistence rules of a permanent one were not studied.
* The Flameshot GUI delay is the same capture (it grabs the whole desktop before it draws anything); I did not look inside Flameshot.

## Reproduce
```bash
# size and time of one capture
( time gnome-screenshot -f /tmp/w11c-sandbox.XXXXXX/x.png ) ; file /tmp/w11c-sandbox.XXXXXX/x.png
# temporary scale (method 1 is not saved); get the serial first
gdbus call --session --dest org.gnome.Mutter.DisplayConfig --object-path /org/gnome/Mutter/DisplayConfig \
  --method org.gnome.Mutter.DisplayConfig.GetCurrentState
# apply a layout, then restore the original the same way
gdbus call --session --dest org.gnome.Mutter.DisplayConfig --object-path /org/gnome/Mutter/DisplayConfig \
  --method org.gnome.Mutter.DisplayConfig.ApplyMonitorsConfig <serial> 1 "<logical monitors>" "@a{sv} {}"
```
The layout was restored after every run and checked with `spawn-at query layout`.
