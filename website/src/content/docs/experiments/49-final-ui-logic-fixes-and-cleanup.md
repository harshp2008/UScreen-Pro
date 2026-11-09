---
title: "49 Final UI logic fixes and cleanup"
sidebar:
  order: 34
  label: "49 Final UI logic fixes and cleanup"
---

Final UI logic fixes and cleanup

| Field | Value |
| :-- | :-- |
| **ID** | 49 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

The write-up does not separate a question or a setup from its result; it is under Result.

## Result

### Changes

| # | Area | File(s) | Change |
|---|------|---------|--------|
| 1 | Wizard auto-advance | `uscreen-pro-gui/src/components/AddDeviceWizard.tsx` | Removed the 700 ms `setStep(FINISH_STEP)` timer after `bootstrap_wireless_over_usb`. The step shows the green "Wireless mode ready at IP:port" check and waits for **Next**. (Reconfigure mode still saves and closes, it has no Next.) |
| 2 | Android onboarding | `ConnectionScreen.kt`, `MainActivity.kt` | Onboarding is hidden whenever `isConnected`, plus a 1.5 s grace after a drop so a quick reconnect never flashes it; first live stream sets `hasCompletedOnboarding`. Wizard stays reachable via Settings > Review Setup Guide (`forceReviewMode`). |
| 3 | Unplug handling | `AddDeviceWizard.tsx`, `devMock.ts` | Once an authorized device was seen on Connect USB / Install / Wireless, its absence (2.5 s debounce, 8 s while wireless setup runs because `adb tcpip` restarts adbd) halts the wizard with the red banner **"⚠️ Device disconnected. Please plug the USB cable back in to continue."** and a **Retry Connection** button (restarts adb, re-polls). Next / Skip install are blocked. Interrupted install or wireless setup become a failure with **Restart installation** / **Restart wireless setup**; stale results from the interrupted run are ignored. Unplugging after "Wireless mode ready" is allowed. |
| 4 | Theme | `uscreen-pro-gui/src/lib/theme.ts`, `TitleBar.tsx`, `App.tsx`, `index.html`; Android `Prefs.kt`, `Theme.kt`, `MainActivity.kt`, `SettingsSheet.kt`, `ConnectionScreen.kt`, manifest, `themes.xml`, `build.gradle.kts` | Desktop: Light / Dark / System (Auto) dropdown, System follows `matchMedia('(prefers-color-scheme: dark)')` live; default is System. Android: same 3-way chips in Settings, applied with `AppCompatDelegate.setDefaultNightMode()` (activity is now an `AppCompatActivity`, theme is `Theme.AppCompat.DayNight`, `uiMode` added to `configChanges` so the stream is not restarted) plus a new light palette from the desktop `:root` tokens. The Compose theme reads the saved choice directly, because AppCompat alone did not repaint Compose when the activity handles `uiMode` itself. |
| 5 | Rogue tray | `host/src/tray.rs`, `packaging/icons/*`, `host/src/doctor.rs`, `build-all.sh` | See below. |

### Task 5: root cause

The tray lives in the daemon (`host/src/tray.rs`, used by `main.rs`) and is still useful (mode toggle, quit), so it was fixed rather than deleted:

* "Settings…" and left click spawned `uscreen-gui`, the retired `gui-legacy` app. Now they launch `uscreen-pro-gui` (sibling of the daemon binary first), and do not start a second copy if one is running (`wmctrl` raise, best effort).
* Icon was the old brown/amber logo, also resolved through a theme icon named `uscreen` that an old install left in `~/.local/share/icons`. The tray now ships only the emerald pixmaps (regenerated `packaging/icons/uscreen*.svg` / `*-64.rgba`) and names no theme icon. Tray id and title are "UScreen Pro".
* A running daemon keeps the old tray in memory, and `build-all.sh` never restarted it. It now restarts `uscreen.service` when active.
* `build-all.sh` also deletes the stale legacy leftovers (single named files): `~/.local/bin/uscreen-gui`, `~/.local/share/applications/uscreen.desktop` ("UScreen" entry with the old icon), and the old `uscreen.svg` / `uscreen-pen.svg` theme icons.

### Verification (this machine)

* `./build-all.sh` exit 0: APK, daemon, Tauri binary built; deployed files `cmp`-verified; `uscreen.service` restarted. Journal: `uscreen::tray: Tray icon registered`.
* Top bar shows a single emerald UScreen icon; no `uscreen-gui` process or binary remains.
* Wizard (dev-mock preview, `?scenario=two`): after "Set up wireless" the step stays on **Wireless** with the green check for 5+ s (was: jumped at 0.7 s).
* Unplug (mock `window.__unplugged = true`) on Connect USB: banner + Retry Connection shown, Next disabled; replug clears it (also checked on Install).
* Desktop theme: dropdown shows Light / Dark / System (Auto); with System, flipping the emulated OS scheme dark -> light flips `html.dark` live.
* Android (SM-A556E, USB): streaming shows the video feed with no onboarding; Settings has Theme chips; Light repaints the sheet live and persists in prefs; reset to System afterwards.


## Not verified

* A physical cable pull was not done (cannot be done from this session); the unplug path was exercised through the mock IPC. The "install interrupted mid-run" branch is code-only: the mock install finishes faster than the 2.5 s debounce.
* Android light theme covers the palette, sheets and onboarding cards; fixed translucent video overlays (stats pill, banners) stay dark by design.
* `gui-legacy/` (excluded from the workspace) is still referenced by `Makefile`, `scripts/install.sh`, packaging and CI, so it was not deleted. Nothing in the daemon launches it any more.
* `~/.config/systemd/user/uscreen.service.d/override.conf` (outside the repo) points the service at `target/release/uscreen`; left as is.
* The wizard landing page's QR / manual pairing path still jumps to Finish on success (no Next button exists there); Task 1 covered the USB Wireless step.

## Sources

- [`experiments_and_findings/49_final_ui_logic_and_cleanup.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/49_final_ui_logic_and_cleanup.md)
