---
title: "41 Final polish: layout, Add device modal, Emerald theme, app icon"
sidebar:
  order: 27
  label: "41 Final polish: layout, Add device modal, Emerald theme, ap"
---

Rig: deployed binary launched with `GDK_BACKEND=x11` (XWayland) and driven with `xdotool`, captured with `gnome-screenshot` and cropped with ImageMagick. Screenshots are in `screenshots/41_*.png`.

| Field | Value |
| :-- | :-- |
| **ID** | 41 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

Research: component behaviour and tokens come from the `@bug-on/m3-expressive` docs shipped in `node_modules` (`llms-full.txt`, `.d.mts`) and from reading the rendered DOM. The MD3 and Tauri websites were **not** re-fetched this round (experiment 39 found the M3 site returns no text); contrast is computed from the WCAG 2.x formula below, not taken from guidelines.

### Layout, spacing, headers

- **Hypothesis:** the status bar and "+ Add device" touched the window edge because the footer had no bottom padding and the drawer used `p-2`; the two buttons differed in width because "Start daemon" sat inside a padded card and "Add device" did not.
- **Method:** drawer is now `px-4 pb-4 pt-2`; the daemon card (and its padding) is gone, so status row, "Start daemon", nav and "Add device" share one content column; status bar is `px-4 pb-4 pt-2`; title bar is `bg-transparent` with no bottom border; "Add device" is `tonal`; page headers are `text-2xl font-normal leading-8` (Headline Small, 24/32); health "Daemon process" + "Control socket" + "Socket path" merged into one "Daemon" card and the status-bar "Daemon stopped" text removed (the sidebar owns that state).
- **Raw log (pixel read of `41_light_settings.png`, 1024px-wide crop):** "Start daemon" and "Add device" both span x = 20..278; status-bar text sits 16px above the window bottom by the `pb-4` class (not separately measured).
- **Conclusion:** verified in `41_dark_settings.png`, `41_light_settings.png`, `41_*_health.png`. "Not running" now appears once on the Health page.

### Add device modal

| Item | Change | Evidence |
|---|---|---|
| Contrast | note box `bg-surface-container-high` + `text-on-surface`, `text-sm leading-relaxed` | 14.04:1 light, 9.11:1 dark |
| Terminology | "tablet" -> "device" in all wizard strings | `grep -i tablet` leaves only the default nickname placeholder |
| Next | footer, bottom-right, always visible; enabled by step: 1 device present, 2 state `device`, 3 install ok, 4 Wi-Fi endpoint paired | disabled state shown in both modal screenshots |
| Cancel | library text Button on the left | |
| Progress | `h-1 rounded-full`, filled with `primary` up to the current step | |
| Empty space | body `flex-col` with a `my-auto` wrapper, width 520 -> 480, radius 24 | |

**Not verified:** the *enabled* Next state and steps 2-5, because no Android device was attached. Only step 1 (disabled Next) was exercised.

### Component violations

- **Sliders:** the library handle is a 4dp pill. Reshaped with CSS in `styles.css` to a 20dp circle (`!important`, because the library animates the pill's width on press) that scales to 1.3 on press/focus with an overshoot spring. This is a CSS override of library internals, not a library option.
- **Fields:** Select and TextInput now pass `label`, so the library draws a floating label on an outlined field with the `field` tone behind it. The row label on the left is now duplicated by the floating label; kept because the brief asked for floating labels.
- **Truncation:** select width `w-64`, field text `13px`; "Intel / AMD H.264 (VAAPI)" fits (see screenshots).
- **Dirty badge:** `tertiary-container` / `on-tertiary-container`, pill.
- **Disabled Save:** the library already uses `on-surface` 12% container / 38% label (read from computed style: `oklab(... / 0.12)` and `/ 0.38`). Those are the spec tokens and I left them. By the spec they are intentionally low contrast (2.25:1 light, 2.78:1 dark, below).
- **Info icons:** 12px, `opacity-40` until hover.

### Emerald theme (seed #10B981)

- **Hypothesis:** the library's engine can generate the scheme from the seed.
- **Method:** generated it with `generateM3Theme('#10B981', ...)` through Vite SSR.
- **Raw log:**
  ```
  light primary #2f6a4f  surface #f7faf5  on-surface #2b352f
  dark  primary #a3d1b7  surface #0b0f0d  on-surface #dee8df
  ```
- **Conclusion:** too muted, and it misses the required anchors (`#047857`, `#34D399`, `#F4FBF6`, `#061E14`). Roles are therefore hand-authored in `src/theme.css` around the seed and override the library's `--md-sys-color-*`. Anchors are exact; every other role is a tonal step.
- **Contrast** (`WCAG 2.x`, computed by script):
  ```
  LIGHT text/card 15.23  secondary/card 8.32  white on #047857 5.48  nav label 12.01  note 14.04  dirty 11.90
  DARK  text/card 11.90  secondary/card 9.02  #00382a on #34d399 6.83  nav label 7.54  note 9.11  dirty 7.17
  disabled (spec 12%/38%): light 2.25  dark 2.78
  ```
- Purple/lavender is gone from the desktop UI. Android was **not** re-themed; it is still the purple scheme from the previous task.

### Icon and launcher

- Icon: flat emerald squircle, mint monitor, three signal arcs (`src-tauri/icons-src/uscreen-pro.svg`). ImageMagick's built-in SVG renderer drew a gradient fill black, so the tile is a flat `#059669`.
- `tauri icon` regenerated `src-tauri/icons/*`; its Android and iOS output was deleted (desktop-only app) and `tauri.conf.json` now lists `icons/icon.png` first.
- `build-all.sh` now installs the icon into `~/.local/share/icons/hicolor/*/apps/uscreen-pro.png` and writes `~/.local/share/applications/uscreen-pro.desktop` (`Icon=uscreen-pro`, `StartupWMClass=uscreen-pro-gui`, which matches the window's `WM_CLASS`).
- The brief says "update" `uscreen-pro.desktop`; it did not exist, so it was created. The older `uscreen.desktop` (`Icon=uscreen`) is untouched and still lists a second launcher.
- The custom title bar logo is the SVG itself (`src/assets/uscreen-pro.svg`).
- **Raw log:** `WM_CLASS = "uscreen-pro-gui", "Uscreen-pro-gui"`; `_NET_WM_ICON` is **not set** by the window under XWayland, so the dock icon comes from the `.desktop` file, not the window. Dock shows the green icon with the running indicator (`41_dock_icon.png`).

### Build and deploy

- `./build-all.sh` completed; `cmp dist/uscreen-pro-gui ~/.local/bin/uscreen-pro-gui` identical; `desktop-file-validate` passed.
- Gotcha: the script runs `pkill -9 -f uscreen-pro-gui`, which also kills any shell whose own command line contains that string (my wrapper exited 144 three times after the build had finished). Run it from a command line that does not contain the string.
- An earlier `rm -rf` of the generated `icons/android` and `icons/ios` ran after a malformed `findmnt` call (no output), so the mount check did not actually happen; the directories were untracked and inside the repo.


## Result

not recorded as a separate section; see What was tested.

## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/41_final_polish_and_modal_fix.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/41_final_polish_and_modal_fix.md)
