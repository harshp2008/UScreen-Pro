# 103 - Capturing the real scanout to see the ghost cursor (dropped)

Goal: the "second cursor" (stale software-cursor image) is not in screenshots, because a screenshot
is composed fresh. Try to read the picture the GPU actually sends to the panel, so an automatic
check could see the ghost.

Method: `sudo ffmpeg -f kmsgrab -device /dev/dri/card1 -i - -vf 'hwmap=derive_device=vaapi,...' -frames:v 1`
(ffmpeg 6.1.1, Intel xe, eDP-1 1920x1080).

Result:
* The grab works and returns a 1920x1080 frame of the primary plane.
* The framebuffer is tiled and compressed (`modifier 10000000000000f`, format XR30, 10-bit), and both
  conversions tried (`nv12`, `bgra` through VAAPI) produced a scrambled picture. Position and size are
  right, content is not decodable this way.
* The second attempt was followed about a minute later by the compositor not answering; the daemon's
  stall watchdog tripped ("shell main loop not answering") and the laptop needed a forced restart. The
  virtual display was up at the time, so the cause is either the capture, the known EVDI stall of
  experiment 56, or both. Not separated.
* After the restart the guard stayed in SAFE MODE (no virtual display) until `uscreen reset-guard`.

Decision: do not use kmsgrab for checks. It needs root, does not give a readable picture on this GPU,
and it may hang the machine. Ghost-cursor checks use a camera photo.

How the ghost was actually fixed: snapping the pointer to whole physical pixels (daemon, GNOME), found by
replaying a recorded pen stream on a virtual pointer with variants (raw, slow, snap, logical, settle).
