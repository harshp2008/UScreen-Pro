---
title: "uscreen wifi"
sidebar:
  order: 5
---

With the cable plugged in, `uscreen wifi` switches the tablet to adb over Wi-Fi, remembers its address in `wifi_address` and reconnects to it by itself when the cable is out. The daemon prefers the cable when both are there. `uscreen wifi --off` forgets the address and stops reconnecting. Wi-Fi has more stutter than the cable.

The options are in the [CLI reference](/UScreen-Pro/reference/uscreen-wifi/).

## Sources

- `host/src/net/adb/wifi.rs` (`setup_wifi`)
