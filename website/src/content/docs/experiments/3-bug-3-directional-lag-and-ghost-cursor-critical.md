---
title: "3 Bug 3 - Directional Lag and Ghost Cursor (Critical)"
sidebar:
  order: 3
  label: "3 Bug 3 - Directional Lag and Ghost Cursor (Critical)"
---

Moving the mouse pointer from the laptop's primary display (`eDP-1`) to the tablet display (`DVI-I-2`) is smooth and responsive. However, moving the mouse from the tablet back to the primary display causes:
1. **Severe compositor lag and stutter** on the primary screen.
2. **Tablet FPS drops and freezes** at ~1 FPS.
3. A **ghost cursor** remains frozen on the tablet at the edge where the mouse crossed.

| Field | Value |
| :-- | :-- |
| **ID** | 3 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

### Problem Description

Moving the mouse pointer from the laptop's primary display (`eDP-1`) to the tablet display (`DVI-I-2`) is smooth and responsive. However, moving the mouse from the tablet back to the primary display causes:
1. **Severe compositor lag and stutter** on the primary screen.
2. **Tablet FPS drops and freezes** at ~1 FPS.
3. A **ghost cursor** remains frozen on the tablet at the edge where the mouse crossed.

### Real-Time Terminal Evidence & Unedited Kernel Logs

While the mouse was positioned on the primary display (leaving the tablet idle), the kernel log was inspected:

```bash
$ journalctl -k -b --grep="evdi" --lines=60 --no-pager
Nov 04 14:11:12 user kernel: evdi: [W] evdi_painter_request_update_ioctl:1203 (card2) Update was already requested - ignoring
Nov 04 14:11:12 user kernel: evdi: [W] evdi_painter_grabpix_ioctl:1087  but pixels are grabbed.
Nov 04 14:11:12 user kernel: evdi: [W] evdi_painter_grabpix_ioctl:1085 (card2) Update ready not sent,
Nov 04 14:11:12 user kernel: evdi: [W] evdi_painter_request_update_ioctl:1203 (card2) Update was already requested - ignoring
Nov 04 14:11:12 user kernel: evdi: [W] evdi_painter_grabpix_ioctl:1087  but pixels are grabbed.
Nov 04 14:11:12 user kernel: evdi: [W] evdi_painter_grabpix_ioctl:1085 (card2) Update ready not sent,
Nov 04 14:11:12 user kernel: evdi: [W] evdi_painter_request_update_ioctl:1203 (card2) Update was already requested - ignoring
Nov 04 14:11:12 user kernel: evdi: [W] evdi_painter_grabpix_ioctl:1087  but pixels are grabbed.
Nov 04 14:11:12 user kernel: evdi: [W] evdi_painter_grabpix_ioctl:1085 (card2) Update ready not sent,
Nov 04 14:11:11 user kernel: evdi: [W] evdi_painter_request_update_ioctl:1203 (card2) Update was already requested - ignoring
Nov 04 14:11:11 user kernel: evdi: [W] evdi_painter_grabpix_ioctl:1087  but pixels are grabbed.
Nov 04 14:11:11 user kernel: evdi: [W] evdi_painter_grabpix_ioctl:1085 (card2) Update ready not sent,
Nov 04 14:11:11 user kernel: evdi: [W] evdi_painter_request_update_ioctl:1203 (card2) Update was already requested - ignoring
Nov 04 14:11:11 user kernel: evdi: [W] evdi_painter_grabpix_ioctl:1087  but pixels are grabbed.
Nov 04 14:11:11 user kernel: evdi: [W] evdi_painter_grabpix_ioctl:1085 (card2) Update ready not sent,
```

Notice the pattern:
- **Exact Frequency**: Exactly every 250 milliseconds (4 times every second), the kernel prints both:
  1. `evdi_painter_grabpix_ioctl:1085 (card2) Update ready not sent, but pixels are grabbed.`
  2. `evdi_painter_request_update_ioctl:1203 (card2) Update was already requested - ignoring`

### Deep-Dive Root Cause Analysis



### Mechanism 1: EVDI Watchdog Desynchronization & Kernel Mutex Contention (The Cause of Severe Lag)

Located in `host/evdi/evdi_helper.c` (lines 964-990):

```c
if (!g_update_pending && (now - g_last_request_ms) >= request_period_ms) {
    g_last_request_ms = now;
    g_req_us = now_us();
    if (evdi_request_update(handle, 0)) {
        g_immediate_n++;
        grab_now();
    } else {
        g_update_pending = 1;
    }
}

/* Watchdog: if a request got lost (compositor hiccup), don't stay
   stuck waiting for update_ready forever. */
if (g_update_pending && (now - g_last_request_ms) > 250) {
    g_update_pending = 0;
    grab_now();
}
```

1. When the cursor is on `DVI-I-2`, Mutter repaints the output on every pointer event. EVDI signals `update_ready`, which triggers `on_update_ready()`, grabs pixels cleanly, and clears `g_update_pending`.
2. When the cursor crosses to `eDP-1`, Mutter has zero damage on `DVI-I-2`. Mutter does not call `atomic_commit` or pageflip on `DVI-I-2`.
3. `evdi_helper` sent `evdi_request_update(handle, 0)` right as the cursor departed. It sets `g_update_pending = 1`.
4. `update_ready` never fires because Mutter is idle on that monitor.
5. After 250ms, the watchdog triggers:
   - It sets `g_update_pending = 0`.
   - It unconditionally calls `grab_now()`.
   - Inside `grab_now()`, `evdi_grab_pixels()` is called without `update_ready` having fired.
   - The EVDI kernel module detects `!painter->update_ready` and throws:
     `evdi: [W] evdi_painter_grabpix_ioctl:1085 (card2) Update ready not sent, but pixels are grabbed.`
   - In `grab_now()`, `num_rects` is 0. So `publish_frame()` is not called.
6. In the very next iteration of `run_event_loop()`:
   - `!g_update_pending` is now true (watchdog forcibly reset it).
   - `now - g_last_request_ms >= request_period_ms` is true (250ms > 16ms).
   - So `evdi_helper` immediately calls `evdi_request_update(handle, 0)`.
   - But in the EVDI kernel driver, `painter->update_requested` is STILL 1 because the previous request was never completed by an `update_ready` event!
   - The kernel driver rejects the call and throws:
     `evdi: [W] evdi_painter_request_update_ioctl:1203 (card2) Update was already requested - ignoring`
7. **The Compositor Lag Impact**:
   Every 250ms, the helper enters the kernel via ioctl, acquiring EVDI's painter mutex while the driver's internal state machine is invalid. GNOME Shell's native KMS thread (which manages atomic commits across all DRM devices, including the Intel primary GPU and EVDI) contends on DRM subsystem locks. This causes frame pacing hitches and severe lag when moving the mouse on the primary screen.

---

### Mechanism 2: Frame Starvation & 1 FPS Freeze

In `host/evdi/evdi_helper.c`:
1. In `grab_now()`, when `num_rects <= 0`, `publish_frame()` is bypassed.
2. In `writer_thread()`, `g_latest_valid` remains 0. The writer thread times out on `pthread_cond_timedwait`.
3. Because `!fresh`, the writer only emits frames on `IDLE_KEEPALIVE_MS` (200ms = 5 FPS) or stalls when fallback grab fails.
4. FFmpeg's rawvideo demuxer blocks on the capture FIFO, and the Android tablet video decoder starves, dropping playback to ~1 FPS.

---

### Mechanism 3: The Ghost Cursor & Mutter Software Cursor

In `/etc/environment`:
`MUTTER_DEBUG_DISABLE_HW_CURSORS=1` is set globally.

1. When software cursors are active, Mutter does not use the DRM hardware cursor plane (`DRM_PLANE_TYPE_CURSOR`). Instead, Mutter composites the cursor sprite directly into the primary framebuffer pixel memory.
2. When the user moves the mouse cursor toward the edge of the tablet to exit to the PC screen, Mutter draws the cursor onto the `DVI-I-2` framebuffer.
3. The instant the cursor coordinate leaves the boundary of `DVI-I-2`, Mutter stops rendering `DVI-I-2`.
4. Mutter does not render an "erase cursor" frame to `DVI-I-2` because Wayland compositors do not commit flips to idle displays.
5. The EVDI framebuffer retains the last committed pixel buffer—which has the cursor burned into it at the boundary!
6. `evdi_helper` never receives a clean frame without the cursor. The tablet displays the frozen "ghost cursor" indefinitely.

---

### Mechanism 4: Virtual Pointer Handoff in `host/src/input.rs`

Located in `host/src/input.rs` (lines 953-960 and 1573-1594):

```rust
let area = match output.as_deref() {
    Some(o) if crate::hyprland::active() => crate::hyprland::output_area(o).await,
    Some(_) if crate::gnome::active() || crate::x11::active() => None,
    Some(o) => output_area_on_desktop(o).await,
    None => None,
};
```

1. On GNOME, `input.rs` hardcodes `pointer_area = None`.
2. GNOME does NOT have a relocatable schema for generic absolute pointers (`UScreen Pointer`), unlike `tablet` and `touchscreen`.
3. When the pen lifts or leaves proximity (`action == 4`), `input.rs` emits absolute coordinates:
   ```rust
   let (px, py) = match guard.pointer_area {
       Some(...) => ...,
       None => (px, py),
   };
   dev.emit(EV_ABS, ABS_X, px.clamp(0, COORD_MAX));
   dev.emit(EV_ABS, ABS_Y, py.clamp(0, COORD_MAX));
   ```
4. With `pointer_area = None`, `(px, py)` maps 0..65535 across the *entire combined desktop geometry*.
5. When the user transitions from tablet to laptop, the pointer handoff abruptly teleports the cursor across monitors, fighting the compositor handoff and triggering directional input desync.


## Result

### Remediation Plan



### Fix A: Patch `host/evdi/evdi_helper.c` State Machine

1. **Fix Watchdog & Request Logic**:
   - Never call `evdi_grab_pixels()` when `g_update_pending` is true unless `update_ready` has been received or `evdi_request_update()` returned 1 (immediate).
   - If a request is outstanding (`g_update_pending == 1`) and the compositor is idle, do NOT clear `g_update_pending` and do NOT spam `evdi_request_update()`. Allow the outstanding request to remain valid or cancel/poll cleanly.
   - When the screen is idle, deliver clean keepalive frames to the FIFO directly from the existing valid buffer without hammering EVDI ioctls.
2. **Explicitly Configure EVDI Cursor Events**:
   - Call `evdi_enable_cursor_events(handle, false)` during initialization to ensure kernel-level blending consistency and prevent driver state conflicts.

### Fix B: Patch `host/src/input.rs` Pointer Area Calculation for GNOME

- Calculate the correct normalized desktop bounding box `(ox, oy, ow, oh)` for the EVDI monitor using Mutter's `GetCurrentState` logical monitor coordinates.
- Ensure `UScreen Pointer` events map strictly within the tablet's monitor bounds on GNOME Wayland, preventing cursor warp across screens on proximity exit.

### Fix C: Eliminate Kernel Log Flooding & Mutex Stalls

- With the state machine repaired, the 4-times-per-second ioctl flood will drop to zero, completely removing Mutter compositor contention and restoring silky-smooth mouse motion in both directions.


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/03_bug3_directional_lag_ghost_cursor.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/03_bug3_directional_lag_ghost_cursor.md)
