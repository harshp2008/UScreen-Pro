#!/usr/bin/env python3
"""
Checks that the settings synced between the computer and the tablet exist on every side:
the host's `config_update` message (host/src/config/options/device.rs), the tablet app's `HostConfig.withSetting`
(android .../HostLink.kt) and the desktop app's per-device settings type (uscreen-pro-gui/src/components/power/shared.ts).
Exits 1 and lists the differences, minus the known one-sided settings below.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# Settings that only make sense on one side.
HOST_ONLY = {"type", "wifi_configured", "input_mode", "dim_screen", "duration", "curve", "orientation_lock"}
TABLET_ONLY = {"immersive_mode", "input_mode", "ease_in", "ease_in_out", "ease_out", "exponential"}
DESKTOP_ONLY = set()

def host_keys() -> set[str]:
    src = (ROOT / "host/src/config/options/device.rs").read_text()
    body = src[src.index("pub fn config_update"):]
    return set(re.findall(r'^\s*"([a-z_0-9]+)":', body, re.M))

def tablet_keys() -> set[str]:
    src = (ROOT / "android/app/src/main/java/com/uscreen/HostLink.kt").read_text()
    # Known to the tablet app if it can change it (`withSetting`) or reads it from the host's message (`fromJson`).
    return set(re.findall(r'^\s*"([a-z_0-9]+)" ->', src, re.M)) | set(re.findall(r'\.opt[A-Za-z]+\("([a-z_0-9]+)"', src))

def desktop_keys() -> set[str]:
    src = (ROOT / "uscreen-pro-gui/src/components/power/shared.ts").read_text()
    block = src[src.index("interface Values"):] if "interface Values" in src else src
    return set(re.findall(r"^\s+([a-z_0-9]+)\??: (?:boolean|number|string);", block, re.M))

def main() -> int:
    host, tablet, desktop = host_keys(), tablet_keys(), desktop_keys()
    problems = []
    for k in sorted(host - HOST_ONLY - tablet):
        problems.append(f"{k}: sent by the host but the tablet app does not know it")
    for k in sorted((tablet - TABLET_ONLY) - host):
        problems.append(f"{k}: the tablet app handles it but the host never sends it")
    for k in sorted(((host & tablet) - HOST_ONLY - TABLET_ONLY) - desktop - DESKTOP_ONLY):
        problems.append(f"{k}: synced to the tablet but missing from the desktop app's settings type")
    for p in problems:
        print("PARITY:", p)
    print(f"host {len(host)} keys, tablet {len(tablet)}, desktop {len(desktop)}; {len(problems)} difference(s)")
    return 1 if problems else 0

if __name__ == "__main__":
    sys.exit(main())
