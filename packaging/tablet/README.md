# Physical drawing tablet udev rules (optional)

Example rules for an XP-Pen `28bd:0931`. Generate the ones for your tablet with
`uscreen tablet-rules --hide` (it finds the plugged-in pen tablet). **Nothing here is installed by the
packages or by `install.sh`.**

| File | Purpose | Needed? |
|---|---|---|
| `60-uscreen-tablet-access.rules` | `uaccess`: the logged-in user may read the pen without being in the `input` group | Only if you are not in `input` |
| `99-uscreen-hide-tablet.rules` | `LIBINPUT_IGNORE_DEVICE=1`: Mutter/KWin/X ignore the real tablet (same mechanism as OpenTabletDriver) | Optional: the daemon already grabs the tablet exclusively while *Physical Tablet Support* is on |

Install (needs root; review first):

```bash
sudo install -m644 60-uscreen-tablet-access.rules /etc/udev/rules.d/
sudo install -m644 99-uscreen-hide-tablet.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=input --action=change
```

**Warning:** with the hide rule installed and UScreen not running (or Physical Tablet Support off) the tablet does
nothing for the whole desktop. Remove the file and re-run the `udevadm` line to get it back. It also hides the tablet's
express-key and pad-as-mouse nodes.
