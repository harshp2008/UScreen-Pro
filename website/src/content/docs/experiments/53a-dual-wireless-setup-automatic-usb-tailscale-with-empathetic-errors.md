---
title: "53a Dual wireless setup (Automatic USB + Tailscale) with empathetic errors"
sidebar:
  order: 38
  label: "53a Dual wireless setup (Automatic USB + Tailscale) with emp"
---

**Wizard Step 4 (Wireless)** is now a two-tab chooser (`AddDeviceWizard.tsx`):

| Field | Value |
| :-- | :-- |
| **ID** | 53a |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | Kept in the repository under `experiments_and_findings/53_screenshots/`; not published. |

## What was tested

The write-up does not separate a question or a setup from its result; it is under Result.

## Result

### What changed

**Wizard Step 4 (Wireless)** is now a two-tab chooser (`AddDeviceWizard.tsx`):

- **Option A, Automatic Setup (Recommended).** Button "Configure Wireless over USB". Success shows the green step ticks and the `IP:port`. If the backend's reachability check fails, an amber banner "Connection Blocked by Router" explains it is not the user's fault and lists the two usual causes (MAC randomization, AP isolation). Buttons: **How to fix MAC Randomization** (modal with the four steps, plus a Retry shortcut) and **Retry Setup**.
- **Option B, Tailscale (Advanced).** Explanation text, **Download Tailscale** (opens tailscale.com/download through `plugin-opener`), "Tailscale IP of this PC" (auto-filled with `tailscale ip -4`, plus a Detect button), "Tailscale IP of the target device", **Connect via Tailscale**. Failure shows the three-point checklist from the spec plus adb's own output in small mono text.

**Backend** (`src-tauri/src/pairing.rs`, registered in `lib.rs`):

- `pair_via_tailscale(host_ip, target_ip, serial?)`: validates both as IPv4, runs `adb connect <target_ip>:5555` (3 attempts), then saves `tailscale_host_ip` and `tailscale_target_ip` into `config.toml` (`extra` table; the host daemon ignores unknown keys). Returns the endpoint; the wizard stores it as the device's `wifi_address` exactly like the USB path. If a cable is plugged in, `serial` is passed and adbd is first switched with `adb tcpip 5555`, otherwise nothing would be listening on that port.
- `detect_tailscale_ip()`: `tailscale ip -4`.
- The reachability failure in `bootstrap_wireless_over_usb` now carries the prefix `ROUTER_BLOCKED|<ip>|`, which the UI turns into the banner. Other failures still show their plain message with a "Retry Setup" button.

**UX**

- Each tab keeps its own state in the wizard component (result, typed IPs, error), so switching tabs loses nothing. A tick on a tab header shows which path already succeeded; Next unlocks when either has.
- Unplugging the cable no longer halts the wizard once either path has succeeded.
- "Configure Wireless Mode" on a device whose wireless was skipped opens the wizard straight on this step (`reconfigure` starts at `WIRELESS_STEP`). Verified in the dev preview. In reconfigure mode both paths save into the existing profile and close the dialog.
- `devMock.ts` gained `?bootstrap=blocked` plus Tailscale handlers (a target ending in `.1` fails) so the states can be previewed without hardware.

### Limits

- The Tailscale path needs adbd listening on 5555 on the device. With the cable attached the app arranges that; without it the user must have run `adb tcpip 5555` earlier, or the connect fails (and shows the checklist).
- IPv4 only. MagicDNS names are rejected with a hint.

### Verification

- `cargo check`, `tsc --noEmit` and `./build-all.sh` all pass; `~/.local/bin/uscreen-pro-gui` is byte-identical to `dist/` and the app launches (`53_screenshots/0_real_app_running.png`).
- The Step 4 screenshots (`53_screenshots/1..3`) come from the dev preview (`npm run dev`) of the same frontend code with mock IPC (`?scenario=skipped&bootstrap=blocked`), not from the real wizard. On this network the router passes device-to-device traffic, so the real "blocked" state cannot be produced, and driving the real wizard would have installed the APK and run `adb tcpip` on the tablet.
- Not run against real hardware: `pair_via_tailscale` (no tailnet device available here).


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/53_dual_wireless_setup.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/53_dual_wireless_setup.md)
