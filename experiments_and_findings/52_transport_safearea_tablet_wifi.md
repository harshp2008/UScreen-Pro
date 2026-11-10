# 52 - Transport order, Android safe area, tablet wireless setup

## 1. Transport order (done, verified)
- Desktop: `lib/options.ts` `TRANSPORTS` (Device Settings dropdown) and the wizard's Finish grid are now **USB cable / Auto / Wi-Fi**. Auto is still the default and sits in the middle.
- Android: `SettingsSheet.kt` chips are **USB / Auto / Wi-Fi**; default is still Auto.
- Verified: tablet and A55 screenshots (`52_screenshots/`); desktop via the mock-IPC dev server (dropdown and wizard Finish step). The native desktop window was not driven (no `ydotoold` on this machine).

## 2. Android safe area (done, verified on A55)
- `SettingsSheet.kt`: `systemBarsPadding()` -> `windowInsetsPadding(WindowInsets.safeDrawing)` (adds the display cutout).
- `ConnectionScreen.kt`: same inset on the waiting screen, the stats overlay and the version-mismatch banner.
- **Deliberately not applied to the video `SurfaceView`**: padding it would shrink and offset the stream and break the touch-to-host coordinate mapping. Only non-streaming UI is inset.
- A55 landscape: the cutout side now shows a ~89 px black inset and content starts clear of it (`phone_a55_landscape_cutout_inset.png`). Rotation setting restored to `free` afterwards.

## 3. Tablet wireless setup - root cause is the network, not the code
Devices: tablet `TABLET_SERIAL` (SM-X616B), phone `PHONE_SERIAL` (SM-A556E).

| Hypothesis from the brief | Result |
|---|---|
| Wrong device's IP / missing `-s` | No. `bootstrap_wireless_over_usb` pins every call to the serial; tablet IP read correctly (`192.168.1.101`). |
| Race between two devices | Not the cause here, but real in principle; now serialised (below). |
| API < 30 | No. Both are Android 16 / API 36. |
| Different Wi-Fi | No. Same SSID, same BSSID (`00:11:22:33:44:55`), same /24 as the laptop (`192.168.1.100`). |

Actual cause: **laptop and tablet cannot talk to each other over Wi-Fi.**
- Laptop -> tablet: ping 100% loss, ARP `FAILED`, TCP 5555/5037/80 all time out.
- Tablet -> laptop: ping 100% loss, ARP `192.168.1.100 FAILED`.
- Tablet <-> gateway and tablet <-> phone work; laptop <-> phone works (`192.168.1.102:5555` open).
- Tablet awake, Wi-Fi toggled off/on: no change. `adb mdns services` is empty.
- Conclusion: the router/AP drops traffic between these two clients (AP/client isolation, device block, or MAC filter on the tablet's private MAC `da:70:1a:e6:78:ce`). Needs a router-side fix; not fixable from the app.

**Not achieved: the tablet did not reach "Wireless mode ready" and there is no successful `adb pair` log for it.** No `adb pair` was run, because on this network no pairing path to the tablet exists. That is not a failure of the new code to pass a test.

### What changed in `pairing.rs` (`bootstrap_wireless_over_usb`)
1. `WIRELESS_SETUP` tokio mutex: one device's wireless setup at a time.
2. `host_reachable(ip)` pre-check (TCP connect to 5555/5037/80, 1.5 s each): connected or `ConnectionRefused` = reachable; timeout / `EHOSTUNREACH` = not. If unreachable, fail in ~5 s with a message naming the router-isolation cause, instead of ~90 s of spinner ending in "adb connect timed out".
3. `ro.build.version.sdk` check: below 30 skips Wireless debugging and goes straight to `adb tcpip 5555`.
4. Two unit tests for `host_reachable` (127.0.0.1 reachable; TEST-NET-1 unreachable): pass.

The first draft of the pre-check treated any connect error as "reachable"; that would have misreported `EHOSTUNREACH`. Fixed before the final build.

## Build / deploy
`./build-all.sh` OK; `~/.local/bin/uscreen-pro-gui` identical to `dist/`; APK reinstalled on both devices (`-s` needed with two attached).

## Next, for the user
Router: disable AP/client isolation or remove any block/MAC-filter for the tablet, or turn off "Private Wi-Fi address" for this network on the tablet, then rerun the wizard on the tablet.
