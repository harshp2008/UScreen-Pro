---
title: "4 Bug 4 - Stubborn Meta-0 (MetaVendor) Monitor"
sidebar:
  order: 4
  label: "4 Bug 4 - Stubborn Meta-0 (MetaVendor) Monitor"
---

The virtual monitor `Meta-0` (`MetaVendor`, `MetaVirtualMonitor`) with resolution `1024x600` was persistently appearing on the desktop layout at position (3384, 0), creating an unwanted phantom monitor to the right of the tablet.

| Field | Value |
| :-- | :-- |
| **ID** | 4 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | Meta-0 is completely eliminated from the live compositor layout and cannot return on subsequent boots because the systemd drop-in generating --virtual-monitor has been removed. |
| **Raw results** | not recorded |

## What was tested

### Problem Description

The virtual monitor `Meta-0` (`MetaVendor`, `MetaVirtualMonitor`) with resolution `1024x600` was persistently appearing on the desktop layout at position (3384, 0), creating an unwanted phantom monitor to the right of the tablet.

### Root Cause Analysis

During a previous attempt to fix the Mutter inactive display throttling issue, a systemd user drop-in override was created at:
`~/.config/systemd/user/user@example.com/persistent-virtual-monitor.conf`
containing:
```ini
[Service]
ExecStart=
ExecStart=/usr/bin/gnome-shell --virtual-monitor 1024x600
```
When GNOME Shell started under systemd user management, the `--virtual-monitor 1024x600` parameter instructed Mutter to instantiate an emulated virtual DRM output (`Meta-0`). Mutter added this output to its active logical monitor list, causing it to appear in GNOME display settings and persist across reboots.


## Result

### Investigation & Verification Log



### Terminal Check: Inspecting Running GNOME Shell Process

```bash
$ ps aux | grep -i gnome-shell
user 5773 11.2 1.9 11551024 471040 ? Rsl 13:38 1:23 /usr/bin/gnome-shell --virtual-monitor 1024x600
```

### D-Bus Query: Mutter DisplayConfig GetCurrentState

```bash
$ python3 -c "
import dbus
bus = dbus.SessionBus()
obj = bus.get_object('org.gnome.Mutter.DisplayConfig', '/org/gnome/Mutter/DisplayConfig')
iface = dbus.Interface(obj, 'org.gnome.Mutter.DisplayConfig')
state = iface.GetCurrentState()
for lm in state[2]:
    print('Logical monitor:', lm[0], lm[1], lm[2], lm[3], lm[4], lm[5])
"
Logical monitor: 0 0 1.0 0 1 dbus.Array([dbus.Struct((dbus.String('eDP-1'), dbus.String('AUO'), dbus.String('0x6db2'), dbus.String('0x00000000')), signature=None)], signature=dbus.Signature('(ssss)'))
Logical monitor: 1920 0 1.748633861541748 0 0 dbus.Array([dbus.Struct((dbus.String('DVI-I-2'), dbus.String('USC'), dbus.String('UScreen'), dbus.String('0x00000001')), signature=None)], signature=dbus.Signature('(ssss)'))
Logical monitor: 3384 0 1.0 0 0 dbus.Array([dbus.Struct((dbus.String('Meta-0'), dbus.String('MetaVendor'), dbus.String('MetaVirtualMonitor'), dbus.String('0x00')), signature=None)], signature=dbus.Signature('(ssss)'))
```

### Checking User Services, Timers, and Cron Jobs

- Timers: `systemctl --user list-timers` showed only standard firmware update and launchpadlib timers.
- Services: No custom services recreating the drop-in.
- Cron: Crontab does not touch `monitors.xml` or systemd configs.

### Remediation Actions Taken

1. **Removed the systemd drop-in override directory**:
   ```bash
   rm -rf ~/.config/systemd/user/user@example.com/
   systemctl --user daemon-reload
   ```

2. **Cleaned up `monitors.xml`**:
   Verified that `~/.config/monitors.xml` contains no stale Meta-0 entries (`clean-monitors-xml.py` confirmed clean).

3. **Dynamically Deactivated `Meta-0` in Live Mutter Session**:
   Called `org.gnome.Mutter.DisplayConfig.ApplyMonitorsConfig` via D-Bus omitting `Meta-0`, applying a clean layout containing only `eDP-1` and `DVI-I-2`.

### Post-Remediation Verification Log

```bash
$ python3 -c "
import dbus
bus = dbus.SessionBus()
obj = bus.get_object('org.gnome.Mutter.DisplayConfig', '/org/gnome/Mutter/DisplayConfig')
iface = dbus.Interface(obj, 'org.gnome.Mutter.DisplayConfig')
state = iface.GetCurrentState()
print('Active logical monitors:', len(state[2]))
for lm in state[2]:
    print('  Position:', (lm[0], lm[1]), 'Output:', lm[5][0][0])
"
Active logical monitors: 2
  Position: (dbus.Int32(0), dbus.Int32(0)) Output: eDP-1
  Position: (dbus.Int32(1920), dbus.Int32(0)) Output: DVI-I-2
```

### Conclusion

`Meta-0` is completely eliminated from the live compositor layout and cannot return on subsequent boots because the systemd drop-in generating `--virtual-monitor` has been removed.


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/04_bug4_meta0_virtual_monitor.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/04_bug4_meta0_virtual_monitor.md)
